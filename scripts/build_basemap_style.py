"""Собирает стиль карты в палитре сайта из тёмной темы Protomaps."""
import json, sys

source = sys.argv[1]
target = sys.argv[2]
maxzoom = int(sys.argv[3])

# Токены из public/styles/tokens.css: карта должна выглядеть частью панели.
PALETTE = {
    "background": "#04070b",
    "earth": "#121924",
    "landcover": "#141c27",
    "landuse": "#161f2b",
    "water": "#060a11",
    "buildings": "#1a2432",
    "road_minor": "#1d2735",
    "road_major": "#25313f",
    "road_casing": "#0c1219",
    "rail": "#1b2431",
    "boundary_country": "#3c4a5e",
    "boundary_region": "#28323f",
    "label": "#9fb2c6",
    "label_strong": "#d3dfec",
    "label_water": "#54687e",
    "halo": "#04070b",
}

def fill(layer, color):
    layer.setdefault("paint", {})["fill-color"] = color

def line(layer, color):
    layer.setdefault("paint", {})["line-color"] = color

def label(layer, color):
    paint = layer.setdefault("paint", {})
    paint["text-color"] = color
    paint["text-halo-color"] = PALETTE["halo"]
    paint["text-halo-width"] = 1.4

style = json.load(open(source))
style["name"] = "Validator Clock dark"
style["glyphs"] = "/basemap/fonts/{fontstack}/{range}.pbf"
style["sprite"] = "/basemap/sprite/dark"
style["sources"] = {
    "protomaps": {
        "type": "vector",
        # Явные зумы: архив обрезан, без них MapLibre просит несуществующие тайлы.
        "tiles": ["pmtiles:///basemap/tiles.pmtiles/{z}/{x}/{y}"],
        "minzoom": 0,
        "maxzoom": maxzoom,
        "attribution": "OpenStreetMap Protomaps",
    }
}

dropped = {"pois", "address_label", "roads_labels_minor", "water_waterway_label", "landuse_aerodrome"}
layers = []
for layer in style["layers"]:
    name = layer["id"]
    if name in dropped:
        continue

    if name == "background":
        layer["paint"] = {"background-color": PALETTE["background"]}
    elif name == "earth":
        fill(layer, PALETTE["earth"])
    elif name == "landcover":
        fill(layer, PALETTE["landcover"])
    elif name.startswith("landuse"):
        fill(layer, PALETTE["landuse"])
    elif name.startswith("water"):
        if layer["type"] == "fill":
            fill(layer, PALETTE["water"])
        elif layer["type"] == "line":
            line(layer, PALETTE["water"])
        else:
            label(layer, PALETTE["label_water"])
    elif name == "buildings":
        fill(layer, PALETTE["buildings"])
    elif name == "roads_rail":
        line(layer, PALETTE["rail"])
    elif name.startswith("roads"):
        if layer["type"] != "line":
            continue
        if "casing" in name:
            line(layer, PALETTE["road_casing"])
        elif "highway" in name or "major" in name:
            line(layer, PALETTE["road_major"])
        else:
            line(layer, PALETTE["road_minor"])
    elif name == "boundaries_country":
        line(layer, PALETTE["boundary_country"])
    elif name.startswith("boundaries"):
        line(layer, PALETTE["boundary_region"])
    elif name in ("places_country", "places_region"):
        label(layer, PALETTE["label"])
    elif name.startswith("places"):
        label(layer, PALETTE["label_strong"])
    elif layer["type"] == "symbol":
        label(layer, PALETTE["label"])

    layers.append(layer)

style["layers"] = layers
json.dump(style, open(target, "w"), separators=(",", ":"))
print("слоёв:", len(layers), "| зумы 0 –", maxzoom)
