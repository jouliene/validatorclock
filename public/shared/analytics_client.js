// Shared by the dashboard bundle and the visitor stats page.
const ANALYTICS_EVENT_ENDPOINT = "/api/analytics/event";

function sendAnalyticsEvent(event) {
  try {
    const payload = JSON.stringify({
      event,
      path: window.location.pathname || "/",
      visible: document.visibilityState === "visible",
      ts: Date.now(),
    });
    const blob = new Blob([payload], { type: "application/json" });
    if (navigator.sendBeacon && navigator.sendBeacon(ANALYTICS_EVENT_ENDPOINT, blob)) {
      return;
    }
    fetch(ANALYTICS_EVENT_ENDPOINT, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: payload,
      keepalive: true,
    }).catch(() => {});
  } catch (_) {}
}

function sendVisibleAnalyticsHeartbeat() {
  if (document.visibilityState === "visible") {
    sendAnalyticsEvent("heartbeat");
  }
}

function formatAnalyticsNumber(value) {
  const number = Number(value);
  if (!Number.isFinite(number) || number < 0) {
    return "0";
  }
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 }).format(number);
}
