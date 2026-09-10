(function () {
  const HEARTBEAT_MS = 30_000;

  let analyticsStarted = false;
  let heartbeatTimer = null;

  function startAnalytics() {
    if (analyticsStarted) {
      return;
    }
    analyticsStarted = true;

    sendAnalyticsEvent("page_open");

    heartbeatTimer = window.setInterval(sendVisibleAnalyticsHeartbeat, HEARTBEAT_MS);
    document.addEventListener("visibilitychange", handleAnalyticsVisibility);
  }

  function handleAnalyticsVisibility() {
    if (document.visibilityState !== "visible") {
      return;
    }
    sendAnalyticsEvent("heartbeat");
  }

  window.startAnalytics = startAnalytics;
})();
