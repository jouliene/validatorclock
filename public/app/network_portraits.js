const NETWORK_PORTRAIT_ROTATION_MS = 60 * 1000;
const NETWORK_PORTRAIT_FADE_MS = 1400;
const NETWORK_PORTRAIT_SOURCES = Array.from({ length: 13 }, (_, index) => (
  assetPath(`/brands/portraits/portrait-${String(index).padStart(2, "0")}.webp`)
));

function startNetworkPortraits() {
  const stage = $("networkPortrait");
  const primary = $("networkPortraitPrimary");
  const secondary = $("networkPortraitSecondary");
  const portraits = shuffledNetworkPortraits();
  if (!stage || !primary || !secondary || portraits.length < 2) {
    return;
  }

  let active = primary;
  let standby = secondary;
  let index = 0;
  primary.src = portraits[index];
  primary.classList.add("is-active");
  preloadNetworkPortrait(portraits[1]);

  window.setInterval(() => {
    index = (index + 1) % portraits.length;
    transitionNetworkPortrait(active, standby, portraits[index]);
    [active, standby] = [standby, active];
    preloadNetworkPortrait(portraits[(index + 1) % portraits.length]);
  }, NETWORK_PORTRAIT_ROTATION_MS);
}

function shuffledNetworkPortraits() {
  const portraits = [...NETWORK_PORTRAIT_SOURCES];
  for (let index = portraits.length - 1; index > 0; index -= 1) {
    const swapIndex = Math.floor(networkPortraitRandom() * (index + 1));
    [portraits[index], portraits[swapIndex]] = [portraits[swapIndex], portraits[index]];
  }
  return portraits;
}

function networkPortraitRandom() {
  const values = new Uint32Array(1);
  if (window.crypto?.getRandomValues) {
    window.crypto.getRandomValues(values);
    return values[0] / 0x100000000;
  }
  return Math.random();
}

function transitionNetworkPortrait(active, standby, src) {
  const show = () => {
    standby.classList.add("is-active");
    active.classList.remove("is-active");
    window.setTimeout(() => {
      active.removeAttribute("src");
    }, NETWORK_PORTRAIT_FADE_MS);
  };

  if (standby.src === src && standby.complete) {
    show();
    return;
  }

  standby.onload = () => {
    standby.onload = null;
    show();
  };
  standby.src = src;
}

function preloadNetworkPortrait(src) {
  const image = new Image();
  image.decoding = "async";
  image.src = src;
}
