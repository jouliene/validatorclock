function renderRoundPanelsIfNeeded(snapshot, model) {
  const key = [
    snapshot.chain.id,
    snapshot.fetched_at,
    snapshot.current_set.utime_since,
    snapshot.previous_set?.utime_since || "",
    snapshot.next_set?.utime_since || "",
    model.inElections ? "election" : "closed",
    selectedAddressType(snapshot.chain.id),
    selectedSourceDisplayMode(snapshot.chain.id),
  ].join("|");
  if (state.roundRenderKey === key) {
    return;
  }
  state.roundRenderKey = key;
  renderRoundPanels(snapshot, model);
}

function renderRoundPanels(snapshot, model) {
  renderRoundPanel("blue", snapshot, model);
  renderRoundPanel("green", snapshot, model);
  renderRecentRoundPanels(snapshot, model);
}

function renderRoundPanel(color, snapshot, model) {
  const current = snapshot.current_set.round_color === color ? snapshot.current_set : null;
  const next = snapshot.next_set?.round_color === color ? snapshot.next_set : null;
  const previous = model.beforeElections && snapshot.previous_set?.round_color === color ? snapshot.previous_set : null;
  const candidates = model.inElections ? snapshot.election.candidates : [];
  const isActive = Boolean(current);
  const isNext = Boolean(next);
  const list = $(`${color}Validators`);
  const meta = $(`${color}RoundMeta`);
  const badge = $(`${color}RoundBadge`);
  const stats = $(`${color}RoundStats`);
  list.replaceChildren();
  stats.replaceChildren();
  meta.replaceChildren();
  badge.className = "round-badge";

  if (isActive) {
    renderRoundMeta(meta, current, snapshot);
    badge.textContent = "active";
    badge.classList.add("is-active");
    renderRoundStats(stats, current);
    renderValidators(list, current.validators, validatorRenderOptions(snapshot, {
      rewards: true,
      fakeValidatorPeers: tychoSetFakePeers(current),
      fakeSourceTooltip: fakeValidatorTooltip(true),
    }));
    return;
  }

  if (isNext) {
    renderRoundMeta(meta, next, snapshot);
    badge.textContent = "elected";
    renderRoundStats(stats, next);
    renderValidators(list, next.validators, validatorRenderOptions(snapshot, { rewards: true }));
    return;
  }

  if (model.inElections && candidates.length > 0) {
    renderRoundMeta(meta, electionRoundMeta(snapshot), snapshot);
    badge.textContent = "elections open";
    badge.classList.add("is-election");
    renderCandidateStats(stats, candidates);
    renderValidators(list, candidates, validatorRenderOptions(snapshot, { rewards: false }));
    return;
  }

  if (previous) {
    renderRoundMeta(meta, previous, snapshot);
    badge.textContent = "previous";
    badge.classList.add("is-previous");
    renderRoundStats(stats, previous);
    renderValidators(list, previous.validators, validatorRenderOptions(snapshot, {
      rewards: true,
      fakeValidatorPeers: tychoSetFakePeers(previous),
      fakeSourceTooltip: fakeValidatorTooltip(false),
    }));
    return;
  }

  renderWaitingMeta(meta, color);
  badge.textContent = "waiting";
  renderEmptyStats(stats);
  list.appendChild(emptyState("No validators announced for this round yet."));
}

function renderRoundMeta(container, round, snapshot) {
  container.replaceChildren(
    roundMetaItem("Round ID", String(round.utime_since)),
    roundMetaItem("Round", String(calculatedRoundNumber(round, snapshot)))
  );
}

function electionRoundMeta(snapshot) {
  return {
    utime_since: snapshot.current_set.utime_until,
  };
}

function calculatedRoundNumber(round, snapshot) {
  const period = Math.max(1, Number(snapshot.params15.validators_elected_for || 1));
  return Math.floor(Number(round.utime_since) / period);
}

function renderStatusMeta(container, status) {
  container.replaceChildren(roundMetaItem(status, null, true));
}

function renderWaitingMeta(container, color) {
  container.replaceChildren(roundMetaItem("Status", "Waiting"));
}

function roundMetaItem(label, value, strong = false) {
  const item = document.createElement("span");
  item.className = `round-meta-item${strong ? " round-meta-strong" : ""}`;
  if (value == null) {
    item.textContent = label;
    return item;
  }

  const labelNode = document.createElement("span");
  labelNode.className = "round-meta-label";
  labelNode.textContent = label;
  const valueNode = document.createElement("span");
  valueNode.textContent = value;
  item.append(labelNode, valueNode);
  return item;
}

function renderRoundStats(container, round) {
  const totalStake = round.total_stake || sumTokenValues(round.validators, "stake");
  const totalReward = round.total_reward || sumTokenValues(round.validators, "reward");
  const stats = [
    ["Validators", String(round.total)],
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
  for (const [label, value] of stats) {
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
    container.appendChild(item);
  }
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

function renderRecentRoundPanels(snapshot, model) {
  const grid = $("recentRoundsGrid");
  grid.replaceChildren();

  for (const color of ["blue", "green"]) {
    const round = displayedRoundForColor(color, snapshot, model);
    const validators = round?.recent_absent_validators || [];
    grid.appendChild(recentRoundPanel(color, validators, snapshot));
  }

  grid.hidden = false;
}

function displayedRoundForColor(color, snapshot, model) {
  if (snapshot.current_set.round_color === color) {
    return snapshot.current_set;
  }
  if (snapshot.next_set?.round_color === color) {
    return snapshot.next_set;
  }
  if (model.beforeElections && snapshot.previous_set?.round_color === color) {
    return snapshot.previous_set;
  }
  return null;
}

function recentRoundPanel(color, validators, snapshot) {
  const section = document.createElement("section");
  section.className = `recent-round-panel recent-${color}`;
  if (validators.length === 0) {
    section.classList.add("is-empty");
  }

  const heading = document.createElement("div");
  heading.className = "recent-round-heading";
  const title = document.createElement("h2");
  title.append(recentRoundTitleIcon(), document.createTextNode(`Seen in recent ${color} rounds`));
  const count = document.createElement("span");
  count.className = "recent-round-count";
  count.textContent = validators.length === 0 ? "empty" : `${validators.length} absent now`;
  heading.append(title, count);
  section.appendChild(heading);

  const list = document.createElement("div");
  list.className = "validator-list";
  if (validators.length === 0) {
    const empty = document.createElement("div");
    empty.className = "recent-round-empty";
    empty.textContent = "No absent validators";
    list.appendChild(empty);
  } else {
    renderRecentAbsentValidators(list, validators, {
      ...validatorRenderOptions(snapshot),
    });
  }
  section.appendChild(list);

  return section;
}

function validatorRenderOptions(snapshot, extra = {}) {
  return {
    chainId: snapshot.chain.id,
    addressType: selectedAddressType(snapshot.chain.id),
    onAddressTypeChange: setAddressType,
    sourceDisplayMode: selectedSourceDisplayMode(snapshot.chain.id),
    onSourceDisplayModeChange: setSourceDisplayMode,
    glossaryLabels: validatorGlossaryLabelsForSnapshot(snapshot),
    ...extra,
  };
}

function fakeValidatorTooltip(current) {
  return current
    ? "No reachable validator node IP is currently published for this validator public key."
    : "No reachable validator node IP was published for this validator while this round was active.";
}

function tychoSetFakePeers(round) {
  const peers = round && round.fake_validator_peers;
  if (!Array.isArray(peers)) {
    return null;
  }

  return new Set(
    peers
      .map((peer) => String(peer || "").toLowerCase())
      .filter(Boolean)
  );
}

function recentRoundTitleIcon() {
  const icon = document.createElement("span");
  icon.className = "recent-title-icon";
  icon.innerHTML = [
    '<svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">',
    '<path d="M2.5 12s3.4-6.2 9.5-6.2 9.5 6.2 9.5 6.2-3.4 6.2-9.5 6.2S2.5 12 2.5 12Z"></path>',
    '<circle cx="12" cy="12" r="2.7"></circle>',
    '<path d="m17.8 17.8 3 3"></path>',
    '</svg>',
  ].join("");
  return icon;
}
