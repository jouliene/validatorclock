async function boot() {
  try {
    startNetworkMessages();
    await loadChains();
    setupValidatorMapControls();
    setupRoundStatsControls();
    setupValidatorSelection();
    loadRuntimeStatus();
    window.setTimeout(prefetchRoundStatsSnapshots, 0);
    window.setTimeout(prefetchChainSnapshots, 0);
    await loadClock(false);
    loadRuntimeStatus();
    startTimers();
  } catch (error) {
    setError(error.message);
  }
}

boot();
