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

  state.nodeStatsRenderKey = null;
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
    renderNodeStats();
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
    content.innerHTML = `<div class="node-stats-state">Loading node statistics</div>`;
  }
}

function renderNodeStatsError(error) {
  updateNodeStatsTitle();
  state.nodeStatsRenderKey = null;
  const summary = $("nodeStatsSummary");
  const content = $("nodeStatsContent");
  if (summary) {
    clearNodeStatsSummary(summary);
  }
  if (content) {
    content.innerHTML = `<div class="node-stats-state is-error">${escapeHtml(formatValidatorMapError(error))}</div>`;
  }
}

function renderNodeStats() {
  updateNodeStatsTitle();
  const summary = $("nodeStatsSummary");
  const content = $("nodeStatsContent");
  if (!summary || !content) {
    return;
  }

  const validators = state.snapshot?.current_set?.validators || [];
  const nodes = validatorMapNodes && validatorMapNodesChainId === state.selectedChainId ? validatorMapNodes : [];
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
    content.innerHTML = resolutionNotice
      ? `<div class="node-stats-state is-notice">${escapeHtml(resolutionNotice)}</div>`
      : `<div class="node-stats-state">No mapped ${escapeHtml(nodeStatsChainName())} validators</div>`;
    return;
  }

  clearNodeStatsSummary(summary);
  hideNodeStatsTooltip();
  content.innerHTML = `
    <div class="node-stats-overview">
      ${nodeStatsCardHtml(NODE_STATS_LABELS.cards.round, nodeStatsRoundValue(stats), "", NODE_STATS_LABELS.tooltips.round, false, `is-summary-round is-round ${nodeStatsRoundCardClass(stats.roundColor)}`)}
      ${nodeStatsCardHtml(NODE_STATS_LABELS.cards.totalNodes, formatNodeStatsInteger(stats.networkValidators), "", NODE_STATS_LABELS.tooltips.totalNodes, false, "is-summary-total-nodes")}
      ${nodeStatsCardHtml(NODE_STATS_LABELS.cards.mappedNodes, formatNodeStatsInteger(stats.mappedNodes), "", NODE_STATS_LABELS.tooltips.mappedNodes, false, "is-summary-mapped-nodes")}
      ${nodeStatsCardHtml(NODE_STATS_LABELS.cards.totalStake, formatNodeStatsStake(stats.networkStake), "", NODE_STATS_LABELS.tooltips.totalStake, false, "is-summary-total-stake")}
      ${nodeStatsCardHtml(NODE_STATS_LABELS.cards.mappedStake, formatPercent(stats.mappedStakePercent), "", NODE_STATS_LABELS.tooltips.mappedStake, false, "is-summary-mapped-stake")}
      ${nodeStatsCardHtml(NODE_STATS_LABELS.cards.bestGeoLocation, stats.medoid?.label || "-", "", NODE_STATS_LABELS.tooltips.bestGeoLocation, true, "is-summary-best-location")}
    </div>
    <div class="node-stats-layout">
      <section class="node-stats-block node-stats-block-countries">
        ${nodeStatsBlockTitleHtml(NODE_STATS_LABELS.blocks.countries, "countries")}
        ${nodeStatsCountryTableHtml(stats.countryRows)}
      </section>
      <section class="node-stats-block node-stats-block-isps">
        ${nodeStatsBlockTitleHtml(NODE_STATS_LABELS.blocks.isps, "isp")}
        ${nodeStatsRankTableHtml(stats.ispRows)}
      </section>
      <section class="node-stats-block node-stats-block-cities">
        ${nodeStatsBlockTitleHtml(NODE_STATS_LABELS.blocks.cities, "city")}
        ${nodeStatsRankTableHtml(stats.locationRows)}
      </section>
      <section class="${escapeHtml(nodeStatsPlacementBlockClass())}">
        ${nodeStatsBlockTitleHtml(NODE_STATS_LABELS.blocks.geoRanking, "ranking")}
        ${nodeStatsPlacementHtml(stats)}
      </section>
    </div>
  `;
  wireNodeStatsCardTooltips(content);
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

function nodeStatsRenderKey(stats) {
  return [
    state.selectedChainId,
    state.snapshot?.fetched_at || "",
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

