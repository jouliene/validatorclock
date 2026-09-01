// The node marks are the point of this map, so they are drawn as light: a
// saturated core, a bright rim and a soft glow. The basemap labels are painted
// dim in the style for the same reason — nothing on the ground competes with a
// node for attention.
const VALIDATOR_MAP_NODE_COLOR = "#5ed7ff";
const VALIDATOR_MAP_NODE_RIM = "#eaf8ff";
const VALIDATOR_MAP_GLOW_OPACITY = 0.3;

function addValidatorNodeLayers(features) {
  addValidatorNodeSource(features);
  for (const layer of validatorNodeLayers()) {
    validatorMap.addLayer(layer);
  }
  wireValidatorMapInteractions();
  resetValidatorMapView(0);
}

function addValidatorNodeSource(features) {
  validatorMap.addSource("nodes", {
    type: "geojson",
    data: {
      type: "FeatureCollection",
      features
    },
    cluster: true,
    clusterMaxZoom: VALIDATOR_MAP_CLUSTER_MAX_ZOOM,
    clusterRadius: VALIDATOR_MAP_CLUSTER_RADIUS,
    clusterProperties: {
      total_nodes: ["+", ["get", "node_count"]]
    }
  });
}

function validatorNodeLayers() {
  return [
    validatorClusterHaloLayer(),
    validatorClusterLayer(),
    validatorClusterCountLayer(),
    validatorNodeHaloLayer(),
    validatorNodePointLayer(),
    validatorLocationCountLayer(),
  ];
}

function validatorClusterHaloLayer() {
  return {
    id: "clusters-halo",
    type: "circle",
    source: "nodes",
    filter: ["has", "point_count"],
    paint: {
      "circle-color": VALIDATOR_MAP_NODE_COLOR,
      "circle-radius": [
        "step",
        ["get", "total_nodes"],
        13,
        4,
        16,
        8,
        20,
        16,
        24
      ],
      "circle-opacity": VALIDATOR_MAP_GLOW_OPACITY,
      "circle-blur": 0.8
    }
  };
}

function validatorClusterLayer() {
  return {
    id: "clusters",
    type: "circle",
    source: "nodes",
    filter: ["has", "point_count"],
    paint: {
      "circle-color": VALIDATOR_MAP_NODE_COLOR,
      "circle-radius": [
        "step",
        ["get", "total_nodes"],
        6,
        4,
        8,
        8,
        10,
        16,
        12
      ],
      "circle-opacity": 0.95,
      "circle-stroke-width": 1.6,
      "circle-stroke-color": VALIDATOR_MAP_NODE_RIM
    }
  };
}

function validatorClusterCountLayer() {
  return {
    id: "cluster-count",
    type: "symbol",
    source: "nodes",
    filter: ["has", "point_count"],
    layout: {
      "text-field": ["to-string", ["get", "total_nodes"]],
      "text-size": 10,
      "text-font": validatorMapFontStack(),
      "text-allow-overlap": true,
      "text-ignore-placement": true
    },
    paint: {
      "text-color": "#04070b",
      "text-halo-color": VALIDATOR_MAP_NODE_RIM,
      "text-halo-width": 0.6
    }
  };
}

function validatorNodeHaloLayer() {
  return {
    id: "node-halo",
    type: "circle",
    source: "nodes",
    filter: ["!", ["has", "point_count"]],
    paint: {
      "circle-color": VALIDATOR_MAP_NODE_COLOR,
      "circle-radius": [
        "interpolate",
        ["linear"],
        ["zoom"],
        1.35, ["min", ["+", 9, ["*", ["get", "node_count"], 0.9]], 22],
        5, ["min", ["+", 12, ["*", ["get", "node_count"], 1.1]], 26],
        9, ["min", ["+", 15, ["*", ["get", "node_count"], 1.3]], 30]
      ],
      "circle-opacity": VALIDATOR_MAP_GLOW_OPACITY,
      "circle-blur": 0.8
    }
  };
}

function validatorNodePointLayer() {
  return {
    id: "node-points",
    type: "circle",
    source: "nodes",
    filter: ["!", ["has", "point_count"]],
    paint: {
      "circle-color": VALIDATOR_MAP_NODE_COLOR,
      "circle-radius": [
        "interpolate",
        ["linear"],
        ["zoom"],
        1.35, ["min", ["+", 5, ["*", ["get", "node_count"], 0.28]], 11],
        5, ["min", ["+", 6.2, ["*", ["get", "node_count"], 0.34]], 13],
        9, ["min", ["+", 7, ["*", ["get", "node_count"], 0.4]], 15]
      ],
      "circle-opacity": 1,
      "circle-stroke-width": 1.6,
      "circle-stroke-color": VALIDATOR_MAP_NODE_RIM
    }
  };
}

function validatorLocationCountLayer() {
  return {
    id: "location-count",
    type: "symbol",
    source: "nodes",
    filter: [
      "all",
      ["!", ["has", "point_count"]],
      [">", ["get", "node_count"], 1]
    ],
    layout: {
      "text-field": ["to-string", ["get", "node_count"]],
      "text-size": 10,
      "text-font": validatorMapFontStack(),
      "text-allow-overlap": true,
      "text-ignore-placement": true
    },
    paint: {
      "text-color": "#04070b",
      "text-halo-color": VALIDATOR_MAP_NODE_RIM,
      "text-halo-width": 0.6
    }
  };
}
