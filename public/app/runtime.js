function setError(message) {
  const banner = $("errorBanner");
  banner.hidden = !message;
  banner.textContent = message || "";
}

async function loadChains() {
  const data = await fetchJson("/api/chains");
  state.chains = data.chains;
  state.refreshSeconds = data.refresh_seconds || 60;
  state.selectedChainId = state.selectedChainId || state.chains[0]?.id;
  renderChainTabs();
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

function renderChainTabs() {
  const tabs = $("chainTabs");
  tabs.replaceChildren();

  for (const chain of state.chains) {
    const isSelected = chain.id === state.selectedChainId;
    const button = document.createElement("button");
    button.type = "button";
    button.className = "chain-tab";
    button.setAttribute("role", "tab");
    button.setAttribute("aria-selected", String(isSelected));
    button.style.setProperty("--chain-color", palette.blue);

    const main = document.createElement("span");
    main.className = "chain-tab-main";
    const mark = document.createElement("span");
    mark.className = "chain-mark";

    const logoSrc = chainLogos[chain.id];
    if (logoSrc) {
      const logo = document.createElement("img");
      logo.src = logoSrc;
      logo.alt = "";
      logo.decoding = "async";
      mark.append(logo);
    } else {
      mark.classList.add("chain-swatch");
    }

    main.append(mark, document.createTextNode(chainTabLabel(chain)));

    button.append(main);

    button.addEventListener("click", () => selectChain(chain.id));
    tabs.appendChild(button);
  }

  updateTychoMapAvailability();
}

function chainTabLabel(chain) {
  if (chain.id === "tycho-testnet") {
    return "Tycho";
  }
  return chain.name;
}

async function selectChain(chainId) {
  state.selectedChainId = chainId;
  state.roundRenderKey = null;
  renderChainTabs();
  const cachedSnapshot = state.snapshotsByChain.get(chainId);
  if (cachedSnapshot) {
    state.snapshot = cachedSnapshot;
    setError(cachedSnapshot.warning || "");
    renderChainTabs();
    renderNow();
  } else {
    state.snapshot = null;
    clearClock();
  }
  renderRuntimeStatus(Math.trunc(Date.now() / 1000));
  await loadClock(false);
  loadRuntimeStatus();
}

async function loadClock(force = false) {
  const chainId = state.selectedChainId;
  if (!chainId) {
    return;
  }

  const requestSeq = state.clockRequestSeq + 1;
  state.clockRequestSeq = requestSeq;
  state.clockLoading = true;
  state.lastClockRefreshAttempt = Math.trunc(Date.now() / 1000);
  try {
    const snapshot = await fetchClockSnapshot(chainId, force);
    if (requestSeq !== state.clockRequestSeq || chainId !== state.selectedChainId) {
      return;
    }
    await applySelectedClockSnapshot(chainId, snapshot, requestSeq);
  } finally {
    if (requestSeq !== state.clockRequestSeq || chainId !== state.selectedChainId) {
      return;
    }
    state.clockLoading = false;
  }
}

function clockSnapshotUrl(chainId, force = false) {
  const suffix = force ? "?refresh=1" : "";
  return `/api/chains/${encodeURIComponent(chainId)}/clock${suffix}`;
}

function fetchClockSnapshot(chainId, force = false) {
  if (!force) {
    const pending = state.clockFetchesByChain.get(chainId);
    if (pending) {
      return pending;
    }
  }

  const request = fetchJson(clockSnapshotUrl(chainId, force)).finally(() => {
    if (state.clockFetchesByChain.get(chainId) === request) {
      state.clockFetchesByChain.delete(chainId);
    }
  });

  if (!force) {
    state.clockFetchesByChain.set(chainId, request);
  }

  return request;
}

async function applySelectedClockSnapshot(chainId, snapshot, requestSeq) {
  if (requestSeq !== state.clockRequestSeq || chainId !== state.selectedChainId) {
    return;
  }

  state.snapshot = snapshot;
  state.snapshotsByChain.set(chainId, snapshot);
  if (chainId === TYCHO_MAP_CHAIN_ID) {
    await refreshTychoMapNodesForSnapshot();
  } else {
    state.tychoMappedPeers = null;
    state.tychoFakePeers = null;
  }
  if (requestSeq !== state.clockRequestSeq || chainId !== state.selectedChainId) {
    return;
  }
  state.roundRenderKey = null;
  setError(snapshot.warning || "");
  renderChainTabs();
  renderNow();
  updateStaleSnapshotRetry(chainId, snapshot);
}

function prefetchChainSnapshots() {
  for (const chain of state.chains) {
    if (!chain.id || chain.id === state.selectedChainId) {
      continue;
    }
    prefetchChainSnapshot(chain.id);
  }
}

async function prefetchChainSnapshot(chainId) {
  if (state.snapshotsByChain.has(chainId)) {
    return;
  }

  try {
    const snapshot = await fetchClockSnapshot(chainId, false);
    state.snapshotsByChain.set(chainId, snapshot);
  } catch (error) {
    console.warn(`Unable to prefetch ${chainId} clock snapshot`, error);
  }
}

function startTimers() {
  window.clearInterval(state.pollTimer);
  window.clearInterval(state.statusTimer);
  window.clearInterval(state.drawTimer);

  const pollSeconds = refreshPollSeconds();

  state.pollTimer = window.setInterval(() => {
    loadClock(false).catch((error) => setError(error.message));
  }, pollSeconds * 1000);

  state.statusTimer = window.setInterval(() => {
    loadRuntimeStatus();
  }, pollSeconds * 1000);

  state.drawTimer = window.setInterval(renderNow, 1000);
}

function refreshPollSeconds() {
  return Math.max(10, Math.floor(Math.max(10, state.refreshSeconds) / 2));
}

const networkMessageState = {
  messages: [],
  queue: [],
  last: "",
  timer: null,
  fadeMs: 1800,
  holdMs: 10000,
};

async function startNetworkMessages() {
  const element = $("networkMessageText");
  if (!element) {
    return;
  }

  window.clearTimeout(networkMessageState.timer);
  const fallback = element.textContent.trim();
  const messages = await loadNetworkMessages(fallback);
  networkMessageState.messages = messages;
  networkMessageState.queue = [];
  networkMessageState.last = "";
  showNextNetworkMessage(element);
}

async function loadNetworkMessages(fallback) {
  try {
    const data = await fetchJson(assetPath("/jokes.json"));
    const source = Array.isArray(data) ? data : Array.isArray(data.jokes) ? data.jokes : [];
    const messages = source
      .filter((message) => typeof message === "string")
      .map((message) => message.trim())
      .filter(Boolean);
    return messages.length ? messages : [fallback];
  } catch (error) {
    console.warn("Unable to load network messages", error);
    return fallback ? [fallback] : [];
  }
}

function showNextNetworkMessage(element) {
  const message = nextNetworkMessage();
  if (!message) {
    return;
  }

  element.classList.remove("is-visible", "is-exiting");
  element.textContent = message;
  void element.offsetWidth;
  element.classList.add("is-visible");

  networkMessageState.timer = window.setTimeout(() => {
    element.classList.add("is-exiting");
    element.classList.remove("is-visible");
    networkMessageState.timer = window.setTimeout(() => {
      showNextNetworkMessage(element);
    }, networkMessageState.fadeMs + 250);
  }, networkMessageState.holdMs);
}

function nextNetworkMessage() {
  if (!networkMessageState.messages.length) {
    return "";
  }

  if (!networkMessageState.queue.length) {
    networkMessageState.queue = shuffleNetworkMessages(networkMessageState.messages);
    if (
      networkMessageState.queue.length > 1
      && networkMessageState.queue[0] === networkMessageState.last
    ) {
      networkMessageState.queue.push(networkMessageState.queue.shift());
    }
  }

  const message = networkMessageState.queue.shift();
  networkMessageState.last = message;
  return message;
}

function shuffleNetworkMessages(messages) {
  const shuffled = [...messages];
  for (let index = shuffled.length - 1; index > 0; index -= 1) {
    const swapIndex = Math.floor(Math.random() * (index + 1));
    [shuffled[index], shuffled[swapIndex]] = [shuffled[swapIndex], shuffled[index]];
  }
  return shuffled;
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
  renderRoundPanelsIfNeeded(state.snapshot, model);
}

function setAddressType(type) {
  if ((type !== "ever" && type !== "ton") || !state.selectedChainId) {
    return;
  }
  state.addressTypes[state.selectedChainId] = type;
  try {
    window.localStorage?.setItem(ADDRESS_TYPE_KEY, JSON.stringify(state.addressTypes));
  } catch (error) {
    // The preference is optional; private browsing can reject storage writes.
  }
  state.roundRenderKey = null;
  renderNow();
}

function setSourceDisplayMode(mode) {
  if ((mode !== "meta" && mode !== "addr") || state.selectedChainId !== "ton") {
    return;
  }
  state.sourceDisplayModes[state.selectedChainId] = mode;
  try {
    window.localStorage?.setItem(SOURCE_DISPLAY_KEY, JSON.stringify(state.sourceDisplayModes));
  } catch (error) {
    // The preference is optional; private browsing can reject storage writes.
  }
  state.roundRenderKey = null;
  renderNow();
}

function refreshStaleSnapshot(now) {
  if (!state.snapshot || state.clockLoading) {
    return;
  }

  const refreshSeconds = Math.max(10, state.refreshSeconds);
  const age = now - state.snapshot.fetched_at;
  const attemptAge = now - state.lastClockRefreshAttempt;
  if (age >= refreshSeconds && attemptAge >= 5) {
    loadClock(false).catch((error) => setError(error.message));
  }
}

function updateStaleSnapshotRetry(chainId, snapshot) {
  window.clearTimeout(state.staleRetryTimer);
  state.staleRetryTimer = null;
  const warning = snapshot.warning || "";
  const retryKey = `${chainId}:${snapshot.fetched_at}`;
  if (!warning.includes("refresh is running in background") || state.staleRetryKey === retryKey) {
    if (!warning) {
      state.staleRetryKey = null;
    }
    return;
  }

  state.staleRetryKey = retryKey;
  state.staleRetryTimer = window.setTimeout(() => {
    if (state.selectedChainId === chainId) {
      loadClock(false).catch((error) => setError(error.message));
    }
  }, 5000);
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
