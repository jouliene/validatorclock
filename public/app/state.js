const ADDRESS_TYPE_KEY = "validatorclock-address-type";
const SOURCE_DISPLAY_KEY = "validatorclock-source-display";

function initialAddressTypes() {
  try {
    const stored = JSON.parse(window.localStorage?.getItem(ADDRESS_TYPE_KEY) || "{}");
    const types = stored && typeof stored === "object" ? stored : {};
    const legacyTonFormat = window.localStorage?.getItem("validatorclock-ton-address-format");
    if (!types.ton && (legacyTonFormat === "raw" || legacyTonFormat === "friendly")) {
      types.ton = legacyTonFormat === "raw" ? "ever" : "ton";
    }
    return types;
  } catch (error) {
    return {};
  }
}

function defaultAddressType(chainId) {
  return chainId === "ton" ? "ton" : "ever";
}

function selectedAddressType(chainId = state.selectedChainId) {
  return state.addressTypes[chainId] || defaultAddressType(chainId);
}

function initialSourceDisplayModes() {
  try {
    const stored = JSON.parse(window.localStorage?.getItem(SOURCE_DISPLAY_KEY) || "{}");
    return stored && typeof stored === "object" ? stored : {};
  } catch (error) {
    return {};
  }
}

function defaultSourceDisplayMode(chainId) {
  return chainId === "ton" ? "meta" : "addr";
}

function selectedSourceDisplayMode(chainId = state.selectedChainId) {
  return state.sourceDisplayModes[chainId] || defaultSourceDisplayMode(chainId);
}

const state = {
  chains: [],
  selectedChainId: null,
  addressTypes: initialAddressTypes(),
  sourceDisplayModes: initialSourceDisplayModes(),
  refreshSeconds: 60,
  runtimeStatus: null,
  runtimeStatusRequestSeq: 0,
  snapshot: null,
  snapshotsByChain: new Map(),
  clockFetchesByChain: new Map(),
  pollTimer: null,
  statusTimer: null,
  drawTimer: null,
  staleRetryTimer: null,
  staleRetryKey: null,
  clockLoading: false,
  roundStatsOpen: false,
  roundStatsLoadingTimer: null,
  roundStatsRequestSeq: 0,
  roundStatsRenderKey: null,
  roundStatsByChain: new Map(),
  roundStatsCachedAtByChain: new Map(),
  roundStatsFetchesByChain: new Map(),
  roundStatsPrefetchTimer: null,
  validatorMapOpen: false,
  nodeStatsOpen: false,
  nodeStatsLoadingTimer: null,
  nodeStatsRequestSeq: 0,
  nodeStatsRenderKey: null,
  nodeStatsInputKey: null,
  nodeStatsLocationRankingExpanded: false,
  validatorMapNodesByChain: new Map(),
  validatorMapNodeCacheKeysByChain: new Map(),
  validatorMapFetchesByChain: new Map(),
  validatorMapPrefetchTimer: null,
  validatorMapNodesByPeer: null,
  // Bumped whenever the map nodes change, so that what is drawn from them - the tables,
  // their "mapped: N" - can tell one set of nodes from another in a render key.
  validatorMapNodesVersion: 0,
  clockRequestSeq: 0,
  roundRenderKey: null,
  selectedValidatorKey: null,
  visibilityBound: false,
};

const palette = {
  blue: "#2f93dc",
  green: "#32af68",
  yellow: "#ead06a",
  gold: "#caa85c",
  red: "#dc3f4d",
  seam: "#07080c",
  center: "#080a0f",
};

const scriptUrl = document.currentScript?.src ? new URL(document.currentScript.src) : null;
const assetVersion = scriptUrl?.searchParams.get("v") || "";
const assetPath = (path) => assetVersion ? `${path}?v=${encodeURIComponent(assetVersion)}` : path;

const chainLogos = {
  everscale: assetPath("/brands/everscale.svg"),
  "tycho-testnet": assetPath("/brands/tycho.svg"),
  ton: assetPath("/brands/ton.svg"),
};

const $ = (id) => document.getElementById(id);

// Three things are cached per chain here - the clock, the round statistics and the map
// nodes - and each of them needs the same three mechanisms. They are written once here;
// what differs between them, which is how long an answer stays worth keeping, is written
// where that answer is understood.

// One request per key at a time. Callers asking for the same thing while it is in flight
// wait on the same promise, and the entry is removed by the request that owns it, so a
// newer one is never dropped by an older one settling.
function dedupedRequest(requests, key, start) {
  const pending = requests.get(key);
  if (pending) {
    return pending;
  }
  const request = start().finally(() => {
    if (requests.get(key) === request) {
      requests.delete(key);
    }
  });
  requests.set(key, request);
  return request;
}

// The chains the reader is not looking at, asked for one after another rather than all at
// once: three chains starting together made the one on screen wait behind them. The
// selected chain goes first because it is the one about to be needed.
const PREFETCH_STAGGER_MS = 350;

function prefetchChainsInTurn(chainIds, prefetchOne, what) {
  const selectedFirst = chainIds.slice().sort((left, right) => {
    if (left === state.selectedChainId) {
      return -1;
    }
    return right === state.selectedChainId ? 1 : 0;
  });

  selectedFirst.forEach((chainId, index) => {
    window.setTimeout(() => {
      prefetchOne(chainId).catch((error) => {
        console.warn(`Unable to prefetch ${chainId} ${what}`, error);
      });
    }, index * PREFETCH_STAGGER_MS);
  });
}

// A request is still worth acting on only while nothing has superseded it and the reader
// has not moved to another chain. Both halves matter, and the pair was written out eight
// times across three files.
function requestIsCurrent(requestSeq, currentSeq, chainId) {
  return requestSeq === currentSeq && chainId === state.selectedChainId;
}

// Long enough that an answer already in hand never flashes "loading", short enough that a
// slow one does not look stuck.
const PANEL_LOADING_DELAY_MS = 180;
