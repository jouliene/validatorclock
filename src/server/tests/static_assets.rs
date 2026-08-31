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
    assert!(body.contains(&format!("version {}", env!("CARGO_PKG_VERSION"))));
    assert!(!body.contains("__APP_VERSION__"));
    assert_no_native_title_attributes(&body);

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
    assert!(body.contains("const ROUND_STATS_CHARTS ="));
    assert_no_native_title_assignments(&body);
    assert_app_js_order(
        &body,
        &[
            "function formatRoundStatsPercent",
            "const ROUND_STATS_CHARTS =",
            "function renderRoundStatsColor",
            "function fetchRoundStatsSnapshot",
            "function setupRoundStatsControls",
        ],
    );
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

#[tokio::test]
async fn app_router_serves_the_visitor_stats_page_to_authenticated_requests() {
    let state = test_state(Vec::new());
    let asset_version = asset_version();

    let response = authed_stats_response(Arc::clone(&state), "/stats").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains(&format!("/styles.css?v={asset_version}")));
    assert!(body.contains(&format!("/stats/app.js?v={asset_version}")));
    assert!(body.contains(&format!("version {}", env!("CARGO_PKG_VERSION"))));
    assert!(!body.contains("__ASSET_VERSION__"));
    assert!(!body.contains("__APP_VERSION__"));
    assert_no_native_title_attributes(&body);

    let response = authed_stats_response(
        Arc::clone(&state),
        &format!("/stats/app.js?v={asset_version}"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(
        response.headers(),
        header::CONTENT_TYPE,
        "application/javascript; charset=utf-8",
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("/stats/visitors"));
    assert_no_native_title_assignments(&body);
}

#[tokio::test]
async fn stats_routes_challenge_requests_without_valid_credentials() {
    let state = test_state(Vec::new());

    for uri in ["/stats", "/stats/", "/stats/app.js", "/stats/visitors"] {
        let response = stats_response(Arc::clone(&state), uri, None).await;

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "`{uri}` should be protected"
        );
        assert_header_starts_with(response.headers(), header::WWW_AUTHENTICATE, "Basic realm=");
    }

    let wrong_password = stats_credentials(TEST_STATS_USERNAME, "not-the-password");
    let response = stats_response(Arc::clone(&state), "/stats", Some(&wrong_password)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let wrong_user = stats_credentials("someone", TEST_STATS_PASSWORD);
    let response = stats_response(Arc::clone(&state), "/stats", Some(&wrong_user)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = stats_response(Arc::clone(&state), "/stats", Some("Basic not-base64")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn stats_routes_stay_hidden_when_no_password_is_configured() {
    let mut config = test_config(Vec::new());
    config.security.stats_auth = crate::config::StatsAuthConfig {
        password: None,
        password_env: "VALIDATORCLOCK_STATS_PASSWORD_UNSET_FOR_TESTS".to_owned(),
        ..crate::config::StatsAuthConfig::default()
    };
    let state = state_from_config(config);

    for uri in ["/stats", "/stats/visitors"] {
        let response = stats_response(Arc::clone(&state), uri, None).await;

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "`{uri}` should stay hidden without a configured password"
        );
        assert!(response.headers().get(header::WWW_AUTHENTICATE).is_none());
    }
}

#[tokio::test]
async fn old_public_stats_routes_are_gone() {
    let state = test_state(Vec::new());

    for uri in ["/stats.js", "/stats.html", "/api/analytics/visitors"] {
        let response = authed_stats_response(Arc::clone(&state), uri).await;

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "`{uri}` should no longer be routed"
        );
    }
}

#[tokio::test]
async fn index_no_longer_claims_that_addresses_are_not_stored() {
    let state = test_state(Vec::new());

    let response = app_response(state, "/").await;

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("Public Stats"));
    assert!(!body.contains("IP addresses are not stored"));
    assert!(!body.contains("No analytics cookies"));
}

fn assert_app_js_order(body: &str, needles: &[&str]) {
    let mut previous = 0;
    for needle in needles {
        let position = body[previous..]
            .find(needle)
            .map(|offset| previous + offset)
            .unwrap_or_else(|| panic!("missing app.js marker `{needle}`"));
        assert!(
            position >= previous,
            "app.js marker `{needle}` appeared out of order"
        );
        previous = position + needle.len();
    }
}

fn assert_no_native_title_attributes(body: &str) {
    for needle in [" title=\"", " title='"] {
        assert!(
            !body.contains(needle),
            "native title attributes should use custom tooltip components instead"
        );
    }
}

fn assert_no_native_title_assignments(body: &str) {
    for needle in [
        ".title =",
        ".title=",
        "setAttribute(\"title\"",
        "setAttribute('title'",
    ] {
        assert!(
            !body.contains(needle),
            "native title tooltip assignment should use setValidatorTooltip instead"
        );
    }
}
