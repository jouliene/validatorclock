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
      "circle-color": "#58c9f6",
      "circle-radius": [
        "step",
        ["get", "total_nodes"],
        12,
        4,
        16,
        8,
        20,
        16,
        24
      ],
      "circle-opacity": 0.16,
      "circle-blur": 0.55
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
      "circle-color": "#58c9f6",
      "circle-radius": [
        "step",
        ["get", "total_nodes"],
        5,
        4,
        7,
        8,
        9,
        16,
        11
      ],
      "circle-opacity": 0.78,
      "circle-stroke-width": 1.3,
      "circle-stroke-color": "#d3f1ff"
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
      "text-font": ["Open Sans Semibold", "Arial Unicode MS Bold"],
      "text-allow-overlap": true,
      "text-ignore-placement": true
    },
    paint: {
      "text-color": "#ffffff"
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
      "circle-color": "#58c9f6",
      "circle-radius": [
        "interpolate",
        ["linear"],
        ["zoom"],
        1.35, ["+", 6, ["*", ["get", "node_count"], 1.0]],
        5, ["+", 9, ["*", ["get", "node_count"], 1.2]],
        9, ["+", 12, ["*", ["get", "node_count"], 1.4]]
      ],
      "circle-opacity": 0.14,
      "circle-blur": 0.65
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
      "circle-color": "#58c9f6",
      "circle-radius": [
        "interpolate",
        ["linear"],
        ["zoom"],
        1.35, ["+", 2.8, ["*", ["get", "node_count"], 0.25]],
        5, ["+", 4.2, ["*", ["get", "node_count"], 0.32]],
        9, ["+", 5.8, ["*", ["get", "node_count"], 0.38]]
      ],
      "circle-opacity": 0.90,
      "circle-stroke-width": 1.25,
      "circle-stroke-color": "#d3f1ff"
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
      "text-font": ["Open Sans Semibold", "Arial Unicode MS Bold"],
      "text-allow-overlap": true,
      "text-ignore-placement": true
    },
    paint: {
      "text-color": "#ffffff"
    }
  };
}
