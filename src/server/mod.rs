use self::connection::{serve_plain_connections, serve_tls_connections};
use crate::state::AppState;
use crate::tls;
use anyhow::{Context, Result, bail};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{error, info};

mod acme;
mod api;
mod assets;
mod connection;
mod responses;
mod routes;
mod security;

#[cfg(test)]
mod tests;

pub(crate) use security::{normalize_host, public_url_host};

pub(crate) async fn run_plain_http_server(state: Arc<AppState>) -> Result<()> {
    let listener = TcpListener::bind(&state.config.listen)
        .await
        .with_context(|| format!("failed to bind {}", state.config.listen))?;
    info!(listen = %state.config.listen, "validatorclock listening on HTTP");

    serve_plain_connections(
        listener,
        routes::app_router(Arc::clone(&state)),
        state.config.security.max_connections,
        "HTTP",
    )
    .await
}

pub(crate) async fn run_tls_server(state: Arc<AppState>) -> Result<()> {
    let tls_config = &state.config.tls;
    let http_listener = TcpListener::bind(&tls_config.http_listen)
        .await
        .with_context(|| format!("failed to bind {}", tls_config.http_listen))?;
    let https_listener = TcpListener::bind(&tls_config.https_listen)
        .await
        .with_context(|| format!("failed to bind {}", tls_config.https_listen))?;

    let http_state = Arc::clone(&state);
    let max_connections = state.config.security.max_connections;
    tokio::spawn(async move {
        if let Err(error) = serve_plain_connections(
            http_listener,
            routes::challenge_redirect_router(http_state),
            max_connections,
            "HTTP challenge/redirect",
        )
        .await
        {
            error!(error = ?error, "HTTP challenge/redirect listener failed");
        }
    });

    if tls_config.acme.enabled {
        tls::ensure_acme_certificate(&state).await?;
    } else if !tls_config.cert_path.exists() || !tls_config.key_path.exists() {
        bail!("TLS is enabled but cert/key files do not exist and tls.acme.enabled is false");
    }

    let acceptor = Arc::new(RwLock::new(tls::load_tls_acceptor(tls_config)?));

    if tls_config.acme.enabled {
        let renewal_state = Arc::clone(&state);
        let renewal_acceptor = Arc::clone(&acceptor);
        tokio::spawn(async move {
            tls::acme_renewal_loop(renewal_state, renewal_acceptor).await;
        });
    }

    info!(listen = %tls_config.https_listen, "validatorclock listening on HTTPS");
    serve_tls_connections(
        https_listener,
        routes::app_router(Arc::clone(&state)),
        acceptor,
        state.config.security.max_connections,
    )
    .await
}
