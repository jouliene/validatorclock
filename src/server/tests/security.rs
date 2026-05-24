use super::*;
use crate::server::routes::{app_router, challenge_redirect_router};
use crate::server::security::{normalize_host, redirect_location, request_host_allowed};
use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, header};
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
async fn app_router_rejects_bad_host() {
    let state = test_state(vec!["allowed.example".to_owned()]);
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
async fn challenge_route_is_available_before_host_check() {
    let state = test_state(vec!["allowed.example".to_owned()]);
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
