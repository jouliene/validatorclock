const TYCHO_MAP_CHAIN_ID = "tycho-testnet";
const MAP_CHAIN_IDS = new Set([TYCHO_MAP_CHAIN_ID, "ton", "everscale"]);
const MAPLIBRE_JS_URL = "https://unpkg.com/maplibre-gl@5.9.0/dist/maplibre-gl.js";
const MAPLIBRE_CSS_URL = "https://unpkg.com/maplibre-gl@5.9.0/dist/maplibre-gl.css";
const TYCHO_MAP_DEFAULT_BOUNDS = [
  [-130, -42],
  [120, 68]
];
const TYCHO_MAP_DEFAULT_OPTIONS = {
  padding: 45,
  maxZoom: 2.05
};
const TYCHO_MAP_MAX_ZOOM = 17;
const TYCHO_MAP_CLUSTER_MAX_ZOOM = 15;
const TYCHO_MAP_CLUSTER_RADIUS = 24;
const TYCHO_MAP_CLOSE_LOCATION_RADIUS_KM = 0.25;
const TYCHO_MAP_PROVIDER_CITY_RADIUS_KM = 25;
const TYCHO_MAP_EARTH_RADIUS_KM = 6371.0088;

let mapLibrePromise = null;
let tychoMap = null;
let tychoMapLoaded = false;
let tychoMapNodes = null;
let tychoMapNodesChainId = null;
const tychoMapPopups = new Set();

