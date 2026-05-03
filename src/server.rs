use crate::chain::{chains_response, get_chain_snapshot};
use crate::config::AppConfig;
use crate::state::AppState;
use crate::tls;
use anyhow::{Context, Result, bail};
use axum::extract::{Path, Query, Request, State};
use axum::http::header::{self, HeaderName, HeaderValue};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use serde_json::json;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::{Duration, timeout};
use tokio_rustls::TlsAcceptor;
use tower::{ServiceBuilder, ServiceExt};

const INDEX_HTML: &str = include_str!("../public/index.html");
const STYLES_CSS: &str = include_str!("../public/styles.css");
const APP_JS: &str = include_str!("../public/app.js");
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
        app_router(Arc::clone(&state)),
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
            challenge_redirect_router(http_state),
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
        app_router(Arc::clone(&state)),
        acceptor,
        state.config.security.max_connections,
    )
    .await
}

fn app_router(state: Arc<AppState>) -> Router {
    let layers = ServiceBuilder::new()
        .layer(middleware::from_fn(add_security_headers))
        .layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            enforce_allowed_host,
        ))
        .layer(middleware::from_fn(handle_options));

    Router::new()
        .route("/", get(index))
        .route("/index.html", get(index))
        .route("/styles.css", get(styles))
        .route("/app.js", get(app_js))
        .route("/api/health", get(health))
        .route("/api/chains", get(list_chains))
        .route("/api/chains/{chain_id}/clock", get(chain_clock))
        .fallback(not_found)
        .with_state(state)
        .layer(layers)
}

fn challenge_redirect_router(state: Arc<AppState>) -> Router {
    let layers = ServiceBuilder::new()
        .layer(middleware::from_fn(add_security_headers))
        .layer(middleware::from_fn(handle_options));

    Router::new()
        .route("/.well-known/acme-challenge/{token}", get(acme_challenge))
        .fallback(redirect_to_https)
        .with_state(state)
        .layer(layers)
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

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn styles() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/css; charset=utf-8"),
        )],
        STYLES_CSS,
    )
}

async fn app_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/javascript; charset=utf-8"),
        )],
        APP_JS,
    )
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn list_chains(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(chains_response(&state.config))
}

async fn chain_clock(
    State(state): State<Arc<AppState>>,
    Path(chain_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let force_refresh = state.config.security.allow_force_refresh && query_forces_refresh(&query);
    match get_chain_snapshot(&state, &chain_id, force_refresh).await {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => {
            eprintln!("snapshot request failed for {chain_id}: {error:#}");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to fetch chain snapshot",
            )
        }
    }
}

async fn acme_challenge(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(value) = state.acme_challenges.read().await.get(&token).cloned() {
        return (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            )],
            value,
        )
            .into_response();
    }

    redirect_response(&state, &headers, &uri)
}

async fn redirect_to_https(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    redirect_response(&state, &headers, &uri)
}

async fn not_found() -> Response {
    json_error(StatusCode::NOT_FOUND, "not found")
}

async fn handle_options(request: Request, next: Next) -> Response {
    if request.method() == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    }
}

async fn enforce_allowed_host(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    if !request_host_allowed(request.headers(), &state.config) {
        return json_error(StatusCode::BAD_REQUEST, "bad host");
    }

    next.run(request).await
}

async fn add_security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    add_common_headers(response.headers_mut());
    response
}

fn redirect_response(state: &AppState, headers: &HeaderMap, uri: &Uri) -> Response {
    if !request_host_allowed(headers, &state.config) {
        return json_error(StatusCode::BAD_REQUEST, "bad host");
    }

    let location = redirect_location(
        &state.config.tls.public_url,
        uri.path_and_query()
            .map(|value| value.as_str())
            .unwrap_or("/"),
    );
    let Ok(location) = HeaderValue::from_str(&location) else {
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid redirect location",
        );
    };

    (
        StatusCode::PERMANENT_REDIRECT,
        [(header::LOCATION, location)],
    )
        .into_response()
}

fn add_common_headers(headers: &mut HeaderMap) {
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000"),
    );
}

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
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

fn request_host_allowed(headers: &HeaderMap, config: &AppConfig) -> bool {
    let allowed_hosts = config.effective_allowed_hosts();
    if allowed_hosts.is_empty() {
        return true;
    }

    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(normalize_host)
    else {
        return false;
    };

    allowed_hosts
        .iter()
        .filter_map(|host| normalize_host(host))
        .any(|allowed| allowed == host)
}

fn query_forces_refresh(query: &HashMap<String, String>) -> bool {
    query
        .get("refresh")
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ChainConfig, SecurityConfig, TlsConfig};
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
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
    fn builds_redirect_location_with_path_and_query() {
        assert_eq!(
            redirect_location("https://104.238.222.200/", "/api/health?x=1"),
            "https://104.238.222.200/api/health?x=1"
        );
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
        let config = test_config(vec!["104.238.222.200".to_owned()]);

        let mut allowed = HeaderMap::new();
        allowed.insert(
            header::HOST,
            HeaderValue::from_static("104.238.222.200:443"),
        );
        let mut rejected = HeaderMap::new();
        rejected.insert(header::HOST, HeaderValue::from_static("example.com"));

        assert!(request_host_allowed(&allowed, &config));
        assert!(!request_host_allowed(&rejected, &config));
    }

    #[tokio::test]
    async fn app_router_serves_health_with_security_headers() {
        let state = Arc::new(AppState::new(Arc::new(test_config(Vec::new()))));
        let response = app_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .and_then(|value| value.to_str().ok()),
            Some("nosniff")
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], br#"{"status":"ok"}"#);
    }

    #[tokio::test]
    async fn app_router_rejects_bad_host() {
        let state = Arc::new(AppState::new(Arc::new(test_config(vec![
            "allowed.example".to_owned(),
        ]))));
        let response = app_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .header(header::HOST, "blocked.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], br#"{"error":"bad host"}"#);
    }

    #[tokio::test]
    async fn challenge_route_is_available_before_host_check() {
        let state = Arc::new(AppState::new(Arc::new(test_config(vec![
            "allowed.example".to_owned(),
        ]))));
        state
            .acme_challenges
            .write()
            .await
            .insert("token123".to_owned(), "challenge-value".to_owned());

        let response = challenge_redirect_router(state)
            .oneshot(
                Request::builder()
                    .uri("/.well-known/acme-challenge/token123")
                    .header(header::HOST, "blocked.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"challenge-value");
    }

    fn test_config(allowed_hosts: Vec<String>) -> AppConfig {
        AppConfig {
            listen: "127.0.0.1:8787".to_owned(),
            refresh_seconds: 60,
            cache_path: PathBuf::from("cache.json"),
            security: SecurityConfig {
                allowed_hosts,
                ..SecurityConfig::default()
            },
            tls: TlsConfig {
                public_url: "https://allowed.example".to_owned(),
                ..TlsConfig::default()
            },
            chains: vec![ChainConfig {
                id: "test".to_owned(),
                name: "Test".to_owned(),
                rpc: "https://example.com".to_owned(),
                color: "#38bdf8".to_owned(),
                token_symbol: "TEST".to_owned(),
                rpc_label: None,
            }],
        }
    }
}
