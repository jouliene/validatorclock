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
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration as StdDuration, SystemTime};
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};
use tokio_rustls::TlsAcceptor;
use tracing::{error, info};

pub(crate) fn install_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

pub(crate) fn load_tls_acceptor(tls: &TlsConfig) -> Result<TlsAcceptor> {
    let certs = load_cert_chain(&tls.cert_path)?;
    let key = load_private_key(&tls.key_path)?;
    let mut config =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .context("failed to configure TLS protocol versions")?
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .context("failed to load TLS certificate")?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];

    Ok(TlsAcceptor::from(Arc::new(config)))
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

    let cert_modified = fs::metadata(&tls.cert_path)
        .and_then(|metadata| metadata.modified())
        .with_context(|| format!("failed to read metadata for {}", tls.cert_path.display()))?;
    let key_modified = fs::metadata(&tls.key_path)
        .and_then(|metadata| metadata.modified())
        .with_context(|| format!("failed to read metadata for {}", tls.key_path.display()))?;
    let oldest = cert_modified.min(key_modified);
    let age = SystemTime::now()
        .duration_since(oldest)
        .unwrap_or(StdDuration::ZERO);

    Ok(age >= StdDuration::from_secs(tls.acme.renew_after_seconds))
}

async fn issue_acme_certificate(state: &AppState) -> Result<()> {
    let tls = &state.config.tls;
    let acme = &tls.acme;

    let account = load_or_create_acme_account(acme).await?;
    let identifiers = vec![acme_identifier(&acme.identifier)?];
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
        identifier = %acme.identifier,
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
    if let Ok(address) = value.parse::<IpAddr>() {
        return Ok(Identifier::Ip(address));
    }

    let name = value.trim();
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
