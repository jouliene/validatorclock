const BUNDLED_TYCHO_MAP_CHAIN_ID = "tycho-testnet";
const MAP_CHAIN_IDS = new Set([BUNDLED_TYCHO_MAP_CHAIN_ID, "ton", "everscale"]);
const MAPLIBRE_JS_URL = "https://unpkg.com/maplibre-gl@5.9.0/dist/maplibre-gl.js";
const MAPLIBRE_CSS_URL = "https://unpkg.com/maplibre-gl@5.9.0/dist/maplibre-gl.css";
const VALIDATOR_MAP_DEFAULT_BOUNDS = [
  [-130, -42],
  [120, 68]
];
const VALIDATOR_MAP_DEFAULT_OPTIONS = {
  padding: 45,
  maxZoom: 2.05
};
const VALIDATOR_MAP_MAX_ZOOM = 17;
const VALIDATOR_MAP_CLUSTER_MAX_ZOOM = 15;
const VALIDATOR_MAP_CLUSTER_RADIUS = 24;
const VALIDATOR_MAP_CLOSE_LOCATION_RADIUS_KM = 0.25;
const VALIDATOR_MAP_PROVIDER_CITY_RADIUS_KM = 25;
const VALIDATOR_MAP_EARTH_RADIUS_KM = 6371.0088;

let mapLibrePromise = null;
let validatorMap = null;
let validatorMapLoaded = false;
let validatorMapNodes = null;
let validatorMapNodesChainId = null;
let validatorMapPopupFocusWired = false;
const validatorMapPopups = new Set();
