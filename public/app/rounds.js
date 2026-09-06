function renderRoundPanelsIfNeeded(snapshot, model) {
  const key = [
    snapshot.chain.id,
    snapshot.fetched_at,
    snapshot.current_set.utime_since,
    snapshot.previous_set?.utime_since || "",
    snapshot.next_set?.utime_since || "",
    model.inElections ? "election" : "closed",
    state.validatorMapNodesVersion,
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
  // Both of these hang off an element in the tables below and are positioned against it.
  // Replacing the rows detaches that element without a mouseleave, so whatever was open
  // would be left floating over the rebuilt table until the next scroll or click.
  hideValidatorTooltip();
  closeValidatorTypeGlossary();
  renderRoundPanel("blue", snapshot, model);
  renderRoundPanel("green", snapshot, model);
  refreshRoundAprBadges();
  renderRecentRoundPanels(snapshot);
  syncSelectedValidatorRows({ clearMissing: true });
}

function renderRoundPanel(color, snapshot, model) {
  const current = snapshot.current_set.round_color === color ? snapshot.current_set : null;
  const next = snapshot.next_set?.round_color === color ? snapshot.next_set : null;
  const previous = snapshot.previous_set?.round_color === color ? snapshot.previous_set : null;
  const candidates = model.inElections ? snapshot.election.candidates : [];
  const isActive = Boolean(current);
  const isNext = Boolean(next);
  const list = $(`${color}Validators`);
  const meta = $(`${color}RoundMeta`);
  const badge = $(`${color}RoundBadge`);
  const stats = $(`${color}RoundStats`);
  const panel = document.querySelector(`.round-${color}`);
  list.replaceChildren();
  stats.replaceChildren();
  meta.replaceChildren();
  badge.className = "round-badge";
  panel?.classList.remove("is-active-round", "is-secondary-round");

  if (isActive) {
    panel?.classList.add("is-active-round");
    renderRoundMeta(meta, current, snapshot);
    badge.textContent = "active";
    badge.classList.add("is-active");
    renderRoundStats(stats, current, {
      showMapped: true,
      mapNodesByPeer: state.validatorMapNodesByPeer,
    });
    renderValidators(list, current.validators, validatorRenderOptions(snapshot, {
      rewards: true,
      validatorSelectionScope: "active",
      validatorSelectionColor: color,
      fakeValidatorPeers: fakeValidatorPeerSet(current),
      fakeSourceTooltip: fakeValidatorTooltip(),
    }));
    return;
  }

  if (isNext) {
    panel?.classList.add("is-secondary-round");
    renderRoundMeta(meta, next, snapshot);
    badge.textContent = "elected";
    renderRoundStats(stats, next);
    renderValidators(list, next.validators, validatorRenderOptions(snapshot, {
      rewards: true,
      validatorSelectionScope: "next",
      validatorSelectionColor: color,
    }));
    return;
  }

  if (model.inElections) {
    panel?.classList.add("is-secondary-round");
    renderRoundMeta(meta, electionRoundMeta(snapshot), snapshot);
    badge.textContent = "elections open";
    badge.classList.add("is-election");
    // An election that is open but has drawn nobody yet is not a round anybody is waiting
    // for: it is this round, before the first stake was placed.
    if (candidates.length === 0) {
      renderEmptyStats(stats);
      list.appendChild(emptyState("Elections are open; no candidates have staked yet."));
      return;
    }
    renderCandidateStats(stats, candidates);
    renderValidators(list, candidates, validatorRenderOptions(snapshot, {
      rewards: false,
      validatorSelectionScope: "candidate",
      validatorSelectionColor: color,
    }));
    return;
  }

  if (previous) {
    panel?.classList.add("is-secondary-round");
    renderRoundMeta(meta, previous, snapshot);
    badge.textContent = "previous";
    badge.classList.add("is-previous");
    renderRoundStats(stats, previous);
    renderValidators(list, previous.validators, validatorRenderOptions(snapshot, {
      rewards: true,
      validatorSelectionScope: "previous",
      validatorSelectionColor: color,
      fakeValidatorPeers: fakeValidatorPeerSet(previous),
      fakeSourceTooltip: fakeValidatorTooltip(),
    }));
    return;
  }

  panel?.classList.add("is-secondary-round");
  renderWaitingMeta(meta, snapshot);
  badge.textContent = "waiting";
  badge.classList.add("is-waiting");
  renderEmptyStats(stats);
  list.appendChild(emptyState("No validators announced for this round yet."));
}
