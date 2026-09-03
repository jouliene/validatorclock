use axum::extract::Path;
use axum::http::StatusCode;
use axum::http::header::{self, HeaderValue};
use axum::response::{Html, IntoResponse, Response};
use std::sync::LazyLock;

mod embedded;
mod version;

pub(in crate::server) use embedded::BASEMAP_STYLE_JSON;
use embedded::{
    APP_JS_PARTS, EVERSCALE_LOGO_SVG, INDEX_HTML, JOKES_JSON, MAPLIBRE_CSS, MAPLIBRE_JS,
    PMTILES_JS, PORTRAIT_IMAGES, SMOKING_MAN_PNG, STATS_HTML, STATS_JS_PARTS, STYLES_CSS_PARTS,
    TON_LOGO_SVG, TYCHO_LOGO_SVG,
};

pub(super) use version::asset_version;

/// The map's own source, so a test can check that every vendored file it asks
/// for is one the router serves.
#[cfg(test)]
pub(in crate::server) fn map_js_source() -> &'static str {
    embedded::APP_MAP_JS
}

const ASSET_CACHE_CONTROL: HeaderValue =
    HeaderValue::from_static("public, max-age=31536000, immutable");
const PRIVATE_ASSET_CACHE_CONTROL: HeaderValue =
    HeaderValue::from_static("private, max-age=31536000, immutable");

static INDEX_PAGE: LazyLock<String> = LazyLock::new(|| render_page(INDEX_HTML));
static STATS_PAGE: LazyLock<String> = LazyLock::new(|| render_page(STATS_HTML));
static APP_JS_BUNDLE: LazyLock<String> = LazyLock::new(|| APP_JS_PARTS.join("\n\n"));
static STATS_JS_BUNDLE: LazyLock<String> = LazyLock::new(|| STATS_JS_PARTS.join("\n\n"));
static STYLES_BUNDLE: LazyLock<String> = LazyLock::new(|| STYLES_CSS_PARTS.join("\n"));

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
        STATS_JS_BUNDLE.as_str(),
    )
}

fn render_page(template: &str) -> String {
    template
        .replace("__ASSET_VERSION__", asset_version())
        .replace("__APP_VERSION__", env!("CARGO_PKG_VERSION"))
}

pub(super) async fn styles() -> impl IntoResponse {
    (
        asset_response_headers("text/css; charset=utf-8"),
        STYLES_BUNDLE.as_str(),
    )
}

pub(super) async fn app_js() -> impl IntoResponse {
    (
        asset_response_headers("application/javascript; charset=utf-8"),
        APP_JS_BUNDLE.as_str(),
    )
}

pub(super) async fn maplibre_js() -> impl IntoResponse {
    (
        asset_response_headers("application/javascript; charset=utf-8"),
        MAPLIBRE_JS,
    )
}

pub(super) async fn maplibre_css() -> impl IntoResponse {
    (
        asset_response_headers("text/css; charset=utf-8"),
        MAPLIBRE_CSS,
    )
}

pub(super) async fn pmtiles_js() -> impl IntoResponse {
    (
        asset_response_headers("application/javascript; charset=utf-8"),
        PMTILES_JS,
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
        let mut scripts = scripts_in("public/app");
        scripts.extend(scripts_in("public/shared"));
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
            "APP_JS_PARTS should hold every public/app and public/shared script plus public/app.js"
        );
    }

    #[test]
    fn every_stylesheet_is_bundled() {
        let sheets = std::fs::read_dir(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("public/styles"),
        )
        .expect("public/styles should be readable")
        .map(|entry| entry.expect("directory entry should be readable").path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("css"))
        .collect::<Vec<_>>();

        let missing = sheets
            .iter()
            .filter(|path| {
                let source = std::fs::read_to_string(path).expect("stylesheet should be readable");
                !STYLES_CSS_PARTS
                    .iter()
                    .any(|part| part.trim_end() == source.trim_end())
            })
            .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
            .collect::<Vec<_>>();

        assert!(
            missing.is_empty(),
            "these stylesheets never reach the browser: {missing:?}"
        );
        assert_eq!(STYLES_CSS_PARTS.len(), sheets.len());
    }

    #[test]
    fn the_stats_page_ships_the_shared_analytics_client() {
        assert!(STATS_JS_BUNDLE.contains("function sendAnalyticsEvent"));
        assert!(STATS_JS_BUNDLE.contains("function formatAnalyticsNumber"));
        assert!(STATS_JS_BUNDLE.contains("/stats/visitors"));
    }

    // The bundle shares one global scope across 60 files, so two files can
    // declare the same name and the last one silently wins.
    #[test]
    fn bundled_scripts_do_not_declare_the_same_name_twice() {
        for (bundle, parts) in [("app.js", APP_JS_PARTS), ("stats", STATS_JS_PARTS)] {
            let mut seen = std::collections::BTreeMap::<String, usize>::new();
            for part in parts {
                for name in top_level_declarations(part) {
                    *seen.entry(name).or_default() += 1;
                }
            }

            let clashes = seen
                .into_iter()
                .filter(|(_, count)| *count > 1)
                .map(|(name, count)| format!("{name} ({count}x)"))
                .collect::<Vec<_>>();

            assert!(
                clashes.is_empty(),
                "the {bundle} bundle declares these names more than once: {clashes:?}"
            );
        }
    }

    // Data reaches the page as text nodes; the one place allowed to assign
    // markup is the shared DOM helper, which only ever takes constant icons.
    #[test]
    fn only_the_dom_helper_assigns_markup() {
        let offenders = APP_JS_PARTS
            .iter()
            .chain(STATS_JS_PARTS.iter())
            .filter(|part| !part.contains("// Element builder shared by both pages."))
            .flat_map(|part| part.lines())
            .filter(|line| {
                let code = line.split("//").next().unwrap_or_default();
                code.contains("innerHTML") || code.contains("insertAdjacentHTML")
            })
            .map(str::trim)
            .collect::<Vec<_>>();

        assert!(
            offenders.is_empty(),
            "render data through the DOM instead of markup, or use setStaticMarkup for constant icons: {offenders:?}"
        );
    }

    fn top_level_declarations(source: &str) -> Vec<String> {
        source
            .lines()
            .filter_map(|line| {
                let rest = ["async function ", "function ", "const ", "let ", "class "]
                    .iter()
                    .find_map(|keyword| line.strip_prefix(*keyword))?;
                let name = rest
                    .split(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '$'))
                    .next()?;
                (!name.is_empty()).then(|| name.to_owned())
            })
            .collect()
    }

    // A label layer that asks for a font the basemap style does not serve gets
    // a 404, and that failure takes down every layer sharing its source: the
    // node circles disappear with the labels. The font stack therefore comes
    // from the module that picks the style.
    #[test]
    fn label_layers_take_their_font_from_the_basemap_module() {
        let layers = APP_JS_PARTS
            .iter()
            .find(|part| part.contains("function validatorNodeLayers("))
            .expect("the map layer module should be bundled");

        for line in layers.lines() {
            let Some(font) = line.split("\"text-font\":").nth(1) else {
                continue;
            };
            assert!(
                font.contains("validatorMapFontStack()"),
                "name the font through validatorMapFontStack(), not inline: {}",
                line.trim()
            );
        }

        let style = APP_JS_PARTS
            .iter()
            .find(|part| part.contains("function validatorMapFontStack("))
            .expect("the basemap module should be bundled");
        assert!(
            style.contains("VALIDATOR_MAP_STYLE_URL"),
            "the font stack belongs next to the style it belongs to"
        );
    }

    // Every later refresh hangs off the timers boot() starts. They used to be
    // started only after the first clock load returned, so one blip while the
    // page was opening left it dead until a manual reload.
    #[test]
    fn boot_starts_the_refresh_timers_even_when_the_first_load_fails() {
        let entry = APP_JS_PARTS
            .iter()
            .find(|part| part.contains("async function boot("))
            .expect("the boot module should be bundled");
        let after_catch = entry
            .split_once("} catch (error) {")
            .expect("boot should handle a failed load")
            .1;

        assert!(
            after_catch.contains("} finally {"),
            "boot should finish its setup in a finally block"
        );
        assert!(
            after_catch.contains("startTimers()"),
            "startTimers() belongs after the catch, so a failed first load still polls"
        );
    }

    // A request with no deadline never settles, and the callers that share an
    // in-flight request would wait on it for the life of the page: the clock
    // simply stops refreshing, with nothing on screen to say so.
    #[test]
    fn every_request_carries_a_deadline() {
        let api = APP_JS_PARTS
            .iter()
            .find(|part| part.contains("async function fetchJson("))
            .expect("the api module should be bundled");

        assert!(
            api.contains("signal:"),
            "fetchJson should hand fetch a signal that gives up"
        );
        assert!(
            api.contains("AbortSignal.timeout"),
            "the signal should be a timeout, not just an abort handle"
        );
    }

    // The style is fetched over the network and the selected chain can change
    // while it arrives. Reading the nodes when the map was created drew the
    // previous chain's nodes on the new chain's map.
    #[test]
    fn the_map_reads_its_nodes_when_the_style_has_loaded() {
        let render = APP_JS_PARTS
            .iter()
            .find(|part| part.contains("function renderValidatorMap("))
            .expect("the map render module should be bundled");
        let load_handler = render
            .split_once("validatorMap.on(\"load\"")
            .expect("the map should draw its nodes once the style has loaded")
            .1;

        assert!(
            load_handler.contains("addValidatorNodeLayers(validatorMapFeatures())"),
            "read the nodes inside the load handler, not before the map is built"
        );
    }

    // A round with no reward data is serialized as null, and Number(null) is
    // 0 - which passes every finite check downstream. The round the filter
    // meant to drop was averaged into the published rate as a flat zero and
    // charted as a real point at the axis floor.
    #[test]
    fn a_round_with_no_data_is_not_read_as_zero() {
        let format = APP_JS_PARTS
            .iter()
            .find(|part| part.contains("function roundStatsNumber("))
            .expect("the round stats format module should be bundled");
        assert!(
            format.contains("value === null || value === undefined || value === \"\""),
            "roundStatsNumber must refuse the values that coerce to zero"
        );

        for part in APP_JS_PARTS.iter().filter(|part| {
            part.contains("function averageRoundStatsProfitability(")
                || part.contains("key: \"profitability\"")
        }) {
            for line in part.lines() {
                let bare_coercion = line
                    .match_indices("Number(round")
                    .any(|(at, _)| !line[..at].ends_with("roundStats"));
                assert!(
                    !bare_coercion,
                    "read the value through roundStatsNumber, not Number(): {}",
                    line.trim()
                );
            }
        }
    }

    fn scripts_in(directory: &str) -> Vec<std::path::PathBuf> {
        let script_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(directory);
        std::fs::read_dir(&script_dir)
            .unwrap_or_else(|_| panic!("{directory} should be readable"))
            .map(|entry| entry.expect("directory entry should be readable").path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("js"))
            .collect()
    }
}
