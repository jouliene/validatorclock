// Nothing else in the bundle asks for the chain list, and every timer needs a
// selected chain before it can do anything. So when that one request fails the
// page has nothing to recover from on its own - it is asked for again here.
const BOOT_RETRY_MIN_MS = 3000;
const BOOT_RETRY_MAX_MS = 60000;

let bootRetryDelayMs = BOOT_RETRY_MIN_MS;

async function boot() {
  startNetworkMessages();
  startNetworkPortraits();
  await startDashboard();
}

async function startDashboard() {
  try {
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
    bootRetryDelayMs = BOOT_RETRY_MIN_MS;
  } catch (error) {
    setError(error.message);
    // Only the chain list leaves the page with nothing to work from. A clock
    // that failed is picked up by the timers below on their next tick.
    if (!state.chains.length) {
      scheduleDashboardRetry();
    }
  } finally {
    // Every later refresh hangs off these timers and each tick handles its own
    // failure, so they start even when the first load did not. startTimers
    // clears the previous intervals, so calling it again is safe.
    startTimers();
  }
}

function scheduleDashboardRetry() {
  const delay = bootRetryDelayMs;
  bootRetryDelayMs = Math.min(bootRetryDelayMs * 2, BOOT_RETRY_MAX_MS);
  window.setTimeout(() => {
    startDashboard();
  }, delay);
}

boot();
