const ROUND_STATS_CHARTS = [
  {
    key: "totalStake",
    title: "Total stake",
    unit: "stake",
    series: [{
      key: "total",
      label: "Total stake",
      value: (round) => roundStatsAmount(round.total_stake, round.total_stake_raw),
      tooltip: (round) => formatRoundStatsExactAmount(round.total_stake),
    }],
    latest: (round) => formatStakeAmount(round?.total_stake),
  },
  {
    key: "stakeRange",
    title: "Min/max stake",
    unit: "stake",
    series: [
      {
        key: "min",
        label: "Min stake",
        value: (round) => roundStatsAmount(round.min_stake, null),
        tooltip: (round) => formatRoundStatsExactAmount(round.min_stake),
      },
      {
        key: "max",
        label: "Max stake",
        value: (round) => roundStatsAmount(round.max_stake, null),
        tooltip: (round) => formatRoundStatsExactAmount(round.max_stake),
      },
    ],
    latest: (round) => {
      if (!round) {
        return "-";
      }
      return `${formatStakeAmount(round.min_stake)} / ${formatStakeAmount(round.max_stake)}`;
    },
  },
  {
    key: "validators",
    title: "Number of validators",
    unit: "count",
    series: [{
      key: "validators",
      label: "Validators",
      value: (round) => Number(round.validator_count),
      tooltip: (round) => formatWeight(round.validator_count || 0),
    }],
    latest: (round) => round?.validator_count ? formatWeight(round.validator_count) : "-",
  },
  {
    key: "profitability",
    title: "Profitability",
    unit: "percent",
    series: [{
      key: "profitability",
      label: "Profitability",
      value: (round) => Number(round.profitability_percent),
      tooltip: (round) => formatRoundStatsExactPercent(round.profitability_percent),
    }],
    latest: (round) => formatRoundStatsPercent(round?.profitability_percent),
  },
];

function setupRoundStatsControls() {
  const toggle = $("roundStatsToggle");
  if (!toggle) {
    return;
  }

  toggle.addEventListener("click", () => {
    setRoundStatsOpen(!state.roundStatsOpen);
  });

  if (window.location.hash === "#round-stats") {
    setRoundStatsOpen(true);
    return;
  }

  syncRoundStatsPanel();
}

function roundStatsCacheMaxAgeSeconds() {
  return Math.max(10, Math.floor(Math.max(10, state.refreshSeconds) / 2));
}

function roundStatsCacheIsFresh(chainId) {
  const cachedAt = state.roundStatsCachedAtByChain.get(chainId);
  if (!cachedAt) {
    return false;
  }

  const age = Math.trunc(Date.now() / 1000) - cachedAt;
  return age < roundStatsCacheMaxAgeSeconds();
}

function storeRoundStatsSnapshot(chainId, stats) {
  state.roundStatsByChain.set(chainId, stats);
  state.roundStatsCachedAtByChain.set(chainId, Math.trunc(Date.now() / 1000));
  if (chainId === state.selectedChainId) {
    renderRoundAprBadges(stats);
  }
}

function prefetchRoundStatsSnapshots() {
  const chainIds = state.chains
    .map((chain) => chain.id)
    .filter(Boolean)
    .sort((left, right) => {
      if (left === state.selectedChainId) {
        return -1;
      }
      if (right === state.selectedChainId) {
        return 1;
      }
      return 0;
    });

  chainIds.forEach((chainId, index) => {
    window.setTimeout(() => {
      prefetchRoundStatsForChain(chainId).catch((error) => {
        console.warn(`Unable to prefetch ${chainId} round statistics`, error);
      });
    }, index * 350);
  });
}

async function prefetchRoundStatsForChain(chainId, force = false) {
  if (!chainId || (!force && roundStatsCacheIsFresh(chainId))) {
    return;
  }

  const stats = await fetchRoundStatsSnapshot(chainId, !force);
  storeRoundStatsSnapshot(chainId, stats);
  if (state.roundStatsOpen && chainId === state.selectedChainId) {
    renderRoundStatsPanel(stats);
  }
}

function handleRoundStatsClockSnapshot(chainId, snapshot) {
  if (!chainId || !snapshot) {
    return;
  }

  const cached = state.roundStatsByChain.get(chainId);
  const activeRoundChanged = cached?.active_round_id !== snapshot.current_set?.round_id;
  if (activeRoundChanged || !roundStatsCacheIsFresh(chainId)) {
    prefetchRoundStatsForChain(chainId, activeRoundChanged).catch((error) => {
      console.warn(`Unable to refresh ${chainId} round statistics`, error);
    });
  }
}

function setRoundStatsOpen(open) {
  const willOpen = Boolean(open);
  if (willOpen && state.validatorMapOpen) {
    setValidatorMapOpen(false);
  }

  state.roundStatsOpen = willOpen;
  syncRoundStatsPanel();
  if (!state.roundStatsOpen) {
    clearRoundStatsLoadingTimer();
    return;
  }

  loadSelectedRoundStats(false).catch((error) => {
    renderRoundStatsError(error);
  });
}

function syncRoundStatsPanel() {
  const panel = $("roundStatsPanel");
  const toggle = $("roundStatsToggle");
  if (!panel || !toggle) {
    return;
  }

  panel.hidden = !state.roundStatsOpen;
  toggle.setAttribute("aria-expanded", String(state.roundStatsOpen));
  toggle.setAttribute(
    "aria-label",
    state.roundStatsOpen ? "Hide rounds statistics" : "Show rounds statistics",
  );
}

function handleRoundStatsChainChange(previousChainId, nextChainId) {
  if (previousChainId === nextChainId) {
    return;
  }

  state.roundStatsRenderKey = null;
  refreshRoundAprBadges();
  prefetchRoundStatsForChain(nextChainId).catch((error) => {
    console.warn(`Unable to prefetch ${nextChainId} round statistics`, error);
  });
  if (!state.roundStatsOpen) {
    return;
  }

  const cached = state.roundStatsByChain.get(nextChainId);
  if (cached) {
    renderRoundStatsPanel(cached);
  } else {
    renderRoundStatsLoading();
  }
}

function roundStatsSnapshotUrl(chainId, preferCache = false) {
  const suffix = preferCache ? "?prefer_cache=1" : "";
  return `/api/chains/${encodeURIComponent(chainId)}/round-stats${suffix}`;
}

function fetchRoundStatsSnapshot(chainId, preferCache = false) {
  const fetchKey = `${chainId}:${preferCache ? "cache" : "live"}`;
  const pending = state.roundStatsFetchesByChain.get(fetchKey);
  if (pending) {
    return pending;
  }

  const request = fetchJson(roundStatsSnapshotUrl(chainId, preferCache)).finally(() => {
    if (state.roundStatsFetchesByChain.get(fetchKey) === request) {
      state.roundStatsFetchesByChain.delete(fetchKey);
    }
  });
  state.roundStatsFetchesByChain.set(fetchKey, request);
  return request;
}

async function loadSelectedRoundStats(force = false) {
  const chainId = state.selectedChainId;
  if (!chainId) {
    return;
  }

  const requestSeq = state.roundStatsRequestSeq + 1;
  state.roundStatsRequestSeq = requestSeq;
  state.roundStatsLoading = true;

  const cached = state.roundStatsByChain.get(chainId);
  if (cached && !force) {
    renderRoundStatsPanel(cached);
  } else {
    scheduleRoundStatsLoading(requestSeq, chainId);
  }

  try {
    const stats = await fetchRoundStatsSnapshot(chainId, !force);
    if (requestSeq !== state.roundStatsRequestSeq || chainId !== state.selectedChainId) {
      return;
    }
    storeRoundStatsSnapshot(chainId, stats);
    clearRoundStatsLoadingTimer();
    renderRoundStatsPanel(stats);
    if (!force) {
      prefetchRoundStatsForChain(chainId, true).catch((error) => {
        console.warn(`Unable to refresh ${chainId} round statistics`, error);
      });
    }
  } catch (error) {
    if (!cached) {
      throw error;
    }
    console.warn(`Unable to refresh ${chainId} round statistics`, error);
  } finally {
    clearRoundStatsLoadingTimer();
    if (requestSeq === state.roundStatsRequestSeq && chainId === state.selectedChainId) {
      state.roundStatsLoading = false;
    }
  }
}

function scheduleRoundStatsLoading(requestSeq, chainId) {
  clearRoundStatsLoadingTimer();
  state.roundStatsLoadingTimer = window.setTimeout(() => {
    if (requestSeq === state.roundStatsRequestSeq && chainId === state.selectedChainId) {
      renderRoundStatsLoading();
    }
  }, 180);
}

function clearRoundStatsLoadingTimer() {
  window.clearTimeout(state.roundStatsLoadingTimer);
  state.roundStatsLoadingTimer = null;
}

function renderRoundStatsPanel(stats) {
  const panel = $("roundStatsPanel");
  if (!panel || !stats) {
    return;
  }

  const key = [
    stats.chain?.id || state.selectedChainId,
    stats.fetched_at,
    stats.active_round_id,
    stats.blue?.rounds?.length || 0,
    stats.green?.rounds?.length || 0,
  ].join("|");
  if (state.roundStatsRenderKey === key) {
    return;
  }
  state.roundStatsRenderKey = key;

  panel.hidden = !state.roundStatsOpen;
  renderRoundStatsColor("blue", stats.blue?.rounds || [], stats.chain?.token_symbol || "");
  renderRoundStatsColor("green", stats.green?.rounds || [], stats.chain?.token_symbol || "");
}

function refreshRoundAprBadges() {
  renderRoundAprBadges(state.roundStatsByChain.get(state.selectedChainId));
}

function renderRoundAprBadges(stats) {
  renderRoundAprBadge("blue", stats?.blue?.rounds || []);
  renderRoundAprBadge("green", stats?.green?.rounds || []);
}

function renderRoundAprBadge(color, rounds) {
  const badge = $(`${color}RoundApr`);
  const label = badge?.querySelector("span");
  const value = badge?.querySelector("strong");
  if (!badge || !label || !value) {
    return;
  }

  const apr = averageRoundStatsProfitability(rounds);
  const hasAverage = Number.isFinite(apr.average);
  label.textContent = `${apr.count} ROUNDS APR`;
  value.textContent = hasAverage ? formatRoundStatsPercent(apr.average) : "-";
  badge.classList.toggle("is-empty", !hasAverage);
  const colorLabel = color === "blue" ? "blue" : "green";
  setValidatorTooltip(
    badge,
    hasAverage
      ? [
          "Metric: Average APR",
          `Rounds: ${apr.count} completed ${colorLabel} rounds with rewards data`,
          `APR: ${value.textContent}`,
        ]
      : [`Rounds: 0 completed ${colorLabel} rounds with rewards data`, "APR: unavailable"],
  );
  badge.setAttribute(
    "aria-label",
    hasAverage ? `${apr.count} rounds APR ${value.textContent}` : "APR unavailable",
  );
}

function averageRoundStatsProfitability(rounds) {
  const values = (rounds || [])
    .map((round) => Number(round?.profitability_percent))
    .filter(Number.isFinite);
  if (!values.length) {
    return { average: NaN, count: 0 };
  }

  return {
    average: values.reduce((sum, value) => sum + value, 0) / values.length,
    count: values.length,
  };
}

function renderRoundStatsColor(color, rounds, tokenSymbol) {
  const container = $(`${color}RoundStatsCharts`);
  const count = $(`${color}RoundStatsCount`);
  if (!container) {
    return;
  }

  container.replaceChildren();
  if (count) {
    count.textContent = rounds.length ? `${rounds.length} rounds` : "empty";
  }

  for (const chart of ROUND_STATS_CHARTS) {
    container.appendChild(roundStatsChartCard(chart, rounds, tokenSymbol));
  }
}

function renderRoundStatsLoading() {
  state.roundStatsRenderKey = null;
  for (const color of ["blue", "green"]) {
    const container = $(`${color}RoundStatsCharts`);
    const count = $(`${color}RoundStatsCount`);
    if (count) {
      count.textContent = "loading";
    }
    if (container) {
      container.replaceChildren(roundStatsStatus("Loading statistics"));
    }
  }
}

function renderRoundStatsError(error) {
  state.roundStatsRenderKey = null;
  for (const color of ["blue", "green"]) {
    const container = $(`${color}RoundStatsCharts`);
    const count = $(`${color}RoundStatsCount`);
    if (count) {
      count.textContent = "unavailable";
    }
    if (container) {
      container.replaceChildren(roundStatsStatus(roundStatsErrorMessage(error)));
    }
  }
}

function roundStatsErrorMessage(error) {
  const message = String(error?.message || error || "");
  if (message.includes("timeout")) {
    return "Statistics request timed out.";
  }
  return "Statistics are unavailable.";
}

function roundStatsStatus(message) {
  const status = document.createElement("div");
  status.className = "round-stats-status";
  status.textContent = message;
  return status;
}

function roundStatsChartCard(chart, rounds, tokenSymbol) {
  const card = document.createElement("section");
  card.className = `round-stats-chart-card is-${chart.key}`;

  const header = document.createElement("div");
  header.className = "round-stats-chart-header";
  const title = document.createElement("h3");
  title.textContent = chart.title;
  const latest = document.createElement("strong");
  latest.textContent = chart.latest(rounds.at(-1), tokenSymbol);
  header.append(title, latest);

  const body = document.createElement("div");
  body.className = "round-stats-chart-body";
  body.appendChild(roundStatsSvg(chart, rounds));

  card.append(header, body);
  return card;
}

function roundStatsSvg(chart, rounds) {
  const svg = createSvg("svg");
  svg.setAttribute("viewBox", "0 0 360 176");
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", chart.title);
  svg.classList.add("round-stats-chart");

  const plot = { left: 47, top: 12, right: 14, bottom: 38 };
  const width = 360 - plot.left - plot.right;
  const height = 176 - plot.top - plot.bottom;
  const allValues = chart.series
    .flatMap((series) => rounds.map((round) => series.value(round)))
    .filter(roundStatsFinite);
  const scale = roundStatsScale(allValues, chart.unit);
  const xFor = (index) => plot.left + (rounds.length === 1 ? width / 2 : width * index / (rounds.length - 1));
  const yFor = (value) => plot.top + (scale.max - value) / (scale.max - scale.min) * height;

  appendRoundStatsGrid(svg, plot, width, height, scale, chart.unit);

  chart.series.forEach((series, seriesIndex) => {
    const points = rounds
      .map((round, index) => {
        const value = series.value(round);
        if (!roundStatsFinite(value)) {
          return null;
        }
        return { x: xFor(index), y: yFor(value), value, round };
      })
      .filter(Boolean);
    if (!points.length) {
      return;
    }

    const line = createSvg("polyline");
    line.setAttribute("points", points.map((point) => `${point.x.toFixed(2)},${point.y.toFixed(2)}`).join(" "));
    line.classList.add("round-stats-line", `series-${seriesIndex + 1}`);
    svg.appendChild(line);

    for (const point of points) {
      const dot = createSvg("circle");
      dot.setAttribute("cx", point.x.toFixed(2));
      dot.setAttribute("cy", point.y.toFixed(2));
      dot.setAttribute("r", "3.2");
      dot.classList.add("round-stats-dot", `series-${seriesIndex + 1}`);
      svg.appendChild(dot);

      const hitArea = createSvg("circle");
      hitArea.setAttribute("cx", point.x.toFixed(2));
      hitArea.setAttribute("cy", point.y.toFixed(2));
      hitArea.setAttribute("r", "9");
      hitArea.setAttribute("tabindex", "0");
      hitArea.setAttribute("aria-label", roundStatsTooltipLabel(series, point.round));
      hitArea.classList.add("round-stats-hit-area");
      setValidatorTooltip(hitArea, roundStatsTooltipLines(series, point.round));
      svg.appendChild(hitArea);
    }
  });

  appendRoundStatsXAxis(svg, plot, width, rounds);
  return svg;
}

function appendRoundStatsGrid(svg, plot, width, height, scale, unit) {
  const ticks = [scale.max, (scale.min + scale.max) / 2, scale.min];
  ticks.forEach((value, index) => {
    const y = plot.top + height * index / 2;
    const line = createSvg("line");
    line.setAttribute("x1", String(plot.left));
    line.setAttribute("x2", String(plot.left + width));
    line.setAttribute("y1", y.toFixed(2));
    line.setAttribute("y2", y.toFixed(2));
    line.classList.add("round-stats-grid-line");
    svg.appendChild(line);

    const label = createSvg("text");
    label.setAttribute("x", String(plot.left - 8));
    label.setAttribute("y", String(y + 3));
    label.setAttribute("text-anchor", "end");
    label.classList.add("round-stats-axis-label");
    label.textContent = roundStatsAxisLabel(value, unit);
    svg.appendChild(label);
  });

  const axis = createSvg("path");
  axis.setAttribute("d", `M${plot.left} ${plot.top}V${plot.top + height}H${plot.left + width}`);
  axis.classList.add("round-stats-axis");
  svg.appendChild(axis);
}

function appendRoundStatsXAxis(svg, plot, width, rounds) {
  const baseline = 176 - plot.bottom + 11;
  rounds.forEach((round, index) => {
    const x = plot.left + (rounds.length === 1 ? width / 2 : width * index / (rounds.length - 1));
    const tick = createSvg("line");
    tick.setAttribute("x1", x.toFixed(2));
    tick.setAttribute("x2", x.toFixed(2));
    tick.setAttribute("y1", String(176 - plot.bottom));
    tick.setAttribute("y2", String(176 - plot.bottom + 4));
    tick.classList.add("round-stats-axis-tick");
    svg.appendChild(tick);

    const label = createSvg("text");
    label.setAttribute("x", x.toFixed(2));
    label.setAttribute("y", String(baseline + 12));
    label.setAttribute("text-anchor", index === 0 ? "start" : index === rounds.length - 1 ? "end" : "middle");
    label.classList.add("round-stats-x-label");
    label.textContent = String(round.round_id);
    svg.appendChild(label);
  });

}

function roundStatsScale(values, unit) {
  if (!values.length) {
    return { min: 0, max: 1 };
  }

  let min = Math.min(...values);
  let max = Math.max(...values);
  if (min === max) {
    const pad = unit === "count" ? 1 : Math.max(Math.abs(max) * 0.01, unit === "percent" ? 0.1 : 1);
    min -= pad;
    max += pad;
  } else {
    const pad = (max - min) * 0.16;
    min -= pad;
    max += pad;
  }

  if (unit === "count") {
    min = Math.floor(min);
    max = Math.ceil(max);
    if (min === max) {
      max += 1;
    }
  }

  return { min, max };
}

function roundStatsAxisLabel(value, unit) {
  if (!roundStatsFinite(value)) {
    return "-";
  }
  if (unit === "percent") {
    return `${value.toFixed(Math.abs(value) >= 10 ? 1 : 2)}%`;
  }
  if (unit === "count") {
    return String(Math.round(value));
  }
  return compactRoundStatsAmount(value);
}

function compactRoundStatsAmount(value) {
  const abs = Math.abs(value);
  if (abs >= 1_000_000_000) {
    return `${(value / 1_000_000_000).toFixed(1)}B`;
  }
  if (abs >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(1)}M`;
  }
  if (abs >= 1_000) {
    return `${(value / 1_000).toFixed(1)}K`;
  }
  return value.toFixed(abs >= 10 ? 0 : 2);
}

function roundStatsAmount(display, raw) {
  const displayNumber = Number(String(display || "").replace(/,/g, ""));
  if (Number.isFinite(displayNumber)) {
    return displayNumber;
  }
  const rawNumber = Number(raw);
  return Number.isFinite(rawNumber) ? rawNumber : NaN;
}

function formatRoundStatsPercent(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) {
    return "-";
  }
  return `${number.toFixed(2)}%`;
}

function formatRoundStatsExactAmount(value) {
  if (!value && value !== 0) {
    return "-";
  }
  const number = Number(String(value).replace(/,/g, ""));
  if (!Number.isFinite(number)) {
    return String(value);
  }
  return number.toLocaleString(undefined, {
    minimumFractionDigits: 0,
    maximumFractionDigits: 9,
  });
}

function formatRoundStatsExactPercent(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) {
    return "-";
  }
  return `${number.toFixed(2)}%`;
}

function roundStatsTooltipLabel(series, round) {
  return `Round ${round.round_id}, ${series.label}: ${series.tooltip(round)}`;
}

function roundStatsTooltipLines(series, round) {
  return [
    `Round: ${round.round_id}`,
    `${series.label}: ${series.tooltip(round)}`,
  ];
}

function roundStatsFinite(value) {
  return Number.isFinite(Number(value));
}

function createSvg(tagName) {
  return document.createElementNS("http://www.w3.org/2000/svg", tagName);
}
