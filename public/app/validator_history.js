function formatSeenRounds(validator) {
  const rounds = Array.isArray(validator?.history)
    ? validator.history
      .filter((point) => point.status === "participated" && point.round != null)
      .map((point) => Number(point.round))
      .filter((round) => Number.isFinite(round))
    : [];

  if (rounds.length === 0 && validator?.last_seen_round != null) {
    rounds.push(Number(validator.last_seen_round));
  }

  return [...new Set(rounds)]
    .sort((left, right) => right - left)
    .join(", ");
}

function validatorHistoryCell(history) {
  const cell = document.createElement("div");
  cell.className = "validator-cell validator-history";
  const dots = document.createElement("span");
  dots.className = "validator-history-dots";
  const points = Array.isArray(history) && history.length > 0
    ? history
    : Array.from({ length: 5 }, () => ({ status: "unknown" }));

  for (const point of points.slice(0, 5)) {
    const dot = document.createElement("span");
    const status = point.status || "unknown";
    const fakeNode = status === "participated" && point.fake_node === true;
    dot.className = `validator-history-dot is-${status}${fakeNode ? " is-fake-node" : ""}`;
    if (fakeNode) {
      dot.setAttribute("aria-label", "Fake Node participation");
    }
    setValidatorTooltip(
      dot,
      point.round == null
        ? "Round: unknown"
        : [
            `Round: ${point.round}`,
            `Status: ${historyStatusLabel(status)}`,
            ...(fakeNode ? ["Node: Fake Node"] : []),
          ]
    );
    dots.appendChild(dot);
  }

  cell.appendChild(dots);
  return cell;
}

function historyStatusLabel(status) {
  if (status === "participated") {
    return "participated";
  }
  if (status === "missed") {
    return "missed";
  }
  return "unknown";
}
