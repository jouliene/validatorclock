function loadValidatorMap() {
  // Opening the map twice in quick succession used to build two maps on one
  // container and leak the first. The second call waits for the first.
  validatorMapLoading =
    validatorMapLoading ||
    buildValidatorMap().finally(() => {
      validatorMapLoading = null;
    });

  return validatorMapLoading;
}

async function buildValidatorMap() {
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

  const loading = new Promise((resolve, reject) => {
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

  // A failed load must not be remembered. Holding on to the rejected promise
  // meant one bad moment at the CDN left the map unable to load for the rest
  // of the session, however long the network had been back.
  mapLibrePromise = loading.catch((error) => {
    mapLibrePromise = null;
    throw error;
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
    script.onerror = () => {
      // A tag that failed is taken back out: left in place, the next attempt
      // would find it by id and report the library as already loaded.
      script.remove();
      reject(new Error(`${url} failed to load`));
    };
    document.head.appendChild(script);
  });
}

function renderValidatorMap() {
  const container = $("validatorMapCanvas");
  if (!container || !window.maplibregl) {
    return;
  }

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
    // The style is fetched over the network and the selected chain can change
    // while it arrives, so the nodes are read here rather than captured when
    // the map was created - otherwise the previous chain's nodes get drawn.
    addValidatorNodeLayers(validatorMapFeatures());
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
