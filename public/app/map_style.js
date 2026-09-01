// CARTO stopped serving tiles without a key and offers no free tier, so the
// basemap comes from VersaTiles: open data, open infrastructure, no account.
const VALIDATOR_MAP_STYLE_URL = "https://tiles.versatiles.org/assets/styles/eclipse/style.json";

// Label layers must ask for a font the style actually serves. A missing font
// answers 404, and that failure takes down every layer sharing the source,
// including the node circles.
const VALIDATOR_MAP_FONT_STACK = ["noto_sans_bold"];

// The style ships a blue ocean and warm ground; the dashboard is neither.
const VALIDATOR_MAP_PALETTE = {
  background: "#04070b",
  water: "#070c14",
  land: "#1d242e",
};

function validatorMapBaseStyle() {
  return VALIDATOR_MAP_STYLE_URL;
}

function validatorMapFontStack() {
  return VALIDATOR_MAP_FONT_STACK;
}

// Repaints the basemap into the dashboard palette once its layers exist.
function applyValidatorMapPalette(map) {
  for (const layer of map.getStyle()?.layers || []) {
    if (layer.id === "background") {
      map.setPaintProperty(layer.id, "background-color", VALIDATOR_MAP_PALETTE.background);
    } else if (layer.id.startsWith("water") && layer.type === "fill") {
      map.setPaintProperty(layer.id, "fill-color", VALIDATOR_MAP_PALETTE.water);
    } else if (layer.id.startsWith("water") && layer.type === "line") {
      map.setPaintProperty(layer.id, "line-color", VALIDATOR_MAP_PALETTE.water);
    } else if (layer.id.startsWith("land") && layer.type === "fill") {
      map.setPaintProperty(layer.id, "fill-color", VALIDATOR_MAP_PALETTE.land);
    }
  }
}
