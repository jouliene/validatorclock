const TON_ADDRESS_FORMAT_KEY = "validators-clock-ton-address-format";

function initialTonAddressFormat() {
  try {
    const value = window.localStorage?.getItem(TON_ADDRESS_FORMAT_KEY);
    return value === "raw" || value === "friendly" ? value : "friendly";
  } catch (error) {
    return "friendly";
  }
}

const state = {
  chains: [],
  selectedChainId: null,
  tonAddressFormat: initialTonAddressFormat(),
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
