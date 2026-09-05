use crate::etag::Fnv1a64;
use std::sync::LazyLock;

use super::embedded::{
    APP_JS_PARTS, EVERSCALE_LOGO_SVG, INDEX_HTML, JOKES_JSON, SMOKING_MAN_PNG, STATS_HTML,
    STATS_JS, STYLES_CSS_PARTS, TON_LOGO_SVG, TYCHO_LOGO_SVG,
};

pub(in crate::server) fn asset_version() -> &'static str {
    static ASSET_VERSION: LazyLock<String> = LazyLock::new(build_asset_version);
    &ASSET_VERSION
}

fn build_asset_version() -> String {
    let mut hash = Fnv1a64::new();
    hash.update(INDEX_HTML.as_bytes());
    for part in STYLES_CSS_PARTS {
        hash.update(part.as_bytes());
    }
    hash.update(STATS_HTML.as_bytes());
    hash.update(STATS_JS.as_bytes());
    for part in APP_JS_PARTS {
        hash.update(part.as_bytes());
    }
    hash.update(EVERSCALE_LOGO_SVG.as_bytes());
    hash.update(TYCHO_LOGO_SVG.as_bytes());
    hash.update(TON_LOGO_SVG.as_bytes());
    hash.update(SMOKING_MAN_PNG);
    hash.update(JOKES_JSON.as_bytes());

    format!("{}-{:016x}", env!("CARGO_PKG_VERSION"), hash.finish())
}
