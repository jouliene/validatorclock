function renderMetrics(snapshot, model, now) {
  $("metricChain").textContent = snapshot.chain.name;
  $("metricRpc").textContent = snapshot.chain.rpc_label;
  $("metricGlobalId").textContent = snapshot.global_id;
  $("metricSeqno").textContent = snapshot.seqno;
  $("metricStatus").textContent = model.status;
  $("activeRoundWindowCard").style.setProperty("--card-accent", roundAccentColor(snapshot.current_set.round_color));
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
