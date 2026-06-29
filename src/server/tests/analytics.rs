use super::*;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::test]
async fn analytics_counts_public_traffic_without_counting_heartbeats_as_pageviews() {
    let state = test_state(Vec::new());

    let response =
        analytics_event_response(Arc::clone(&state), "page_open", "203.0.113.42:1200").await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    analytics_event_response(Arc::clone(&state), "page_open", "203.0.113.42:1201").await;
    analytics_event_response(Arc::clone(&state), "heartbeat", "203.0.113.42:1202").await;

    let json = response_json(app_response(state, "/api/analytics/public").await).await;
    assert_eq!(json["today"]["online_now"], 1);
    assert_eq!(json["today"]["unique_visitors"], 1);
    assert_eq!(json["today"]["visits"], 1);
    assert_eq!(json["today"]["pageviews"], 2);
    assert_eq!(json["all_time"]["visits"], 1);
    assert_eq!(json["all_time"]["pageviews"], 2);
    assert!(json.get("visitor_hashes").is_none());
    assert!(json["today"].get("visitor_hashes").is_none());
}

#[tokio::test]
async fn analytics_store_does_not_persist_raw_request_identifiers() {
    let analytics_path = temp_state_path("analytics_privacy");
    let mut config = test_config(Vec::new());
    config.analytics_path = Some(analytics_path.clone());
    let state = state_from_config(config);

    analytics_event_response(Arc::clone(&state), "page_open", "198.51.100.77:1200").await;

    let content = std::fs::read_to_string(&analytics_path).unwrap();
    assert!(!content.contains("198.51.100.77"));
    assert!(!content.contains("198.51.100.0"));
    assert!(!content.contains("Firefox"));
    assert!(!content.contains("en-US"));
    assert!(content.contains("visitor_hashes"));
}

#[tokio::test]
async fn analytics_ignores_obvious_bot_user_agents() {
    let state = test_state(Vec::new());

    let response = crate::server::routes::app_router(Arc::clone(&state))
        .oneshot(analytics_request(
            "page_open",
            "203.0.113.42:1200",
            "Slackbot-LinkExpanding 1.0",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let json = response_json(app_response(state, "/api/analytics/public").await).await;
    assert_eq!(json["today"]["unique_visitors"], 0);
    assert_eq!(json["today"]["visits"], 0);
    assert_eq!(json["today"]["pageviews"], 0);
}

async fn analytics_event_response(
    state: Arc<AppState>,
    event: &str,
    peer_addr: &str,
) -> axum::response::Response {
    crate::server::routes::app_router(state)
        .oneshot(analytics_request(
            event,
            peer_addr,
            "Mozilla/5.0 Firefox/127.0",
        ))
        .await
        .unwrap()
}

fn analytics_request(event: &str, peer_addr: &str, user_agent: &str) -> Request<Body> {
    let payload = format!(r#"{{"event":"{event}","path":"/","visible":true,"ts":1782723120000}}"#);
    let mut request = Request::builder()
        .method(Method::POST)
        .uri("/api/analytics/event")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::USER_AGENT, user_agent)
        .header(header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
        .body(Body::from(payload))
        .unwrap();
    request
        .extensions_mut()
        .insert(peer_addr.parse::<SocketAddr>().unwrap());
    request
}
