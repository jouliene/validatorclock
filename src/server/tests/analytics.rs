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
    assert_eq!(json["all_time"]["visits"], 1);
    assert_eq!(json["last_30_days"]["unique_visitors"], 1);
    assert_eq!(json["last_30_days"]["visits"], 1);
    assert!(json["today"].get("pageviews").is_none());
    assert!(json["last_30_days"].get("pageviews").is_none());
    assert!(json["all_time"].get("pageviews").is_none());
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
    assert!(
        content.contains("unique_visitors"),
        "the aggregate keeps counts only"
    );
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
    assert!(json["today"].get("pageviews").is_none());
}

#[tokio::test]
async fn visitor_stats_expose_ip_level_traffic_for_the_public_stats_page() {
    let state = test_state(Vec::new());

    analytics_event_response(Arc::clone(&state), "page_open", "198.51.100.7:1200").await;
    analytics_event_response(Arc::clone(&state), "heartbeat", "198.51.100.7:1201").await;
    analytics_event_response(Arc::clone(&state), "page_open", "198.51.100.8:1202").await;

    let json = response_json(authed_stats_response(state, "/stats/visitors").await).await;

    assert_eq!(json["known_visitors"], 2);
    assert_eq!(json["listed_visitors"], 2);
    assert_eq!(json["online_now"], 2);
    let visitors = json["visitors"].as_array().unwrap();
    assert_eq!(visitors.len(), 2);
    let addresses = visitors
        .iter()
        .map(|visitor| visitor["ip"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(addresses.contains(&"198.51.100.7"));
    assert!(addresses.contains(&"198.51.100.8"));
    let first = visitors
        .iter()
        .find(|visitor| visitor["ip"] == "198.51.100.7")
        .unwrap();
    assert_eq!(first["today_visits"], 1);
    assert_eq!(first["last_30_days_visits"], 1);
    assert_eq!(first["total_visits"], 1);
    assert_eq!(first["online"], true);
}

#[tokio::test]
async fn visitor_store_keeps_addresses_while_the_aggregate_store_stays_anonymous() {
    let analytics_path = temp_state_path("analytics_visitor_split");
    let visitors_path = temp_state_path("visitors_split");
    let mut config = test_config(Vec::new());
    config.analytics_path = Some(analytics_path.clone());
    config.visitors_path = Some(visitors_path.clone());
    let state = state_from_config(config);

    analytics_event_response(Arc::clone(&state), "page_open", "198.51.100.77:1200").await;

    let analytics = std::fs::read_to_string(&analytics_path).unwrap();
    assert!(!analytics.contains("198.51.100.77"));
    let visitors = std::fs::read_to_string(&visitors_path).unwrap();
    assert!(visitors.contains("198.51.100.77"));
    assert!(!visitors.contains("Firefox"));
    assert!(!visitors.contains("en-US"));
}

#[tokio::test]
async fn visitor_stats_ignore_obvious_bot_user_agents() {
    let state = test_state(Vec::new());

    crate::server::routes::app_router(Arc::clone(&state))
        .oneshot(analytics_request(
            "page_open",
            "198.51.100.9:1200",
            "Mozilla/5.0 (compatible; Googlebot/2.1)",
        ))
        .await
        .unwrap();

    let json = response_json(authed_stats_response(state, "/stats/visitors").await).await;

    assert_eq!(json["known_visitors"], 0);
    assert!(json["visitors"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn repeat_heartbeats_do_not_rewrite_the_analytics_store() {
    let analytics_path = temp_state_path("analytics_heartbeat_writes");
    let mut config = test_config(Vec::new());
    config.analytics_path = Some(analytics_path.clone());
    let state = state_from_config(config);

    analytics_event_response(Arc::clone(&state), "page_open", "198.51.100.5:1200").await;
    let written_at = std::fs::metadata(&analytics_path)
        .unwrap()
        .modified()
        .unwrap();

    for _ in 0..3 {
        analytics_event_response(Arc::clone(&state), "heartbeat", "198.51.100.5:1201").await;
    }

    assert_eq!(
        std::fs::metadata(&analytics_path)
            .unwrap()
            .modified()
            .unwrap(),
        written_at,
        "a heartbeat that changes no counter should not rewrite the store"
    );

    let json = response_json(app_response(Arc::clone(&state), "/api/analytics/public").await).await;
    assert_eq!(json["today"]["visits"], 1);
    assert_eq!(json["today"]["unique_visitors"], 1);

    // A visitor the day has not seen changes the counters, so it is written.
    // Aggregation is by /24, so this needs a different network.
    analytics_event_response(Arc::clone(&state), "page_open", "198.51.101.6:1202").await;

    assert_ne!(
        std::fs::metadata(&analytics_path)
            .unwrap()
            .modified()
            .unwrap(),
        written_at
    );
    let json = response_json(app_response(state, "/api/analytics/public").await).await;
    assert_eq!(json["today"]["visits"], 2);
    assert_eq!(json["today"]["unique_visitors"], 2);
}

#[tokio::test]
async fn the_summary_and_the_address_table_agree_on_visits_and_visitors() {
    let state = test_state(Vec::new());

    // Two addresses, one of them sending several events in one session.
    analytics_event_response(Arc::clone(&state), "page_open", "198.51.100.7:1200").await;
    analytics_event_response(Arc::clone(&state), "heartbeat", "198.51.100.7:1201").await;
    analytics_event_response(Arc::clone(&state), "page_open", "203.0.113.8:1202").await;
    analytics_event_response(Arc::clone(&state), "heartbeat", "203.0.113.8:1203").await;

    let summary = response_json(app_response(Arc::clone(&state), "/api/analytics/public").await)
        .await["today"]
        .clone();
    let table = response_json(authed_stats_response(state, "/stats/visitors").await).await;

    let listed_addresses = table["visitors"].as_array().unwrap();
    let visits_today: u64 = listed_addresses
        .iter()
        .map(|visitor| visitor["today_visits"].as_u64().unwrap())
        .sum();
    let addresses_today = listed_addresses
        .iter()
        .filter(|visitor| visitor["today_visits"].as_u64().unwrap() > 0)
        .count() as u64;

    assert_eq!(
        summary["visits"].as_u64(),
        Some(visits_today),
        "the summary counts the visits the table shows"
    );
    assert_eq!(
        summary["unique_visitors"].as_u64(),
        Some(addresses_today),
        "a unique visitor is an address, in both readings"
    );
    assert_eq!(summary["online_now"].as_u64(), table["online_now"].as_u64());
    assert_eq!(visits_today, 2);
    assert_eq!(addresses_today, 2);
}

#[tokio::test]
async fn a_restart_does_not_invent_a_visit_for_a_live_session() {
    let analytics_path = temp_state_path("analytics_restart");
    let visitors_path = temp_state_path("visitors_restart");
    let mut config = test_config(Vec::new());
    config.analytics_path = Some(analytics_path.clone());
    config.visitors_path = Some(visitors_path.clone());
    let state = state_from_config(config.clone());

    analytics_event_response(Arc::clone(&state), "page_open", "198.51.100.7:1200").await;
    let json = response_json(app_response(state, "/api/analytics/public").await).await;
    assert_eq!(json["today"]["visits"], 1);

    // The process starts again while the visitor keeps the page open.
    let restarted = state_from_config(config);
    analytics_event_response(Arc::clone(&restarted), "heartbeat", "198.51.100.7:1201").await;

    let json = response_json(app_response(restarted, "/api/analytics/public").await).await;
    assert_eq!(
        json["today"]["visits"], 1,
        "the session survived the restart, so no new visit was counted"
    );
    assert_eq!(json["today"]["unique_visitors"], 1);
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
