use super::assets::asset_version;
use super::routes::{app_router, challenge_redirect_router};
use super::security::{normalize_host, redirect_location, request_host_allowed};
use crate::chain::test_clock_snapshot;
use crate::config::{AcmeConfig, AppConfig, ChainConfig, SecurityConfig, TlsConfig};
use crate::state::AppState;
use crate::tls;
use axum::body::{Body, to_bytes};
use axum::http::header;
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tower::ServiceExt;

#[test]
fn normalizes_host_header_values() {
    assert_eq!(
        normalize_host("203.0.113.10:443").as_deref(),
        Some("203.0.113.10")
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
        redirect_location("https://203.0.113.10/", "/api/health?x=1"),
        "https://203.0.113.10/api/health?x=1"
    );
}

#[test]
fn rejects_acme_identifier_with_port() {
    assert!(tls::acme_identifier("203.0.113.10").is_ok());
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
fn acme_profile_is_optional_for_domain_certificates() {
    let mut config = test_config(Vec::new());
    config.tls = TlsConfig {
        enabled: true,
        public_url: "https://example.com".to_owned(),
        acme: AcmeConfig {
            enabled: true,
            identifier: "example.com".to_owned(),
            ..AcmeConfig::default()
        },
        ..TlsConfig::default()
    };

    assert!(config.validate().is_ok());
    assert_eq!(config.tls.acme.profile_value(), None);
    assert_eq!(config.tls.acme.renew_before_seconds(), 30 * 24 * 60 * 60);
}

#[test]
fn shortlived_profile_uses_short_default_renewal_window() {
    let acme = AcmeConfig {
        profile: Some("shortlived".to_owned()),
        ..AcmeConfig::default()
    };

    assert_eq!(acme.renew_before_seconds(), 2 * 24 * 60 * 60);
}

#[test]
fn refresh_timeout_must_be_positive() {
    let mut config = test_config(Vec::new());
    config.refresh_timeout_seconds = 0;

    assert!(config.validate().is_err());
}

#[test]
fn checks_allowed_hosts_with_ports() {
    let config = test_config(vec!["203.0.113.10".to_owned()]);

    let mut allowed = HeaderMap::new();
    allowed.insert(header::HOST, HeaderValue::from_static("203.0.113.10:443"));
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
    assert_header_starts_with(response.headers(), header::CONTENT_TYPE, "application/json");
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
async fn app_router_versions_and_caches_static_assets() {
    let state = Arc::new(AppState::new(Arc::new(test_config(Vec::new()))));
    let response = app_router(Arc::clone(&state))
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    let asset_version = asset_version();
    assert!(body.contains(&format!("/styles.css?v={asset_version}")));
    assert!(body.contains(&format!("/app.js?v={asset_version}")));

    let response = app_router(Arc::clone(&state))
        .oneshot(
            Request::builder()
                .uri(format!("/styles.css?v={asset_version}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(
        response.headers(),
        header::CONTENT_TYPE,
        "text/css; charset=utf-8",
    );
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("public, max-age=31536000, immutable")
    );

    let response = app_router(Arc::clone(&state))
        .oneshot(
            Request::builder()
                .uri(format!("/app.js?v={asset_version}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(
        response.headers(),
        header::CONTENT_TYPE,
        "application/javascript; charset=utf-8",
    );
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("public, max-age=31536000, immutable")
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("const state ="));
    assert!(body.contains("function drawClock"));
    assert!(body.contains("function renderValidators"));
    assert!(body.contains("boot();"));

    let response = app_router(Arc::clone(&state))
        .oneshot(
            Request::builder()
                .uri(format!("/brands/everscale.svg?v={asset_version}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(
        response.headers(),
        header::CONTENT_TYPE,
        "image/svg+xml; charset=utf-8",
    );
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("public, max-age=31536000, immutable")
    );

    let response = app_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/brands/ton.svg?v={asset_version}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(
        response.headers(),
        header::CONTENT_TYPE,
        "image/svg+xml; charset=utf-8",
    );
}

#[tokio::test]
async fn app_router_lists_configured_chains() {
    let state = Arc::new(AppState::new(Arc::new(test_config(Vec::new()))));
    let response = app_router(state)
        .oneshot(
            Request::builder()
                .uri("/api/chains")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(response.headers(), header::CONTENT_TYPE, "application/json");
    let body = response_json(response).await;
    assert_eq!(body["refresh_seconds"], 60);
    assert_eq!(body["chains"][0]["id"], "test");
    assert_eq!(body["chains"][0]["name"], "Test");
    assert_eq!(body["chains"][0]["color"], "#38bdf8");
    assert_eq!(body["chains"][0]["token_symbol"], "TEST");
    assert_eq!(body["chains"][0]["rpc_label"], "example.com");
}

#[tokio::test]
async fn app_router_serves_runtime_status() {
    let state = Arc::new(AppState::new(Arc::new(test_config(Vec::new()))));
    state
        .record_refresh_failure("test", 123, "rpc down".to_owned())
        .await;

    let response = app_router(state)
        .oneshot(
            Request::builder()
                .uri("/api/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(response.headers(), header::CONTENT_TYPE, "application/json");
    let body = response_json(response).await;
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert!(body["started_at"].is_number());
    assert!(body["uptime_seconds"].is_number());
    assert_eq!(body["refresh_seconds"], 60);
    assert_eq!(body["refresh_timeout_seconds"], 90);
    assert_eq!(body["chains"][0]["id"], "test");
    assert_eq!(body["chains"][0]["name"], "Test");
    assert_eq!(body["chains"][0]["cached"], false);
    assert_eq!(body["chains"][0]["fetched_at"], Value::Null);
    assert_eq!(body["chains"][0]["age_seconds"], Value::Null);
    assert_eq!(body["chains"][0]["stale"], true);
    assert_eq!(body["chains"][0]["last_attempt_at"], 123);
    assert_eq!(body["chains"][0]["last_success_at"], Value::Null);
    assert_eq!(body["chains"][0]["last_error"], "rpc down");
}

#[tokio::test]
async fn app_router_serves_cached_clock_shape() {
    let state = Arc::new(AppState::new(Arc::new(test_config(Vec::new()))));
    let snapshot = test_clock_snapshot("test");
    state
        .store_cached_snapshot("test", now_sec_for_test(), snapshot)
        .await;

    let response = app_router(state)
        .oneshot(
            Request::builder()
                .uri("/api/chains/test/clock")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(response.headers(), header::CONTENT_TYPE, "application/json");
    let body = response_json(response).await;
    assert_eq!(body["chain"]["id"], "test");
    assert_eq!(body["chain"]["name"], "Test");
    assert_eq!(body["fetched_at"], 123);
    assert_eq!(body["global_id"], 42);
    assert_eq!(body["seqno"], 7);
    assert_eq!(body["params15"]["validators_elected_for"], 65536);
    assert_eq!(body["current_set"]["round_id"], 10);
    assert_eq!(body["current_set"]["round_color"], "blue");
    assert_eq!(
        body["current_set"]["validators"][0]["public_key"],
        "validator-key"
    );
    assert_eq!(
        body["current_set"]["validators"][0]["history"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        body["current_set"]["validators"][0]["history"][4]["status"],
        "unknown"
    );
    assert_eq!(
        body["current_set"]["recent_absent_validators"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(body["previous_set"], Value::Null);
    assert_eq!(body["next_set"], Value::Null);
    assert_eq!(body["election"]["candidates"].as_array().unwrap().len(), 0);
    assert_eq!(body["warning"], Value::Null);
}

#[tokio::test]
async fn app_router_serves_stale_cached_clock_without_waiting_for_rpc() {
    let state = Arc::new(AppState::new(Arc::new(test_config(Vec::new()))));
    let snapshot = test_clock_snapshot("test");
    state.store_cached_snapshot("test", 1, snapshot).await;

    let response = tokio::time::timeout(
        Duration::from_secs(2),
        app_router(state).oneshot(
            Request::builder()
                .uri("/api/chains/test/clock")
                .body(Body::empty())
                .unwrap(),
        ),
    )
    .await
    .expect("stale cached response should not wait for rpc")
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["chain"]["id"], "test");
    assert_eq!(body["fetched_at"], 123);
    assert!(
        body["warning"]
            .as_str()
            .unwrap()
            .contains("refresh is running in background")
    );
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
        .insert_acme_challenge("token123".to_owned(), "challenge-value".to_owned())
        .await;

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

fn assert_header_starts_with(headers: &HeaderMap, name: header::HeaderName, expected: &str) {
    let value = headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        value.starts_with(expected),
        "expected header to start with `{expected}`, got `{value}`"
    );
}

fn now_sec_for_test() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn test_config(allowed_hosts: Vec<String>) -> AppConfig {
    AppConfig {
        listen: "127.0.0.1:8787".to_owned(),
        refresh_seconds: 60,
        refresh_timeout_seconds: 90,
        cache_path: temp_state_path("cache"),
        history_path: None,
        tycho_map_nodes_path: None,
        map_nodes_paths: HashMap::new(),
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
            rpc_fallbacks: Vec::new(),
            color: "#38bdf8".to_owned(),
            token_symbol: "TEST".to_owned(),
            rpc_label: None,
        }],
    }
}

fn temp_state_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "validators_clock_server_test_{name}_{}_{}.json",
        std::process::id(),
        nonce
    ))
}

async fn cache_tycho_snapshot(state: &AppState, public_keys: &[&str]) {
    cache_snapshot(state, "tycho-testnet", public_keys).await;
}

async fn cache_snapshot(state: &AppState, chain_id: &str, public_keys: &[&str]) {
    let mut snapshot = test_clock_snapshot(chain_id);
    let template = snapshot.current_set.validators[0].clone();
    snapshot.current_set.validators = public_keys
        .iter()
        .map(|public_key| {
            let mut validator = template.clone();
            validator.public_key = (*public_key).to_owned();
            validator
        })
        .collect();
    snapshot.current_set.total = snapshot.current_set.validators.len();
    snapshot.current_set.main = snapshot.current_set.validators.len() as u16;
    state
        .store_cached_snapshot(chain_id, now_sec_for_test(), snapshot)
        .await;
}

#[tokio::test]
async fn app_router_serves_bundled_tycho_map_when_no_file_is_configured() {
    let mut config = test_config(Vec::new());
    config.chains.push(ChainConfig {
        id: "tycho-testnet".to_owned(),
        name: "Tycho".to_owned(),
        rpc: "https://tycho.example.com".to_owned(),
        rpc_fallbacks: Vec::new(),
        color: "#58c9f6".to_owned(),
        token_symbol: "TYCHO".to_owned(),
        rpc_label: None,
    });
    let state = Arc::new(AppState::new(Arc::new(config)));
    cache_tycho_snapshot(
        &state,
        &["1778eb66b9386bcc37031cad14d73e4554413b23d16b4b680726375a622f3a5b"],
    )
    .await;

    let response = app_router(state)
        .oneshot(
            Request::builder()
                .uri("/api/chains/tycho-testnet/map")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(response.headers(), header::CONTENT_TYPE, "application/json");
    let body = response_json(response).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(
        body[0]["peer"],
        "1778eb66b9386bcc37031cad14d73e4554413b23d16b4b680726375a622f3a5b"
    );
    assert!(body[0]["ip"].is_string());
}

#[tokio::test]
async fn app_router_serves_configured_tycho_map_file() {
    let map_path = std::env::temp_dir().join(format!(
        "validators_clock_tycho_map_test_{}_{}.json",
        std::process::id(),
        now_sec_for_test()
    ));
    fs::write(
        &map_path,
        r#"[
            {"peer":"active-validator-public-key","ip":"203.0.113.10","city":"Test City","country":"Testland","isp":"Test ISP","lat":1.25,"lon":2.5},
            {"peer":"inactive-validator-public-key","ip":"203.0.113.11","city":"Other City","country":"Testland","isp":"Test ISP","lat":3.25,"lon":4.5}
        ]"#,
    )
    .unwrap();

    let mut config = test_config(Vec::new());
    config.tycho_map_nodes_path = Some(map_path.clone());
    config.chains.push(ChainConfig {
        id: "tycho-testnet".to_owned(),
        name: "Tycho".to_owned(),
        rpc: "https://tycho.example.com".to_owned(),
        rpc_fallbacks: Vec::new(),
        color: "#58c9f6".to_owned(),
        token_symbol: "TYCHO".to_owned(),
        rpc_label: None,
    });
    let state = Arc::new(AppState::new(Arc::new(config)));
    cache_tycho_snapshot(&state, &["active-validator-public-key"]).await;

    let response = app_router(state)
        .oneshot(
            Request::builder()
                .uri("/api/chains/tycho-testnet/map")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["peer"], "active-validator-public-key");
    assert_eq!(body[0]["ip"], "203.0.113.10");

    let _ = fs::remove_file(map_path);
}

#[tokio::test]
async fn app_router_serves_configured_ton_map_file() {
    let map_path = std::env::temp_dir().join(format!(
        "validators_clock_ton_map_test_{}_{}.json",
        std::process::id(),
        now_sec_for_test()
    ));
    fs::write(
        &map_path,
        r#"[
            {"peer":"active-ton-validator","ip":"203.0.113.20","city":"TON City","country":"TONland","isp":"TON ISP","lat":5.25,"lon":6.5},
            {"peer":"inactive-ton-validator","ip":"203.0.113.21","city":"Other City","country":"TONland","isp":"TON ISP","lat":7.25,"lon":8.5}
        ]"#,
    )
    .unwrap();

    let mut config = test_config(Vec::new());
    config
        .map_nodes_paths
        .insert("ton".to_owned(), map_path.clone());
    config.chains.push(ChainConfig {
        id: "ton".to_owned(),
        name: "TON".to_owned(),
        rpc: "https://ton.example.com".to_owned(),
        rpc_fallbacks: Vec::new(),
        color: "#4DB8FF".to_owned(),
        token_symbol: "TON".to_owned(),
        rpc_label: None,
    });
    let state = Arc::new(AppState::new(Arc::new(config)));
    cache_snapshot(&state, "ton", &["active-ton-validator"]).await;

    let response = app_router(state)
        .oneshot(
            Request::builder()
                .uri("/api/chains/ton/map")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["peer"], "active-ton-validator");
    assert_eq!(body[0]["ip"], "203.0.113.20");

    let _ = fs::remove_file(map_path);
}

#[tokio::test]
async fn app_router_marks_configured_ton_validators_without_map_ip_as_fake() {
    let map_path = std::env::temp_dir().join(format!(
        "validators_clock_ton_fake_map_test_{}_{}.json",
        std::process::id(),
        now_sec_for_test()
    ));
    fs::write(
        &map_path,
        r#"[
            {"peer":"mapped-ton-validator","ip":"203.0.113.20","city":"TON City","country":"TONland","isp":"TON ISP","lat":5.25,"lon":6.5}
        ]"#,
    )
    .unwrap();

    let mut config = test_config(Vec::new());
    config
        .map_nodes_paths
        .insert("ton".to_owned(), map_path.clone());
    config.chains.push(ChainConfig {
        id: "ton".to_owned(),
        name: "TON".to_owned(),
        rpc: "https://ton.example.com".to_owned(),
        rpc_fallbacks: Vec::new(),
        color: "#4DB8FF".to_owned(),
        token_symbol: "TON".to_owned(),
        rpc_label: None,
    });
    let state = Arc::new(AppState::new(Arc::new(config)));
    cache_snapshot(
        &state,
        "ton",
        &["mapped-ton-validator", "missing-ton-validator"],
    )
    .await;

    let response = app_router(state)
        .oneshot(
            Request::builder()
                .uri("/api/chains/ton/clock")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(
        body["current_set"]["fake_validator_peers"]
            .as_array()
            .unwrap(),
        &vec![Value::String("missing-ton-validator".to_owned())]
    );

    let _ = fs::remove_file(map_path);
}

#[tokio::test]
async fn app_router_rejects_map_for_chain_without_map_file() {
    let state = Arc::new(AppState::new(Arc::new(test_config(Vec::new()))));
    let response = app_router(state)
        .oneshot(
            Request::builder()
                .uri("/api/chains/test/map")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response_json(response).await;
    assert_eq!(body["code"], "map_not_available");
}
