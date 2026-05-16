async function boot() {
  try {
    startNetworkMessages();
    await loadChains();
    setupTychoMapControls();
    await loadRuntimeStatus();
    await loadClock(true);
    await loadRuntimeStatus();
    startTimers();
  } catch (error) {
    setError(error.message);
  }
}

boot();
