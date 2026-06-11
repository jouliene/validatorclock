async function loadChains() {
  const data = await fetchJson("/api/chains");
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

  updateValidatorMapAvailability();
}

function chainTabLabel(chain) {
  if (chain.id === "tycho-testnet") {
    return "Tycho";
  }
  return chain.name;
}

async function selectChain(chainId) {
  const previousChainId = state.selectedChainId;
  state.selectedChainId = chainId;
  state.roundRenderKey = null;
  if (previousChainId !== chainId) {
    setSelectedValidatorKey(null);
  }
  resetValidatorMapForChainChange(previousChainId, chainId);
  handleRoundStatsChainChange(previousChainId, chainId);
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
    updateValidatorMapRoundBadge();
  }
  renderRuntimeStatus(Math.trunc(Date.now() / 1000));
  await loadClock(false);
  if (state.roundStatsOpen) {
    loadSelectedRoundStats(false).catch((error) => {
      renderRoundStatsError(error);
    });
  }
  loadRuntimeStatus();
}
