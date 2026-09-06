async function loadClock(force = false) {
  const chainId = state.selectedChainId;
  if (!chainId) {
    return;
  }

  const requestSeq = state.clockRequestSeq + 1;
  state.clockRequestSeq = requestSeq;
  state.clockLoading = true;
  try {
    const snapshot = await fetchClockSnapshot(chainId, force);
    if (requestSeq !== state.clockRequestSeq || chainId !== state.selectedChainId) {
      return;
    }
    await applySelectedClockSnapshot(chainId, snapshot, requestSeq);
  } finally {
    if (requestSeq !== state.clockRequestSeq || chainId !== state.selectedChainId) {
      return;
    }
    state.clockLoading = false;
  }
}

function clockSnapshotUrl(chainId, force = false) {
  const suffix = force ? "?refresh=1" : "";
  return `/api/chains/${encodeURIComponent(chainId)}/clock${suffix}`;
}

function fetchClockSnapshot(chainId, force = false) {
  if (!force) {
    const pending = state.clockFetchesByChain.get(chainId);
    if (pending) {
      return pending;
    }
  }

  const request = fetchJson(clockSnapshotUrl(chainId, force)).finally(() => {
    if (state.clockFetchesByChain.get(chainId) === request) {
      state.clockFetchesByChain.delete(chainId);
    }
  });

  if (!force) {
    state.clockFetchesByChain.set(chainId, request);
  }

  return request;
}

async function applySelectedClockSnapshot(chainId, snapshot, requestSeq) {
  if (requestSeq !== state.clockRequestSeq || chainId !== state.selectedChainId) {
    return;
  }

  state.snapshot = snapshot;
  state.snapshotsByChain.set(chainId, snapshot);
  if (mapAvailableForChain(chainId)) {
    const cachedNodes = applyCachedValidatorMapNodesForChain(chainId);
    if (!cachedNodes) {
      await refreshValidatorMapNodesForSnapshot(chainId);
    } else {
      refreshValidatorMapNodesForSnapshot(chainId).catch((error) => {
        console.warn(`Unable to refresh ${chainId} map nodes`, error);
      });
    }
  } else {
    state.validatorMapNodesByPeer = null;
  }
  if (requestSeq !== state.clockRequestSeq || chainId !== state.selectedChainId) {
    return;
  }
  // The key below tracks everything the panels are built from, the map included, so a
  // poll that brought the same snapshot back - the server refreshes once a minute and
  // answers the other polls with a 304 - no longer tears the tables down and builds them
  // again for a page that would come out identical.
  setError(snapshot.warning || "");
  renderChainTabs();
  renderNow();
  updateStaleSnapshotRetry(chainId, snapshot);
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

// The server answers with the snapshot it has while a refresh of its own is running, and
// says so. Asking once more shortly after gets the new one sooner than the next poll
// would - once per snapshot, not once every five seconds: the key below is what stops
// this from becoming the loop it replaced.
function updateStaleSnapshotRetry(chainId, snapshot) {
  window.clearTimeout(state.staleRetryTimer);
  state.staleRetryTimer = null;
  const warning = snapshot.warning || "";
  const retryKey = `${chainId}:${snapshot.fetched_at}`;
  if (!warning.includes("refresh is running in background") || state.staleRetryKey === retryKey) {
    if (!warning) {
      state.staleRetryKey = null;
    }
    return;
  }

  state.staleRetryKey = retryKey;
  state.staleRetryTimer = window.setTimeout(() => {
    if (state.selectedChainId === chainId) {
      loadClock(false).catch((error) => setError(error.message));
    }
  }, 5000);
}
