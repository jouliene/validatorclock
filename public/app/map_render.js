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
  showValidatorMapEmptyStatus();
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

    loadMapScript("maplibreJs", MAPLIBRE_JS_URL)
      .then(() => loadMapScript("pmtilesJs", PMTILES_JS_URL))
      .then(() => {
        // The basemap is a pmtiles archive this app serves, so MapLibre needs
        // the protocol that reads it over byte ranges.
        maplibregl.addProtocol("pmtiles", new pmtiles.Protocol().tile);
        resolve();
      })
      .catch(reject);
  });

  return mapLibrePromise;
}

function loadMapScript(id, url) {
  return new Promise((resolve, reject) => {
    const existing = document.getElementById(id);
    if (existing) {
      resolve();
      return;
    }

    const script = document.createElement("script");
    script.id = id;
    script.src = url;
    script.async = true;
    script.onload = () => resolve();
    script.onerror = () => reject(new Error(`${url} failed to load`));
    document.head.appendChild(script);
  });
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

  validatorMap.on("load", () => {
    applyValidatorMapPalette(validatorMap);
    addValidatorNodeLayers(features);
  });
}

function refreshValidatorMapSource() {
  const source = validatorMap?.getSource("nodes");
  const features = validatorMapFeatures();
  if (!source) {
    showValidatorMapEmptyStatus(features);
    return;
  }

  source.setData({
    type: "FeatureCollection",
    features
  });
  showValidatorMapEmptyStatus(features);
}

function showValidatorMapEmptyStatus(features = validatorMapFeatures()) {
  if (features.length) {
    showValidatorMapStatus("");
    return;
  }

  const notice = mapNodeResolutionNotice(features.length);
  showValidatorMapStatus(
    notice || `No mapped ${currentMapChainName()} validators in the current set`,
    notice ? "notice" : "empty",
  );
}
