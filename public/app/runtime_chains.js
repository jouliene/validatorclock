async function loadChains() {
  const data = await fetchJson("/api/chains");
  // An answer with no list is one this page cannot work from, and saying so here leaves
  // the boot retry something to catch - the next line used to throw a TypeError instead,
  // which the retry in app.js could not tell from a page that had already started.
  if (!Array.isArray(data.chains) || data.chains.length === 0) {
    throw new Error("The server did not list any chains");
  }
  state.chains = data.chains;
  state.refreshSeconds = data.refresh_seconds || 60;
  state.selectedChainId = state.selectedChainId || state.chains[0]?.id;
  renderChainTabs();
}

function renderChainTabs() {
  const tabs = $("chainTabs");
  tabs.replaceChildren();

  for (const chain of state.chains) {
    const isSelected = chain.id === state.selectedChainId;
    const button = document.createElement("button");
    button.type = "button";
    button.className = "chain-tab";
    if (isSelected) {
      button.setAttribute("aria-current", "true");
    }
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

    const networkBadge = document.createElement("span");
    const networkKind = chainNetworkKind(chain);
    networkBadge.className = `chain-network-badge is-${networkKind}`;
    networkBadge.textContent = networkKind;

    const copy = document.createElement("span");
    copy.className = "chain-tab-copy";
    const label = document.createElement("span");
    label.className = "chain-tab-label";
    label.textContent = chainTabLabel(chain);
    copy.append(label, networkBadge);

    main.append(mark, copy);
    button.append(main);

    button.addEventListener("click", () => selectChain(chain.id));
    tabs.appendChild(button);
  }

  updateValidatorMapAvailability();
}

function chainTabLabel(chain) {
  if (chain.id === "tycho-testnet") {
    return "Tycho";
  }
  return chain.name;
}

function chainNetworkKind(chain) {
  return chain.id === "tycho-testnet" ? "testnet" : "mainnet";
}

async function selectChain(chainId) {
  const previousChainId = state.selectedChainId;
  // Clicking the tab that is already active used to reset the render key, rebuild the
  // tabs twice and fetch the whole clock again for a page that would come out identical.
  if (chainId === previousChainId && state.snapshot) {
    return;
  }
  state.selectedChainId = chainId;
  state.roundRenderKey = null;
  if (previousChainId !== chainId) {
    setSelectedValidatorKey(null);
  }
  resetValidatorMapForChainChange(previousChainId, chainId);
  renderChainTabs();
  const cachedSnapshot = state.snapshotsByChain.get(chainId);
  // Whatever this chain's map was last known to be, before anything is drawn from it -
  // the summary and the tables read it, and the snapshot above is what it is counted
  // against.
  if (cachedSnapshot) {
    state.snapshot = cachedSnapshot;
    applyCachedValidatorMapNodesForChain(chainId);
    setError(cachedSnapshot.warning || "");
    renderChainTabs();
    renderNow();
  } else {
    state.snapshot = null;
    clearClock();
    updateValidatorMapRoundBadge();
  }
  handleNodeStatsChainChange(previousChainId, chainId);
  handleRoundStatsChainChange(previousChainId, chainId);
  renderRuntimeStatus(nowSeconds());
  // A chain whose clock will not load leaves the rest of the switch to finish
  // and says why, rather than rejecting out of a click handler and leaving a
  // blank clock with no explanation.
  await loadClock(false).catch((error) => setError(error.message));
  if (state.roundStatsOpen) {
    loadSelectedRoundStats(false).catch((error) => {
      renderRoundStatsError(error);
    });
  }
  loadRuntimeStatus();
}
