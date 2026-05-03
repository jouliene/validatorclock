use crate::state::AppState;
use crate::tls;
use anyhow::{Context, Result, bail};
use axum::Router;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::{Duration, timeout};
use tokio_rustls::TlsAcceptor;
use tower::ServiceExt;

mod routes;
mod security;

#[cfg(test)]
mod tests;

pub(crate) use security::{normalize_host, public_url_host};

const REQUEST_TIMEOUT_SECS: u64 = 10;

pub(crate) async fn run_plain_http_server(state: Arc<AppState>) -> Result<()> {
    let listener = TcpListener::bind(&state.config.listen)
        .await
        .with_context(|| format!("failed to bind {}", state.config.listen))?;
    println!(
        "validators_clock listening on http://{}",
        state.config.listen
    );

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
            eprintln!("HTTP challenge/redirect listener failed: {error:#}");
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

    println!(
        "validators_clock listening on https://{}",
        tls_config.https_listen
    );
    serve_tls_connections(
        https_listener,
        routes::app_router(Arc::clone(&state)),
        acceptor,
        state.config.security.max_connections,
    )
    .await
}

async fn serve_plain_connections(
    listener: TcpListener,
    app: Router,
    max_connections: usize,
    label: &'static str,
) -> Result<()> {
    let permits = Arc::new(Semaphore::new(max_connections));

    loop {
        let (stream, _) = listener.accept().await?;
        let permit = Arc::clone(&permits).acquire_owned().await?;
        let app = app.clone();
        tokio::spawn(async move {
            let _permit = permit;
            serve_connection(stream, app, label).await;
        });
    }
}

async fn serve_tls_connections(
    listener: TcpListener,
    app: Router,
    acceptor: Arc<RwLock<TlsAcceptor>>,
    max_connections: usize,
) -> Result<()> {
    let permits = Arc::new(Semaphore::new(max_connections));

    loop {
        let (stream, _) = listener.accept().await?;
        let acceptor = acceptor.read().await.clone();
        let permit = Arc::clone(&permits).acquire_owned().await?;
        let app = app.clone();
        tokio::spawn(async move {
            let _permit = permit;
            match acceptor.accept(stream).await {
                Ok(tls_stream) => serve_connection(tls_stream, app, "HTTPS").await,
                Err(error) => eprintln!("TLS handshake failed: {error:#}"),
            }
        });
    }
}

async fn serve_connection<S>(stream: S, app: Router, label: &'static str)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let service = service_fn(move |request: hyper::Request<Incoming>| {
        let app = app.clone();
        async move { app.oneshot(request).await }
    });
    let io = TokioIo::new(stream);
    let mut builder = http1::Builder::new();
    builder.keep_alive(false);
    let connection = builder.serve_connection(io, service);

    match timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS), connection).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("{label} request failed: {error:#}"),
        Err(_) => eprintln!("{label} request timed out"),
    }
}
