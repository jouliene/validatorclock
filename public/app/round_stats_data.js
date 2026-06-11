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
