use anyhow::Result;
use axum::Router;
use axum::http::StatusCode;
use axum::http::header::{self, HeaderValue};
use axum::response::{IntoResponse, Response};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore};
use tokio::time::{Duration, timeout};
use tokio_rustls::TlsAcceptor;
use tower::ServiceExt;
use tracing::{debug, info, warn};

const REQUEST_TIMEOUT_SECS: u64 = 10;
/// How long a kept-alive connection may sit between requests before the server
/// closes it.
///
/// This has to outlast the page's poll interval, which is thirty seconds. At
/// twenty the connection was always gone before the next poll arrived, so every
/// poll opened a new one and paid a fresh TCP and TLS handshake - the very cost
/// keeping connections alive was meant to avoid. Near the server that is a few
/// milliseconds; a round trip away it is seconds, on every poll.
pub(super) const IDLE_TIMEOUT_SECS: u64 = 75;
const CONNECTION_LIFETIME_SECS: u64 = 300;
/// Once the lifetime is up the connection stops taking new requests. This is
/// how long the one already in flight has to finish before the socket goes.
const LIFETIME_GRACE_SECS: u64 = 15;
const TLS_HANDSHAKE_TIMEOUT_SECS: u64 = 15;
/// How long to wait before accepting again after an error that is not about
/// one connection - a descriptor limit, say - so the loop cannot spin hot.
const ACCEPT_RETRY_DELAY: Duration = Duration::from_secs(1);

pub(super) async fn serve_plain_connections(
    listener: TcpListener,
    app: Router,
    max_connections: usize,
    label: &'static str,
) -> Result<()> {
    let permits = Arc::new(Semaphore::new(max_connections));
    let saturated = Arc::new(AtomicBool::new(false));

    loop {
        let Some((stream, peer_addr)) = accept(&listener, label).await else {
            continue;
        };
        let Some(permit) = claim_slot(&permits, &saturated, label) else {
            continue;
        };
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
    let saturated = Arc::new(AtomicBool::new(false));
    let label = "HTTPS";

    loop {
        let Some((stream, peer_addr)) = accept(&listener, label).await else {
            continue;
        };
        let Some(permit) = claim_slot(&permits, &saturated, label) else {
            continue;
        };
        let acceptor = acceptor.read().await.clone();
        let app = app.clone();
        tokio::spawn(async move {
            let _permit = permit;
            // The permit is held for the whole handshake, so a client that
            // opens a connection and then says nothing must not keep one. The
            // idle timeout only starts once TLS is up.
            match timeout(
                Duration::from_secs(TLS_HANDSHAKE_TIMEOUT_SECS),
                acceptor.accept(stream),
            )
            .await
            {
                Ok(Ok(tls_stream)) => serve_connection(tls_stream, peer_addr, app, label).await,
                Ok(Err(error)) => debug!(error = ?error, "TLS handshake failed"),
                Err(_) => debug!(peer = %peer_addr, "TLS handshake timed out"),
            }
        });
    }
}

/// One connection off the listener, or nothing when the error was about the
/// connection being dequeued rather than about the listener.
///
/// accept(2) hands back errors that belong to the connection being dequeued,
/// not to the listener: a client that went away, a route that dropped under it.
/// Ending the loop on one of those retired the listener for the life of the
/// process - and the port 80 listener is spawned without a handle, so nothing
/// noticed it had gone until a certificate renewal failed.
pub(super) async fn accept(listener: &TcpListener, label: &str) -> Option<(TcpStream, SocketAddr)> {
    match listener.accept().await {
        Ok((stream, peer_addr)) => {
            disable_nagle(&stream, label);
            Some((stream, peer_addr))
        }
        Err(error) => {
            if !is_connection_error(&error) {
                warn!(error = ?error, label, "accept failed; retrying");
                tokio::time::sleep(ACCEPT_RETRY_DELAY).await;
            }
            None
        }
    }
}

/// A place to serve this connection from, or nothing when the server is full.
///
/// The loop never waits here. Waiting stopped it dequeuing anything at all, and
/// because the kernel completes the handshake on the server's behalf, a client
/// saw a connection that was open and then simply never answered: a hang, with
/// nothing on either side to say what had happened. Closing the connection at
/// once says so plainly and lets the client try again.
fn claim_slot(
    permits: &Arc<Semaphore>,
    saturated: &AtomicBool,
    label: &str,
) -> Option<OwnedSemaphorePermit> {
    match Arc::clone(permits).try_acquire_owned() {
        Ok(permit) => {
            if saturated.swap(false, Ordering::Relaxed) {
                info!(label, "connection slots available again");
            }
            Some(permit)
        }
        Err(_) => {
            // One line as it fills and one as it drains, rather than one per
            // refused connection: the moment worth logging is a server at its
            // limit, and that is exactly when logging every connection hurts.
            if !saturated.swap(true, Ordering::Relaxed) {
                warn!(
                    label,
                    "all connection slots are taken; refusing new connections"
                );
            }
            None
        }
    }
}

/// Send a small write without waiting for the previous one to be acknowledged.
///
/// Left on, Nagle's algorithm holds a response back until the write before it
/// is acknowledged, so the first reply on every connection arrives a full round
/// trip late. Beside the server that is invisible; a long way from it - behind
/// a VPN, say - it is most of a second, on a connection the page opens for
/// every poll. Serving through hyper directly means nothing sets this for us,
/// which is why it was never set at all.
fn disable_nagle(stream: &TcpStream, label: &str) {
    if let Err(error) = stream.set_nodelay(true) {
        debug!(error = ?error, label, "could not turn off Nagle for this connection");
    }
}

/// An error about the connection being dequeued, not about the listener. The
/// client is simply gone, and the next accept is the right answer.
fn is_connection_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
    )
}

async fn serve_connection<S>(stream: S, peer_addr: SocketAddr, app: Router, label: &'static str)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let service = service_fn(move |mut request: hyper::Request<Incoming>| {
        let app = app.clone();
        request.extensions_mut().insert(peer_addr);
        async move {
            match timeout(
                Duration::from_secs(REQUEST_TIMEOUT_SECS),
                app.oneshot(request),
            )
            .await
            {
                Ok(response) => response,
                Err(_) => {
                    warn!(label, "request timed out");
                    Ok(timeout_response())
                }
            }
        }
    });
    let io = TokioIo::new(stream);
    let mut builder = http1::Builder::new();
    builder.timer(TokioTimer::new());
    // Keeping the connection open lets a page load its assets and poll for
    // snapshots without a new TCP and TLS handshake per request. The idle
    // timeout closes it once the client goes quiet between requests.
    builder.header_read_timeout(Duration::from_secs(IDLE_TIMEOUT_SECS));
    let connection = builder.serve_connection(io, service);
    tokio::pin!(connection);

    let lifetime = tokio::time::sleep(Duration::from_secs(CONNECTION_LIFETIME_SECS));
    tokio::pin!(lifetime);
    let mut closing = false;

    // The lifetime cap used to drop the connection outright, which cut whatever
    // was on it: the reader got a truncated response for no reason of their
    // own. Now it stops the connection taking new requests and lets the one in
    // flight finish, with the outer timeout still there to bound a client that
    // will not let go.
    let outcome = timeout(
        Duration::from_secs(CONNECTION_LIFETIME_SECS + LIFETIME_GRACE_SECS),
        async {
            loop {
                tokio::select! {
                    result = connection.as_mut() => break result,
                    () = &mut lifetime, if !closing => {}
                }
                closing = true;
                connection.as_mut().graceful_shutdown();
            }
        },
    )
    .await;

    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => debug!(label, error = ?error, "connection closed with an error"),
        Err(_) => debug!(label, "connection reached its lifetime limit"),
    }
}

fn timeout_response() -> Response {
    (
        StatusCode::GATEWAY_TIMEOUT,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        r#"{"error":"request timed out","code":"timeout"}"#,
    )
        .into_response()
}
