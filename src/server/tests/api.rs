use super::*;
use crate::chain::RoundColor;
use axum::http::{StatusCode, header};

#[tokio::test]
async fn app_router_serves_health_with_security_headers() {
    let response = app_response(test_state(Vec::new()), "/api/health").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(response.headers(), header::CONTENT_TYPE, "application/json");
    assert_eq!(
        response
            .headers()
            .get(header::X_CONTENT_TYPE_OPTIONS)
            .and_then(|value| value.to_str().ok()),
        Some("nosniff")
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], br#"{"status":"ok"}"#);
}

#[tokio::test]
async fn app_router_lists_configured_chains() {
    let response = app_response(test_state(Vec::new()), "/api/chains").await;

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
    let state = test_state(Vec::new());
    state
        .record_refresh_failure("test", 123, "rpc down".to_owned())
        .await;

    let response = app_response(state, "/api/status").await;

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
async fn app_router_reports_unknown_chain() {
    let response = app_response(test_state(Vec::new()), "/api/chains/missing/clock").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response_json(response).await;
    assert_eq!(body["error"], "unknown chain id `missing`");
    assert_eq!(body["code"], "unknown_chain");
}

#[tokio::test]
async fn app_router_reports_unknown_chain_for_round_stats() {
    let response = app_response(test_state(Vec::new()), "/api/chains/missing/round-stats").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response_json(response).await;
    assert_eq!(body["error"], "unknown chain id `missing`");
    assert_eq!(body["code"], "unknown_chain");
}

#[tokio::test]
async fn app_router_serves_cached_round_stats_when_preferred() {
    let state = test_state(Vec::new());
    cache_snapshot_with(&state, "test", &["alice"], |snapshot| {
        let round_duration = 65_536;
        snapshot.current_set.round_id = 12;
        snapshot.current_set.round_color = RoundColor::Blue;
        snapshot.current_set.utime_since = 12 * round_duration;
        snapshot.current_set.utime_until = 13 * round_duration;
        snapshot.current_set.fake_validator_status_known = true;

        let mut previous_set = snapshot.current_set.clone();
        previous_set.round_id = 10;
        previous_set.round_color = RoundColor::Blue;
        previous_set.utime_since = 10 * round_duration;
        previous_set.utime_until = 11 * round_duration;
        previous_set.total_stake = Some("100".to_owned());
        previous_set.total_reward = Some("1".to_owned());
        previous_set.validators[0].stake = Some("100".to_owned());
        previous_set.validators[0].reward = Some("1".to_owned());
        snapshot.previous_set = Some(previous_set);
        snapshot.next_set = None;
    })
    .await;
    state.cached_snapshot("test").await.unwrap();

    let response = app_response(state, "/api/chains/test/round-stats?prefer_cache=1").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["active_round_id"], 12);
    assert_eq!(body["blue"]["rounds"][0]["round_id"], 10);
    assert_eq!(body["blue"]["rounds"][0]["total_stake"], "100");
    assert!(
        body["blue"]["rounds"][0]["profitability_percent"]
            .as_f64()
            .is_some()
    );
}

#[tokio::test]
async fn app_router_reports_not_found_code() {
    let response = app_response(test_state(Vec::new()), "/missing").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response_json(response).await;
    assert_eq!(body["error"], "not found");
    assert_eq!(body["code"], "not_found");
}
