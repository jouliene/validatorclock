const BUNDLED_TYCHO_MAP_CHAIN_ID = "tycho-testnet";
const MAP_CHAIN_IDS = new Set([BUNDLED_TYCHO_MAP_CHAIN_ID, "ton", "everscale"]);
// Served from here rather than from a CDN. A reader whose network cannot reach
// the CDN had nothing to go on: a script tag whose connection is black-holed
// fires neither `load` nor `error`, so the map sat on "Loading map" for as long
// as the page was open. The version is in the path because these are served
// immutable for a year.
const MAPLIBRE_JS_URL = "/vendor/maplibre-gl-5.9.0.js";
const PMTILES_JS_URL = "/vendor/pmtiles-4.3.0.js";
const MAPLIBRE_CSS_URL = "/vendor/maplibre-gl-5.9.0.css";
// Nothing answering at all still has to end somewhere, or the promise waits for
// as long as the page is open and every later attempt waits behind it.
const MAP_SCRIPT_TIMEOUT_MS = 20000;
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
let validatorMapLibraryReady = false;
let validatorMapLoading = null;
let validatorMap = null;
let validatorMapLoaded = false;
let validatorMapNodes = null;
let validatorMapNodesChainId = null;
let validatorMapPopupFocusWired = false;
const validatorMapPopups = new Set();
