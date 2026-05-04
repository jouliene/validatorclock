function renderRoundPanelsIfNeeded(snapshot, model) {
  const key = [
    snapshot.chain.id,
    snapshot.fetched_at,
    snapshot.current_set.utime_since,
    snapshot.previous_set?.utime_since || "",
    snapshot.next_set?.utime_since || "",
    model.inElections ? "election" : "closed",
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
    renderValidators(list, current.validators, { rewards: true });
    return;
  }

  if (isNext) {
    renderRoundMeta(meta, next, snapshot);
    badge.textContent = "elected";
    renderRoundStats(stats, next);
    renderValidators(list, next.validators, { rewards: true });
    return;
  }

  if (model.inElections && candidates.length > 0) {
    renderRoundMeta(meta, electionRoundMeta(snapshot), snapshot);
    badge.textContent = "elections open";
    badge.classList.add("is-election");
    renderCandidateStats(stats, candidates);
    renderValidators(list, candidates, { rewards: false });
    return;
  }

  if (previous) {
    renderRoundMeta(meta, previous, snapshot);
    badge.textContent = "previous";
    badge.classList.add("is-previous");
    renderRoundStats(stats, previous);
    renderValidators(list, previous.validators, { rewards: true });
    return;
  }

  renderWaitingMeta(meta, color);
  badge.textContent = "waiting";
  renderEmptyStats(stats);
  list.appendChild(emptyState("No validators announced for this round yet."));
}

function renderRoundMeta(container, round, snapshot) {
  container.replaceChildren(
    roundMetaItem("Round_Id", String(round.utime_since)),
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
    const labelNode = document.createElement("span");
    labelNode.textContent = label;
    const valueNode = document.createElement("strong");
    valueNode.textContent = value;
    valueNode.title = value;
    item.append(labelNode, valueNode);
    container.appendChild(item);
  }
}

function renderRecentRoundPanels(snapshot, model) {
  const grid = $("recentRoundsGrid");
  grid.replaceChildren();

  for (const color of ["blue", "green"]) {
    const round = displayedRoundForColor(color, snapshot, model);
    const validators = round?.recent_absent_validators || [];
    grid.appendChild(recentRoundPanel(color, validators));
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

function recentRoundPanel(color, validators) {
  const section = document.createElement("section");
  section.className = `recent-round-panel recent-${color}`;
  if (validators.length === 0) {
    section.classList.add("is-empty");
  }

  const heading = document.createElement("div");
  heading.className = "recent-round-heading";
  const title = document.createElement("h2");
  title.textContent = `Seen in recent ${color} rounds`;
  const count = document.createElement("span");
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
    renderRecentAbsentValidators(list, validators);
  }
  section.appendChild(list);

  return section;
}
