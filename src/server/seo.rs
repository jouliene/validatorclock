//! Search metadata only. The dashboard body, styles and interaction stay unchanged.
use crate::state::AppState;
use axum::extract::State;
use axum::http::{Uri, header};
use axum::response::{Html, IntoResponse, Response};
use serde_json::json;
use std::sync::Arc;

const DASHBOARD: &str = include_str!("../../public/index.html");

pub(super) fn public_base(state: &AppState) -> String {
    let configured = state.config.tls.public_url.trim_end_matches('/');
    if configured.is_empty() {
        format!("http://{}", state.config.listen)
    } else {
        configured.to_owned()
    }
}

/// Consolidate the short-lived 2.5.5 pages into the original dashboard.
/// Unknown URLs still return 404; no catch-all redirect or hidden content.
pub(super) fn canonical_path(state: &AppState, path: &str) -> Option<String> {
    let retired = matches!(
        path.trim_end_matches('/'),
        "/index.html" | "/about" | "/methodology" | "/guides/validator-elections"
    ) || state
        .config
        .chains
        .iter()
        .any(|chain| path.trim_end_matches('/') == format!("/{}", chain.id));
    retired.then(|| "/".to_owned())
}

pub(super) async fn page(State(state): State<Arc<AppState>>, uri: Uri) -> Response {
    if uri.path() != "/" {
        return super::responses::not_found().await;
    }
    let html = DASHBOARD.replace("__SEO_HEAD__", &head(&state));
    Html(super::assets::render_page(&html)).into_response()
}

fn head(state: &AppState) -> String {
    let base = public_base(state);
    let canonical = format!("{base}/");
    let networks = state
        .config
        .chains
        .iter()
        .map(|chain| chain.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let title = format!("{networks} Validator Dashboard | Validator Clock");
    let description = format!(
        "Track validator rounds, election windows, stakes, rewards and node distribution for {networks}."
    );
    // Describe the existing application, without inserting additional body content.
    let structured = json!({"@context": "https://schema.org", "@graph": [
        {"@type": "WebSite", "@id": format!("{base}/#website"), "name": "Validator Clock", "url": canonical, "inLanguage": "en"},
        {"@type": "WebApplication", "name": "Validator Clock", "url": canonical,
         "description": description, "applicationCategory": "UtilitiesApplication",
         "operatingSystem": "Web browser", "isAccessibleForFree": true}
    ]}).to_string().replace('<', "\\u003c");
    format!(
        r#"<title>{title}</title>
  <meta name="description" content="{description}">
  <link rel="canonical" href="{canonical}">
  <meta property="og:type" content="website">
  <meta property="og:site_name" content="Validator Clock">
  <meta property="og:title" content="{title}">
  <meta property="og:description" content="{description}">
  <meta property="og:url" content="{canonical}">
  <meta property="og:image" content="{base}/social-preview.png?v={version}">
  <meta property="og:image:alt" content="Validator Clock — validator rounds, elections and network data">
  <meta name="twitter:card" content="summary_large_image">
  <script type="application/ld+json">{structured}</script>"#,
        title = escape(&title),
        description = escape(&description),
        canonical = escape(&canonical),
        base = escape(&base),
        version = env!("CARGO_PKG_VERSION")
    )
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(super) async fn robots(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Rendering APIs and assets remain available to search and AI search crawlers.
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!(
            "User-agent: *\nAllow: /\n\nSitemap: {}/sitemap.xml\n",
            public_base(&state)
        ),
    )
}

pub(super) async fn sitemap(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let canonical = escape(&format!("{}/", public_base(&state)));
    (
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\"><url><loc>{canonical}</loc></url></urlset>"
        ),
    )
}

pub(super) async fn social_preview() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_bytes!("../../public/brands/social-preview.png").as_slice(),
    )
}
