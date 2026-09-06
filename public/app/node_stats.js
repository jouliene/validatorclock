const NODE_STATS_VISIBLE_ROWS = 8;
const NODE_STATS_LABELS = {
  titleSuffix: "Node Location Stats",
  cards: {
    round: "Round",
    totalNodes: "Total Nodes",
    mappedNodes: "Mapped Nodes",
    totalStake: "Total Stake",
    mappedStake: "Mapped Stake",
    bestGeoLocation: "Best Geo Location",
  },
  blocks: {
    countries: "Top Countries",
    isps: "Top ISP Clusters",
    cities: "Top City Clusters",
    geoRanking: "Geo Location Ranking",
  },
  columns: {
    rank: "#",
    country: "Country",
    cluster: "Cluster",
    nodes: "Nodes",
    stake: "Stake",
    weightPercent: "Weight %",
    mappedLocation: "Mapped Location",
    weightedAverage: "Weighted Avg",
    median: "Median",
    p90: "P90",
  },
  actions: {
    viewFullRanking: "View full ranking \u2192",
    showTopFive: "Show top 5 \u2191",
  },
  tooltips: {
    round: "Current active validator round.",
    totalNodes: "Current active validators in the selected network.",
    mappedNodes: "Active validators with current or retained IP/location data.",
    totalStake: "Total stake in the current active validator set.",
    mappedStake: "Share of active stake covered by mapped validators.",
    bestGeoLocation: "Best mapped GeoIP city cluster by lowest stake-weighted geographic distance.",
  },
};

function handleNodeStatsChainChange(previousChainId, nextChainId) {
  if (previousChainId === nextChainId) {
    return;
  }

  forgetNodeStatsRender();
  state.nodeStatsLocationRankingExpanded = false;
  if (state.nodeStatsOpen) {
    loadSelectedNodeStats(false).catch((error) => {
      renderNodeStatsError(error);
    });
  }
}

function renderNodeStatsIfOpen() {
  if (!state.nodeStatsOpen) {
    return;
  }

  if (!state.snapshot) {
    renderNodeStatsLoading();
    return;
  }

  if (validatorMapNodesChainId !== state.selectedChainId && !applyCachedValidatorMapNodesForChain(state.selectedChainId)) {
    renderNodeStatsLoading();
    return;
  }

  renderNodeStats();
}

async function loadSelectedNodeStats(force = false) {
  const chainId = state.selectedChainId;
  if (!chainId) {
    return;
  }

  const requestSeq = state.nodeStatsRequestSeq + 1;
  state.nodeStatsRequestSeq = requestSeq;

  const cached = !force ? applyCachedValidatorMapNodesForChain(chainId) : null;
  if (cached && state.snapshot?.chain?.id === chainId) {
    clearNodeStatsLoadingTimer();
    renderNodeStats();
  } else {
    scheduleNodeStatsLoading(requestSeq, chainId);
  }

  try {
    await refreshValidatorMapNodesForSnapshot(chainId);
    if (requestSeq !== state.nodeStatsRequestSeq || chainId !== state.selectedChainId) {
      return;
    }
    clearNodeStatsLoadingTimer();
    // Through the same door as the cached path: with no snapshot for this chain there is
    // nothing to count the nodes against, and the panel would say the chain has no mapped
    // validators when all it has is no clock yet.
    renderNodeStatsIfOpen();
  } catch (error) {
    if (!cached) {
      throw error;
    }
    console.warn(`Unable to refresh ${chainId} node statistics`, error);
  } finally {
    clearNodeStatsLoadingTimer();
  }
}

function scheduleNodeStatsLoading(requestSeq, chainId) {
  clearNodeStatsLoadingTimer();
  state.nodeStatsLoadingTimer = window.setTimeout(() => {
    if (requestSeq === state.nodeStatsRequestSeq && chainId === state.selectedChainId) {
      renderNodeStatsLoading();
    }
  }, 180);
}

function clearNodeStatsLoadingTimer() {
  window.clearTimeout(state.nodeStatsLoadingTimer);
  state.nodeStatsLoadingTimer = null;
}

function renderNodeStatsLoading() {
  updateNodeStatsTitle();
  const summary = $("nodeStatsSummary");
  const content = $("nodeStatsContent");
  if (summary) {
    clearNodeStatsSummary(summary);
  }
  if (content) {
    replaceChildren(content, el("div", { className: "node-stats-state", text: "Loading node statistics" }));
  }
}

function renderNodeStatsError(error) {
  updateNodeStatsTitle();
  forgetNodeStatsRender();
  const summary = $("nodeStatsSummary");
  const content = $("nodeStatsContent");
  if (summary) {
    clearNodeStatsSummary(summary);
  }
  if (content) {
    replaceChildren(
      content,
      el("div", {
        className: "node-stats-state is-error",
        text: formatValidatorMapError(error),
      }),
    );
  }
}

// Two keys guard the panel - what it is built from, and what it would say - so a caller
// that wants it built again has to open both. Forgetting the second one left the panel
// frozen on what it last said.
function forgetNodeStatsRender() {
  state.nodeStatsRenderKey = null;
  state.nodeStatsInputKey = null;
}

function renderNodeStats() {
  updateNodeStatsTitle();
  const summary = $("nodeStatsSummary");
  const content = $("nodeStatsContent");
  if (!summary || !content) {
    return;
  }

  // Two gates, and this is the cheap one. renderNow calls this every second while the
  // panel is open, and building the model means aggregating every validator and, for the
  // distance table, a haversine per location per node. Nothing it reads can have changed
  // unless one of these did.
  const inputKey = [
    state.selectedChainId,
    state.snapshot?.fetched_at || "",
    state.validatorMapNodesVersion,
    state.nodeStatsLocationRankingExpanded ? "expanded" : "",
  ].join("|");
  if (state.nodeStatsInputKey === inputKey) {
    return;
  }
  state.nodeStatsInputKey = inputKey;

  const validators = state.snapshot?.current_set?.validators || [];
  const nodes = currentChainMapNodes() || [];
  const stats = buildNodeStats(nodes, validators, state.validatorMapNodesByPeer);
  const resolutionNotice = mapNodeResolutionNotice(stats.mappedNodes);
  const renderKey = nodeStatsRenderKey(stats);
  if (state.nodeStatsRenderKey === renderKey) {
    return;
  }
  state.nodeStatsRenderKey = renderKey;

  if (!stats.mappedNodes) {
    clearNodeStatsSummary(summary);
    hideNodeStatsTooltip();
    replaceChildren(
      content,
      resolutionNotice
        ? el("div", { className: "node-stats-state is-notice", text: resolutionNotice })
        : el("div", {
            className: "node-stats-state",
            text: `No mapped ${nodeStatsChainName()} validators`,
          }),
    );
    return;
  }

  clearNodeStatsSummary(summary);
  hideNodeStatsTooltip();
  replaceChildren(content, [
    el("div", "node-stats-overview", [
      nodeStatsCard(
        NODE_STATS_LABELS.cards.round,
        nodeStatsRoundValue(stats),
        "",
        NODE_STATS_LABELS.tooltips.round,
        false,
        `is-summary-round is-round ${nodeStatsRoundCardClass(stats.roundColor)}`,
      ),
      nodeStatsCard(
        NODE_STATS_LABELS.cards.totalNodes,
        formatNodeStatsInteger(stats.networkValidators),
        "",
        NODE_STATS_LABELS.tooltips.totalNodes,
        false,
        "is-summary-total-nodes",
      ),
      nodeStatsCard(
        NODE_STATS_LABELS.cards.mappedNodes,
        formatNodeStatsInteger(stats.mappedNodes),
        "",
        NODE_STATS_LABELS.tooltips.mappedNodes,
        false,
        "is-summary-mapped-nodes",
      ),
      nodeStatsCard(
        NODE_STATS_LABELS.cards.totalStake,
        formatNodeStatsStake(stats.networkStake),
        "",
        NODE_STATS_LABELS.tooltips.totalStake,
        false,
        "is-summary-total-stake",
      ),
      nodeStatsCard(
        NODE_STATS_LABELS.cards.mappedStake,
        formatPercent(stats.mappedStakePercent),
        "",
        NODE_STATS_LABELS.tooltips.mappedStake,
        false,
        "is-summary-mapped-stake",
      ),
      nodeStatsCard(
        NODE_STATS_LABELS.cards.bestGeoLocation,
        stats.medoid?.label || "-",
        "",
        NODE_STATS_LABELS.tooltips.bestGeoLocation,
        true,
        "is-summary-best-location",
      ),
    ]),
    el("div", "node-stats-layout", [
      el("section", "node-stats-block node-stats-block-countries", [
        nodeStatsBlockTitle(NODE_STATS_LABELS.blocks.countries, "countries"),
        nodeStatsCountryTable(stats.countryRows),
      ]),
      el("section", "node-stats-block node-stats-block-isps", [
        nodeStatsBlockTitle(NODE_STATS_LABELS.blocks.isps, "isp"),
        nodeStatsRankTable(stats.ispRows),
      ]),
      el("section", "node-stats-block node-stats-block-cities", [
        nodeStatsBlockTitle(NODE_STATS_LABELS.blocks.cities, "city"),
        nodeStatsRankTable(stats.locationRows),
      ]),
      el("section", nodeStatsPlacementBlockClass(), [
        nodeStatsBlockTitle(NODE_STATS_LABELS.blocks.geoRanking, "ranking"),
        nodeStatsPlacement(stats),
      ]),
    ]),
  ]);
  wireNodeStatsRankingToggle(content);
  wireNodeStatsTableScrollHints(content);
}

function updateNodeStatsTitle() {
  const title = $("nodeStatsTitle");
  const panel = $("nodeStatsPanel");
  const chainName = nodeStatsChainName();
  const label = `${chainName} ${NODE_STATS_LABELS.titleSuffix}`;
  if (title) {
    title.textContent = label;
  }
  panel?.setAttribute("aria-label", `${chainName} node location stats`);
}

function clearNodeStatsSummary(summary) {
  if (!summary) {
    return;
  }
  summary.textContent = "";
  summary.removeAttribute("title");
}

function nodeStatsChainName() {
  const chain = currentMapChain();
  if (chain?.id === "tycho-testnet") {
    return "Tycho";
  }
  return chain?.name || state.selectedChainId || "Network";
}

// And this is the second gate: what the panel would actually say. It used to carry
// fetched_at, so every poll rebuilt the whole panel - and reset the scroll of its tables -
// for numbers that were identical.
function nodeStatsRenderKey(stats) {
  return [
    state.selectedChainId,
    stats.roundId,
    stats.roundColor,
    stats.networkValidators,
    stats.mappedNodes,
    stats.networkStake,
    stats.mappedStake,
    stats.countryRows.length,
    stats.locationRows.length,
    stats.ispRows.length,
    stats.mappedLocationRows.length,
    stats.medoid?.label || "",
    stats.medoid?.weightedAverageKm || "",
    stats.medoid?.medianKm || "",
    stats.medoid?.p90Km || "",
    mapNodeResolutionNotice(stats.mappedNodes) ? "round-map-resolution" : "",
  ].join("|");
}

function nodeStatsRoundValue(stats) {
  const color = formatNodeStatsRoundColor(stats.roundColor);
  if (!color) {
    return "-";
  }
  const parity = color.toLowerCase() === "blue" ? "Even" : color.toLowerCase() === "green" ? "Odd" : "";
  return parity ? `${color.toUpperCase()} (${parity.toUpperCase()})` : color.toUpperCase();
}

function nodeStatsRoundCardClass(value) {
  const color = String(value || "").trim().toLowerCase();
  return color === "green" || color === "blue" ? `is-round-${color}` : "";
}

