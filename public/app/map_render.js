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
    style: validatorMapBaseStyle(),
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
