const TYCHO_MAP_CHAIN_ID = "tycho-testnet";
const MAPLIBRE_JS_URL = "https://unpkg.com/maplibre-gl@5.9.0/dist/maplibre-gl.js";
const MAPLIBRE_CSS_URL = "https://unpkg.com/maplibre-gl@5.9.0/dist/maplibre-gl.css";
const TYCHO_MAP_DEFAULT_BOUNDS = [
  [-130, -42],
  [120, 68]
];
const TYCHO_MAP_DEFAULT_OPTIONS = {
  padding: 45,
  maxZoom: 2.05
};

let mapLibrePromise = null;
let tychoMap = null;
let tychoMapLoaded = false;
let tychoMapNodes = null;
const tychoMapPopups = new Set();

function setupTychoMapControls() {
  const controls = $("mapControls");
  const toggle = $("tychoMapToggle");
  const reset = $("tychoMapReset");
  if (!controls || !toggle) {
    return;
  }

  controls.addEventListener("click", (event) => {
    const mapToggle = event.target.closest("#tychoMapToggle");
    if (!mapToggle || mapToggle.disabled) {
      return;
    }
    setTychoMapOpen(!state.tychoMapOpen);
  });

  reset?.addEventListener("click", () => {
    resetTychoMapView(450);
  });

  updateTychoMapAvailability();
  updateTychoMapSummary();
}

function updateTychoMapAvailability() {
  const controls = $("mapControls");
  const toggle = $("tychoMapToggle");
  if (!controls || !toggle) {
    return;
  }

  const available = state.selectedChainId === TYCHO_MAP_CHAIN_ID;
  controls.hidden = false;
  toggle.disabled = !available;
  toggle.removeAttribute("title");
  delete toggle.dataset.tooltip;
  toggle.setAttribute("aria-label", available ? "Show Tycho validator map" : "Map is available for Tycho only");
  toggle.setAttribute("aria-disabled", String(!available));

  if (!available && state.tychoMapOpen) {
    setTychoMapOpen(false);
  } else {
    syncTychoMapPanel();
  }
}

function setTychoMapOpen(open) {
  state.tychoMapOpen = Boolean(open) && state.selectedChainId === TYCHO_MAP_CHAIN_ID;
  syncTychoMapPanel();

  if (!state.tychoMapOpen) {
    return;
  }

  loadTychoMap().catch((error) => {
    showTychoMapStatus(`Unable to load map: ${error.message}`);
  });
}

function syncTychoMapPanel() {
  const panel = $("tychoMapPanel");
  const toggle = $("tychoMapToggle");
  const reset = $("tychoMapReset");
  if (!panel || !toggle) {
    return;
  }

  panel.hidden = !state.tychoMapOpen;
  toggle.setAttribute("aria-expanded", String(state.tychoMapOpen));
  if (reset) {
    reset.disabled = !state.tychoMapOpen;
  }

  if (state.tychoMapOpen && tychoMap) {
    window.setTimeout(() => tychoMap.resize(), 0);
  }
}

async function loadTychoMap() {
  await loadTychoMapNodes();

  if (tychoMapLoaded) {
    if (tychoMap) {
      tychoMap.resize();
    }
    return;
  }

  showTychoMapStatus("Loading map");
  await ensureMapLibre();
  renderTychoMap();
  tychoMapLoaded = true;
  showTychoMapStatus("");
}

async function loadTychoMapNodes() {
  if (tychoMapNodes) {
    return tychoMapNodes;
  }

  try {
    const nodes = await fetchJson(`/api/chains/${TYCHO_MAP_CHAIN_ID}/map`);
    tychoMapNodes = Array.isArray(nodes) ? nodes : [];
  } catch (error) {
    console.warn("Using bundled Tycho map nodes", error);
    tychoMapNodes = Array.isArray(window.TYCHO_NODES) ? window.TYCHO_NODES : [];
  }

  tychoMapNodes = filterTychoMapNodesToCurrentValidators(tychoMapNodes);
  updateTychoMapSummary();
  return tychoMapNodes;
}

function filterTychoMapNodesToCurrentValidators(nodes) {
  const validators = state.snapshot?.current_set?.validators;
  if (!Array.isArray(nodes) || !Array.isArray(validators)) {
    return [];
  }

  const activePeers = new Set(
    validators
      .map((validator) => String(validator.public_key || "").toLowerCase())
      .filter(Boolean)
  );

  return nodes.filter((node) => activePeers.has(String(node.peer || "").toLowerCase()));
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

function renderTychoMap() {
  const container = $("tychoNodeMap");
  if (!container || !window.maplibregl) {
    return;
  }

  const rawNodes = tychoMapNodes || (Array.isArray(window.TYCHO_NODES) ? window.TYCHO_NODES : []);
  const locationGroups = groupNodesByLocation(rawNodes);
  const features = locationGroups.map((group) => ({
    type: "Feature",
    geometry: {
      type: "Point",
      coordinates: [group.lon, group.lat]
    },
    properties: {
      city: group.city,
      country: group.country,
      isp: group.isp,
      node_count: group.nodes.length,
      nodes_json: JSON.stringify(group.nodes)
    }
  }));

  tychoMap = new maplibregl.Map({
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
    maxZoom: 10,
    pitch: 0,
    bearing: 0,
    renderWorldCopies: false,
    attributionControl: false
  });

  tychoMap.addControl(new maplibregl.NavigationControl({
    showCompass: false,
    visualizePitch: false
  }), "bottom-right");

  tychoMap.dragRotate.disable();
  tychoMap.touchZoomRotate.disableRotation();
  tychoMap.setMaxBounds([
    [-179.9, -58],
    [179.9, 75]
  ]);

  tychoMap.on("load", () => addTychoNodeLayers(features));
}

function addTychoNodeLayers(features) {
  tychoMap.addSource("nodes", {
    type: "geojson",
    data: {
      type: "FeatureCollection",
      features
    },
    cluster: true,
    clusterMaxZoom: 4,
    clusterRadius: 14,
    clusterProperties: {
      total_nodes: ["+", ["get", "node_count"]]
    }
  });

  tychoMap.addLayer({
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

  tychoMap.addLayer({
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

  tychoMap.addLayer({
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

  tychoMap.addLayer({
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

  tychoMap.addLayer({
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

  tychoMap.addLayer({
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

  tychoMap.on("mouseenter", "node-points", () => {
    tychoMap.getCanvas().style.cursor = "pointer";
  });

  tychoMap.on("mouseleave", "node-points", () => {
    tychoMap.getCanvas().style.cursor = "grab";
  });

  tychoMap.on("mouseenter", "clusters", () => {
    tychoMap.getCanvas().style.cursor = "pointer";
  });

  tychoMap.on("mouseleave", "clusters", () => {
    tychoMap.getCanvas().style.cursor = "grab";
  });

  tychoMap.on("click", "node-points", (event) => {
    const feature = event.features[0];

    trackTychoPopup(new maplibregl.Popup({
      closeButton: true,
      closeOnClick: true,
      maxWidth: "720px"
    }))
      .setLngLat(feature.geometry.coordinates)
      .setHTML(locationPopupHtml(feature.properties))
      .addTo(tychoMap);
  });

  tychoMap.on("click", "clusters", async (event) => {
    const clusterFeatures = tychoMap.queryRenderedFeatures(event.point, {
      layers: ["clusters"]
    });

    if (!clusterFeatures.length) {
      return;
    }

    const cluster = clusterFeatures[0];
    const clusterId = cluster.properties.cluster_id;
    const source = tychoMap.getSource("nodes");
    const zoom = await source.getClusterExpansionZoom(clusterId);

    tychoMap.easeTo({
      center: cluster.geometry.coordinates,
      zoom,
      duration: 450
    });
  });

  tychoMap.on("contextmenu", "clusters", (event) => {
    event.preventDefault();

    const cluster = event.features[0];
    const pointCount = cluster.properties.point_count;
    const totalNodes = cluster.properties.total_nodes || pointCount;

    trackTychoPopup(new maplibregl.Popup({
      closeButton: true,
      closeOnClick: true,
      maxWidth: "420px"
    }))
      .setLngLat(cluster.geometry.coordinates)
      .setHTML(clusterPopupHtml(pointCount, totalNodes))
      .addTo(tychoMap);
  });

  resetTychoMapView(0);
}

function groupNodesByLocation(nodes) {
  const grouped = new Map();

  for (const node of nodes) {
    const key = `${node.lat},${node.lon}`;

    if (!grouped.has(key)) {
      grouped.set(key, {
        lat: node.lat,
        lon: node.lon,
        city: node.city,
        country: node.country,
        isp: node.isp,
        nodes: []
      });
    }

    grouped.get(key).nodes.push(node);
  }

  return Array.from(grouped.values());
}

function updateTychoMapSummary() {
  const summary = $("tychoMapSummary");
  if (!summary) {
    return;
  }

  const nodes = tychoMapNodes || (Array.isArray(window.TYCHO_NODES) ? window.TYCHO_NODES : []);
  const locations = groupNodesByLocation(nodes).length;
  summary.textContent = `${nodes.length} nodes / ${locations} locations`;
}

function locationPopupHtml(properties) {
  let nodes = [];

  try {
    nodes = JSON.parse(properties.nodes_json || "[]");
  } catch (error) {
    nodes = [];
  }

  const nodeCount = Number(properties.node_count || nodes.length || 0);

  return `
    <div class="popup-title">${escapeHtml(properties.city)}, ${escapeHtml(properties.country)}</div>
    <div class="popup-muted">${nodeCount} validator${nodeCount === 1 ? "" : "s"} at this location</div>

    <div class="popup-node-list">
      ${nodes.map((node) => `
      <div class="popup-node-row">
        <div class="popup-ip">${escapeHtml(node.ip)}</div>
        <div class="popup-isp">${escapeHtml(node.isp)}</div>
        <code class="popup-peer">${escapeHtml(node.peer)}</code>
      </div>
      `).join("")}
    </div>
  `;
}

function clusterPopupHtml(clusterPointCount, totalNodeCount) {
  return `
    <div class="popup-title">Node cluster</div>
    <div class="popup-muted">${clusterPointCount} locations</div>
    <div class="popup-node-row">
      <div class="popup-ip">${totalNodeCount} total nodes</div>
      <div class="popup-isp">Cluster</div>
      <div class="popup-peer">Click to zoom in</div>
    </div>
  `;
}

function resetTychoMapView(duration = 450) {
  if (!tychoMap) {
    return;
  }

  closeTychoMapPopups();
  tychoMap.fitBounds(TYCHO_MAP_DEFAULT_BOUNDS, {
    ...TYCHO_MAP_DEFAULT_OPTIONS,
    duration
  });
}

function trackTychoPopup(popup) {
  tychoMapPopups.add(popup);
  popup.on("close", () => {
    tychoMapPopups.delete(popup);
  });
  return popup;
}

function closeTychoMapPopups() {
  for (const popup of Array.from(tychoMapPopups)) {
    popup.remove();
  }
  tychoMapPopups.clear();
}

function showTychoMapStatus(message) {
  const status = $("tychoMapStatus");
  if (!status) {
    return;
  }
  status.hidden = !message;
  status.textContent = message || "";
}

function escapeHtml(value) {
  return String(value ?? "").replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    "\"": "&quot;",
    "'": "&#39;"
  })[char]);
}
