function setupValidatorMapControls() {
  const controls = $("mapControls");
  const toggle = $("validatorMapToggle");
  const reset = $("validatorMapReset");
  if (!controls || !toggle) {
    return;
  }

  controls.addEventListener("click", (event) => {
    const mapToggle = event.target.closest("#validatorMapToggle");
    if (!mapToggle || mapToggle.disabled) {
      return;
    }
    setValidatorMapOpen(!state.validatorMapOpen);
  });

  reset?.addEventListener("click", () => {
    resetValidatorMapView(450);
  });

  updateValidatorMapAvailability();
  updateValidatorMapRoundBadge();
  updateValidatorMapSummary();
}

function updateValidatorMapAvailability() {
  const controls = $("mapControls");
  const toggle = $("validatorMapToggle");
  if (!controls || !toggle) {
    return;
  }

  const available = mapAvailableForChain(state.selectedChainId);
  updateValidatorMapTitle();
  updateValidatorMapRoundBadge();
  controls.hidden = false;
  toggle.disabled = !available;
  toggle.removeAttribute("title");
  delete toggle.dataset.tooltip;
  toggle.setAttribute("aria-label", available ? "Show validator map" : "Map is not available for this chain");
  toggle.setAttribute("aria-disabled", String(!available));

  if (!available && state.validatorMapOpen) {
    setValidatorMapOpen(false);
  } else {
    syncValidatorMapPanel();
  }
}

function setValidatorMapOpen(open) {
  state.validatorMapOpen = Boolean(open) && mapAvailableForChain(state.selectedChainId);
  syncValidatorMapPanel();

  if (!state.validatorMapOpen) {
    closeValidatorMapPopups();
    return;
  }

  loadValidatorMap().catch((error) => {
    console.warn("Unable to load validator map", error);
    showValidatorMapStatus(formatValidatorMapError(error), "error");
  });
}

function syncValidatorMapPanel() {
  const panel = $("validatorMapPanel");
  const toggle = $("validatorMapToggle");
  const reset = $("validatorMapReset");
  if (!panel || !toggle) {
    return;
  }

  panel.hidden = !state.validatorMapOpen;
  toggle.setAttribute("aria-expanded", String(state.validatorMapOpen));
  if (reset) {
    reset.disabled = !state.validatorMapOpen;
  }

  if (state.validatorMapOpen && validatorMap) {
    window.setTimeout(() => validatorMap.resize(), 0);
  }
}

function resetValidatorMapForChainChange(previousChainId, nextChainId) {
  if (previousChainId === nextChainId) {
    return;
  }

  closeValidatorMapPopups();
  if (validatorMap) {
    resetValidatorMapView(0);
  }
}

function updateValidatorMapTitle() {
  const title = $("validatorMapTitleText");
  const panel = $("validatorMapPanel");
  const chainName = currentMapChainName();
  const label = `${chainName} Validator Map`;
  if (title) {
    title.textContent = label;
  }
  panel?.setAttribute("aria-label", `${chainName} validator world map`);
}

function updateValidatorMapRoundBadge() {
  const badge = $("validatorMapRoundBadge");
  const value = $("validatorMapRoundValue");
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

function updateValidatorMapSummary() {
  const nodeCount = $("validatorMapNodeCount");
  const locationCount = $("validatorMapLocationCount");
  const summary = $("validatorMapSummary");
  if (!nodeCount && !locationCount && !summary) {
    return;
  }

  let nodes = [];
  if (validatorMapNodes && validatorMapNodesChainId === state.selectedChainId) {
    nodes = validatorMapNodes;
  } else if (state.selectedChainId === BUNDLED_TYCHO_MAP_CHAIN_ID && Array.isArray(window.TYCHO_NODES)) {
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

function showValidatorMapStatus(message, stateName = "info") {
  const status = $("validatorMapStatus");
  if (!status) {
    return;
  }
  status.hidden = !message;
  status.dataset.state = stateName;
  status.textContent = message || "";
}

function formatValidatorMapError(error) {
  const message = String(error?.message || error || "");
  if (/webgl|context/i.test(message)) {
    return "Map rendering is unavailable in this browser session. WebGL could not be initialized.";
  }

  if (/assets failed to load/i.test(message)) {
    return "Map assets could not be loaded. Check the network connection and try again.";
  }

  return "Map could not be loaded. Try again in a moment.";
}
