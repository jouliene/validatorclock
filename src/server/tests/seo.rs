use super::*;
use axum::http::StatusCode;
use std::sync::Arc;

async fn html(response: axum::response::Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn network_pages_serve_dated_validator_data_without_javascript() {
    let state = test_state(Vec::new());
    let mut snapshot = test_clock_snapshot("test");
    snapshot.fetched_at = 1_788_000_000;
    snapshot.current_set.total_stake = Some("123,456.789".to_owned());
    snapshot.current_set.validators[0].public_key =
        "<script>alert('validator')</script>".to_owned();
    state
        .store_cached_snapshot("test", snapshot.fetched_at, snapshot)
        .await;
    let response = app_response(Arc::clone(&state), "/test/").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = html(response).await;
    assert!(body.contains("data-chain-id=\"test\""));
    assert!(body.contains("https://allowed.example/test/"));
    assert!(body.contains("123,456.789"));
    assert!(body.contains("2026-08-29T"));
    assert!(body.contains("This snapshot is stale"));
    assert!(body.contains("&lt;script&gt;"));
    assert!(!body.contains("<script>alert"));
    assert!(body.contains("<table"));
    assert!(!body.contains("__NETWORK_SNAPSHOT__"));
    let graph = body
        .split("<script type=\"application/ld+json\">")
        .nth(1)
        .unwrap()
        .split("</script>")
        .next()
        .unwrap();
    let graph: Value = serde_json::from_str(graph).unwrap();
    assert_eq!(graph["@graph"][1]["url"], "https://allowed.example/test/");
    // Serving HTML neither starts a refresh nor modifies the snapshot.
    assert_eq!(
        state
            .with_cached_snapshot("test", |snapshot| snapshot.fetched_at)
            .await,
        Some(1_788_000_000)
    );
}

#[tokio::test]
async fn missing_data_is_immediate_and_not_reported_as_zero() {
    let state = test_state(Vec::new());
    let response = tokio::time::timeout(Duration::from_secs(1), app_response(state, "/test/"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = html(response).await;
    assert!(body.contains("Network data is not available yet"));
    assert!(body.contains("href=\"/test/\""));
}

#[tokio::test]
async fn sitemap_lists_only_canonical_public_pages_that_resolve() {
    let state = test_state(Vec::new());
    let response = app_response(Arc::clone(&state), "/sitemap.xml").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_header_starts_with(response.headers(), header::CONTENT_TYPE, "application/xml");
    let xml = html(response).await;
    assert!(!xml.contains("/stats"));
    assert!(!xml.contains("/api/"));
    assert!(!xml.contains("index.html"));
    let urls: Vec<_> = xml
        .split("<loc>")
        .skip(1)
        .map(|part| part.split("</loc>").next().unwrap())
        .collect();
    assert_eq!(urls.len(), 5);
    for url in urls {
        let path = url.strip_prefix("https://allowed.example").unwrap();
        let response = app_response(Arc::clone(&state), path).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let page = html(response).await;
        assert!(page.contains(&format!("rel=\"canonical\" href=\"{url}\"")));
        assert_eq!(page.matches("<h1>").count(), 1);
        assert!(!page.contains("__PAGE_"));
        assert!(!page.contains("__SEO_HEAD__"));
    }
    let response = app_response(Arc::clone(&state), "/robots.txt").await;
    let robots = html(response).await;
    assert!(robots.contains("Sitemap: https://allowed.example/sitemap.xml"));
    assert!(!robots.contains("Disallow: /api"));
    for path in ["/unknown/", "/unknown", "/guides/unknown/"] {
        assert_eq!(
            app_response(Arc::clone(&state), path).await.status(),
            StatusCode::NOT_FOUND
        );
    }
}

#[tokio::test]
async fn canonical_redirects_preserve_queries_and_do_not_trust_host() {
    let mut config = test_config(vec!["allowed.example".into(), "www.allowed.example".into()]);
    config.tls.enabled = true;
    let state = state_from_config(config);
    for (path, host, expected) in [
        (
            "/index.html?utm_source=test",
            "allowed.example",
            "/?utm_source=test",
        ),
        ("/test?x=1", "allowed.example", "/test/?x=1"),
        ("/methodology", "allowed.example", "/methodology/"),
        (
            "/test?x=1",
            "www.allowed.example",
            "https://allowed.example/test/?x=1",
        ),
        ("/", "www.allowed.example", "https://allowed.example/"),
    ] {
        let response = app_response_with(Arc::clone(&state), path, &[(header::HOST, host)]).await;
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(response.headers()[header::LOCATION], expected);
    }
    let bad = app_response_with(state, "/index.html", &[(header::HOST, "attacker.example")]).await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
}
