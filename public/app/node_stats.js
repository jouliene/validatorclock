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
    mappedNodes: "Active validators with current IP/location data.",
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
  const stats = buildNodeStats(nodes, validators);
  const renderKey = nodeStatsRenderKey(stats);
  if (state.nodeStatsRenderKey === renderKey) {
    return;
  }
  state.nodeStatsRenderKey = renderKey;

  if (!stats.mappedNodes) {
    clearNodeStatsSummary(summary);
    hideNodeStatsTooltip();
    content.innerHTML = `<div class="node-stats-state">No mapped ${escapeHtml(nodeStatsChainName())} validators</div>`;
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

function nodeStatsCardHtml(label, value, detail, tooltip = "", featured = false, extraClass = "") {
  const className = ["node-stats-card", featured ? "is-featured" : "", extraClass].filter(Boolean).join(" ");
  return `
    <div class="${escapeHtml(className)}"${tooltip ? ` data-node-stats-tooltip="${escapeHtml(tooltip)}"` : ""}>
      <span>${escapeHtml(label)}</span>
      <strong>${escapeHtml(value)}</strong>
      ${detail ? `<small>${escapeHtml(detail)}</small>` : ""}
    </div>
  `;
}

function wireNodeStatsCardTooltips(root) {
  for (const card of root.querySelectorAll("[data-node-stats-tooltip]")) {
    setValidatorTooltip(card, card.dataset.nodeStatsTooltip || "");
  }
}

function hideNodeStatsTooltip() {
  if (typeof hideValidatorTooltip === "function") {
    hideValidatorTooltip();
  }
}

function wireNodeStatsTableScrollHints(root) {
  const shells = Array.from(root.querySelectorAll(".node-stats-table-shell"));
  const updateShell = (shell) => {
    const maxScrollLeft = Math.max(0, shell.scrollWidth - shell.clientWidth);
    const hasMore = maxScrollLeft > 1 && shell.scrollLeft < maxScrollLeft - 1;
    shell.classList.toggle("has-scroll-more", hasMore);
  };

  for (const shell of shells) {
    shell.addEventListener("scroll", () => updateShell(shell), { passive: true });
    window.requestAnimationFrame(() => updateShell(shell));
  }
}

function nodeStatsBlockTitleHtml(label, icon) {
  return `
    <h3 class="node-stats-block-title">
      <span class="node-stats-block-icon node-stats-icon-${escapeHtml(icon)}" aria-hidden="true">
        ${nodeStatsBlockIconSvg(icon)}
      </span>
      <span>${escapeHtml(label)}</span>
    </h3>
  `;
}

function nodeStatsBlockIconSvg(icon) {
  if (icon === "countries") {
    return `
      <svg viewBox="0 0 24 24" focusable="false">
        <circle cx="12" cy="12" r="8.2"></circle>
        <path d="M3.8 12h16.4"></path>
        <path d="M12 3.8a12.2 12.2 0 0 1 0 16.4"></path>
        <path d="M12 3.8a12.2 12.2 0 0 0 0 16.4"></path>
      </svg>
    `;
  }
  if (icon === "isp") {
    return `
      <svg viewBox="0 0 24 24" focusable="false">
        <circle cx="7" cy="8" r="2"></circle>
        <circle cx="17" cy="8" r="2"></circle>
        <circle cx="12" cy="17" r="2"></circle>
        <path d="M8.7 9.2 11 15.1"></path>
        <path d="m15.3 9.2-2.3 5.9"></path>
        <path d="M9 8h6"></path>
      </svg>
    `;
  }
  if (icon === "city") {
    return `
      <svg viewBox="0 0 24 24" focusable="false">
        <path d="M5 19V7l5-2v14"></path>
        <path d="M10 19V9l5-2v12"></path>
        <path d="M15 19v-7l4 1.8V19"></path>
        <path d="M4 19h16"></path>
      </svg>
    `;
  }
  return `
    <svg viewBox="0 0 24 24" focusable="false">
      <path d="M8 21h8"></path>
      <path d="M12 17v4"></path>
      <path d="M7 4h10v3a5 5 0 0 1-10 0Z"></path>
      <path d="M7 6H4a3 3 0 0 0 3 3"></path>
      <path d="M17 6h3a3 3 0 0 1-3 3"></path>
    </svg>
  `;
}

function wireNodeStatsRankingToggle(root) {
  const button = root.querySelector("[data-node-stats-ranking-toggle]");
  if (!button) {
    return;
  }

  button.addEventListener("click", () => {
    const block = button.closest(".node-stats-block-placement");
    if (!block) {
      return;
    }
    const expanded = block.classList.toggle("is-ranking-expanded");
    state.nodeStatsLocationRankingExpanded = expanded;
    const summary = block.querySelector("[data-node-stats-ranking-summary]");
    button.textContent = nodeStatsRankingActionText(expanded);
    button.setAttribute("aria-expanded", expanded ? "true" : "false");
    if (summary) {
      summary.textContent = nodeStatsRankingSummaryText(
        Number(summary.dataset.visibleCount || 0),
        Number(summary.dataset.totalCount || 0),
        expanded,
      );
    }
  });
}

function nodeStatsCountryTableHtml(rows) {
  return nodeStatsAggregateTableHtml(rows, NODE_STATS_LABELS.columns.country, "country", "countries");
}

function nodeStatsRankTableHtml(rows) {
  return nodeStatsAggregateTableHtml(rows, NODE_STATS_LABELS.columns.cluster, "cluster", "clusters");
}

function nodeStatsAggregateTableHtml(rows, nameHeader, singularLabel, pluralLabel) {
  const tableRows = nodeStatsVisibleRows(rows, singularLabel, pluralLabel);
  return `
    <div class="node-stats-table-shell">
      <table class="node-stats-table">
        <colgroup>
          <col class="node-stats-col-rank">
          <col class="node-stats-col-name">
          <col class="node-stats-col-count">
          <col class="node-stats-col-stake">
          <col class="node-stats-col-percent">
        </colgroup>
        <thead>
          <tr>
            <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.rank)}</th>
            <th scope="col">${escapeHtml(nameHeader)}</th>
            <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.nodes)}</th>
            <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.stake)}</th>
            <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.weightPercent)}</th>
          </tr>
        </thead>
        <tbody>
          ${tableRows.map((row, index) => nodeStatsAggregateRowHtml(row, index)).join("")}
        </tbody>
      </table>
    </div>
  `;
}

function nodeStatsAggregateRowHtml(row, index) {
  return `
    <tr${row.isRemainder ? ` class="is-remainder"` : ""}>
      <td>${formatNodeStatsInteger(index + 1)}</td>
      <td>${escapeHtml(row.label)}</td>
      <td>${formatNodeStatsInteger(row.nodes)}</td>
      <td>${formatNodeStatsStake(row.stake)}</td>
      <td>${formatPercent(row.weightPercent)}</td>
    </tr>
  `;
}

function nodeStatsVisibleRows(rows, singularLabel, pluralLabel) {
  const visibleRows = rows.slice(0, NODE_STATS_VISIBLE_ROWS);
  const remainderRows = rows.slice(NODE_STATS_VISIBLE_ROWS);
  if (!remainderRows.length) {
    return visibleRows;
  }

  const label = remainderRows.length === 1 ? singularLabel : pluralLabel;
  return [
    ...visibleRows,
    {
      label: `Other ${formatNodeStatsInteger(remainderRows.length)} ${label}`,
      nodes: remainderRows.reduce((sum, row) => sum + row.nodes, 0),
      stake: remainderRows.reduce((sum, row) => sum + row.stake, 0),
      stakePercent: remainderRows.reduce((sum, row) => sum + row.stakePercent, 0),
      weightPercent: remainderRows.reduce((sum, row) => sum + row.weightPercent, 0),
      isRemainder: true,
    },
  ];
}

function nodeStatsPlacementHtml(stats) {
  const mappedLocations = stats.mappedLocationRows;
  const visibleCount = Math.min(5, mappedLocations.length);
  const extraCount = Math.max(0, mappedLocations.length - visibleCount);
  const expanded = isNodeStatsLocationRankingExpanded();
  return `
    <div class="node-stats-placement">
      <div class="node-stats-table-shell">
        <table class="node-stats-table node-stats-ranking-table">
          <colgroup>
            <col class="node-stats-col-rank">
            <col class="node-stats-col-location">
            <col class="node-stats-col-distance-primary">
            <col class="node-stats-col-distance-secondary">
            <col class="node-stats-col-distance-tertiary">
          </colgroup>
          <thead>
            <tr>
              <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.rank)}</th>
              <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.mappedLocation)}</th>
              <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.weightedAverage)}</th>
              <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.median)}</th>
              <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.p90)}</th>
            </tr>
          </thead>
          <tbody>
            ${mappedLocations.map((row, index) => `
            <tr${nodeStatsPlacementRowClass(index, visibleCount)}>
              <td>${formatNodeStatsInteger(index + 1)}</td>
              <td>${escapeHtml(row.label)}</td>
              <td>${formatNodeStatsDistance(row.weightedAverageKm)}</td>
              <td>${formatNodeStatsDistance(row.medianKm)}</td>
              <td>${formatNodeStatsDistance(row.p90Km)}</td>
            </tr>
            `).join("")}
          </tbody>
        </table>
      </div>
      ${extraCount ? `
      <div class="node-stats-ranking-footer">
        <span data-node-stats-ranking-summary data-visible-count="${visibleCount}" data-total-count="${mappedLocations.length}">${escapeHtml(nodeStatsRankingSummaryText(visibleCount, mappedLocations.length, expanded))}</span>
        <button class="node-stats-ranking-action" type="button" data-node-stats-ranking-toggle aria-expanded="${expanded ? "true" : "false"}">${escapeHtml(nodeStatsRankingActionText(expanded))}</button>
      </div>
      ` : ""}
    </div>
  `;
}

function nodeStatsPlacementBlockClass() {
  return [
    "node-stats-block",
    "node-stats-block-placement",
    isNodeStatsLocationRankingExpanded() ? "is-ranking-expanded" : "",
  ].filter(Boolean).join(" ");
}

function isNodeStatsLocationRankingExpanded() {
  return Boolean(state.nodeStatsLocationRankingExpanded);
}

function nodeStatsRankingActionText(expanded) {
  return expanded ? NODE_STATS_LABELS.actions.showTopFive : NODE_STATS_LABELS.actions.viewFullRanking;
}

function nodeStatsRankingSummaryText(visibleCount, totalCount, expanded) {
  const visible = Number.isFinite(visibleCount) ? Math.max(0, Math.trunc(visibleCount)) : 0;
  const total = Number.isFinite(totalCount) ? Math.max(0, Math.trunc(totalCount)) : 0;
  if (expanded) {
    return `Showing all ${formatNodeStatsInteger(total)}`;
  }
  return `Top ${formatNodeStatsInteger(visible)} of ${formatNodeStatsInteger(total)}`;
}

function nodeStatsPlacementRowClass(index, visibleCount) {
  const classes = [];
  if (index === 0) {
    classes.push("is-best");
  }
  if (index >= visibleCount) {
    classes.push("is-extra-ranking");
  }
  return classes.length ? ` class="${classes.join(" ")}"` : "";
}

function buildNodeStats(nodes, validators) {
  const validatorsByPeer = new Map();
  let networkStake = 0;
  let networkWeightPercent = 0;

  for (const validator of Array.isArray(validators) ? validators : []) {
    const peer = String(validator.public_key || "").toLowerCase();
    if (!peer) {
      continue;
    }
    const stake = nodeStatsNumericValue(validator.stake);
    const weightPercent = nodeStatsNumericValue(validator.weight_percent);
    validatorsByPeer.set(peer, { ...validator, stakeNumber: stake, weightPercentNumber: weightPercent });
    networkStake += stake;
    networkWeightPercent += weightPercent;
  }

  const mappedNodes = (Array.isArray(nodes) ? nodes : [])
    .map((node) => {
      const peer = String(node.peer || "").toLowerCase();
      const validator = validatorsByPeer.get(peer);
      const lat = Number(node.lat);
      const lon = Number(node.lon);
      return {
        ...node,
        peer,
        validator,
        stake: validator?.stakeNumber || 0,
        weightPercent: validator?.weightPercentNumber || 0,
        lat,
        lon,
      };
    })
    .filter((node) => node.peer && node.validator && Number.isFinite(node.lat) && Number.isFinite(node.lon));
  const uniqueMappedNodes = Array.from(
    mappedNodes.reduce((byPeer, node) => (byPeer.has(node.peer) ? byPeer : byPeer.set(node.peer, node)), new Map()).values(),
  );

  const mappedStake = uniqueMappedNodes.reduce((sum, node) => sum + node.stake, 0);
  const mappedWeightPercent = uniqueMappedNodes.reduce((sum, node) => sum + node.weightPercent, 0);
  const countryRows = aggregateNodeStatsRows(uniqueMappedNodes, (node) => normalizeNodeStatsCountry(node.country), networkStake);
  const locationRows = aggregateNodeStatsRows(uniqueMappedNodes, (node) => nodeStatsLocationLabel(node), networkStake);
  const ispRows = aggregateNodeStatsRows(uniqueMappedNodes, (node) => String(node.isp || "Unknown").trim() || "Unknown", networkStake);
  const mappedLocationRows = nodeStatsMappedLocationCandidates(uniqueMappedNodes);
  const medoid = mappedLocationRows[0] || null;
  const currentSet = state.snapshot?.current_set || {};

  return {
    roundId: currentSet.round_id || "",
    roundColor: currentSet.round_color || "",
    networkValidators: validatorsByPeer.size,
    networkStake,
    networkWeightPercent,
    mappedNodes: uniqueMappedNodes.length,
    mappedStake,
    mappedStakePercent: networkStake ? (mappedStake / networkStake) * 100 : 0,
    mappedWeightPercent: networkWeightPercent ? (mappedWeightPercent / networkWeightPercent) * 100 : mappedWeightPercent,
    countryRows,
    locationRows,
    ispRows,
    mappedLocationRows,
    medoid,
  };
}

function aggregateNodeStatsRows(nodes, labelForNode, networkStake) {
  const rows = new Map();
  for (const node of nodes) {
    const label = labelForNode(node) || "Unknown";
    if (!rows.has(label)) {
      rows.set(label, {
        label,
        nodes: 0,
        stake: 0,
        weightPercent: 0,
      });
    }
    const row = rows.get(label);
    row.nodes += 1;
    row.stake += node.stake;
    row.weightPercent += node.weightPercent;
  }

  return Array.from(rows.values())
    .map((row) => ({
      ...row,
      stakePercent: networkStake ? (row.stake / networkStake) * 100 : 0,
    }))
    .sort((left, right) => right.stake - left.stake || right.nodes - left.nodes || left.label.localeCompare(right.label));
}

function nodeStatsMappedLocationCandidates(nodes) {
  const locations = new Map();
  for (const node of nodes) {
    const label = nodeStatsLocationLabel(node);
    if (!locations.has(label)) {
      locations.set(label, {
        label,
        weightedLat: 0,
        weightedLon: 0,
        totalWeight: 0,
      });
    }
    const location = locations.get(label);
    const weight = nodeStatsDistanceWeight(node);
    location.weightedLat += node.lat * weight;
    location.weightedLon += node.lon * weight;
    location.totalWeight += weight;
  }

  return Array.from(locations.values())
    .filter((location) => location.totalWeight > 0)
    .map((location) => {
      const lat = location.weightedLat / location.totalWeight;
      const lon = location.weightedLon / location.totalWeight;
      return {
        label: location.label,
        ...nodeStatsDistanceForPoint(nodes, lat, lon),
      };
    })
    .sort((left, right) => left.weightedAverageKm - right.weightedAverageKm);
}

function nodeStatsDistanceForPoint(nodes, lat, lon) {
  const distances = [];
  let weightedTotal = 0;
  let totalWeight = 0;

  for (const node of nodes) {
    const distance = distanceBetweenCoordinatesKm(lat, lon, node.lat, node.lon);
    const weight = nodeStatsDistanceWeight(node);
    distances.push(distance);
    weightedTotal += distance * weight;
    totalWeight += weight;
  }

  distances.sort((left, right) => left - right);
  return {
    weightedAverageKm: totalWeight ? weightedTotal / totalWeight : 0,
    medianKm: nodeStatsPercentileFromSorted(distances, 0.5),
    p90Km: nodeStatsPercentileFromSorted(distances, 0.9),
  };
}

function nodeStatsDistanceWeight(node) {
  return node.stake > 0 ? node.stake : Math.max(node.weightPercent, 1);
}

function nodeStatsPercentileFromSorted(values, percentile) {
  if (!values.length) {
    return 0;
  }
  const index = Math.ceil(values.length * percentile) - 1;
  return values[Math.min(values.length - 1, Math.max(0, index))];
}

function nodeStatsLocationLabel(node) {
  const city = String(node.city || "").trim();
  const country = normalizeNodeStatsCountry(node.country);
  return city && country ? `${city}, ${country}` : city || country || "Unknown";
}

function normalizeNodeStatsCountry(value) {
  const country = String(value || "").trim();
  return country === "The Netherlands" ? "Netherlands" : country || "Unknown";
}

function nodeStatsNumericValue(value) {
  const number = Number(value || 0);
  return Number.isFinite(number) ? number : 0;
}

function formatNodeStatsRoundColor(value) {
  const color = String(value || "").trim();
  return color ? `${color.slice(0, 1).toUpperCase()}${color.slice(1).toLowerCase()}` : "";
}

function formatNodeStatsStake(value) {
  return formatTokenAmount(value, 0, 0);
}

function formatNodeStatsInteger(value) {
  return Number(value || 0).toLocaleString(undefined, { maximumFractionDigits: 0 });
}

function formatNodeStatsDistance(value) {
  return `${Math.round(Number(value || 0)).toLocaleString()} km`;
}
