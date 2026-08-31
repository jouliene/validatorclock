use axum::extract::Path;
use axum::http::StatusCode;
use axum::http::header::{self, HeaderValue};
use axum::response::{Html, IntoResponse, Response};
use std::sync::LazyLock;

mod embedded;
mod version;

use embedded::{
    APP_JS_PARTS, EVERSCALE_LOGO_SVG, INDEX_HTML, JOKES_JSON, PORTRAIT_IMAGES, SMOKING_MAN_PNG,
    STATS_HTML, STATS_JS, STYLES_CSS, TON_LOGO_SVG, TYCHO_LOGO_SVG,
};

pub(super) use version::asset_version;

const ASSET_CACHE_CONTROL: HeaderValue =
    HeaderValue::from_static("public, max-age=31536000, immutable");
const PRIVATE_ASSET_CACHE_CONTROL: HeaderValue =
    HeaderValue::from_static("private, max-age=31536000, immutable");

static INDEX_PAGE: LazyLock<String> = LazyLock::new(|| render_page(INDEX_HTML));
static STATS_PAGE: LazyLock<String> = LazyLock::new(|| render_page(STATS_HTML));
static APP_JS_BUNDLE: LazyLock<String> = LazyLock::new(|| APP_JS_PARTS.join("\n\n"));

pub(super) async fn index() -> Html<&'static str> {
    Html(INDEX_PAGE.as_str())
}

pub(super) async fn stats_page() -> Html<&'static str> {
    Html(STATS_PAGE.as_str())
}

pub(super) async fn stats_js() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/javascript; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, PRIVATE_ASSET_CACHE_CONTROL),
        ],
        STATS_JS,
    )
}

fn render_page(template: &str) -> String {
    template
        .replace("__ASSET_VERSION__", asset_version())
        .replace("__APP_VERSION__", env!("CARGO_PKG_VERSION"))
}

pub(super) async fn styles() -> impl IntoResponse {
    text_asset_response("text/css; charset=utf-8", STYLES_CSS)
}

pub(super) async fn app_js() -> impl IntoResponse {
    (
        asset_response_headers("application/javascript; charset=utf-8"),
        APP_JS_BUNDLE.as_str(),
    )
}

pub(super) async fn everscale_logo() -> impl IntoResponse {
    svg_response(EVERSCALE_LOGO_SVG)
}

pub(super) async fn tycho_logo() -> impl IntoResponse {
    svg_response(TYCHO_LOGO_SVG)
}

pub(super) async fn ton_logo() -> impl IntoResponse {
    svg_response(TON_LOGO_SVG)
}

pub(super) async fn smoking_man_png() -> impl IntoResponse {
    bytes_asset_response("image/png", SMOKING_MAN_PNG)
}

pub(super) async fn portrait_image(Path(name): Path<String>) -> Response {
    PORTRAIT_IMAGES
        .iter()
        .find_map(|(file_name, bytes)| (*file_name == name).then_some(*bytes))
        .map(|bytes| bytes_asset_response("image/webp", bytes).into_response())
        .unwrap_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                )],
                "portrait not found",
            )
                .into_response()
        })
}

pub(super) async fn jokes_json() -> impl IntoResponse {
    text_asset_response("application/json; charset=utf-8", JOKES_JSON)
}

fn svg_response(svg: &'static str) -> impl IntoResponse {
    text_asset_response("image/svg+xml; charset=utf-8", svg)
}

fn text_asset_response(content_type: &'static str, body: &'static str) -> impl IntoResponse {
    (asset_response_headers(content_type), body)
}

fn bytes_asset_response(content_type: &'static str, body: &'static [u8]) -> impl IntoResponse {
    (asset_response_headers(content_type), body)
}

fn asset_response_headers(content_type: &'static str) -> [(header::HeaderName, HeaderValue); 2] {
    [
        (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
        (header::CACHE_CONTROL, ASSET_CACHE_CONTROL),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // The bundle order lives in a hand-written list, so a new file in
    // public/app/ can be written, styled, and never shipped.
    #[test]
    fn every_frontend_script_is_bundled() {
        let script_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("public/app");
        let mut scripts = std::fs::read_dir(&script_dir)
            .expect("public/app should be readable")
            .map(|entry| entry.expect("directory entry should be readable").path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("js"))
            .collect::<Vec<_>>();
        scripts.sort();

        let missing = scripts
            .iter()
            .filter(|path| {
                let source = std::fs::read_to_string(path).expect("script should be readable");
                !APP_JS_PARTS.iter().any(|part| *part == source)
            })
            .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
            .collect::<Vec<_>>();

        assert!(
            missing.is_empty(),
            "these scripts are not in APP_JS_PARTS, so they never reach the browser: {missing:?}"
        );
        assert_eq!(
            APP_JS_PARTS.len(),
            scripts.len() + 1,
            "APP_JS_PARTS should hold every public/app script plus public/app.js"
        );
    }
}
