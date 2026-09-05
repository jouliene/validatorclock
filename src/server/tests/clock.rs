use super::*;
use axum::http::{StatusCode, header};
use std::io::Read;
use std::sync::Arc;
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

/// The bytes are written out when the snapshot is built, so what a reader gets
/// has to be what serializing that snapshot would have given them.
#[tokio::test]
async fn the_clock_is_served_as_the_bytes_it_was_written_out_as() {
    let state = test_state(Vec::new());
    state
        .store_cached_snapshot("test", now_sec_for_test(), test_clock_snapshot("test"))
        .await;
    let served = state
        .cached_snapshot("test")
        .await
        .expect("the chain is cached");

    let response = app_response(Arc::clone(&state), "/api/chains/test/clock").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(response.headers(), header::ETAG, "W/\"");
    assert_eq!(
        response
            .headers()
            .get(header::VARY)
            .and_then(|value| value.to_str().ok()),
        Some("accept-encoding"),
        "the same URL answers compressed or not, and a cache has to keep them apart"
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.as_ref(), serde_json::to_vec(served.as_ref()).unwrap());
}

#[tokio::test]
async fn a_reader_offering_the_tag_it_was_given_is_told_nothing_changed() {
    let state = test_state(Vec::new());
    state
        .store_cached_snapshot("test", now_sec_for_test(), test_clock_snapshot("test"))
        .await;

    let first = app_response(Arc::clone(&state), "/api/chains/test/clock").await;
    let entity_tag = first
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .expect("a served clock carries its tag")
        .to_owned();

    let second = conditional_response(state, "/api/chains/test/clock", &entity_tag).await;

    assert_eq!(second.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        second
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok()),
        Some(entity_tag.as_str())
    );
    let body = to_bytes(second.into_body(), usize::MAX).await.unwrap();
    assert!(body.is_empty(), "nothing changed means nothing to send");
}

/// The compressed copy is made once, at the refresh; a reader that takes it
/// must get the same answer as one that does not.
#[tokio::test]
async fn a_reader_that_takes_a_compressed_answer_gets_the_same_clock() {
    let state = test_state(Vec::new());
    state
        .store_cached_snapshot("test", now_sec_for_test(), test_clock_snapshot("test"))
        .await;

    let plain = app_response(Arc::clone(&state), "/api/chains/test/clock").await;
    let plain_tag = plain
        .headers()
        .get(header::ETAG)
        .cloned()
        .expect("a served clock carries its tag");
    let plain_body = to_bytes(plain.into_body(), usize::MAX).await.unwrap();

    let compressed = app_response_with(
        state,
        "/api/chains/test/clock",
        &[(header::ACCEPT_ENCODING, "gzip, deflate")],
    )
    .await;

    assert_eq!(
        compressed.headers().get(header::CONTENT_ENCODING),
        Some(&header::HeaderValue::from_static("gzip"))
    );
    assert_eq!(
        compressed.headers().get(header::ETAG),
        Some(&plain_tag),
        "both encodings answer to the tag taken over the body they carry"
    );
    let compressed_body = to_bytes(compressed.into_body(), usize::MAX).await.unwrap();
    let mut decoded = Vec::new();
    flate2::read::GzDecoder::new(compressed_body.as_ref())
        .read_to_end(&mut decoded)
        .expect("what we send back deflates");
    assert_eq!(decoded, plain_body.as_ref());
}
