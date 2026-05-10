function renderMetrics(snapshot, model, now) {
  $("metricRpc").textContent = snapshot.chain.rpc_label;
  $("metricStatus").textContent = model.status;
  applyRoundAccent(snapshot.current_set.round_color);
  applyNetworkAccent(snapshot.chain.id);
  renderDateStack($("metricRound"), snapshot.current_set.utime_since, snapshot.current_set.utime_until);
  $("metricRoundEndsIn").textContent = formatDurationPrecise(Math.max(0, snapshot.current_set.utime_until - now));
  renderDateStack($("metricElections"), model.electionsStart, model.electionsEnd);
  $("metricElectionsCountdownLabel").textContent = model.inElections ? "Elections end in" : "Elections start in";
  $("metricElectionsStartIn").textContent = model.inElections
    ? formatDurationPrecise(Math.max(0, model.electionsEnd - now))
    : formatDurationPrecise(Math.max(0, model.electionsStart - now));
  renderInfoUpdated($("metricFetched"), snapshot.fetched_at, now);
}

function roundAccentColor(color) {
  return color === "green" ? "rgba(50, 175, 104, 0.78)" : "rgba(47, 147, 220, 0.78)";
}

function roundAccentTextColor(color) {
  return color === "green" ? "rgba(116, 230, 154, 0.96)" : "rgba(105, 205, 255, 0.96)";
}

function applyRoundAccent(color) {
  const activeRoundCard = $("activeRoundWindowCard");
  activeRoundCard.style.setProperty("--card-accent", roundAccentColor(color));
  activeRoundCard.style.setProperty("--card-accent-text", roundAccentTextColor(color));
  activeRoundCard.style.setProperty("--card-accent-glow", color === "green"
    ? "rgba(50, 175, 104, 0.1)"
    : "rgba(47, 147, 220, 0.11)");
}

function applyNetworkAccent(chainId) {
  const panel = $("networkPanel");
  if (!panel) {
    return;
  }
  const accents = {
    everscale: ["rgba(99, 71, 245, 0.82)", "rgba(99, 71, 245, 0.032)", "rgba(99, 71, 245, 0.13)"],
    "tycho-testnet": ["rgba(46, 204, 113, 0.78)", "rgba(46, 204, 113, 0.03)", "rgba(46, 204, 113, 0.12)"],
    ton: ["rgba(77, 184, 255, 0.82)", "rgba(77, 184, 255, 0.032)", "rgba(77, 184, 255, 0.13)"],
  };
  const [accent, soft, line] = accents[chainId] || accents.ton;
  panel.style.setProperty("--network-accent", accent);
  panel.style.setProperty("--network-accent-soft", soft);
  panel.style.setProperty("--network-accent-line", line);
}
