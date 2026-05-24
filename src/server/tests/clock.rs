use super::*;
use axum::http::{StatusCode, header};
use std::time::Duration;

#[tokio::test]
async fn app_router_serves_cached_clock_shape() {
    let state = test_state(Vec::new());
    let snapshot = test_clock_snapshot("test");
    state
        .store_cached_snapshot("test", now_sec_for_test(), snapshot)
        .await;

    let response = app_response(state, "/api/chains/test/clock").await;

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
    let state = test_state(Vec::new());
    let snapshot = test_clock_snapshot("test");
    state.store_cached_snapshot("test", 1, snapshot).await;

    let response = tokio::time::timeout(
        Duration::from_secs(2),
        app_response(state, "/api/chains/test/clock"),
    )
    .await
    .expect("stale cached response should not wait for rpc");

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
