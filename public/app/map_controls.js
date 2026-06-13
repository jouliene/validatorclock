function setupValidatorMapControls() {
  const controls = $("mapControls");
  const toggle = $("validatorMapToggle");
  const reset = $("validatorMapReset");
  if (!controls || !toggle) {
    return;
  }

  controls.addEventListener("click", (event) => {
    const mapToggle = event.target.closest("#validatorMapToggle");
    if (mapToggle) {
      if (!mapToggle.disabled) {
        setValidatorMapOpen(!state.validatorMapOpen);
      }
      return;
    }

    const statsToggle = event.target.closest("#nodeStatsToggle");
    if (statsToggle && !statsToggle.disabled) {
      setNodeStatsOpen(!state.nodeStatsOpen);
    }
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
  const nodeStatsToggle = $("nodeStatsToggle");
  if (!controls || !toggle) {
    return;
  }

  const available = mapAvailableForChain(state.selectedChainId);
  updateValidatorMapTitle();
  updateNodeStatsTitle();
  updateValidatorMapRoundBadge();
  controls.hidden = false;
  syncMapControlButton(toggle, available, "Show node map", "Node map is not available for this chain");
  if (nodeStatsToggle) {
    syncMapControlButton(
      nodeStatsToggle,
      available,
      "Show validator node statistics",
      "Node statistics are not available for this chain",
    );
  }

  if (!available && state.validatorMapOpen) {
    setValidatorMapOpen(false);
  } else {
    syncValidatorMapPanel();
  }

  if (!available && state.nodeStatsOpen) {
    setNodeStatsOpen(false);
  } else {
    syncNodeStatsPanel();
  }
}

function syncMapControlButton(button, available, enabledLabel, disabledLabel) {
  button.disabled = !available;
  button.removeAttribute("title");
  delete button.dataset.tooltip;
  button.setAttribute("aria-label", available ? enabledLabel : disabledLabel);
  button.setAttribute("aria-disabled", String(!available));
}

function setValidatorMapOpen(open) {
  const willOpen = Boolean(open) && mapAvailableForChain(state.selectedChainId);
  if (willOpen && state.roundStatsOpen) {
    setRoundStatsOpen(false);
  }
  if (willOpen && state.nodeStatsOpen) {
    setNodeStatsOpen(false);
  }

  state.validatorMapOpen = willOpen;
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

function setNodeStatsOpen(open) {
  const willOpen = Boolean(open) && mapAvailableForChain(state.selectedChainId);
  if (willOpen && state.validatorMapOpen) {
    setValidatorMapOpen(false);
  }
  if (willOpen && state.roundStatsOpen) {
    setRoundStatsOpen(false);
  }

  state.nodeStatsOpen = willOpen;
  syncNodeStatsPanel();
  if (!state.nodeStatsOpen) {
    clearNodeStatsLoadingTimer();
    state.nodeStatsRenderKey = null;
    return;
  }

  state.nodeStatsRenderKey = null;
  loadSelectedNodeStats(false).catch((error) => {
    console.warn("Unable to load validator node statistics", error);
    renderNodeStatsError(error);
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

function syncNodeStatsPanel() {
  const panel = $("nodeStatsPanel");
  const toggle = $("nodeStatsToggle");
  if (!panel || !toggle) {
    return;
  }

  panel.hidden = !state.nodeStatsOpen;
  toggle.setAttribute("aria-expanded", String(state.nodeStatsOpen));
  toggle.setAttribute(
    "aria-label",
    state.nodeStatsOpen ? "Hide validator node statistics" : "Show validator node statistics",
  );
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
  const label = `${chainName} Validator Node Map`;
  if (title) {
    title.textContent = label;
  }
  panel?.setAttribute("aria-label", `${chainName} validator node map`);
}

function updateValidatorMapRoundBadge() {
  const badge = $("validatorMapRoundBadge");
  const value = $("validatorMapRoundValue");
  if (!badge || !value) {
    return;
  }

  const roundColor = String(state.snapshot?.current_set?.round_color || "").toLowerCase();
  const available = mapAvailableForChain(state.selectedChainId) && (roundColor === "blue" || roundColor === "green");
  badge.classList.remove("is-blue", "is-green", "is-round-blue", "is-round-green");
  if (!available) {
    value.textContent = "-";
    badge.removeAttribute("aria-label");
    return;
  }

  const label = roundColor === "blue" ? "BLUE (EVEN)" : "GREEN (ODD)";
  badge.classList.add(`is-round-${roundColor}`);
  value.textContent = label;
  badge.setAttribute("aria-label", `Round: ${label}`);
}

function updateValidatorMapSummary() {
  const totalNodeCount = $("validatorMapTotalNodeCount");
  const mappedNodeCount = $("validatorMapMappedNodeCount") || $("validatorMapNodeCount");
  const locationCount = $("validatorMapLocationCount");
  const summary = $("validatorMapSummary");
  if (!totalNodeCount && !mappedNodeCount && !locationCount && !summary) {
    return;
  }

  let nodes = [];
  if (validatorMapNodes && validatorMapNodesChainId === state.selectedChainId) {
    nodes = validatorMapNodes;
  } else if (state.selectedChainId === BUNDLED_TYCHO_MAP_CHAIN_ID && Array.isArray(window.TYCHO_NODES)) {
    nodes = window.TYCHO_NODES;
  }
  const totalNodes = Array.isArray(state.snapshot?.current_set?.validators)
    ? state.snapshot.current_set.validators.length
    : nodes.length;
  const locations = groupNodesByLocation(nodes).length;
  if (totalNodeCount) {
    totalNodeCount.textContent = String(totalNodes);
  }
  if (mappedNodeCount) {
    mappedNodeCount.textContent = String(nodes.length);
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
