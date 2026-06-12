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

function setRoundStatsOpen(open) {
  const willOpen = Boolean(open);
  if (willOpen && state.validatorMapOpen) {
    setValidatorMapOpen(false);
  }
  if (willOpen && state.nodeStatsOpen) {
    setNodeStatsOpen(false);
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
    state.roundStatsOpen ? "Hide round statistics" : "Show round statistics",
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
  const aprLabel = roundAprCountLabel(apr.count);
  label.textContent = aprLabel;
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
    hasAverage ? `${aprLabel} ${value.textContent}` : "APR unavailable",
  );
}

function roundAprCountLabel(count) {
  return `${Number.isFinite(count) ? Math.max(0, Math.trunc(count)) : 0}-ROUND APR`;
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
