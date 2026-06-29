use anyhow::Result;
use axum::Router;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::{Duration, timeout};
use tokio_rustls::TlsAcceptor;
use tower::ServiceExt;
use tracing::{debug, warn};

const REQUEST_TIMEOUT_SECS: u64 = 10;

pub(super) async fn serve_plain_connections(
    listener: TcpListener,
    app: Router,
    max_connections: usize,
    label: &'static str,
) -> Result<()> {
    let permits = Arc::new(Semaphore::new(max_connections));

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let permit = Arc::clone(&permits).acquire_owned().await?;
        let app = app.clone();
        tokio::spawn(async move {
            let _permit = permit;
            serve_connection(stream, peer_addr, app, label).await;
        });
    }
}

pub(super) async fn serve_tls_connections(
    listener: TcpListener,
    app: Router,
    acceptor: Arc<RwLock<TlsAcceptor>>,
    max_connections: usize,
) -> Result<()> {
    let permits = Arc::new(Semaphore::new(max_connections));

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let acceptor = acceptor.read().await.clone();
        let permit = Arc::clone(&permits).acquire_owned().await?;
        let app = app.clone();
        tokio::spawn(async move {
            let _permit = permit;
            match acceptor.accept(stream).await {
                Ok(tls_stream) => serve_connection(tls_stream, peer_addr, app, "HTTPS").await,
                Err(error) => debug!(error = ?error, "TLS handshake failed"),
            }
        });
    }
}

async fn serve_connection<S>(stream: S, peer_addr: SocketAddr, app: Router, label: &'static str)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let service = service_fn(move |mut request: hyper::Request<Incoming>| {
        let app = app.clone();
        request.extensions_mut().insert(peer_addr);
        async move { app.oneshot(request).await }
    });
    let io = TokioIo::new(stream);
    let mut builder = http1::Builder::new();
    builder.keep_alive(false);
    let connection = builder.serve_connection(io, service);

    match timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS), connection).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => warn!(label, error = ?error, "request failed"),
        Err(_) => warn!(label, "request timed out"),
    }
}
