use axum::http::header::{self, HeaderValue};
use axum::response::{Html, IntoResponse};

const INDEX_HTML: &str = include_str!("../../public/index.html");
const STYLES_CSS: &str = include_str!("../../public/styles.css");
const APP_JS: &str = include_str!("../../public/app.js");
const EVERSCALE_LOGO_SVG: &str = include_str!("../../public/brands/everscale.svg");
const TYCHO_LOGO_SVG: &str = include_str!("../../public/brands/tycho.svg");
const ASSET_CACHE_CONTROL: HeaderValue =
    HeaderValue::from_static("public, max-age=31536000, immutable");

pub(super) async fn index() -> Html<String> {
    Html(INDEX_HTML.replace("__ASSET_VERSION__", &asset_version()))
}

pub(super) fn asset_version() -> String {
    format!(
        "{}-{:016x}",
        env!("CARGO_PKG_VERSION"),
        fnv1a64(&[
            INDEX_HTML,
            STYLES_CSS,
            APP_JS,
            EVERSCALE_LOGO_SVG,
            TYCHO_LOGO_SVG,
        ])
    )
}

fn fnv1a64(parts: &[&str]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    let mut hash = OFFSET;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

pub(super) async fn styles() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/css; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, ASSET_CACHE_CONTROL),
        ],
        STYLES_CSS,
    )
}

pub(super) async fn app_js() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/javascript; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, ASSET_CACHE_CONTROL),
        ],
        APP_JS,
    )
}

pub(super) async fn everscale_logo() -> impl IntoResponse {
    svg_response(EVERSCALE_LOGO_SVG)
}

pub(super) async fn tycho_logo() -> impl IntoResponse {
    svg_response(TYCHO_LOGO_SVG)
}

fn svg_response(svg: &'static str) -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("image/svg+xml; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, ASSET_CACHE_CONTROL),
        ],
        svg,
    )
}
