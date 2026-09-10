async function loadClock(force = false) {
  const chainId = state.selectedChainId;
  if (!chainId) {
    return;
  }

  const requestSeq = state.clockRequestSeq + 1;
  state.clockRequestSeq = requestSeq;
  window.clearTimeout(state.pollTimer);
  state.clockLoading = true;
  renderNow();
  try {
    const snapshot = await fetchClockSnapshot(chainId, force);
    if (!requestIsCurrent(requestSeq, state.clockRequestSeq, chainId)) {
      return;
    }
    await applySelectedClockSnapshot(chainId, snapshot, requestSeq);
  } finally {
    if (!requestIsCurrent(requestSeq, state.clockRequestSeq, chainId)) {
      return;
    }
    state.clockLoading = false;
    scheduleClockRefresh(60_000);
    renderNow();
  }
}

function clockSnapshotUrl(chainId, force = false) {
  const suffix = force ? "?refresh=1" : "";
  return `/api/chains/${encodeURIComponent(chainId)}/clock${suffix}`;
}

function fetchClockSnapshot(chainId, force = false) {
  // A forced refresh is a request for new data, so it is not answered from one already in
  // flight and does not become the answer to anyone else's.
  if (force) {
    return fetchJson(clockSnapshotUrl(chainId, true));
  }
  return dedupedRequest(state.clockFetchesByChain, chainId, () =>
    fetchJson(clockSnapshotUrl(chainId)),
  );
}

async function applySelectedClockSnapshot(chainId, snapshot, requestSeq) {
  if (!requestIsCurrent(requestSeq, state.clockRequestSeq, chainId)) {
    return;
  }

  state.snapshot = snapshot;
  state.snapshotsByChain.set(chainId, snapshot);
  state.clockReceivedAtByChain.set(chainId, performance.now());
  window.clearTimeout(state.clockUpdatedFlashTimer);
  state.clockUpdatedFlashTimer = window.setTimeout(() => whenVisible(renderNow), 250);
  if (mapAvailableForChain(chainId)) {
    applyCachedValidatorMapNodesForChain(chainId);
    // The clock response is ready: an independent map request must not delay it.
    refreshValidatorMapNodesForSnapshot(chainId).catch((error) => {
      console.warn(`Unable to refresh ${chainId} map nodes`, error);
    });
  } else {
    state.validatorMapNodesByPeer = null;
  }
  if (!requestIsCurrent(requestSeq, state.clockRequestSeq, chainId)) {
    return;
  }
  // The key below tracks everything the panels are built from, the map included, so a
  // poll that brought the same snapshot back - the server refreshes once a minute and
  // answers the other polls with a 304 - no longer tears the tables down and builds them
  // again for a page that would come out identical.
  setError(snapshot.warning || "");
  renderChainTabs();
  renderNow();
  handleRoundStatsClockSnapshot(chainId, snapshot);
}

function prefetchChainSnapshots() {
  for (const chain of state.chains) {
    if (!chain.id || chain.id === state.selectedChainId) {
      continue;
    }
    prefetchChainSnapshot(chain.id);
  }
}

async function prefetchChainSnapshot(chainId) {
  if (state.snapshotsByChain.has(chainId)) {
    return state.snapshotsByChain.get(chainId);
  }

  try {
    const snapshot = await fetchClockSnapshot(chainId, false);
    state.snapshotsByChain.set(chainId, snapshot);
    prefetchValidatorMapNodesForChain(chainId).catch((error) => {
      console.warn(`Unable to prefetch ${chainId} map nodes`, error);
    });
    return snapshot;
  } catch (error) {
    console.warn(`Unable to prefetch ${chainId} clock snapshot`, error);
    return null;
  }
}
