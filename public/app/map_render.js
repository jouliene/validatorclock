async function loadValidatorMap() {
  await loadValidatorMapNodes();

  if (validatorMapLoaded) {
    if (validatorMap) {
      validatorMap.resize();
    }
    return;
  }

  showValidatorMapStatus("Loading map", "loading");
  await ensureMapLibre();
  renderValidatorMap();
  validatorMapLoaded = true;
  showValidatorMapStatus(
    validatorMapFeatures().length ? "" : `No mapped ${currentMapChainName()} validators in the current set`,
    "empty"
  );
}

function ensureMapLibre() {
  if (window.maplibregl) {
    return Promise.resolve();
  }

  if (mapLibrePromise) {
    return mapLibrePromise;
  }

  mapLibrePromise = new Promise((resolve, reject) => {
    if (!document.getElementById("maplibreCss")) {
      const link = document.createElement("link");
      link.id = "maplibreCss";
      link.rel = "stylesheet";
      link.href = MAPLIBRE_CSS_URL;
      document.head.appendChild(link);
    }

    const script = document.createElement("script");
    script.id = "maplibreJs";
    script.src = MAPLIBRE_JS_URL;
    script.async = true;
    script.onload = () => resolve();
    script.onerror = () => reject(new Error("MapLibre assets failed to load"));
    document.head.appendChild(script);
  });

  return mapLibrePromise;
}

function renderValidatorMap() {
  const container = $("validatorMapCanvas");
  if (!container || !window.maplibregl) {
    return;
  }

  const features = validatorMapFeatures();

  validatorMap = new maplibregl.Map({
    container,
    style: {
      version: 8,
      sources: {
        "carto-dark": {
          type: "raster",
          tiles: [
            "https://a.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png",
            "https://b.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png",
            "https://c.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png",
            "https://d.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png"
          ],
          tileSize: 256,
          attribution: "OpenStreetMap CARTO"
        }
      },
      layers: [
        {
          id: "carto-dark-layer",
          type: "raster",
          source: "carto-dark",
          minzoom: 0,
          maxzoom: 19,
          paint: {
            "raster-opacity": 0.94
          }
        }
      ]
    },
    center: [5, 23],
    zoom: 1.75,
    minZoom: 1.35,
    maxZoom: VALIDATOR_MAP_MAX_ZOOM,
    pitch: 0,
    bearing: 0,
    renderWorldCopies: false,
    attributionControl: false
  });

  validatorMap.addControl(new maplibregl.NavigationControl({
    showCompass: false,
    visualizePitch: false
  }), "bottom-right");

  validatorMap.dragRotate.disable();
  validatorMap.touchZoomRotate.disableRotation();
  validatorMap.setMaxBounds([
    [-179.9, -58],
    [179.9, 75]
  ]);

  validatorMap.on("load", () => addValidatorNodeLayers(features));
}

function refreshValidatorMapSource() {
  const source = validatorMap?.getSource("nodes");
  const features = validatorMapFeatures();
  if (!source) {
    showValidatorMapStatus(
      features.length ? "" : `No mapped ${currentMapChainName()} validators in the current set`,
      "empty"
    );
    return;
  }

  source.setData({
    type: "FeatureCollection",
    features
  });
  showValidatorMapStatus(
    features.length ? "" : `No mapped ${currentMapChainName()} validators in the current set`,
    "empty"
  );
}

function addValidatorNodeLayers(features) {
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

  validatorMap.addLayer({
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
  });

  validatorMap.addLayer({
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
  });

  validatorMap.addLayer({
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
  });

  validatorMap.addLayer({
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
  });

  validatorMap.addLayer({
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
  });

  validatorMap.addLayer({
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
  });

  validatorMap.on("mouseenter", "node-points", () => {
    validatorMap.getCanvas().style.cursor = "pointer";
  });

  validatorMap.on("mouseleave", "node-points", () => {
    validatorMap.getCanvas().style.cursor = "grab";
  });

  validatorMap.on("mouseenter", "clusters", () => {
    validatorMap.getCanvas().style.cursor = "pointer";
  });

  validatorMap.on("mouseleave", "clusters", () => {
    validatorMap.getCanvas().style.cursor = "grab";
  });

  validatorMap.on("click", "node-points", (event) => {
    const feature = event.features[0];

    trackValidatorMapPopup(new maplibregl.Popup({
      closeButton: true,
      closeOnClick: true,
      maxWidth: "720px"
    }))
      .setLngLat(feature.geometry.coordinates)
      .setHTML(locationPopupHtml(feature.properties))
      .addTo(validatorMap);
  });

  validatorMap.on("click", "clusters", async (event) => {
    event.preventDefault();

    const clusterFeatures = validatorMap.queryRenderedFeatures(event.point, {
      layers: ["clusters"]
    });

    if (!clusterFeatures.length) {
      return;
    }

    const cluster = clusterFeatures[0];
    const clusterId = cluster.properties.cluster_id;
    const source = validatorMap.getSource("nodes");
    const pointCount = Number(cluster.properties.point_count || 0);
    const totalNodes = Number(cluster.properties.total_nodes || pointCount);
    const leaves = await source.getClusterLeaves(clusterId, Math.max(pointCount, 1), 0);
    const bounds = boundsForFeatures(leaves);
    const zoom = Math.min(
      VALIDATOR_MAP_MAX_ZOOM,
      Math.max(validatorMap.getZoom() + 1.25, await source.getClusterExpansionZoom(clusterId))
    );

    closeValidatorMapPopups();
    if (bounds) {
      validatorMap.fitBounds(bounds, {
        padding: clusterFitPadding(),
        maxZoom: zoom,
        duration: 450
      });
    } else {
      validatorMap.easeTo({
        center: cluster.geometry.coordinates,
        zoom,
        duration: 450
      });
    }

    if (clusterLeavesAreTight(leaves)) {
      window.setTimeout(() => {
        trackValidatorMapPopup(new maplibregl.Popup({
          closeButton: true,
          closeOnClick: true,
          maxWidth: "720px"
        }))
          .setLngLat(cluster.geometry.coordinates)
          .setHTML(clusterLeavesPopupHtml(pointCount, totalNodes, leaves))
          .addTo(validatorMap);
      }, 480);
    }
  });

  validatorMap.on("contextmenu", "clusters", (event) => {
    event.preventDefault();

    const cluster = event.features[0];
    const pointCount = cluster.properties.point_count;
    const totalNodes = cluster.properties.total_nodes || pointCount;

    trackValidatorMapPopup(new maplibregl.Popup({
      closeButton: true,
      closeOnClick: true,
      maxWidth: "420px"
    }))
      .setLngLat(cluster.geometry.coordinates)
      .setHTML(clusterPopupHtml(pointCount, totalNodes))
      .addTo(validatorMap);
  });

  resetValidatorMapView(0);
}

function resetValidatorMapView(duration = 450) {
  if (!validatorMap) {
    return;
  }

  closeValidatorMapPopups();
  validatorMap.fitBounds(VALIDATOR_MAP_DEFAULT_BOUNDS, {
    ...VALIDATOR_MAP_DEFAULT_OPTIONS,
    duration
  });
}
