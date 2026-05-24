use axum::http::header::{self, HeaderValue};
use axum::response::{Html, IntoResponse};

const INDEX_HTML: &str = include_str!("../../public/index.html");
const STYLES_CSS: &str = include_str!("../../public/styles.css");
const APP_STATE_JS: &str = include_str!("../../public/app/state.js");
const APP_API_JS: &str = include_str!("../../public/app/api.js");
const APP_FORMAT_JS: &str = include_str!("../../public/app/format.js");
const APP_CLOCK_JS: &str = include_str!("../../public/app/clock.js");
const APP_METRICS_JS: &str = include_str!("../../public/app/metrics.js");
const APP_VALIDATOR_METADATA_JS: &str = include_str!("../../public/app/validator_metadata.js");
const APP_VALIDATOR_TOOLTIPS_JS: &str = include_str!("../../public/app/validator_tooltips.js");
const APP_VALIDATOR_LOCATIONS_JS: &str = include_str!("../../public/app/validator_locations.js");
const APP_VALIDATOR_SOURCES_JS: &str = include_str!("../../public/app/validator_sources.js");
const APP_VALIDATOR_TYPES_JS: &str = include_str!("../../public/app/validator_types.js");
const APP_VALIDATORS_JS: &str = include_str!("../../public/app/validators.js");
const APP_VALIDATOR_COPY_JS: &str = include_str!("../../public/app/validator_copy.js");
const APP_ROUNDS_JS: &str = include_str!("../../public/app/rounds.js");
const APP_TYCHO_NODES_JS: &str = include_str!("../../public/app/tycho_nodes.js");
const APP_MAP_JS: &str = include_str!("../../public/app/map.js");
const APP_MAP_DATA_JS: &str = include_str!("../../public/app/map_data.js");
const APP_MAP_CONTROLS_JS: &str = include_str!("../../public/app/map_controls.js");
const APP_MAP_RENDER_JS: &str = include_str!("../../public/app/map_render.js");
const APP_RUNTIME_JS: &str = include_str!("../../public/app/runtime.js");
const APP_ENTRY_JS: &str = include_str!("../../public/app.js");
const EVERSCALE_LOGO_SVG: &str = include_str!("../../public/brands/everscale.svg");
const TYCHO_LOGO_SVG: &str = include_str!("../../public/brands/tycho.svg");
const TON_LOGO_SVG: &str = include_str!("../../public/brands/ton.svg");
const SMOKING_MAN_PNG: &[u8] = include_bytes!("../../public/brands/smoking-man.png");
const JOKES_JSON: &str = include_str!("../../public/jokes.json");
const ASSET_CACHE_CONTROL: HeaderValue =
    HeaderValue::from_static("public, max-age=31536000, immutable");
const APP_JS_PARTS: &[&str] = &[
    APP_STATE_JS,
    APP_API_JS,
    APP_FORMAT_JS,
    APP_CLOCK_JS,
    APP_METRICS_JS,
    APP_VALIDATOR_METADATA_JS,
    APP_VALIDATOR_TOOLTIPS_JS,
    APP_VALIDATOR_LOCATIONS_JS,
    APP_VALIDATOR_SOURCES_JS,
    APP_VALIDATOR_TYPES_JS,
    APP_VALIDATORS_JS,
    APP_VALIDATOR_COPY_JS,
    APP_ROUNDS_JS,
    APP_TYCHO_NODES_JS,
    APP_MAP_JS,
    APP_MAP_DATA_JS,
    APP_MAP_CONTROLS_JS,
    APP_MAP_RENDER_JS,
    APP_RUNTIME_JS,
    APP_ENTRY_JS,
];

pub(super) async fn index() -> Html<String> {
    Html(INDEX_HTML.replace("__ASSET_VERSION__", &asset_version()))
}

pub(super) fn asset_version() -> String {
    format!(
        "{}-{:016x}",
        env!("CARGO_PKG_VERSION"),
        fnv1a64(&[
            INDEX_HTML.as_bytes(),
            STYLES_CSS.as_bytes(),
            APP_STATE_JS.as_bytes(),
            APP_API_JS.as_bytes(),
            APP_FORMAT_JS.as_bytes(),
            APP_CLOCK_JS.as_bytes(),
            APP_METRICS_JS.as_bytes(),
            APP_VALIDATOR_METADATA_JS.as_bytes(),
            APP_VALIDATOR_TOOLTIPS_JS.as_bytes(),
            APP_VALIDATOR_LOCATIONS_JS.as_bytes(),
            APP_VALIDATOR_SOURCES_JS.as_bytes(),
            APP_VALIDATOR_TYPES_JS.as_bytes(),
            APP_VALIDATORS_JS.as_bytes(),
            APP_VALIDATOR_COPY_JS.as_bytes(),
            APP_ROUNDS_JS.as_bytes(),
            APP_TYCHO_NODES_JS.as_bytes(),
            APP_MAP_JS.as_bytes(),
            APP_MAP_DATA_JS.as_bytes(),
            APP_MAP_CONTROLS_JS.as_bytes(),
            APP_MAP_RENDER_JS.as_bytes(),
            APP_RUNTIME_JS.as_bytes(),
            APP_ENTRY_JS.as_bytes(),
            EVERSCALE_LOGO_SVG.as_bytes(),
            TYCHO_LOGO_SVG.as_bytes(),
            TON_LOGO_SVG.as_bytes(),
            SMOKING_MAN_PNG,
            JOKES_JSON.as_bytes(),
        ])
    )
}

fn fnv1a64(parts: &[&[u8]]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    let mut hash = OFFSET;
    for part in parts {
        for byte in *part {
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
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("image/png")),
            (header::CACHE_CONTROL, ASSET_CACHE_CONTROL),
        ],
        SMOKING_MAN_PNG,
    )
}

pub(super) async fn jokes_json() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, ASSET_CACHE_CONTROL),
        ],
        JOKES_JSON,
    )
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
