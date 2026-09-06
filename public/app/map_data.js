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

async function refreshValidatorMapNodesForSnapshot(chainId = state.selectedChainId, force = false) {
  const snapshot = validatorMapSnapshotForChain(chainId);
  if (!mapAvailableForChain(chainId)) {
    if (chainId === state.selectedChainId) {
      state.validatorMapNodesByPeer = null;
      validatorMapNodes = null;
      validatorMapNodesChainId = null;
    }
    return [];
  }

  // Nothing to ask about yet. The nodes are only meaningful next to a validator set - the
  // answer is filtered down to it - so without one the request was made, thrown away, and
  // its empty result written into the cache under the key "no-snapshot".
  if (!snapshot) {
    return state.validatorMapNodesByChain.get(chainId) || [];
  }

  const cacheKey = validatorMapSnapshotCacheKey(snapshot);
  // The map file is republished by the resolver every few minutes, so asking again within
  // a round is right - asking on every poll, four times more often than the file can
  // change, is not.
  if (!force && validatorMapNodesAreRecent(chainId, cacheKey)) {
    return state.validatorMapNodesByChain.get(chainId) || [];
  }

  return dedupedRequest(state.validatorMapFetchesByChain, `${chainId}:${cacheKey}`, () =>
    fetchValidatorMapNodesForChain(chainId, snapshot, cacheKey),
  );
}

function validatorMapNodesAreRecent(chainId, cacheKey) {
  if (state.validatorMapNodeCacheKeysByChain.get(chainId) !== cacheKey) {
    return false;
  }
  const fetchedAt = state.validatorMapFetchedAtByChain.get(chainId);
  return Boolean(fetchedAt) && Math.trunc(Date.now() / 1000) - fetchedAt < Math.max(10, state.refreshSeconds || 60);
}

async function fetchValidatorMapNodesForChain(chainId, snapshot, cacheKey) {
  let nodes = [];
  try {
    const response = await fetchJson(`/api/chains/${encodeURIComponent(chainId)}/map`);
    nodes = Array.isArray(response) ? response : [];
  } catch (error) {
    // A map that could not be loaded is not a map with nothing on it, and writing this
    // failure into the cache said it was: every later attempt found an entry, treated it
    // as an answer, and stopped asking. What was already known for this round stands -
    // it belongs to the same validator set - and a round nothing is known about still
    // shows as empty rather than as somebody else's picture.
    console.warn(`Unable to load ${chainId} map nodes`, error);
    const known = state.validatorMapNodeCacheKeysByChain.get(chainId) === cacheKey
      ? state.validatorMapNodesByChain.get(chainId)
      : null;
    if (chainId === state.selectedChainId) {
      applyValidatorMapNodesForChain(chainId, known || []);
    }
    return known || [];
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

  const list = Array.isArray(nodes) ? nodes : [];
  const fingerprint = validatorMapNodesFingerprint(chainId, list);
  if (fingerprint === validatorMapNodesDrawn) {
    return;
  }
  validatorMapNodesDrawn = fingerprint;

  validatorMapNodesChainId = chainId;
  validatorMapNodes = list;
  state.validatorMapNodesByPeer = validatorMapNodeMapByPeer(validatorMapNodes);
  // The tables show where each validator is and how many are mapped, so they are stale
  // the moment this changes - and nothing else tells them.
  state.validatorMapNodesVersion += 1;
  updateValidatorMapTitle();
  updateValidatorMapSummary();
  refreshValidatorMapSource();
  renderNodeStatsIfOpen();
}

function storeValidatorMapNodesForChain(chainId, nodes, cacheKey = validatorMapSnapshotCacheKey(validatorMapSnapshotForChain(chainId))) {
  state.validatorMapNodesByChain.set(chainId, Array.isArray(nodes) ? nodes : []);
  state.validatorMapNodeCacheKeysByChain.set(chainId, cacheKey);
  state.validatorMapFetchedAtByChain.set(chainId, Math.trunc(Date.now() / 1000));
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

function prefetchValidatorMapNodes() {
  prefetchChainsInTurn(
    state.chains.map((chain) => chain.id).filter((chainId) => chainId && mapAvailableForChain(chainId)),
    prefetchValidatorMapNodesForChain,
    "map nodes",
  );
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

  return refreshValidatorMapNodesForSnapshot(chainId, force);
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
  // The server knows which chains it has a map file for and says so in the chain list.
  // This used to be a set of chain ids written into the page, which meant a chain given a
  // map on the server stayed "not available" here until someone remembered to edit it.
  return state.chains.some((chain) => chain.id === chainId && chain.has_map);
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

// A node the resolver could not reach on its latest pass, and is offering from
// memory. It keeps the address for an hour after the node last answered, so a
// point on the map is not by itself a claim that anyone reached it just now -
// and the page should not present it as one.
//
// The threshold is relative rather than absolute: every node the resolver did
// reach in a pass carries that pass's timestamp exactly, so anything behind the
// freshest in the file sat the pass out. The margin is there only so a node is
// not called remembered for being a few seconds out of step; it is well inside
// one refresh, which is five minutes. Reading the file's own age instead would
// only ever tell us when it was written.
const VALIDATOR_MAP_REMEMBERED_AFTER_SECONDS = 120;

function validatorMapNewestSeenAt(nodes) {
  let newest = 0;
  for (const node of nodes || []) {
    const seenAt = Number(node?.last_seen_at) || 0;
    if (seenAt > newest) {
      newest = seenAt;
    }
  }
  return newest;
}

function validatorMapNodeIsRemembered(node, newestSeenAt) {
  const seenAt = Number(node?.last_seen_at) || 0;
  if (!seenAt || !newestSeenAt) {
    return false;
  }
  return newestSeenAt - seenAt >= VALIDATOR_MAP_REMEMBERED_AFTER_SECONDS;
}

function validatorMapLastSeenLabel(node, newestSeenAt) {
  if (!validatorMapNodeIsRemembered(node, newestSeenAt)) {
    return null;
  }
  // Whether it is remembered is decided against the rest of the file; how long
  // ago is decided against the clock, because that is what "ago" means to a
  // reader.
  const seenAt = Number(node?.last_seen_at) || 0;
  const minutes = Math.max(1, Math.round((Date.now() / 1000 - seenAt) / 60));
  return minutes < 60 ? `${minutes} min ago` : `${Math.round(minutes / 60)} h ago`;
}
