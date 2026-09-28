use super::*;
use axum::http::StatusCode;
use sha2::{Digest, Sha256};
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

#[test]
fn seo_keeps_the_original_dashboard_body() {
    // Body from d6450f7 (2.5.4), before the SEO work. Metadata may change;
    // adding body content requires an explicit design decision, not an SEO edit.
    let template = include_str!("../../../public/index.html");
    let body = template.split_once("<body>").unwrap().1;
    assert_eq!(
        hex::encode(Sha256::digest(body)),
        "b9ff13a47aec3c13b83587e6b2691b4dc535e948d8ff20270c60adda9b2ef5e8"
    );
}

#[tokio::test]
async fn metadata_is_in_the_head_without_extra_body_content() {
    let state = test_state(Vec::new());
    let response = app_response(state, "/").await;
    assert_eq!(response.status(), StatusCode::OK);
    let page = html(response).await;
    let (head, body) = page.split_once("<body>").unwrap();
    assert!(head.contains("rel=\"canonical\" href=\"https://allowed.example/\""));
    assert!(head.contains("name=\"description\""));
    assert!(head.contains("property=\"og:image\""));
    let graph = head
        .split("<script type=\"application/ld+json\">")
        .nth(1)
        .unwrap()
        .split("</script>")
        .next()
        .unwrap();
    let graph: Value = serde_json::from_str(graph).unwrap();
    assert_eq!(graph["@graph"][1]["url"], "https://allowed.example/");
    assert!(body.contains("<h1>VALIDATOR CLOCK</h1>"));
    for added in [
        "page-intro",
        "network-snapshot",
        "content-nav",
        "dashboard-live",
        "__SEO_HEAD__",
    ] {
        assert!(!body.contains(added), "{added}");
    }
}

#[tokio::test]
async fn sitemap_contains_only_the_original_dashboard() {
    let state = test_state(Vec::new());
    let response = app_response(Arc::clone(&state), "/sitemap.xml").await;
    assert_eq!(response.status(), StatusCode::OK);
    let xml = html(response).await;
    assert_eq!(xml.matches("<loc>").count(), 1);
    assert!(xml.contains("<loc>https://allowed.example/</loc>"));
    let response = app_response(Arc::clone(&state), "/robots.txt").await;
    assert_eq!(response.status(), StatusCode::OK);
    let robots = html(response).await;
    assert!(robots.contains("Sitemap: https://allowed.example/sitemap.xml"));
    assert!(!robots.contains("Disallow: /api"));
    for path in ["/unknown/", "/unknown", "/guides/unknown/", "/content.js"] {
        assert_eq!(
            app_response(Arc::clone(&state), path).await.status(),
            StatusCode::NOT_FOUND
        );
    }
}

#[tokio::test]
async fn removed_pages_and_mirrors_redirect_without_changing_the_dashboard() {
    let mut config = test_config(vec!["allowed.example".into(), "www.allowed.example".into()]);
    config.tls.enabled = true;
    let state = state_from_config(config);
    for path in [
        "/index.html",
        "/test",
        "/test/",
        "/methodology",
        "/methodology/",
        "/about/",
        "/guides/validator-elections/",
    ] {
        let response = app_response_with(
            Arc::clone(&state),
            &format!("{path}?x=1"),
            &[(header::HOST, "allowed.example")],
        )
        .await;
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT, "{path}");
        assert_eq!(response.headers()[header::LOCATION], "/?x=1");
    }
    let response = app_response_with(
        Arc::clone(&state),
        "/test/?x=1",
        &[(header::HOST, "www.allowed.example")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        response.headers()[header::LOCATION],
        "https://allowed.example/?x=1"
    );
    let bad = app_response_with(state, "/index.html", &[(header::HOST, "attacker.example")]).await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
}
