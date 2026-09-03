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
        Some("no-cache")
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
async fn text_responses_are_compressed_when_the_client_accepts_gzip() {
    let state = test_state(Vec::new());

    let plain = app_response(Arc::clone(&state), "/app.js").await;
    assert!(plain.headers().get(header::CONTENT_ENCODING).is_none());
    let plain_size = to_bytes(plain.into_body(), usize::MAX).await.unwrap().len();

    let compressed = crate::server::routes::app_router(state)
        .oneshot(
            axum::http::Request::builder()
                .uri("/app.js")
                .header(header::ACCEPT_ENCODING, "gzip")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        compressed
            .headers()
            .get(header::CONTENT_ENCODING)
            .and_then(|value| value.to_str().ok()),
        Some("gzip")
    );
    assert_header_starts_with(compressed.headers(), header::VARY, "accept-encoding");
    let compressed_size = to_bytes(compressed.into_body(), usize::MAX)
        .await
        .unwrap()
        .len();
    assert!(
        compressed_size * 2 < plain_size,
        "gzip should more than halve the bundle: {compressed_size} vs {plain_size}"
    );
}

#[tokio::test]
async fn revalidated_responses_carry_entity_tags_and_answer_304() {
    let state = test_state(Vec::new());

    let response = app_response(Arc::clone(&state), "/api/chains").await;

    assert_eq!(response.status(), StatusCode::OK);
    let entity_tag = response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .expect("chains response should carry an entity tag")
        .to_owned();
    assert!(entity_tag.starts_with("W/\""));

    let response = conditional_response(Arc::clone(&state), "/api/chains", &entity_tag).await;

    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        response
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok()),
        Some(entity_tag.as_str())
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(body.is_empty());

    let response = conditional_response(state, "/api/chains", "W/\"stale\"").await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn private_stats_responses_are_never_revalidated_from_a_stored_copy() {
    let state = test_state(Vec::new());

    let response = authed_stats_response(Arc::clone(&state), "/stats/visitors").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    assert!(response.headers().get(header::ETAG).is_none());

    let response = authed_stats_response(state, "/stats").await;

    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
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

#[tokio::test]
async fn vendored_map_libraries_are_served() {
    // The map names these URLs; the router has to answer them. A version bump
    // that changes one and not the other is a 404, and a 404 here means the map
    // never loads at all.
    let state = test_state(Vec::new());
    let referenced = referenced_vendor_urls();

    assert_eq!(
        referenced.len(),
        3,
        "expected the map to name three vendored files, found {referenced:?}"
    );

    for url in referenced {
        let response = app_response(Arc::clone(&state), &url).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "the map asks for {url}, which the router does not serve"
        );
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("public, max-age=31536000, immutable"),
            "{url} carries the upstream version in its name, so it can be cached for good"
        );
    }
}

#[tokio::test]
async fn the_page_loads_nothing_from_another_origin() {
    // Everything the page pulls in comes from this server. A subresource on
    // some other origin is the one failure the browser cannot report: a script
    // tag whose connection is black-holed fires neither `load` nor `error`, so
    // whatever waited on it waits for as long as the page is open. Links a
    // reader clicks are not subresources and are none of this test's business.
    let state = test_state(Vec::new());

    let response = app_response(Arc::clone(&state), "/").await;
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let page = String::from_utf8_lossy(&body).to_string();
    for tag in page.split('<').filter(|tag| {
        tag.starts_with("script") || tag.starts_with("link") || tag.starts_with("img")
    }) {
        let loaded_from = tag
            .split_once("src=\"")
            .or_else(|| tag.split_once("href=\""));
        if let Some((_, url)) = loaded_from {
            let url = url.split('"').next().unwrap_or_default();
            assert!(
                !url.contains("//"),
                "the page loads {url} from another origin"
            );
        }
    }

    let response = app_response(Arc::clone(&state), "/app.js").await;
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let bundle = String::from_utf8_lossy(&body).to_string();
    for line in bundle
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
    {
        let Some((name, rest)) = line.split_once("_URL = \"") else {
            continue;
        };
        let url = rest.split('"').next().unwrap_or_default();
        assert!(
            url.starts_with('/'),
            "{} points at another origin: {url}",
            name.trim()
        );
    }
}

/// The vendored files the map asks for, read out of the map's own source.
fn referenced_vendor_urls() -> Vec<String> {
    crate::server::assets::map_js_source()
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .filter_map(|line| line.split_once("\"/vendor/"))
        .filter_map(|(_, rest)| rest.split_once('"'))
        .map(|(path, _)| format!("/vendor/{path}"))
        .collect()
}
