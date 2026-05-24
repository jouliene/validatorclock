use super::*;
use crate::server::assets::asset_version;
use axum::body::to_bytes;
use axum::http::{StatusCode, header};
use std::sync::Arc;

#[tokio::test]
async fn app_router_versions_and_caches_static_assets() {
    let state = test_state(Vec::new());
    let response = app_response(Arc::clone(&state), "/").await;

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

    let response = app_response(
        Arc::clone(&state),
        &format!("/styles.css?v={asset_version}"),
    )
    .await;

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

    let response = app_response(Arc::clone(&state), &format!("/app.js?v={asset_version}")).await;

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

    let response = app_response(
        Arc::clone(&state),
        &format!("/brands/everscale.svg?v={asset_version}"),
    )
    .await;

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

    let response = app_response(state, &format!("/brands/ton.svg?v={asset_version}")).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(
        response.headers(),
        header::CONTENT_TYPE,
        "image/svg+xml; charset=utf-8",
    );
}
