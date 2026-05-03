use super::routes::{app_router, challenge_redirect_router};
use super::security::{normalize_host, redirect_location, request_host_allowed};
use crate::config::{AcmeConfig, AppConfig, ChainConfig, SecurityConfig, TlsConfig};
use crate::state::AppState;
use crate::tls;
use axum::body::{Body, to_bytes};
use axum::http::header;
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

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
fn tls_public_url_can_match_extra_acme_identifier() {
    let mut config = test_config(Vec::new());
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://www.example.com".to_owned(),
        acme: AcmeConfig {
            enabled: true,
            identifier: "example.com".to_owned(),
            extra_identifiers: vec!["www.example.com".to_owned()],
            ..AcmeConfig::default()
        },
        ..TlsConfig::default()
    };

    assert!(config.validate().is_ok());
}

#[test]
fn tls_public_url_must_match_one_acme_identifier() {
    let mut config = test_config(Vec::new());
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://other.example.com".to_owned(),
        acme: AcmeConfig {
            enabled: true,
            identifier: "example.com".to_owned(),
            extra_identifiers: vec!["www.example.com".to_owned()],
            ..AcmeConfig::default()
        },
        ..TlsConfig::default()
    };

    assert!(config.validate().is_err());
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
    let body = response_json(response).await;
    assert_eq!(body["error"], "bad host");
    assert_eq!(body["code"], "bad_host");
}

#[tokio::test]
async fn app_router_reports_unknown_chain() {
    let state = Arc::new(AppState::new(Arc::new(test_config(Vec::new()))));
    let response = app_router(state)
        .oneshot(
            Request::builder()
                .uri("/api/chains/missing/clock")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response_json(response).await;
    assert_eq!(body["error"], "unknown chain id `missing`");
    assert_eq!(body["code"], "unknown_chain");
}

#[tokio::test]
async fn app_router_reports_not_found_code() {
    let state = Arc::new(AppState::new(Arc::new(test_config(Vec::new()))));
    let response = app_router(state)
        .oneshot(
            Request::builder()
                .uri("/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response_json(response).await;
    assert_eq!(body["error"], "not found");
    assert_eq!(body["code"], "not_found");
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

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
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
