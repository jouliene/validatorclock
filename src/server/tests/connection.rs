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

    // The header read timeout is 20s, far longer than this test should wait, so
    // this only checks that an idle connection is not closed immediately.
    let mut buffer = [0u8; 1];
    let idle = tokio::time::timeout(Duration::from_millis(250), stream.read(&mut buffer)).await;

    assert!(idle.is_err(), "the connection should stay open while idle");
}

async fn spawn_test_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = test_state(Vec::new());
    let router = crate::server::routes::app_router(Arc::clone(&state));
    let server = tokio::spawn(async move {
        let _ =
            crate::server::connection::serve_plain_connections(listener, router, 8, "test").await;
    });
    (address, server)
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
