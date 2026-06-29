(function () {
  const EVENT_ENDPOINT = "/api/analytics/event";
  const PUBLIC_ENDPOINT = "/api/analytics/public";
  const HEARTBEAT_MS = 30_000;
  const STATS_REFRESH_MS = 60_000;

  let analyticsStarted = false;
  let heartbeatTimer = null;
  let statsTimer = null;

  function startAnalytics() {
    if (analyticsStarted) {
      return;
    }
    analyticsStarted = true;

    sendAnalyticsEvent("page_open");
    refreshPublicStats();

    heartbeatTimer = window.setInterval(sendVisibleHeartbeat, HEARTBEAT_MS);
    statsTimer = window.setInterval(refreshPublicStats, STATS_REFRESH_MS);
    document.addEventListener("visibilitychange", handleAnalyticsVisibility);
  }

  function handleAnalyticsVisibility() {
    if (document.visibilityState !== "visible") {
      return;
    }
    sendAnalyticsEvent("heartbeat");
    refreshPublicStats();
  }

  function sendVisibleHeartbeat() {
    if (document.visibilityState === "visible") {
      sendAnalyticsEvent("heartbeat");
    }
  }

  function sendAnalyticsEvent(event) {
    try {
      const payload = JSON.stringify({
        event,
        path: window.location.pathname || "/",
        visible: document.visibilityState === "visible",
        ts: Date.now(),
      });
      const blob = new Blob([payload], { type: "application/json" });
      if (navigator.sendBeacon && navigator.sendBeacon(EVENT_ENDPOINT, blob)) {
        return;
      }
      fetch(EVENT_ENDPOINT, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: payload,
        keepalive: true,
      }).catch(() => {});
    } catch (_) {}
  }

  async function refreshPublicStats() {
    try {
      const response = await fetch(PUBLIC_ENDPOINT, {
        headers: { Accept: "application/json" },
        cache: "no-store",
      });
      if (!response.ok) {
        return;
      }
      renderPublicStats(await response.json());
    } catch (_) {}
  }

  function renderPublicStats(stats) {
    const todayEl = document.getElementById("publicStatsToday");
    const last30El = document.getElementById("publicStatsLast30");
    const allTimeEl = document.getElementById("publicStatsAllTime");
    if (
      !todayEl ||
      !last30El ||
      !allTimeEl ||
      !stats ||
      !stats.today ||
      !stats.last_30_days ||
      !stats.all_time
    ) {
      return;
    }

    renderStatsGroup(todayEl, "Today:", [
      `${formatAnalyticsNumber(stats.today.online_now)} online`,
      `${formatAnalyticsNumber(stats.today.unique_visitors)} unique visitors`,
      `${formatAnalyticsNumber(stats.today.visits)} visits`,
    ]);
    renderStatsGroup(last30El, "Last 30 days:", [
      `${formatAnalyticsNumber(stats.last_30_days.visits)} visits`,
      `${formatAnalyticsNumber(stats.last_30_days.unique_visitors)} unique visitors`,
    ]);
    renderStatsGroup(allTimeEl, "All time:", [
      `${formatAnalyticsNumber(stats.all_time.visits)} visits`,
    ]);
  }

  function renderStatsGroup(element, label, parts) {
    element.replaceChildren();
    const labelEl = document.createElement("span");
    labelEl.className = "public-stats-label";
    labelEl.textContent = label;
    element.append(labelEl, document.createTextNode(` ${parts.join(" · ")}`));
  }

  function formatAnalyticsNumber(value) {
    const number = Number(value);
    if (!Number.isFinite(number) || number < 0) {
      return "0";
    }
    return new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 }).format(number);
  }

  window.startAnalytics = startAnalytics;
})();
