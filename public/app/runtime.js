function startTimers() {
  window.clearInterval(state.pollTimer);
  window.clearInterval(state.statusTimer);
  window.clearInterval(state.drawTimer);
  window.clearInterval(state.roundStatsPrefetchTimer);

  const pollSeconds = refreshPollSeconds();

  state.pollTimer = window.setInterval(() => {
    loadClock(false).catch((error) => setError(error.message));
  }, pollSeconds * 1000);

  state.statusTimer = window.setInterval(() => {
    loadRuntimeStatus();
  }, pollSeconds * 1000);

  state.roundStatsPrefetchTimer = window.setInterval(() => {
    prefetchRoundStatsSnapshots();
  }, pollSeconds * 1000);

  state.drawTimer = window.setInterval(renderNow, 1000);
}

function refreshPollSeconds() {
  return Math.max(10, Math.floor(Math.max(10, state.refreshSeconds) / 2));
}

function renderNow() {
  const now = Math.trunc(Date.now() / 1000);
  renderRuntimeStatus(now);
  refreshStaleSnapshot(now);

  if (!state.snapshot) {
    return;
  }

  const model = buildClockModel(state.snapshot, now);
  drawClock(model);
  renderMetrics(state.snapshot, model, now);
  updateValidatorMapRoundBadge();
  renderRoundPanelsIfNeeded(state.snapshot, model);
}
