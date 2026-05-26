function renderRoundStats(container, round, options = {}) {
  const totalStake = round.total_stake || sumTokenValues(round.validators, "stake");
  const totalReward = round.total_reward || sumTokenValues(round.validators, "reward");
  const stats = [
    [
      "Validators",
      String(round.total),
      options.showMapped ? mappedValidatorsDetail(round.validators, options.mapNodesByPeer) : "",
    ],
    ["Total stake", formatStakeAmount(totalStake)],
    ["Total rewards", totalReward ? formatRewardAmount(totalReward) : "-"],
  ];
  renderStats(container, stats);
}

function renderCandidateStats(container, candidates) {
  const stats = [
    ["Candidates", String(candidates.length)],
    ["Total stake", formatStakeAmount(sumTokenValues(candidates, "stake"))],
    ["Total rewards", "-"],
  ];
  renderStats(container, stats);
}

function renderEmptyStats(container) {
  renderStats(container, [
    ["Validators", "-"],
    ["Total stake", "-"],
    ["Total rewards", "-"],
  ]);
}

function renderStats(container, stats) {
  for (const [label, value, detail] of stats) {
    const item = document.createElement("div");
    item.className = "round-stat";
    item.appendChild(roundStatIcon(label));

    const copy = document.createElement("div");
    copy.className = "round-stat-copy";
    const labelNode = document.createElement("span");
    labelNode.textContent = label;
    const valueNode = document.createElement("strong");
    valueNode.textContent = value;
    valueNode.title = value;
    copy.append(labelNode, valueNode);
    item.appendChild(copy);
    if (detail) {
      const detailNode = document.createElement("span");
      detailNode.className = "round-stat-detail";
      detailNode.textContent = detail;
      item.appendChild(detailNode);
    }
    container.appendChild(item);
  }
}

function mappedValidatorsDetail(validators, mapNodesByPeer) {
  const mapped = mappedValidatorsCount(validators, mapNodesByPeer);
  return `mapped: ${mapped}`;
}

function mappedValidatorsCount(validators, mapNodesByPeer) {
  if (!Array.isArray(validators)) {
    return 0;
  }

  return validators.filter((validator) => {
    if (validator?.map_node) {
      return true;
    }
    const peer = String(validator?.public_key || "").toLowerCase();
    return peer && typeof mapNodesByPeer?.has === "function" && mapNodesByPeer.has(peer);
  }).length;
}

function roundStatIcon(label) {
  const icon = document.createElement("span");
  const key = label.toLowerCase();
  icon.className = "round-stat-icon";
  icon.innerHTML = roundStatSvg(key);
  return icon;
}

function roundStatSvg(key) {
  if (key.includes("stake")) {
    return '<svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><ellipse cx="12" cy="5" rx="7" ry="3"></ellipse><path d="M5 5v5c0 1.66 3.13 3 7 3s7-1.34 7-3V5"></path><path d="M5 10v5c0 1.66 3.13 3 7 3s7-1.34 7-3v-5"></path></svg>';
  }
  if (key.includes("reward")) {
    return '<svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><rect x="4" y="8" width="16" height="12" rx="2"></rect><path d="M12 8v12"></path><path d="M4 12h16"></path><path d="M12 8c-1.7 0-4-1-4-3a2 2 0 0 1 4 0Z"></path><path d="M12 8c1.7 0 4-1 4-3a2 2 0 0 0-4 0Z"></path></svg>';
  }
  return '<svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"></path><circle cx="9" cy="7" r="4"></circle><path d="M22 21v-2a4 4 0 0 0-3-3.87"></path><path d="M16 3.13a4 4 0 0 1 0 7.75"></path></svg>';
}
