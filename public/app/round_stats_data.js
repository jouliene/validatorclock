// How long a prefetched set of round statistics is worth reusing. It used to be the poll
// interval exactly - refreshSeconds / 2 - so at every tick the age was equal to it and
// never below it, and both the timer and the clock's own handler refetched every time.
// The server produces new figures once per refreshSeconds, so that is the window.
function roundStatsCacheMaxAgeSeconds() {
  return Math.max(10, state.refreshSeconds || 60);
}

function roundStatsCacheIsFresh(chainId) {
  const cachedAt = state.roundStatsCachedAtByChain.get(chainId);
  if (!cachedAt) {
    return false;
  }

  const age = nowSeconds() - cachedAt;
  return age < roundStatsCacheMaxAgeSeconds();
}

function storeRoundStatsSnapshot(chainId, stats) {
  // Two requests for one chain can be in flight - the panel asks preferring the cache and
  // then asks again for live figures - and they answer in whichever order they answer.
  // The older of the two is not an update.
  const known = state.roundStatsByChain.get(chainId);
  if (known?.fetched_at && stats?.fetched_at && known.fetched_at > stats.fetched_at) {
    return;
  }
  state.roundStatsByChain.set(chainId, stats);
  state.roundStatsCachedAtByChain.set(chainId, nowSeconds());
  if (chainId === state.selectedChainId) {
    renderRoundAprBadges(stats);
  }
}

function prefetchRoundStatsSnapshots() {
  prefetchChainsInTurn(
    state.chains.map((chain) => chain.id).filter(Boolean),
    prefetchRoundStatsForChain,
    "round statistics",
  );
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
  return dedupedRequest(state.roundStatsFetchesByChain, fetchKey, () =>
    fetchJson(roundStatsSnapshotUrl(chainId, preferCache)),
  );
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
    if (!requestIsCurrent(requestSeq, state.roundStatsRequestSeq, chainId)) {
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
    // Only the request still being waited for may cancel the timer: a superseded one
    // clearing it here would cancel the "loading" paint its successor had just scheduled.
    if (requestSeq === state.roundStatsRequestSeq) {
      clearRoundStatsLoadingTimer();
    }
  }
}

function scheduleRoundStatsLoading(requestSeq, chainId) {
  clearRoundStatsLoadingTimer();
  state.roundStatsLoadingTimer = window.setTimeout(() => {
    if (requestIsCurrent(requestSeq, state.roundStatsRequestSeq, chainId)) {
      renderRoundStatsLoading();
    }
  }, PANEL_LOADING_DELAY_MS);
}

function clearRoundStatsLoadingTimer() {
  window.clearTimeout(state.roundStatsLoadingTimer);
  state.roundStatsLoadingTimer = null;
}
