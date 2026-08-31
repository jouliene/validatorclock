function startTimers() {
  window.clearInterval(state.pollTimer);
  window.clearInterval(state.statusTimer);
  window.clearInterval(state.drawTimer);
  window.clearInterval(state.roundStatsPrefetchTimer);
  window.clearInterval(state.validatorMapPrefetchTimer);

  const pollSeconds = refreshPollSeconds();

  state.pollTimer = window.setInterval(() => {
    whenVisible(() => loadClock(false).catch((error) => setError(error.message)));
  }, pollSeconds * 1000);

  state.statusTimer = window.setInterval(() => {
    whenVisible(loadRuntimeStatus);
  }, pollSeconds * 1000);

  state.roundStatsPrefetchTimer = window.setInterval(() => {
    whenVisible(prefetchRoundStatsSnapshots);
  }, pollSeconds * 1000);

  state.validatorMapPrefetchTimer = window.setInterval(() => {
    whenVisible(prefetchValidatorMapNodes);
  }, pollSeconds * 1000);

  state.drawTimer = window.setInterval(renderNow, 1000);

  if (!state.visibilityBound) {
    state.visibilityBound = true;
    document.addEventListener("visibilitychange", handleRuntimeVisibility);
  }
}

// A hidden tab keeps no one informed, so it stops asking the server and catches
// up in one go when it comes back.
function isPageVisible() {
  return document.visibilityState !== "hidden";
}

function whenVisible(action) {
  if (isPageVisible()) {
    action();
  }
}

function handleRuntimeVisibility() {
  if (!isPageVisible()) {
    return;
  }
  loadRuntimeStatus();
  loadClock(false).catch((error) => setError(error.message));
}

function refreshPollSeconds() {
  return Math.max(10, Math.floor(Math.max(10, state.refreshSeconds) / 2));
}

function renderNow() {
  const now = Math.trunc(Date.now() / 1000);
  renderRuntimeStatus(now);
  whenVisible(() => refreshStaleSnapshot(now));

  if (!state.snapshot) {
    return;
  }

  const model = buildClockModel(state.snapshot, now);
  drawClock(model);
  renderMetrics(state.snapshot, model, now);
  updateValidatorMapRoundBadge();
  renderNodeStatsIfOpen();
  renderRoundPanelsIfNeeded(state.snapshot, model);
}
