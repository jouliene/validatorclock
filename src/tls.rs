use crate::config::{AcmeConfig, TlsConfig};
use crate::fsutil::write_file_atomic;
use crate::state::AppState;
use anyhow::{Context, Result, anyhow, bail};
use instant_acme::{
    Account as AcmeAccount, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier,
    NewAccount, NewOrder, OrderStatus, RetryPolicy,
};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};
use tokio_rustls::TlsAcceptor;
use tracing::{error, info};
use x509_parser::certificate::X509Certificate;
use x509_parser::extensions::GeneralName;
use x509_parser::parse_x509_certificate;

pub(crate) fn install_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

pub(crate) fn load_tls_acceptor(tls: &TlsConfig) -> Result<TlsAcceptor> {
    let certs = load_cert_chain(&tls.cert_path)?;
    let key = load_private_key(&tls.key_path)?;
    let config = build_server_config(certs, key)?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

fn build_server_config(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<ServerConfig> {
    let mut config =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .context("failed to configure TLS protocol versions")?
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .context("failed to load TLS certificate")?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];

    Ok(config)
}

fn load_cert_chain(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    let mut reader = BufReader::new(
        File::open(path).with_context(|| format!("failed to open cert {}", path.display()))?,
    );
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse cert {}", path.display()))?;
    if certs.is_empty() {
        bail!("cert {} contains no certificates", path.display());
    }
    Ok(certs)
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>> {
    let mut reader = BufReader::new(
        File::open(path)
            .with_context(|| format!("failed to open private key {}", path.display()))?,
    );
    rustls_pemfile::private_key(&mut reader)
        .with_context(|| format!("failed to parse private key {}", path.display()))?
        .ok_or_else(|| anyhow!("private key {} contains no key", path.display()))
}

pub(crate) async fn ensure_acme_certificate(state: &AppState) -> Result<()> {
    let tls = &state.config.tls;
    if !certificate_needs_renewal(tls)? {
        return Ok(());
    }

    issue_acme_certificate(state).await
}

pub(crate) async fn acme_renewal_loop(state: Arc<AppState>, acceptor: Arc<RwLock<TlsAcceptor>>) {
    let interval = Duration::from_secs(state.config.tls.acme.renew_after_seconds.min(24 * 60 * 60));

    loop {
        sleep(interval).await;
        match ensure_acme_certificate(&state).await {
            Ok(()) => match load_tls_acceptor(&state.config.tls) {
                Ok(updated) => {
                    *acceptor.write().await = updated;
                    info!("reloaded TLS certificate");
                }
                Err(error) => error!(error = ?error, "failed to reload TLS certificate"),
            },
            Err(error) => error!(error = ?error, "ACME renewal failed"),
        }
    }
}

fn certificate_needs_renewal(tls: &TlsConfig) -> Result<bool> {
    if !tls.cert_path.exists() || !tls.key_path.exists() {
        return Ok(true);
    }

    let key = match load_private_key(&tls.key_path) {
        Ok(key) => key,
        Err(error) => {
            info!(error = ?error, "TLS private key cannot be loaded; renewing certificate");
            return Ok(true);
        }
    };
    let certs = match load_cert_chain(&tls.cert_path) {
        Ok(certs) => certs,
        Err(error) => {
            info!(error = ?error, "TLS certificate cannot be loaded; renewing certificate");
            return Ok(true);
        }
    };
    if let Err(error) = build_server_config(certs.clone(), key) {
        info!(error = ?error, "TLS certificate/key pair cannot be used; renewing certificate");
        return Ok(true);
    }

    let leaf = certs
        .first()
        .ok_or_else(|| anyhow!("certificate chain unexpectedly empty"))?;
    let certificate = match parse_x509_certificate(leaf.as_ref()) {
        Ok((_, certificate)) => certificate,
        Err(error) => {
            info!(error = ?error, "TLS certificate cannot be parsed; renewing certificate");
            return Ok(true);
        }
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(StdDuration::ZERO);
    let now_timestamp = now.as_secs().min(i64::MAX as u64) as i64;
    let not_before = certificate.validity().not_before.timestamp();
    if now_timestamp < not_before {
        info!(
            not_before,
            "TLS certificate is not valid yet; renewing certificate"
        );
        return Ok(true);
    }

    let seconds_until_expiry = certificate
        .validity()
        .not_after
        .timestamp()
        .saturating_sub(now_timestamp);
    let renew_before_expiry = tls.acme.renew_after_seconds as i64;
    if seconds_until_expiry <= renew_before_expiry {
        info!(
            seconds_until_expiry,
            renew_before_expiry, "TLS certificate expires soon; renewing certificate"
        );
        return Ok(true);
    }

    let certificate_names = match certificate_subject_names(&certificate) {
        Ok(names) => names,
        Err(error) => {
            info!(error = ?error, "TLS certificate names cannot be read; renewing certificate");
            return Ok(true);
        }
    };
    let missing_identifiers = missing_certificate_identifiers(&certificate_names, &tls.acme)?;
    if !missing_identifiers.is_empty() {
        info!(
            missing_identifiers = %missing_identifiers.join(","),
            "TLS certificate does not cover configured identifiers; renewing certificate"
        );
        return Ok(true);
    }

    Ok(false)
}

fn certificate_subject_names(certificate: &X509Certificate<'_>) -> Result<HashSet<String>> {
    let mut names = HashSet::new();
    let Some(subject_alternative_name) = certificate
        .subject_alternative_name()
        .map_err(|error| anyhow!("invalid subject alternative name extension: {error:?}"))?
    else {
        return Ok(names);
    };

    for name in &subject_alternative_name.value.general_names {
        match name {
            GeneralName::DNSName(name) => {
                names.insert(normalize_dns_name(name));
            }
            GeneralName::IPAddress(bytes) => {
                if let Some(address) = ip_address_from_san(bytes) {
                    names.insert(address.to_string());
                }
            }
            _ => {}
        }
    }

    Ok(names)
}

fn missing_certificate_identifiers(
    certificate_names: &HashSet<String>,
    acme: &AcmeConfig,
) -> Result<Vec<String>> {
    let mut missing = Vec::new();
    for identifier in acme.identifier_values() {
        let normalized = normalize_certificate_identifier(identifier)?;
        if !certificate_names.contains(&normalized) {
            missing.push(normalized);
        }
    }
    Ok(missing)
}

fn normalize_certificate_identifier(value: &str) -> Result<String> {
    match acme_identifier(value)? {
        Identifier::Dns(name) => Ok(normalize_dns_name(&name)),
        Identifier::Ip(address) => Ok(address.to_string()),
        _ => bail!("unsupported ACME identifier type"),
    }
}

fn normalize_dns_name(name: &str) -> String {
    name.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn ip_address_from_san(bytes: &[u8]) -> Option<IpAddr> {
    match bytes.len() {
        4 => Some(IpAddr::V4(Ipv4Addr::new(
            bytes[0], bytes[1], bytes[2], bytes[3],
        ))),
        16 => {
            let mut octets = [0; 16];
            octets.copy_from_slice(bytes);
            Some(IpAddr::V6(Ipv6Addr::from(octets)))
        }
        _ => None,
    }
}

async fn issue_acme_certificate(state: &AppState) -> Result<()> {
    let tls = &state.config.tls;
    let acme = &tls.acme;

    let account = load_or_create_acme_account(acme).await?;
    let identifier_names = acme.identifier_values().collect::<Vec<_>>();
    let identifiers = identifier_names
        .iter()
        .map(|identifier| acme_identifier(identifier))
        .collect::<Result<Vec<_>>>()?;
    let order_request = NewOrder::new(&identifiers).profile(&acme.profile);
    let mut order = account
        .new_order(&order_request)
        .await
        .context("failed to create ACME order")?;

    let mut challenge_tokens = Vec::new();
    let mut authorizations = order.authorizations();
    while let Some(result) = authorizations.next().await {
        let mut authz = result.context("failed to fetch ACME authorization")?;
        match authz.status {
            AuthorizationStatus::Valid => continue,
            AuthorizationStatus::Pending => {}
            status => bail!("unexpected ACME authorization status: {status:?}"),
        }

        let mut challenge = authz
            .challenge(ChallengeType::Http01)
            .ok_or_else(|| anyhow!("ACME authorization has no http-01 challenge"))?;
        let token = challenge.token.clone();
        let key_authorization = challenge.key_authorization().as_str().to_owned();

        state
            .acme_challenges
            .write()
            .await
            .insert(token.clone(), key_authorization);
        if let Err(error) = challenge.set_ready().await {
            state.acme_challenges.write().await.remove(&token);
            return Err(error).context("failed to mark ACME challenge ready");
        }
        challenge_tokens.push(token);
    }

    let retry_policy =
        RetryPolicy::default().timeout(StdDuration::from_secs(acme.retry_timeout_seconds));
    let status_result = order
        .poll_ready(&retry_policy)
        .await
        .context("ACME order did not become ready");
    for token in &challenge_tokens {
        state.acme_challenges.write().await.remove(token);
    }
    let status = status_result?;
    if status != OrderStatus::Ready {
        bail!("unexpected ACME order status: {status:?}");
    }

    let private_key_pem = order
        .finalize()
        .await
        .context("failed to finalize ACME order")?;
    let cert_chain_pem = order
        .poll_certificate(&retry_policy)
        .await
        .context("failed to retrieve ACME certificate")?;

    write_file_atomic(&tls.cert_path, cert_chain_pem.as_bytes(), 0o644)?;
    write_file_atomic(&tls.key_path, private_key_pem.as_bytes(), 0o600)?;
    info!(
        identifiers = %identifier_names.join(","),
        profile = %acme.profile,
        "issued ACME certificate"
    );
    Ok(())
}

async fn load_or_create_acme_account(acme: &AcmeConfig) -> Result<AcmeAccount> {
    let builder = acme_account_builder()?;
    if acme.account_path.exists() {
        let data = fs::read_to_string(&acme.account_path)
            .with_context(|| format!("failed to read {}", acme.account_path.display()))?;
        let credentials: AccountCredentials =
            serde_json::from_str(&data).context("failed to parse ACME account credentials")?;
        return builder
            .from_credentials(credentials)
            .await
            .context("failed to restore ACME account");
    }

    let contacts = acme.contact.iter().map(String::as_str).collect::<Vec<_>>();
    let (account, credentials) = builder
        .create(
            &NewAccount {
                contact: &contacts,
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            acme.directory_url(),
            None,
        )
        .await
        .context("failed to create ACME account")?;

    let data = serde_json::to_vec_pretty(&credentials)?;
    write_file_atomic(&acme.account_path, &data, 0o600)?;
    Ok(account)
}

fn acme_account_builder() -> Result<instant_acme::AccountBuilder> {
    AcmeAccount::builder().context("failed to create ACME account builder")
}

pub(crate) fn acme_identifier(value: &str) -> Result<Identifier> {
    let name = value.trim();
    if let Ok(address) = name.parse::<IpAddr>() {
        return Ok(Identifier::Ip(address));
    }

    if name.is_empty() {
        bail!("ACME identifier cannot be empty");
    }
    if name.contains('/') || name.starts_with('[') {
        bail!("ACME identifier must be a host or IP address without scheme, path, or brackets");
    }
    if name.rsplit_once(':').is_some_and(|(prefix, port)| {
        !prefix.contains(':') && port.chars().all(|ch| ch.is_ascii_digit())
    }) {
        bail!("ACME identifier must not include a port");
    }
    Ok(Identifier::Dns(name.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_certificate_identifiers() {
        assert_eq!(
            normalize_certificate_identifier("Example.COM.").unwrap(),
            "example.com"
        );
        assert_eq!(
            normalize_certificate_identifier("104.238.222.200").unwrap(),
            "104.238.222.200"
        );
    }

    #[test]
    fn detects_missing_certificate_identifiers() {
        let certificate_names = HashSet::from(["validatorsclock.xyz".to_owned()]);
        let acme = AcmeConfig {
            enabled: true,
            identifier: "validatorsclock.xyz".to_owned(),
            extra_identifiers: vec!["www.validatorsclock.xyz".to_owned()],
            ..AcmeConfig::default()
        };

        assert_eq!(
            missing_certificate_identifiers(&certificate_names, &acme).unwrap(),
            vec!["www.validatorsclock.xyz"]
        );
    }

    #[test]
    fn parses_ip_subject_alternative_names() {
        assert_eq!(
            ip_address_from_san(&[104, 238, 222, 200]).unwrap(),
            "104.238.222.200".parse::<IpAddr>().unwrap()
        );
        assert_eq!(ip_address_from_san(&[0, 1, 2]).as_ref(), None::<&IpAddr>);
    }
}
