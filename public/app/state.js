const ADDRESS_TYPE_KEY = "validators-clock-address-type";
const SOURCE_DISPLAY_KEY = "validators-clock-source-display";

function initialAddressTypes() {
  try {
    const stored = JSON.parse(window.localStorage?.getItem(ADDRESS_TYPE_KEY) || "{}");
    const types = stored && typeof stored === "object" ? stored : {};
    const legacyTonFormat = window.localStorage?.getItem("validators-clock-ton-address-format");
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
  snapshot: null,
  snapshotsByChain: new Map(),
  pollTimer: null,
  statusTimer: null,
  drawTimer: null,
  staleRetryTimer: null,
  staleRetryKey: null,
  clockLoading: false,
  clockRequestSeq: 0,
  lastClockRefreshAttempt: 0,
  roundRenderKey: null,
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
