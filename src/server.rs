use crate::chain::{chains_response, get_chain_snapshot};
use crate::config::AppConfig;
use crate::state::AppState;
use crate::tls;
use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, Semaphore};
use tokio::time::{Duration, timeout};
use tokio_rustls::TlsAcceptor;

const INDEX_HTML: &str = include_str!("../public/index.html");
const STYLES_CSS: &str = include_str!("../public/styles.css");
const APP_JS: &str = include_str!("../public/app.js");
const REQUEST_HEADER_LIMIT: usize = 8192;
const REQUEST_TIMEOUT_SECS: u64 = 10;
const ACME_CHALLENGE_PREFIX: &str = "/.well-known/acme-challenge/";

pub(crate) async fn run_plain_http_server(state: Arc<AppState>) -> Result<()> {
    let listener = TcpListener::bind(&state.config.listen)
        .await
        .with_context(|| format!("failed to bind {}", state.config.listen))?;
    println!(
        "validators_clock listening on http://{}",
        state.config.listen
    );

    accept_plain_connections(listener, state, ConnectionMode::PlainApp).await
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
    tokio::spawn(async move {
        if let Err(error) =
            accept_plain_connections(http_listener, http_state, ConnectionMode::ChallengeRedirect)
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
    accept_tls_connections(https_listener, state, acceptor).await
}

#[derive(Clone, Copy)]
enum ConnectionMode {
    PlainApp,
    ChallengeRedirect,
}

async fn accept_plain_connections(
    listener: TcpListener,
    state: Arc<AppState>,
    mode: ConnectionMode,
) -> Result<()> {
    let permits = Arc::new(Semaphore::new(state.config.security.max_connections));

    loop {
        let (stream, _) = listener.accept().await?;
        let state = Arc::clone(&state);
        let permit = Arc::clone(&permits).acquire_owned().await?;
        tokio::spawn(async move {
            let _permit = permit;
            let result = match mode {
                ConnectionMode::PlainApp => handle_connection(stream, state).await,
                ConnectionMode::ChallengeRedirect => {
                    handle_http_challenge_or_redirect(stream, state).await
                }
            };
            if let Err(error) = result {
                eprintln!("request failed: {error:#}");
            }
        });
    }
}

async fn accept_tls_connections(
    listener: TcpListener,
    state: Arc<AppState>,
    acceptor: Arc<RwLock<TlsAcceptor>>,
) -> Result<()> {
    let permits = Arc::new(Semaphore::new(state.config.security.max_connections));

    loop {
        let (stream, _) = listener.accept().await?;
        let state = Arc::clone(&state);
        let acceptor = acceptor.read().await.clone();
        let permit = Arc::clone(&permits).acquire_owned().await?;
        tokio::spawn(async move {
            let _permit = permit;
            match acceptor.accept(stream).await {
                Ok(tls_stream) => {
                    if let Err(error) = handle_connection(tls_stream, state).await {
                        eprintln!("HTTPS request failed: {error:#}");
                    }
                }
                Err(error) => eprintln!("TLS handshake failed: {error:#}"),
            }
        });
    }
}

async fn handle_connection<S>(mut stream: S, state: Arc<AppState>) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let request = timeout(
        Duration::from_secs(REQUEST_TIMEOUT_SECS),
        read_request(&mut stream),
    )
    .await
    .context("request timed out")??;
    let response = route_request(&request, state).await;
    write_response(&mut stream, response).await?;
    Ok(())
}

async fn handle_http_challenge_or_redirect(
    mut stream: TcpStream,
    state: Arc<AppState>,
) -> Result<()> {
    let request = timeout(
        Duration::from_secs(REQUEST_TIMEOUT_SECS),
        read_request(&mut stream),
    )
    .await
    .context("request timed out")??;

    let response = route_http_challenge_or_redirect(&request, &state).await;
    write_response(&mut stream, response).await?;
    Ok(())
}

async fn read_request<S>(stream: &mut S) -> Result<HttpRequest>
where
    S: AsyncRead + Unpin,
{
    let mut buffer = vec![0_u8; REQUEST_HEADER_LIMIT];
    let mut read = 0;
    let mut headers_complete = false;

    loop {
        let n = stream.read(&mut buffer[read..]).await?;
        if n == 0 {
            break;
        }
        read += n;

        headers_complete = buffer[..read]
            .windows(4)
            .any(|window| window == b"\r\n\r\n");
        if headers_complete || read == buffer.len() {
            break;
        }
    }

    if !headers_complete {
        bail!("request headers too large or incomplete");
    }

    let text = std::str::from_utf8(&buffer[..read]).context("request is not valid UTF-8")?;
    let mut lines = text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| anyhow!("missing request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| anyhow!("missing request method"))?
        .to_owned();
    let target = parts
        .next()
        .ok_or_else(|| anyhow!("missing request target"))?
        .to_owned();

    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() {
            break;
        }

        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }

    Ok(HttpRequest {
        method,
        target,
        headers,
    })
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    target: String,
    headers: HashMap<String, String>,
}

impl HttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

async fn route_request(request: &HttpRequest, state: Arc<AppState>) -> HttpResponse {
    if !request_host_allowed(request, &state.config) {
        return security_headers(json_error(400, "bad host"));
    }

    if request.method == "OPTIONS" {
        return security_headers(HttpResponse::empty(204));
    }

    if request.method != "GET" && request.method != "HEAD" {
        return security_headers(json_error(405, "method not allowed"));
    }

    let (path, query) = split_target(&request.target);
    let force_refresh = state.config.security.allow_force_refresh
        && query.map(query_forces_refresh).unwrap_or(false);
    let response = match path {
        "/" | "/index.html" => HttpResponse::ok("text/html; charset=utf-8", INDEX_HTML.as_bytes()),
        "/styles.css" => HttpResponse::ok("text/css; charset=utf-8", STYLES_CSS.as_bytes()),
        "/app.js" => HttpResponse::ok("application/javascript; charset=utf-8", APP_JS.as_bytes()),
        "/api/chains" => json_response(&chains_response(&state.config)),
        "/api/health" => json_response(&serde_json::json!({ "status": "ok" })),
        _ => match chain_clock_id(path) {
            Some(chain_id) => match get_chain_snapshot(&state, chain_id, force_refresh).await {
                Ok(snapshot) => json_response(&snapshot),
                Err(error) => {
                    eprintln!("snapshot request failed for {chain_id}: {error:#}");
                    json_error(500, "failed to fetch chain snapshot")
                }
            },
            None => json_error(404, "not found"),
        },
    };

    let response = if request.method == "HEAD" {
        response.without_body()
    } else {
        response
    };

    security_headers(response)
}

async fn route_http_challenge_or_redirect(request: &HttpRequest, state: &AppState) -> HttpResponse {
    if request.method != "GET" && request.method != "HEAD" {
        return security_headers(json_error(405, "method not allowed"));
    }
    let is_head = request.method == "HEAD";

    if let Some(token) = challenge_token(&request.target)
        && let Some(value) = state.acme_challenges.read().await.get(token)
    {
        let response = HttpResponse::ok("text/plain; charset=utf-8", value.as_bytes());
        return security_headers(if is_head {
            response.without_body()
        } else {
            response
        });
    }

    if !request_host_allowed(request, &state.config) {
        return security_headers(json_error(400, "bad host"));
    }

    let location = redirect_location(&state.config.tls.public_url, &request.target);
    let response = HttpResponse::empty(308).with_header("Location", location);
    security_headers(if is_head {
        response.without_body()
    } else {
        response
    })
}

fn chain_clock_id(path: &str) -> Option<&str> {
    let prefix = "/api/chains/";
    let suffix = "/clock";
    path.strip_prefix(prefix)?.strip_suffix(suffix)
}

fn challenge_token(target: &str) -> Option<&str> {
    let (path, _) = split_target(target);
    let token = path.strip_prefix(ACME_CHALLENGE_PREFIX)?;
    (!token.is_empty() && !token.contains('/')).then_some(token)
}

fn split_target(target: &str) -> (&str, Option<&str>) {
    target
        .split_once('?')
        .map_or((target, None), |(path, query)| (path, Some(query)))
}

fn redirect_location(public_url: &str, target: &str) -> String {
    format!("{}{}", public_url.trim_end_matches('/'), target)
}

pub(crate) fn public_url_host(public_url: &str) -> Option<String> {
    let rest = public_url.strip_prefix("https://")?;
    let host = rest.split('/').next().unwrap_or(rest);
    normalize_host(host)
}

pub(crate) fn normalize_host(host: &str) -> Option<String> {
    let host = host.trim();
    if host.is_empty() {
        return None;
    }

    if host.starts_with('[')
        && let Some(end) = host.find(']')
    {
        let value = &host[1..end];
        return Some(
            value
                .parse::<IpAddr>()
                .map(|address| address.to_string())
                .unwrap_or_else(|_| value.to_ascii_lowercase()),
        );
    }

    if let Ok(address) = host.parse::<IpAddr>() {
        return Some(address.to_string());
    }

    let host_without_port = host
        .rsplit_once(':')
        .filter(|(name, port)| !name.contains(':') && port.chars().all(|ch| ch.is_ascii_digit()))
        .map(|(name, _)| name)
        .unwrap_or(host);

    Some(host_without_port.trim_end_matches('.').to_ascii_lowercase())
}

fn request_host_allowed(request: &HttpRequest, config: &AppConfig) -> bool {
    let allowed_hosts = config.effective_allowed_hosts();
    if allowed_hosts.is_empty() {
        return true;
    }

    let Some(host) = request.header("host").and_then(normalize_host) else {
        return false;
    };

    allowed_hosts
        .iter()
        .filter_map(|host| normalize_host(host))
        .any(|allowed| allowed == host)
}

fn query_forces_refresh(query: &str) -> bool {
    query.split('&').any(|part| {
        let (key, value) = part.split_once('=').unwrap_or((part, "1"));
        key == "refresh" && matches!(value, "1" | "true" | "yes")
    })
}

fn json_response<T: Serialize>(value: &T) -> HttpResponse {
    match serde_json::to_vec(value) {
        Ok(body) => HttpResponse::owned(200, "application/json; charset=utf-8", body),
        Err(error) => json_error(500, &error.to_string()),
    }
}

fn json_error(status: u16, message: &str) -> HttpResponse {
    let body = serde_json::json!({ "error": message });
    HttpResponse::owned(
        status,
        "application/json; charset=utf-8",
        serde_json::to_vec(&body).unwrap_or_else(|_| b"{\"error\":\"internal error\"}".to_vec()),
    )
}

struct HttpResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    headers: Vec<(&'static str, String)>,
}

impl HttpResponse {
    fn ok(content_type: &'static str, body: &[u8]) -> Self {
        Self::owned(200, content_type, body.to_vec())
    }

    fn owned(status: u16, content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type,
            body,
            headers: Vec::new(),
        }
    }

    fn empty(status: u16) -> Self {
        Self::owned(status, "text/plain; charset=utf-8", Vec::new())
    }

    fn without_body(mut self) -> Self {
        self.body.clear();
        self
    }

    fn with_header(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }
}

fn security_headers(response: HttpResponse) -> HttpResponse {
    response
        .with_header("X-Content-Type-Options", "nosniff")
        .with_header("X-Frame-Options", "DENY")
        .with_header("Referrer-Policy", "no-referrer")
        .with_header(
            "Content-Security-Policy",
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'",
        )
        .with_header("Strict-Transport-Security", "max-age=31536000")
}

async fn write_response<S>(stream: &mut S, response: HttpResponse) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    let reason = reason_phrase(response.status);
    let mut headers = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len()
    );

    for (name, value) in response.headers {
        headers.push_str(name);
        headers.push_str(": ");
        headers.push_str(&value);
        headers.push_str("\r\n");
    }
    headers.push_str("\r\n");

    stream.write_all(headers.as_bytes()).await?;
    stream.write_all(&response.body).await?;
    stream.shutdown().await?;
    Ok(())
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ChainConfig, SecurityConfig, TlsConfig};
    use std::path::PathBuf;

    #[test]
    fn normalizes_host_header_values() {
        assert_eq!(
            normalize_host("104.238.222.200:443").as_deref(),
            Some("104.238.222.200")
        );
        assert_eq!(
            normalize_host("Example.COM.").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            normalize_host("[2001:db8::1]:443").as_deref(),
            Some("2001:db8::1")
        );
        assert_eq!(
            normalize_host("2001:db8::1").as_deref(),
            Some("2001:db8::1")
        );
        assert_eq!(normalize_host(" ").as_deref(), None);
    }

    #[test]
    fn extracts_challenge_tokens_from_request_target() {
        assert_eq!(
            challenge_token("/.well-known/acme-challenge/token123?unused=1"),
            Some("token123")
        );
        assert_eq!(challenge_token("/.well-known/acme-challenge/a/b"), None);
        assert_eq!(challenge_token("/api/health"), None);
    }

    #[test]
    fn rejects_acme_identifier_with_port() {
        assert!(tls::acme_identifier("104.238.222.200").is_ok());
        assert!(tls::acme_identifier("example.com").is_ok());
        assert!(tls::acme_identifier("example.com:443").is_err());
        assert!(tls::acme_identifier("https://example.com").is_err());
        assert!(tls::acme_identifier("[2001:db8::1]").is_err());
    }

    #[test]
    fn checks_allowed_hosts_with_ports() {
        let config = AppConfig {
            listen: "127.0.0.1:8787".to_owned(),
            refresh_seconds: 60,
            cache_path: PathBuf::from("cache.json"),
            security: SecurityConfig {
                allowed_hosts: vec!["104.238.222.200".to_owned()],
                ..SecurityConfig::default()
            },
            tls: TlsConfig::default(),
            chains: vec![ChainConfig {
                id: "test".to_owned(),
                name: "Test".to_owned(),
                rpc: "https://example.com".to_owned(),
                color: "#38bdf8".to_owned(),
                token_symbol: "TEST".to_owned(),
                rpc_label: None,
            }],
        };

        let allowed = HttpRequest {
            method: "GET".to_owned(),
            target: "/".to_owned(),
            headers: HashMap::from([("host".to_owned(), "104.238.222.200:443".to_owned())]),
        };
        let rejected = HttpRequest {
            method: "GET".to_owned(),
            target: "/".to_owned(),
            headers: HashMap::from([("host".to_owned(), "example.com".to_owned())]),
        };

        assert!(request_host_allowed(&allowed, &config));
        assert!(!request_host_allowed(&rejected, &config));
    }
}
