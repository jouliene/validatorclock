async function loadValidatorMapNodes() {
  const chainId = state.selectedChainId;
  const cached = applyCachedValidatorMapNodesForChain(chainId);
  if (cached) {
    refreshValidatorMapNodesForSnapshot(chainId).catch((error) => {
      console.warn(`Unable to refresh ${chainId} map nodes`, error);
    });
    return cached;
  }

  return refreshValidatorMapNodesForSnapshot(chainId);
}

async function refreshValidatorMapNodesForSnapshot(chainId = state.selectedChainId) {
  const snapshot = validatorMapSnapshotForChain(chainId);
  if (!mapAvailableForChain(chainId)) {
    if (chainId === state.selectedChainId) {
      state.validatorMapNodesByPeer = null;
      validatorMapNodes = null;
      validatorMapNodesChainId = null;
    }
    return [];
  }

  const cacheKey = validatorMapSnapshotCacheKey(snapshot);
  const fetchKey = `${chainId}:${cacheKey}`;
  const pending = state.validatorMapFetchesByChain.get(fetchKey);
  if (pending) {
    return pending;
  }

  const request = fetchValidatorMapNodesForChain(chainId, snapshot, cacheKey).finally(() => {
    if (state.validatorMapFetchesByChain.get(fetchKey) === request) {
      state.validatorMapFetchesByChain.delete(fetchKey);
    }
  });
  state.validatorMapFetchesByChain.set(fetchKey, request);
  return request;
}

async function fetchValidatorMapNodesForChain(chainId, snapshot, cacheKey) {
  let nodes = [];
  try {
    const response = await fetchJson(`/api/chains/${encodeURIComponent(chainId)}/map`);
    nodes = Array.isArray(response) ? response : [];
  } catch (error) {
    if (chainId === BUNDLED_TYCHO_MAP_CHAIN_ID) {
      console.warn("Using bundled Tycho map nodes", error);
      nodes = Array.isArray(window.TYCHO_NODES) ? window.TYCHO_NODES : [];
    } else {
      console.warn(`Unable to load ${chainId} map nodes`, error);
      nodes = [];
    }
  }

  nodes = enrichValidatorMapNodes(
    filterValidatorMapNodesToCurrentValidators(nodes, snapshot),
    snapshot,
  );
  storeValidatorMapNodesForChain(chainId, nodes, cacheKey);
  if (chainId === state.selectedChainId) {
    applyValidatorMapNodesForChain(chainId, nodes);
  }
  return nodes;
}

function applyCachedValidatorMapNodesForChain(chainId = state.selectedChainId) {
  if (!mapAvailableForChain(chainId)) {
    return null;
  }

  const cacheKey = validatorMapSnapshotCacheKey(validatorMapSnapshotForChain(chainId));
  if (state.validatorMapNodeCacheKeysByChain.get(chainId) !== cacheKey) {
    return null;
  }

  const nodes = state.validatorMapNodesByChain.get(chainId);
  if (!Array.isArray(nodes)) {
    return null;
  }

  if (chainId === state.selectedChainId) {
    applyValidatorMapNodesForChain(chainId, nodes);
  }
  return nodes;
}

function applyValidatorMapNodesForChain(chainId, nodes) {
  if (chainId !== state.selectedChainId) {
    return;
  }

  validatorMapNodesChainId = chainId;
  validatorMapNodes = Array.isArray(nodes) ? nodes : [];
  state.validatorMapNodesByPeer = validatorMapNodeMapByPeer(validatorMapNodes);
  updateValidatorMapTitle();
  updateValidatorMapSummary();
  refreshValidatorMapSource();
  renderNodeStatsIfOpen();
}

function storeValidatorMapNodesForChain(chainId, nodes, cacheKey = validatorMapSnapshotCacheKey(validatorMapSnapshotForChain(chainId))) {
  state.validatorMapNodesByChain.set(chainId, Array.isArray(nodes) ? nodes : []);
  state.validatorMapNodeCacheKeysByChain.set(chainId, cacheKey);
}

function validatorMapSnapshotForChain(chainId) {
  if (chainId === state.selectedChainId && state.snapshot?.chain?.id === chainId) {
    return state.snapshot;
  }
  return state.snapshotsByChain.get(chainId) || null;
}

function validatorMapSnapshotCacheKey(snapshot) {
  if (!snapshot?.current_set) {
    return "no-snapshot";
  }

  const current = snapshot.current_set;
  return [
    snapshot.chain?.id || "",
    current.round_id || "",
    current.round_color || "",
    current.utime_since || "",
    Array.isArray(current.validators) ? current.validators.length : 0,
  ].join("|");
}

async function prefetchValidatorMapNodes() {
  const chainIds = state.chains
    .map((chain) => chain.id)
    .filter((chainId) => chainId && mapAvailableForChain(chainId))
    .sort((left, right) => {
      if (left === state.selectedChainId) {
        return -1;
      }
      if (right === state.selectedChainId) {
        return 1;
      }
      return 0;
    });

  chainIds.forEach((chainId, index) => {
    window.setTimeout(() => {
      prefetchValidatorMapNodesForChain(chainId).catch((error) => {
        console.warn(`Unable to prefetch ${chainId} map nodes`, error);
      });
    }, index * 350);
  });
}

async function prefetchValidatorMapNodesForChain(chainId, force = false) {
  if (!chainId || !mapAvailableForChain(chainId)) {
    return [];
  }

  let snapshot = validatorMapSnapshotForChain(chainId);
  if (!snapshot) {
    snapshot = await prefetchChainSnapshot(chainId);
  }

  if (!snapshot) {
    return [];
  }

  if (!force) {
    const cached = applyCachedValidatorMapNodesForChain(chainId);
    if (cached) {
      return cached;
    }
  }

  return refreshValidatorMapNodesForSnapshot(chainId);
}

const MAP_NODE_RESOLUTION_NOTICE_SECONDS = 5 * 60;
const MAP_NODE_RESOLUTION_NOTICE_TEXT = "The round has just changed. Validator node IP and location data can take up to 5 minutes to resolve. This view will update automatically.";

function mapNodeResolutionNotice(mappedNodeCount = 0, snapshot = state.snapshot, now = Math.trunc(Date.now() / 1000)) {
  const mapped = Number(mappedNodeCount);
  if (Number.isFinite(mapped) && mapped > 0) {
    return "";
  }

  const roundStartedAt = Number(snapshot?.current_set?.utime_since);
  if (!Number.isFinite(roundStartedAt) || now < roundStartedAt) {
    return "";
  }

  return now - roundStartedAt < MAP_NODE_RESOLUTION_NOTICE_SECONDS ? MAP_NODE_RESOLUTION_NOTICE_TEXT : "";
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

function filterValidatorMapNodesToCurrentValidators(nodes, snapshot = state.snapshot) {
  const validators = snapshot?.current_set?.validators;
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

function enrichValidatorMapNodes(nodes, snapshot = state.snapshot) {
  const validators = snapshot?.current_set?.validators;
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
