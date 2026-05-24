async function boot() {
  try {
    startNetworkMessages();
    await loadChains();
    setupValidatorMapControls();
    loadRuntimeStatus();
    window.setTimeout(prefetchChainSnapshots, 0);
    await loadClock(false);
    loadRuntimeStatus();
    startTimers();
  } catch (error) {
    setError(error.message);
  }
}

boot();
