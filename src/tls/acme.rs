use super::account::load_or_create_acme_account;
use super::cert::{build_server_config, load_cert_chain, load_private_key, load_tls_acceptor};
use super::names::{acme_identifier, certificate_subject_names, missing_certificate_identifiers};
use crate::config::TlsConfig;
use crate::fsutil::write_file_atomic;
use crate::state::AppState;
use anyhow::{Context, Result, anyhow, bail};
use instant_acme::{AuthorizationStatus, ChallengeType, NewOrder, OrderStatus, RetryPolicy};
use std::sync::Arc;
use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};
use tokio_rustls::TlsAcceptor;
use tracing::{error, info};
use x509_parser::parse_x509_certificate;

pub(crate) async fn ensure_acme_certificate(state: &AppState) -> Result<()> {
    let tls = &state.config.tls;
    if !certificate_needs_renewal(tls)? {
        return Ok(());
    }

    issue_acme_certificate(state).await
}

pub(crate) async fn acme_renewal_loop(state: Arc<AppState>, acceptor: Arc<RwLock<TlsAcceptor>>) {
    let interval = Duration::from_secs(
        state
            .config
            .tls
            .acme
            .renew_before_seconds()
            .min(24 * 60 * 60),
    );

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
    let renew_before_expiry = tls.acme.renew_before_seconds() as i64;
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

async fn issue_acme_certificate(state: &AppState) -> Result<()> {
    let tls = &state.config.tls;
    let acme = &tls.acme;

    let account = load_or_create_acme_account(acme).await?;
    let identifier_names = acme.identifier_values().collect::<Vec<_>>();
    let identifiers = identifier_names
        .iter()
        .map(|identifier| acme_identifier(identifier))
        .collect::<Result<Vec<_>>>()?;
    let mut order_request = NewOrder::new(&identifiers);
    if let Some(profile) = acme.profile_value() {
        order_request = order_request.profile(profile);
    }
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
        profile = %acme.profile_value().unwrap_or("default"),
        "issued ACME certificate"
    );
    Ok(())
}
