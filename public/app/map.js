const TYCHO_MAP_CHAIN_ID = "tycho-testnet";
const MAP_CHAIN_IDS = new Set([TYCHO_MAP_CHAIN_ID, "ton", "everscale"]);
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
const TYCHO_MAP_MAX_ZOOM = 17;
const TYCHO_MAP_CLUSTER_MAX_ZOOM = 15;
const TYCHO_MAP_CLUSTER_RADIUS = 24;
const TYCHO_MAP_LOCATION_PRECISION = 4;

let mapLibrePromise = null;
let tychoMap = null;
let tychoMapLoaded = false;
let tychoMapNodes = null;
let tychoMapNodesChainId = null;
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

  const available = mapAvailableForChain(state.selectedChainId);
  updateTychoMapTitle();
  controls.hidden = false;
  toggle.disabled = !available;
  toggle.removeAttribute("title");
  delete toggle.dataset.tooltip;
  toggle.setAttribute("aria-label", available ? "Show validator map" : "Map is not available for this chain");
  toggle.setAttribute("aria-disabled", String(!available));

  if (!available && state.tychoMapOpen) {
    setTychoMapOpen(false);
  } else {
    syncTychoMapPanel();
  }
}

function setTychoMapOpen(open) {
  state.tychoMapOpen = Boolean(open) && mapAvailableForChain(state.selectedChainId);
  syncTychoMapPanel();

  if (!state.tychoMapOpen) {
    closeTychoMapPopups();
    return;
  }

  loadTychoMap().catch((error) => {
    console.warn("Unable to load Tycho map", error);
    showTychoMapStatus(formatTychoMapError(error), "error");
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

function resetTychoMapForChainChange(previousChainId, nextChainId) {
  if (previousChainId === nextChainId) {
    return;
  }

  closeTychoMapPopups();
  if (tychoMap) {
    resetTychoMapView(0);
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

  showTychoMapStatus("Loading map", "loading");
  await ensureMapLibre();
  renderTychoMap();
  tychoMapLoaded = true;
  showTychoMapStatus(
    tychoMapFeatures().length ? "" : `No mapped ${currentMapChainName()} validators in the current set`,
    "empty"
  );
}

async function loadTychoMapNodes() {
  if (tychoMapNodes && tychoMapNodesChainId === state.selectedChainId) {
    return tychoMapNodes;
  }

  return refreshTychoMapNodesForSnapshot();
}

async function refreshTychoMapNodesForSnapshot() {
  const chainId = state.selectedChainId;
  if (!mapAvailableForChain(chainId)) {
    state.tychoMappedPeers = null;
    tychoMapNodes = null;
    tychoMapNodesChainId = null;
    return [];
  }

  try {
    const nodes = await fetchJson(`/api/chains/${encodeURIComponent(chainId)}/map`);
    tychoMapNodes = Array.isArray(nodes) ? nodes : [];
  } catch (error) {
    if (chainId === TYCHO_MAP_CHAIN_ID) {
      console.warn("Using bundled Tycho map nodes", error);
      tychoMapNodes = Array.isArray(window.TYCHO_NODES) ? window.TYCHO_NODES : [];
    } else {
      console.warn(`Unable to load ${chainId} map nodes`, error);
      tychoMapNodes = [];
    }
  }

  tychoMapNodesChainId = chainId;
  tychoMapNodes = filterTychoMapNodesToCurrentValidators(tychoMapNodes);
  state.tychoMappedPeers = tychoMappedPeerSet(tychoMapNodes);
  updateTychoMapTitle();
  updateTychoMapSummary();
  refreshTychoMapSource();
  return tychoMapNodes;
}

function mapAvailableForChain(chainId) {
  return MAP_CHAIN_IDS.has(chainId);
}

function currentMapChain() {
  return state.chains.find((chain) => chain.id === state.selectedChainId) || null;
}

function currentMapChainName() {
  return currentMapChain()?.name || state.selectedChainId || "Validator";
}

function updateTychoMapTitle() {
  const title = $("tychoMapTitleText");
  const panel = $("tychoMapPanel");
  const chainName = currentMapChainName();
  const label = `${chainName} Validator Map`;
  if (title) {
    title.textContent = label;
  }
  panel?.setAttribute("aria-label", `${chainName} validator world map`);
}

function tychoMappedPeerSet(nodes) {
  return new Set(
    (Array.isArray(nodes) ? nodes : [])
      .map((node) => String(node.peer || "").toLowerCase())
      .filter(Boolean)
  );
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

  const features = tychoMapFeatures();

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
    maxZoom: TYCHO_MAP_MAX_ZOOM,
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

function tychoMapFeatures() {
  const rawNodes = tychoMapNodes || [];
  const locationGroups = groupNodesByLocation(rawNodes);
  return locationGroups.map((group) => ({
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
}

function refreshTychoMapSource() {
  const source = tychoMap?.getSource("nodes");
  const features = tychoMapFeatures();
  if (!source) {
    showTychoMapStatus(
      features.length ? "" : `No mapped ${currentMapChainName()} validators in the current set`,
      "empty"
    );
    return;
  }

  source.setData({
    type: "FeatureCollection",
    features
  });
  showTychoMapStatus(
    features.length ? "" : `No mapped ${currentMapChainName()} validators in the current set`,
    "empty"
  );
}

function addTychoNodeLayers(features) {
  tychoMap.addSource("nodes", {
    type: "geojson",
    data: {
      type: "FeatureCollection",
      features
    },
    cluster: true,
    clusterMaxZoom: TYCHO_MAP_CLUSTER_MAX_ZOOM,
    clusterRadius: TYCHO_MAP_CLUSTER_RADIUS,
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
    event.preventDefault();

    const clusterFeatures = tychoMap.queryRenderedFeatures(event.point, {
      layers: ["clusters"]
    });

    if (!clusterFeatures.length) {
      return;
    }

    const cluster = clusterFeatures[0];
    const clusterId = cluster.properties.cluster_id;
    const source = tychoMap.getSource("nodes");
    const pointCount = Number(cluster.properties.point_count || 0);
    const totalNodes = Number(cluster.properties.total_nodes || pointCount);
    const leaves = await source.getClusterLeaves(clusterId, Math.max(pointCount, 1), 0);
    const bounds = boundsForFeatures(leaves);
    const zoom = Math.min(
      TYCHO_MAP_MAX_ZOOM,
      Math.max(tychoMap.getZoom() + 1.25, await source.getClusterExpansionZoom(clusterId))
    );

    closeTychoMapPopups();
    if (bounds) {
      tychoMap.fitBounds(bounds, {
        padding: clusterFitPadding(),
        maxZoom: zoom,
        duration: 450
      });
    } else {
      tychoMap.easeTo({
        center: cluster.geometry.coordinates,
        zoom,
        duration: 450
      });
    }

    if (clusterLeavesAreTight(leaves)) {
      window.setTimeout(() => {
        trackTychoPopup(new maplibregl.Popup({
          closeButton: true,
          closeOnClick: true,
          maxWidth: "720px"
        }))
          .setLngLat(cluster.geometry.coordinates)
          .setHTML(clusterLeavesPopupHtml(pointCount, totalNodes, leaves))
          .addTo(tychoMap);
      }, 480);
    }
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
    const lat = Number(node.lat);
    const lon = Number(node.lon);
    if (!Number.isFinite(lat) || !Number.isFinite(lon)) {
      continue;
    }
    const key = [
      normalizeLocationPart(node.city),
      normalizeLocationPart(node.country),
      roundedLocationCoordinate(lat),
      roundedLocationCoordinate(lon)
    ].join("|");

    if (!grouped.has(key)) {
      grouped.set(key, {
        lat: 0,
        lon: 0,
        city: node.city,
        country: node.country,
        isp: node.isp,
        nodes: []
      });
    }

    const group = grouped.get(key);
    group.lat += lat;
    group.lon += lon;
    group.nodes.push(node);
  }

  return Array.from(grouped.values()).map((group) => ({
    ...group,
    lat: group.lat / Math.max(group.nodes.length, 1),
    lon: group.lon / Math.max(group.nodes.length, 1)
  }));
}

function roundedLocationCoordinate(value) {
  return Number(value).toFixed(TYCHO_MAP_LOCATION_PRECISION);
}

function normalizeLocationPart(value) {
  return String(value || "").trim().toLowerCase();
}

function boundsForFeatures(features) {
  const bounds = new maplibregl.LngLatBounds();
  let hasCoordinates = false;
  for (const feature of Array.isArray(features) ? features : []) {
    const coordinates = feature?.geometry?.coordinates;
    if (!Array.isArray(coordinates) || coordinates.length < 2) {
      continue;
    }
    const lon = Number(coordinates[0]);
    const lat = Number(coordinates[1]);
    if (!Number.isFinite(lat) || !Number.isFinite(lon)) {
      continue;
    }
    bounds.extend([lon, lat]);
    hasCoordinates = true;
  }
  return hasCoordinates ? bounds : null;
}

function clusterFitPadding() {
  const compact = tychoMap?.getContainer()?.clientWidth < 700;
  return compact
    ? { top: 70, right: 48, bottom: 70, left: 48 }
    : { top: 92, right: 110, bottom: 92, left: 110 };
}

function clusterLeavesAreTight(features) {
  const bounds = boundsForFeatures(features);
  if (!bounds) {
    return false;
  }
  const west = bounds.getWest();
  const east = bounds.getEast();
  const south = bounds.getSouth();
  const north = bounds.getNorth();
  return Math.abs(east - west) <= 0.0002 && Math.abs(north - south) <= 0.0002;
}

function updateTychoMapSummary() {
  const summary = $("tychoMapSummary");
  if (!summary) {
    return;
  }

  let nodes = [];
  if (tychoMapNodes && tychoMapNodesChainId === state.selectedChainId) {
    nodes = tychoMapNodes;
  } else if (state.selectedChainId === TYCHO_MAP_CHAIN_ID && Array.isArray(window.TYCHO_NODES)) {
    nodes = window.TYCHO_NODES;
  }
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
    ${nodeTableHtml(nodes)}
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

function clusterLeavesPopupHtml(clusterPointCount, totalNodeCount, leaves) {
  const nodes = nodesFromClusterLeaves(leaves);
  return `
    <div class="popup-title">Node cluster</div>
    <div class="popup-muted">${totalNodeCount} validators / ${clusterPointCount} locations</div>
    ${nodeTableHtml(nodes)}
  `;
}

function nodesFromClusterLeaves(leaves) {
  return (Array.isArray(leaves) ? leaves : []).flatMap((leaf) => {
    try {
      return JSON.parse(leaf?.properties?.nodes_json || "[]");
    } catch (error) {
      return [];
    }
  });
}

function nodeTableHtml(nodes) {
  const safeNodes = Array.isArray(nodes) ? nodes : [];
  if (!safeNodes.length) {
    return "";
  }

  return `
    <div class="popup-node-list">
      <table class="popup-node-table">
        <colgroup>
          <col class="popup-col-ip">
          <col class="popup-col-isp">
          <col class="popup-col-peer">
        </colgroup>
        <thead>
          <tr>
            <th scope="col">IP</th>
            <th scope="col">ISP</th>
            <th scope="col">Validator pubkey</th>
          </tr>
        </thead>
        <tbody>
          ${safeNodes.map((node) => `
          <tr>
            <td class="popup-ip">${escapeHtml(node.ip)}</td>
            <td class="popup-isp">${escapeHtml(node.isp)}</td>
            <td class="popup-peer-cell">
              <code class="popup-peer" title="${escapeHtml(node.peer)}">${escapeHtml(node.peer)}</code>
            </td>
          </tr>
          `).join("")}
        </tbody>
      </table>
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

function showTychoMapStatus(message, stateName = "info") {
  const status = $("tychoMapStatus");
  if (!status) {
    return;
  }
  status.hidden = !message;
  status.dataset.state = stateName;
  status.textContent = message || "";
}

function formatTychoMapError(error) {
  const message = String(error?.message || error || "");
  if (/webgl|context/i.test(message)) {
    return "Map rendering is unavailable in this browser session. WebGL could not be initialized.";
  }

  if (/assets failed to load/i.test(message)) {
    return "Map assets could not be loaded. Check the network connection and try again.";
  }

  return "Map could not be loaded. Try again in a moment.";
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
