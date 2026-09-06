function setError(message) {
  const banner = $("errorBanner");
  banner.hidden = !message;
  banner.textContent = message || "";
}

// Asked for from five places - boot, the poll, coming back to a hidden tab, a chain
// switch - so two can easily be in flight at once, and without this the slower one wins
// whichever it is: a stale "degraded" could land on top of a healthy answer.
async function loadRuntimeStatus() {
  const requestSeq = state.runtimeStatusRequestSeq + 1;
  state.runtimeStatusRequestSeq = requestSeq;
  let status;
  try {
    status = await fetchJson("/api/status");
  } catch (error) {
    status = { status: "degraded", chains: [], error: error.message };
  }
  if (requestSeq !== state.runtimeStatusRequestSeq) {
    return;
  }
  state.runtimeStatus = status;
  renderRuntimeStatus(Math.trunc(Date.now() / 1000));
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
  setValidatorTooltip(container, "Runtime status");

  if (!status) {
    label.textContent = "Starting";
    detail.textContent = "checking";
    return;
  }

  if (status.error) {
    container.className = "runtime-status is-bad";
    setValidatorTooltip(container, status.error);
    label.textContent = "Status error";
    detail.textContent = "retrying";
    return;
  }

  if (!chain) {
    // A server that says it is fine and does not mention this chain is not warming up -
    // it does not have this chain. That happens when the page is left open across a
    // config change, and "Starting / warming cache" was a wait for something that will
    // never arrive.
    const warmingUp = status.status !== "ok";
    container.className = warmingUp ? "runtime-status is-warn" : "runtime-status is-starting";
    label.textContent = warmingUp ? "Degraded" : "No data";
    detail.textContent = warmingUp ? "warming cache" : "chain not reported";
    return;
  }

  const displayedSnapshot = state.snapshot?.chain?.id === state.selectedChainId ? state.snapshot : null;
  const freshnessAt = displayedSnapshot?.fetched_at || chain.fetched_at;
  const age = freshnessAt ? Math.max(0, now - freshnessAt) : null;
  if (chain.stale) {
    container.className = "runtime-status is-bad";
    setValidatorTooltip(container, chain.last_error || "Cached data is stale");
    label.textContent = "Stale";
    detail.textContent = age == null ? "no cache" : `${formatDuration(age)} old`;
    return;
  }

  if (chain.last_error) {
    container.className = "runtime-status is-warn";
    setValidatorTooltip(container, chain.last_error);
    label.textContent = "Retrying";
    detail.textContent = age == null ? "no cache" : `${formatDuration(age)} old`;
    return;
  }

  if (chain.cached) {
    container.hidden = true;
    setValidatorTooltip(container, "Runtime status: data fresh");
    label.textContent = "";
    detail.textContent = "";
    return;
  }

  label.textContent = "Starting";
  detail.textContent = "warming cache";
}
