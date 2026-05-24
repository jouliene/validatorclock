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
  updateTychoMapRoundBadge();
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
  updateTychoMapRoundBadge();
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

function updateTychoMapRoundBadge() {
  const badge = $("tychoMapRoundBadge");
  const value = $("tychoMapRoundValue");
  if (!badge || !value) {
    return;
  }

  const roundColor = String(state.snapshot?.current_set?.round_color || "").toLowerCase();
  const available = mapAvailableForChain(state.selectedChainId) && (roundColor === "blue" || roundColor === "green");
  badge.classList.toggle("is-blue", roundColor === "blue");
  badge.classList.toggle("is-green", roundColor === "green");
  if (!available) {
    value.textContent = "-";
    badge.removeAttribute("aria-label");
    return;
  }

  const label = roundColor === "blue" ? "BLUE (EVEN)" : "GREEN (ODD)";
  value.textContent = label;
  badge.setAttribute("aria-label", `Round: ${label}`);
}

function updateTychoMapSummary() {
  const nodeCount = $("tychoMapNodeCount");
  const locationCount = $("tychoMapLocationCount");
  const summary = $("tychoMapSummary");
  if (!nodeCount && !locationCount && !summary) {
    return;
  }

  let nodes = [];
  if (tychoMapNodes && tychoMapNodesChainId === state.selectedChainId) {
    nodes = tychoMapNodes;
  } else if (state.selectedChainId === TYCHO_MAP_CHAIN_ID && Array.isArray(window.TYCHO_NODES)) {
    nodes = window.TYCHO_NODES;
  }
  const locations = groupNodesByLocation(nodes).length;
  if (nodeCount) {
    nodeCount.textContent = String(nodes.length);
  }
  if (locationCount) {
    locationCount.textContent = String(locations);
  }
  if (summary) {
    summary.textContent = `${nodes.length} nodes / ${locations} locations`;
  }
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
