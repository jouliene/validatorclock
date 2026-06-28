use axum::extract::Path;
use axum::http::StatusCode;
use axum::http::header::{self, HeaderValue};
use axum::response::{Html, IntoResponse, Response};

mod embedded;
mod version;

use embedded::{
    APP_JS_PARTS, EVERSCALE_LOGO_SVG, INDEX_HTML, JOKES_JSON, PORTRAIT_IMAGES, SMOKING_MAN_PNG,
    STYLES_CSS, TON_LOGO_SVG, TYCHO_LOGO_SVG,
};

pub(super) use version::asset_version;

const ASSET_CACHE_CONTROL: HeaderValue =
    HeaderValue::from_static("public, max-age=31536000, immutable");

pub(super) async fn index() -> Html<String> {
    Html(
        INDEX_HTML
            .replace("__ASSET_VERSION__", &asset_version())
            .replace("__APP_VERSION__", env!("CARGO_PKG_VERSION")),
    )
}

pub(super) async fn styles() -> impl IntoResponse {
    text_asset_response("text/css; charset=utf-8", STYLES_CSS)
}

pub(super) async fn app_js() -> impl IntoResponse {
    (
        asset_response_headers("application/javascript; charset=utf-8"),
        APP_JS_PARTS.join("\n\n"),
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
