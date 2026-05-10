const state = {
  chains: [],
  selectedChainId: null,
  refreshSeconds: 60,
  runtimeStatus: null,
  snapshot: null,
  pollTimer: null,
  statusTimer: null,
  drawTimer: null,
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
