function renderMetrics(snapshot, model, now) {
  $("metricStatus").textContent = model.status;
  applyRoundAccent(snapshot.current_set.round_color);
  applyNetworkAccent();
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

function applyNetworkAccent() {
  const panel = $("networkPanel");
  if (!panel) {
    return;
  }
  const accents = [
    "rgba(47, 147, 220, 0.82)",
    "rgba(47, 147, 220, 0.032)",
    "rgba(47, 147, 220, 0.13)",
  ];
  const [accent, soft, line] = accents;
  panel.style.setProperty("--network-accent", accent);
  panel.style.setProperty("--network-accent-soft", soft);
  panel.style.setProperty("--network-accent-line", line);
}
