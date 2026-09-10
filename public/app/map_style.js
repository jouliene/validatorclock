// The basemap is served by this app from a pmtiles archive on disk: no tile
// service, no key, no watermark. Its style, fonts and sprite live next to it.
const VALIDATOR_MAP_STYLE_URL = assetPath("/basemap/style.json");
let validatorMapStylePromise = null;

// Label layers must ask for a font the style actually serves. A missing font
// answers 404, and that failure takes down every layer sharing the source,
// including the node circles.
const VALIDATOR_MAP_FONT_STACK = ["Noto Sans Medium"];

function loadValidatorMapStyle() {
  if (!validatorMapStylePromise) {
    validatorMapStylePromise = fetchJson(VALIDATOR_MAP_STYLE_URL).catch((error) => {
      validatorMapStylePromise = null;
      throw error;
    });
  }
  return validatorMapStylePromise;
}

function validatorMapFontStack() {
  return VALIDATOR_MAP_FONT_STACK;
}

// The basemap ships in the dashboard palette already, so nothing to repaint.
function applyValidatorMapPalette() {}
