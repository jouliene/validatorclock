use crate::chain::{ClockSnapshot, test_clock_snapshot};
use crate::config::{AppConfig, ChainConfig, SecurityConfig, TlsConfig};
use crate::state::AppState;
use axum::body::to_bytes;
use axum::http::{HeaderMap, header};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tower::ServiceExt as _;

mod api;
mod clock;
mod map;
mod security;
mod static_assets;
mod tls_acme;

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
        history_path: Some(temp_state_path("history")),
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

fn test_state(allowed_hosts: Vec<String>) -> std::sync::Arc<AppState> {
    state_from_config(test_config(allowed_hosts))
}

fn state_from_config(config: AppConfig) -> std::sync::Arc<AppState> {
    std::sync::Arc::new(AppState::new(std::sync::Arc::new(config)))
}

fn test_chain_config(id: &str, name: &str, color: &str, token_symbol: &str) -> ChainConfig {
    ChainConfig {
        id: id.to_owned(),
        name: name.to_owned(),
        rpc: format!("https://{id}.example.com"),
        rpc_fallbacks: Vec::new(),
        color: color.to_owned(),
        token_symbol: token_symbol.to_owned(),
        rpc_label: None,
    }
}

async fn app_response(state: std::sync::Arc<AppState>, uri: &str) -> axum::response::Response {
    crate::server::routes::app_router(state)
        .oneshot(
            axum::http::Request::builder()
                .uri(uri)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
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

fn temp_map_path(label: &str) -> PathBuf {
    temp_state_path(&format!("{label}_map"))
}

async fn cache_tycho_snapshot(state: &AppState, public_keys: &[&str]) {
    cache_snapshot(state, "tycho-testnet", public_keys).await;
}

async fn cache_snapshot(state: &AppState, chain_id: &str, public_keys: &[&str]) {
    cache_snapshot_with(state, chain_id, public_keys, |_| {}).await;
}

async fn cache_snapshot_with<F>(state: &AppState, chain_id: &str, public_keys: &[&str], mutate: F)
where
    F: FnOnce(&mut ClockSnapshot),
{
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
    mutate(&mut snapshot);
    state
        .store_cached_snapshot(chain_id, now_sec_for_test(), snapshot)
        .await;
}
