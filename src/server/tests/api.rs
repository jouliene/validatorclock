use super::*;
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
async fn app_router_reports_not_found_code() {
    let response = app_response(test_state(Vec::new()), "/missing").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response_json(response).await;
    assert_eq!(body["error"], "not found");
    assert_eq!(body["code"], "not_found");
}
