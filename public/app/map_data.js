async function loadValidatorMapNodes() {
  if (validatorMapNodes && validatorMapNodesChainId === state.selectedChainId) {
    return validatorMapNodes;
  }

  return refreshValidatorMapNodesForSnapshot();
}

async function refreshValidatorMapNodesForSnapshot() {
  const chainId = state.selectedChainId;
  if (!mapAvailableForChain(chainId)) {
    state.validatorMapNodesByPeer = null;
    validatorMapNodes = null;
    validatorMapNodesChainId = null;
    return [];
  }

  try {
    const nodes = await fetchJson(`/api/chains/${encodeURIComponent(chainId)}/map`);
    validatorMapNodes = Array.isArray(nodes) ? nodes : [];
  } catch (error) {
    if (chainId === BUNDLED_TYCHO_MAP_CHAIN_ID) {
      console.warn("Using bundled Tycho map nodes", error);
      validatorMapNodes = Array.isArray(window.TYCHO_NODES) ? window.TYCHO_NODES : [];
    } else {
      console.warn(`Unable to load ${chainId} map nodes`, error);
      validatorMapNodes = [];
    }
  }

  validatorMapNodesChainId = chainId;
  validatorMapNodes = enrichValidatorMapNodes(
    filterValidatorMapNodesToCurrentValidators(validatorMapNodes)
  );
  state.validatorMapNodesByPeer = validatorMapNodeMapByPeer(validatorMapNodes);
  updateValidatorMapTitle();
  updateValidatorMapSummary();
  refreshValidatorMapSource();
  return validatorMapNodes;
}

function mapAvailableForChain(chainId) {
  return MAP_CHAIN_IDS.has(chainId);
}

function currentMapChain() {
  return state.chains.find((chain) => chain.id === state.selectedChainId) || null;
}

function currentMapChainName() {
  return currentMapChain()?.name || state.selectedChainId || "Validator";
}

function validatorMapNodeMapByPeer(nodes) {
  const byPeer = new Map();
  for (const node of Array.isArray(nodes) ? nodes : []) {
    const peer = String(node.peer || "").toLowerCase();
    if (peer) {
      byPeer.set(peer, node);
    }
  }
  return byPeer;
}

function filterValidatorMapNodesToCurrentValidators(nodes) {
  const validators = state.snapshot?.current_set?.validators;
  if (!Array.isArray(nodes) || !Array.isArray(validators)) {
    return [];
  }

  const activePeers = new Set(
    validators
      .map((validator) => String(validator.public_key || "").toLowerCase())
      .filter(Boolean)
  );

  return nodes.filter((node) => activePeers.has(String(node.peer || "").toLowerCase()));
}

function enrichValidatorMapNodes(nodes) {
  const validators = state.snapshot?.current_set?.validators;
  if (!Array.isArray(nodes) || !Array.isArray(validators)) {
    return [];
  }

  const validatorsByPeer = new Map();
  validators.forEach((validator, index) => {
    const peer = String(validator.public_key || "").toLowerCase();
    if (peer) {
      validatorsByPeer.set(peer, { validator, index });
    }
  });

  return nodes.map((node) => {
    const peer = String(node.peer || "").toLowerCase();
    const match = validatorsByPeer.get(peer);
    if (!match) {
      return node;
    }

    const wallet = validatorWalletAddress(match.validator);
    return {
      ...node,
      validator_row: match.index + 1,
      validator_wallet: wallet === "-" ? "" : wallet,
      validator_source: match.validator.source?.address || "",
    };
  });
}
