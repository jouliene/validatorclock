async function boot() {
  try {
    await loadChains();
    await loadRuntimeStatus();
    await loadClock(true);
    await loadRuntimeStatus();
    startTimers();
  } catch (error) {
    setError(error.message);
  }
}

boot();
