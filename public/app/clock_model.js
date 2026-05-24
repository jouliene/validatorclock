function buildClockModel(snapshot, now) {
  const timings = snapshot.params15;
  const current = snapshot.current_set;
  const next = snapshot.next_set;
  const startBefore = timings.elections_start_before;
  const endBefore = timings.elections_end_before;
  const roundDuration = Math.max(1, current.utime_until - current.utime_since);
  const electionsDuration = Math.max(0, startBefore - endBefore);
  const electionAnchor = next ? next.utime_until : current.utime_until;
  const rawElectionsStart = electionAnchor - startBefore;
  const rawElectionsEnd = electionAnchor - endBefore;
  const electionShift = now > rawElectionsEnd ? roundDuration : 0;
  const electionsStart = rawElectionsStart + electionShift;
  const electionsEnd = rawElectionsEnd + electionShift;
  const activeRoundColor = current.round_color;
  const shift = activeRoundColor === "green" ? 0 : Math.PI;
  const timeToAngle = (timestamp) =>
    -Math.PI / 2 + ((timestamp - current.utime_since) / roundDuration) * Math.PI + shift;
  const angle = timeToAngle(now);
  const inElections = now >= electionsStart && now < electionsEnd;
  const beforeElections = now < electionsStart;

  let status = "After elections";
  let nextChangeAt = current.utime_until;
  if (beforeElections) {
    status = "Before elections";
    nextChangeAt = electionsStart;
  } else if (inElections) {
    status = "Elections open";
    nextChangeAt = electionsEnd;
  }

  return {
    angle,
    baseSegments: [
      { startAngle: Math.PI / 2, sweepAngle: Math.PI, color: "url(#blueRound)", highlight: "rgba(134, 233, 255, 0.42)" },
      { startAngle: Math.PI * 1.5, sweepAngle: Math.PI, color: "url(#greenRound)", highlight: "rgba(135, 244, 169, 0.4)" },
    ],
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
