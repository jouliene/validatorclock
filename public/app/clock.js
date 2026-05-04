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
      { startAngle: Math.PI / 2, sweepAngle: Math.PI, color: "url(#blueRound)" },
      { startAngle: Math.PI * 1.5, sweepAngle: Math.PI, color: "url(#greenRound)" },
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

function clearClock() {
  $("clockSvg").replaceChildren();
}

function drawClock(model) {
  const svg = $("clockSvg");
  svg.replaceChildren();

  const center = 256;
  const outer = 214;
  const inner = 82;
  const electionRadius = 229;

  drawDefs(svg);

  for (const segment of model.baseSegments) {
    drawDonutSlice(svg, center, center, outer, inner, segment.startAngle, segment.sweepAngle, segment.color, 0);
  }

  drawCircle(svg, center, center, inner + 1, palette.center, "none", 0);
  drawArcStroke(svg, center, center, electionRadius, model.electionArc.startAngle, model.electionArc.sweepAngle, model.electionArc.color, 10);
  drawNeedle(svg, center, center, electionRadius + 4, model.angle);
  drawCircle(svg, center, center, 11, "url(#hub)", "rgba(255, 255, 255, 0.24)", 1);
}

function drawDefs(svg) {
  const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
  defs.innerHTML = `
    <linearGradient id="blueRound" x1="104" y1="82" x2="236" y2="430" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#3cc8e9"/>
      <stop offset="0.55" stop-color="#2f9ce1"/>
      <stop offset="1" stop-color="#2378c4"/>
    </linearGradient>
    <linearGradient id="greenRound" x1="304" y1="80" x2="420" y2="430" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#50cc79"/>
      <stop offset="0.55" stop-color="#36b66d"/>
      <stop offset="1" stop-color="#248c59"/>
    </linearGradient>
    <radialGradient id="hub" cx="35%" cy="30%" r="70%">
      <stop offset="0" stop-color="#ff8a92"/>
      <stop offset="0.42" stop-color="#dc3f4d"/>
      <stop offset="1" stop-color="#7e2028"/>
    </radialGradient>
    <linearGradient id="needle" x1="256" y1="72" x2="256" y2="440" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#ff6873"/>
      <stop offset="0.58" stop-color="#dc3f4d"/>
      <stop offset="1" stop-color="#9b2630"/>
    </linearGradient>
  `;
  svg.appendChild(defs);
}

function drawDonutSlice(svg, cx, cy, outerRadius, innerRadius, startAngle, sweepAngle, color, strokeWidth = 2) {
  if (sweepAngle <= 0) {
    return;
  }

  const endAngle = startAngle + Math.min(sweepAngle, Math.PI * 2 - 0.0001);
  const largeArc = endAngle - startAngle > Math.PI ? 1 : 0;
  const startOuter = polar(cx, cy, outerRadius, startAngle);
  const endOuter = polar(cx, cy, outerRadius, endAngle);
  const startInner = polar(cx, cy, innerRadius, startAngle);
  const endInner = polar(cx, cy, innerRadius, endAngle);

  const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute(
    "d",
    [
      `M ${startOuter.x} ${startOuter.y}`,
      `A ${outerRadius} ${outerRadius} 0 ${largeArc} 1 ${endOuter.x} ${endOuter.y}`,
      `L ${endInner.x} ${endInner.y}`,
      `A ${innerRadius} ${innerRadius} 0 ${largeArc} 0 ${startInner.x} ${startInner.y}`,
      "Z",
    ].join(" ")
  );
  path.setAttribute("fill", color);
  path.setAttribute("stroke", palette.seam);
  path.setAttribute("stroke-width", strokeWidth);
  svg.appendChild(path);
}

function drawArcStroke(svg, cx, cy, radius, startAngle, sweepAngle, color, strokeWidth) {
  const endAngle = startAngle + Math.min(sweepAngle, Math.PI * 2 - 0.001);
  const largeArc = Math.abs(endAngle - startAngle) > Math.PI ? 1 : 0;
  const start = polar(cx, cy, radius, startAngle);
  const end = polar(cx, cy, radius, endAngle);
  const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute("d", `M ${start.x} ${start.y} A ${radius} ${radius} 0 ${largeArc} 1 ${end.x} ${end.y}`);
  path.setAttribute("fill", "none");
  path.setAttribute("stroke", color);
  path.setAttribute("stroke-width", strokeWidth);
  path.setAttribute("stroke-linecap", "round");
  svg.appendChild(path);
}

function drawNeedle(svg, cx, cy, radius, angle) {
  const back = polar(cx, cy, 13, angle + Math.PI);
  const tip = polar(cx, cy, radius, angle);
  const shadowTip = { x: tip.x + 2, y: tip.y + 2 };
  const shadowBack = { x: back.x + 2, y: back.y + 2 };

  drawNeedlePolygon(svg, shadowBack, shadowTip, angle, 15, "rgba(0, 0, 0, 0.36)");
  drawNeedlePolygon(svg, back, tip, angle, 11.5, "url(#needle)");
}

function drawNeedlePolygon(svg, back, tip, angle, width, fill) {
  const half = width / 2;
  const backLeft = {
    x: back.x - half * Math.sin(angle),
    y: back.y + half * Math.cos(angle),
  };
  const backRight = {
    x: back.x + half * Math.sin(angle),
    y: back.y - half * Math.cos(angle),
  };

  const needle = document.createElementNS("http://www.w3.org/2000/svg", "polygon");
  needle.setAttribute("points", `${tip.x},${tip.y} ${backLeft.x},${backLeft.y} ${backRight.x},${backRight.y}`);
  needle.setAttribute("fill", fill);
  svg.appendChild(needle);
}

function drawCircle(svg, cx, cy, radius, fill, stroke, strokeWidth) {
  const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
  circle.setAttribute("cx", cx);
  circle.setAttribute("cy", cy);
  circle.setAttribute("r", radius);
  circle.setAttribute("fill", fill);
  circle.setAttribute("stroke", stroke);
  circle.setAttribute("stroke-width", strokeWidth);
  svg.appendChild(circle);
}

function polar(cx, cy, radius, angle) {
  return {
    x: cx + radius * Math.cos(angle),
    y: cy + radius * Math.sin(angle),
  };
}
