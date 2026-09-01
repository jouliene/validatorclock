// CARTO started stamping "API KEY REQUIRED" on keyless tiles. With a key the
// dashboard keeps its original basemap; without one it falls back to
// VersaTiles, which is open and needs no account.
const CARTO_API_KEY = "__CARTO_API_KEY__";
const VERSATILES_STYLE_URL = "https://tiles.versatiles.org/assets/styles/eclipse/style.json";

// The fallback style ships a blue ocean and warm ground; the dashboard is neither.
const VALIDATOR_MAP_PALETTE = {
  background: "#04070b",
  water: "#070b11",
  land: "#191f28",
};

function cartoApiKey() {
  const key = CARTO_API_KEY.trim();
  return key && !key.startsWith("__") ? key : "";
}

function validatorMapBaseStyle() {
  const key = cartoApiKey();
  if (!key) {
    return VERSATILES_STYLE_URL;
  }

  const query = `?api_key=${encodeURIComponent(key)}`;
  return {
    version: 8,
    sources: {
      "carto-dark": {
        type: "raster",
        tiles: ["a", "b", "c", "d"].map(
          (host) => `https://${host}.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png${query}`,
        ),
        tileSize: 256,
        attribution: "OpenStreetMap CARTO",
      },
    },
    layers: [
      {
        id: "carto-dark-layer",
        type: "raster",
        source: "carto-dark",
        minzoom: 0,
        maxzoom: 19,
        paint: { "raster-opacity": 0.94 },
      },
    ],
  };
}

// Repaints the fallback basemap into the dashboard palette. The keyed basemap
// arrives dark already, so it is left alone.
function applyValidatorMapPalette(map) {
  if (cartoApiKey()) {
    return;
  }

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
