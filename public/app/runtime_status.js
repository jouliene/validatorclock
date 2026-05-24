function setError(message) {
  const banner = $("errorBanner");
  banner.hidden = !message;
  banner.textContent = message || "";
}

async function loadRuntimeStatus() {
  try {
    state.runtimeStatus = await fetchJson("/api/status");
    renderRuntimeStatus(Math.trunc(Date.now() / 1000));
  } catch (error) {
    state.runtimeStatus = {
      status: "degraded",
      chains: [],
      error: error.message,
    };
    renderRuntimeStatus(Math.trunc(Date.now() / 1000));
  }
}

function renderRuntimeStatus(now) {
  const container = $("runtimeStatus");
  const label = $("runtimeState");
  const detail = $("runtimeFreshness");
  if (!container || !label || !detail) {
    return;
  }

  const status = state.runtimeStatus;
  const chain = status?.chains?.find((item) => item.id === state.selectedChainId);
  container.hidden = false;
  container.className = "runtime-status is-starting";
  container.title = "Runtime status";

  if (!status) {
    label.textContent = "Starting";
    detail.textContent = "checking";
    return;
  }

  if (status.error) {
    container.className = "runtime-status is-bad";
    container.title = status.error;
    label.textContent = "Status error";
    detail.textContent = "retrying";
    return;
  }

  if (!chain) {
    container.className = status.status === "degraded" ? "runtime-status is-warn" : "runtime-status is-starting";
    label.textContent = status.status === "degraded" ? "Degraded" : "Starting";
    detail.textContent = "warming cache";
    return;
  }

  const displayedSnapshot = state.snapshot?.chain?.id === state.selectedChainId ? state.snapshot : null;
  const freshnessAt = displayedSnapshot?.fetched_at || chain.fetched_at;
  const age = freshnessAt ? Math.max(0, now - freshnessAt) : null;
  if (chain.stale) {
    container.className = "runtime-status is-bad";
    container.title = chain.last_error || "Cached data is stale";
    label.textContent = "Stale";
    detail.textContent = age == null ? "no cache" : `${formatDuration(age)} old`;
    return;
  }

  if (chain.last_error) {
    container.className = "runtime-status is-warn";
    container.title = chain.last_error;
    label.textContent = "Retrying";
    detail.textContent = age == null ? "no cache" : `${formatDuration(age)} old`;
    return;
  }

  if (chain.cached) {
    container.hidden = true;
    container.title = "Runtime status: data fresh";
    label.textContent = "";
    detail.textContent = "";
    return;
  }

  label.textContent = "Starting";
  detail.textContent = "warming cache";
}
