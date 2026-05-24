pub(super) const INDEX_HTML: &str = include_str!("../../../public/index.html");
pub(super) const STYLES_CSS: &str = include_str!("../../../public/styles.css");
pub(super) const APP_STATE_JS: &str = include_str!("../../../public/app/state.js");
pub(super) const APP_API_JS: &str = include_str!("../../../public/app/api.js");
pub(super) const APP_FORMAT_JS: &str = include_str!("../../../public/app/format.js");
pub(super) const APP_CLOCK_JS: &str = include_str!("../../../public/app/clock.js");
pub(super) const APP_METRICS_JS: &str = include_str!("../../../public/app/metrics.js");
pub(super) const APP_VALIDATOR_METADATA_JS: &str =
    include_str!("../../../public/app/validator_metadata.js");
pub(super) const APP_VALIDATOR_TOOLTIPS_JS: &str =
    include_str!("../../../public/app/validator_tooltips.js");
pub(super) const APP_VALIDATOR_LOCATIONS_JS: &str =
    include_str!("../../../public/app/validator_locations.js");
pub(super) const APP_VALIDATOR_SOURCES_JS: &str =
    include_str!("../../../public/app/validator_sources.js");
pub(super) const APP_VALIDATOR_TYPES_JS: &str =
    include_str!("../../../public/app/validator_types.js");
pub(super) const APP_VALIDATORS_JS: &str = include_str!("../../../public/app/validators.js");
pub(super) const APP_VALIDATOR_COPY_JS: &str =
    include_str!("../../../public/app/validator_copy.js");
pub(super) const APP_ROUNDS_JS: &str = include_str!("../../../public/app/rounds.js");
pub(super) const APP_TYCHO_NODES_JS: &str = include_str!("../../../public/app/tycho_nodes.js");
pub(super) const APP_MAP_JS: &str = include_str!("../../../public/app/map.js");
pub(super) const APP_MAP_DATA_JS: &str = include_str!("../../../public/app/map_data.js");
pub(super) const APP_MAP_CONTROLS_JS: &str = include_str!("../../../public/app/map_controls.js");
pub(super) const APP_MAP_RENDER_JS: &str = include_str!("../../../public/app/map_render.js");
pub(super) const APP_RUNTIME_JS: &str = include_str!("../../../public/app/runtime.js");
pub(super) const APP_ENTRY_JS: &str = include_str!("../../../public/app.js");
pub(super) const EVERSCALE_LOGO_SVG: &str = include_str!("../../../public/brands/everscale.svg");
pub(super) const TYCHO_LOGO_SVG: &str = include_str!("../../../public/brands/tycho.svg");
pub(super) const TON_LOGO_SVG: &str = include_str!("../../../public/brands/ton.svg");
pub(super) const SMOKING_MAN_PNG: &[u8] = include_bytes!("../../../public/brands/smoking-man.png");
pub(super) const JOKES_JSON: &str = include_str!("../../../public/jokes.json");

pub(super) const APP_JS_PARTS: &[&str] = &[
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
