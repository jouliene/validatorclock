async function boot() {
  try {
    startNetworkMessages();
    startNetworkPortraits();
    await loadChains();
    startAnalytics();
    setupValidatorMapControls();
    setupRoundStatsControls();
    setupValidatorSelection();
    loadRuntimeStatus();
    window.setTimeout(prefetchRoundStatsSnapshots, 0);
    window.setTimeout(prefetchChainSnapshots, 0);
    window.setTimeout(prefetchValidatorMapNodes, 250);
    await loadClock(false);
    loadRuntimeStatus();
  } catch (error) {
    setError(error.message);
  } finally {
    // Every later refresh hangs off these timers and each tick handles its own
    // failure, so they start even when the first load did not. Without this a
    // single blip while the page was opening left it dead until a reload.
    startTimers();
  }
}

boot();
