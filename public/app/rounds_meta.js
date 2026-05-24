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

function renderWaitingMeta(container) {
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
