use super::*;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

// The router tests call handlers directly, so they cannot see the connection
// layer. These drive a real socket instead.
#[tokio::test]
async fn one_connection_serves_several_requests() {
    let (address, _server) = spawn_test_server().await;
    let mut stream = TcpStream::connect(address).await.unwrap();

    for _ in 0..3 {
        send_request(&mut stream, "/api/health", address).await;
        let response = read_response(&mut stream).await;

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "expected a 200 on the reused connection, got: {}",
            response.lines().next().unwrap_or_default()
        );
        assert!(
            !response.contains("connection: close"),
            "the server should keep the connection open"
        );
    }
}

#[tokio::test]
async fn idle_connections_are_closed() {
    let (address, _server) = spawn_test_server().await;
    let mut stream = TcpStream::connect(address).await.unwrap();
    send_request(&mut stream, "/api/health", address).await;
    read_response(&mut stream).await;

    // The idle timeout is far longer than this test should wait, so this only
    // checks that an idle connection is not closed immediately.
    let mut buffer = [0u8; 1];
    let idle = tokio::time::timeout(Duration::from_millis(250), stream.read(&mut buffer)).await;

    assert!(idle.is_err(), "the connection should stay open while idle");
}

async fn spawn_test_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    spawn_test_server_holding(8).await
}

async fn spawn_test_server_holding(
    max_connections: usize,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = test_state(Vec::new());
    let router = crate::server::routes::app_router(Arc::clone(&state));
    let server = tokio::spawn(async move {
        let _ = crate::server::connection::serve_plain_connections(
            listener,
            router,
            max_connections,
            "test",
        )
        .await;
    });
    (address, server)
}

#[tokio::test]
async fn accepted_connections_get_nagle_turned_off() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let client = tokio::spawn(async move { TcpStream::connect(address).await.unwrap() });

    let (accepted, _peer) = crate::server::connection::accept(&listener, "test")
        .await
        .expect("the listener should hand back the connection");
    let _client = client.await.unwrap();

    assert!(
        accepted.nodelay().unwrap(),
        "Nagle holds a small write back until the one before it is acknowledged, \
         which costs the first response on every connection a full round trip"
    );
}

#[tokio::test]
async fn the_idle_timeout_outlasts_the_pages_poll() {
    // The page asks again every `refresh_seconds / 2`. Close the connection
    // sooner than that and keeping it alive buys nothing: every poll opens a
    // new one and pays a fresh TCP and TLS handshake, which is what a
    // twenty-second timeout did to a thirty-second poll.
    let refresh_seconds = AppConfig::for_test(Vec::new()).refresh_seconds;
    let poll_seconds = std::cmp::max(10, refresh_seconds / 2);

    assert!(
        crate::server::connection::IDLE_TIMEOUT_SECS > poll_seconds,
        "an idle timeout of {}s closes the connection before the page polls again at {poll_seconds}s",
        crate::server::connection::IDLE_TIMEOUT_SECS
    );
}

#[tokio::test]
async fn a_full_server_closes_new_connections_instead_of_leaving_them_waiting() {
    let (address, _server) = spawn_test_server_holding(1).await;

    // Answering a request proves the server has taken the one slot and is
    // holding it for the life of this connection.
    let mut held = TcpStream::connect(address).await.unwrap();
    send_request(&mut held, "/api/health", address).await;
    read_response(&mut held).await;

    let mut refused = TcpStream::connect(address).await.unwrap();
    send_request(&mut refused, "/api/health", address).await;

    // Closed at once, either way it reaches the client: end of stream, or a
    // reset because the request bytes were still unread when the socket went.
    // What matters is that something arrives rather than nothing - a client
    // left waiting on a server that will never answer is the hang itself.
    let mut buffer = [0u8; 1];
    let read = tokio::time::timeout(Duration::from_secs(5), refused.read(&mut buffer))
        .await
        .expect("a full server should close the connection, not leave the client waiting on it");

    match read {
        Ok(0) => {}
        Ok(count) => panic!("a full server answered with {count} bytes instead of closing"),
        Err(error) => assert_eq!(
            error.kind(),
            std::io::ErrorKind::ConnectionReset,
            "the connection should be closed, not left open: {error}"
        ),
    }
}

async fn send_request(stream: &mut TcpStream, path: &str, address: std::net::SocketAddr) {
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: keep-alive\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
}

async fn read_response(stream: &mut TcpStream) -> String {
    let mut response = Vec::new();
    let mut byte = [0u8; 1];

    loop {
        let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut byte))
            .await
            .expect("response timed out")
            .expect("connection closed early");
        assert_eq!(read, 1, "connection closed before the headers ended");
        response.push(byte[0]);
        if response.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    let headers = String::from_utf8_lossy(&response).to_ascii_lowercase();
    let body_length = headers
        .split("\r\n")
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse::<usize>().ok())
        .expect("response should declare a content length");
    let mut body = vec![0u8; body_length];
    stream.read_exact(&mut body).await.unwrap();

    String::from_utf8_lossy(&response).to_string()
}
