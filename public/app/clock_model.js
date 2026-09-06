function buildClockModel(snapshot, now) {
  const timings = snapshot.params15;
  const current = snapshot.current_set;
  const startBefore = timings.elections_start_before;
  const endBefore = timings.elections_end_before;
  const roundDuration = Math.max(1, current.utime_until - current.utime_since);
  const electionsDuration = Math.max(0, startBefore - endBefore);
  // The election window belongs to the round it falls in: the next set is elected before
  // this round ends. This is the arithmetic the chain itself is read with - minik2's
  // ElectionTimeline::compute, which the server uses - so the page cannot disagree with it
  // about which phase a round is in. The window used to be anchored on the next round and
  // pushed forward as soon as it closed, which left "After elections" unreachable: the
  // hours between the vote and the round change were labelled as being before the vote
  // that had just happened.
  const roundElectionsStart = current.utime_until - startBefore;
  const roundElectionsEnd = current.utime_until - endBefore;
  const inElections = now >= roundElectionsStart && now < roundElectionsEnd;
  const beforeElections = now < roundElectionsStart;
  // Once they are over, the window worth putting in front of the reader is the next
  // round's - which is where the dial has a half for it.
  const electionsAhead = beforeElections || inElections ? 0 : roundDuration;
  const electionsStart = roundElectionsStart + electionsAhead;
  const electionsEnd = roundElectionsEnd + electionsAhead;
  const activeRoundColor = current.round_color;
  const shift = activeRoundColor === "green" ? 0 : Math.PI;
  const timeToAngle = (timestamp) =>
    -Math.PI / 2 + ((timestamp - current.utime_since) / roundDuration) * Math.PI + shift;
  const angle = timeToAngle(now);

  let status = "After elections";
  let nextChangeAt = current.utime_until;
  if (beforeElections) {
    status = "Before elections";
    nextChangeAt = roundElectionsStart;
  } else if (inElections) {
    status = "Elections open";
    nextChangeAt = roundElectionsEnd;
  }

  return {
    angle,
    electionArc: {
      startAngle: timeToAngle(electionsStart),
      sweepAngle: (electionsDuration / roundDuration) * Math.PI,
      color: inElections ? palette.yellow : palette.gold,
    },
    status,
    nextChangeAt,
    electionsStart,
    electionsEnd,
    activeRoundColor,
    inElections,
    beforeElections,
  };
}
