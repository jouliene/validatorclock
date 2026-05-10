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

function clearClock() {
  $("clockSvg").replaceChildren();
}

function drawClock(model) {
  const svg = $("clockSvg");
  svg.replaceChildren();

  const center = 256;
  const outer = 208;
  const inner = 88;
  const electionRadius = 229;
  const bezelOuter = 242;
  const bezelInner = 214;

  drawDefs(svg);

  drawCircle(svg, center, center, 250, "url(#clockAura)", "none", 0);
  drawDonutSlice(svg, center, center, bezelOuter, bezelInner, -Math.PI / 2, Math.PI * 2 - 0.0001, "url(#bezelFace)", 0).setAttribute("filter", "url(#bezelShadow)");
  drawCircle(svg, center, center, bezelOuter - 1, "none", "url(#bezelEdge)", 2);
  drawCircle(svg, center, center, bezelOuter - 8, "none", "rgba(198, 242, 255, 0.12)", 8);
  drawCircle(svg, center, center, bezelInner + 1, "rgba(5, 12, 18, 0.72)", "rgba(220, 248, 255, 0.12)", 1);
  drawCircle(svg, center, center, bezelInner - 3, "none", "rgba(0, 2, 5, 0.78)", 5);
  drawDonutSlice(svg, center, center, outer + 2, inner - 2, -Math.PI / 2, Math.PI * 2 - 0.0001, "url(#dialTrack)", 0);

  for (const segment of model.baseSegments) {
    const slice = drawDonutSlice(svg, center, center, outer, inner, segment.startAngle, segment.sweepAngle, segment.color, 0);
    slice.setAttribute("filter", "url(#roundGlow)");
    drawArcStroke(svg, center, center, outer - 4, segment.startAngle, segment.sweepAngle, segment.highlight, 4.2);
  }

  drawDonutSlice(svg, center, center, outer + 1, inner - 1, Math.PI / 2, Math.PI * 2 - 0.0001, "url(#dialGloss)", 0);
  drawArcStroke(svg, center, center, outer - 13, Math.PI * 1.03, Math.PI * 0.42, "rgba(232, 253, 255, 0.14)", 6);
  drawArcStroke(svg, center, center, outer - 13, Math.PI * 1.55, Math.PI * 0.32, "rgba(226, 255, 235, 0.1)", 5);
  drawCircle(svg, center, center, outer + 5, "none", "rgba(0, 7, 12, 0.72)", 5);
  drawCircle(svg, center, center, outer + 5, "none", "url(#bezelEdge)", 1.8);
  drawCircle(svg, center, center, outer - 2, "none", "rgba(255, 255, 255, 0.08)", 1);
  drawGaugeTicks(svg, center, center, outer, inner);
  drawCircle(svg, center, center, inner + 14, "url(#centerLip)", "rgba(255, 255, 255, 0.06)", 1);
  drawCircle(svg, center, center, inner + 6, "none", "rgba(147, 226, 244, 0.08)", 2);
  drawCircle(svg, center, center, inner + 1, "url(#centerWell)", "rgba(255, 255, 255, 0.07)", 1);
  drawCircle(svg, center, center, inner - 13, "none", "rgba(90, 160, 185, 0.13)", 1);
  drawSeam(svg, center, inner, outer);
  drawArcStroke(svg, center, center, electionRadius, model.electionArc.startAngle, model.electionArc.sweepAngle, "rgba(0, 4, 7, 0.72)", 17);
  drawArcStroke(svg, center, center, electionRadius, model.electionArc.startAngle, model.electionArc.sweepAngle, "url(#electionArc)", 12, "url(#arcGlow)");
  drawArcStroke(svg, center, center, electionRadius, model.electionArc.startAngle, model.electionArc.sweepAngle, "rgba(255, 250, 181, 0.62)", 2.4);
  drawArcEndpoint(svg, center, center, electionRadius, model.electionArc.startAngle, model.inElections);
  drawArcEndpoint(svg, center, center, electionRadius, model.electionArc.startAngle + model.electionArc.sweepAngle, model.inElections);
  drawNeedle(svg, center, center, electionRadius + 4, model.angle);
  drawCircle(svg, center, center, 21, "url(#hubRing)", "rgba(255, 255, 255, 0.14)", 1);
  drawCircle(svg, center, center, 13, "url(#hub)", "rgba(255, 255, 255, 0.3)", 1);
  drawCircle(svg, center, center, 5.5, "rgba(255, 190, 194, 0.94)", "none", 0);
}

function drawDefs(svg) {
  const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
  defs.innerHTML = `
    <filter id="roundGlow" x="-18%" y="-18%" width="136%" height="136%">
      <feDropShadow dx="0" dy="18" stdDeviation="20" flood-color="#000712" flood-opacity="0.46"/>
      <feDropShadow dx="0" dy="0" stdDeviation="8" flood-color="#36bdf6" flood-opacity="0.07"/>
    </filter>
    <filter id="arcGlow" x="-28%" y="-28%" width="156%" height="156%">
      <feDropShadow dx="0" dy="0" stdDeviation="4" flood-color="#ffd66a" flood-opacity="0.42"/>
      <feDropShadow dx="0" dy="0" stdDeviation="10" flood-color="#d69835" flood-opacity="0.2"/>
    </filter>
    <filter id="bezelShadow" x="-18%" y="-18%" width="136%" height="136%">
      <feDropShadow dx="0" dy="24" stdDeviation="22" flood-color="#00040a" flood-opacity="0.62"/>
      <feDropShadow dx="0" dy="0" stdDeviation="8" flood-color="#6fd4ff" flood-opacity="0.08"/>
    </filter>
    <filter id="needleShadow" x="-26%" y="-26%" width="152%" height="152%">
      <feDropShadow dx="0" dy="7" stdDeviation="6" flood-color="#02050a" flood-opacity="0.5"/>
      <feDropShadow dx="0" dy="0" stdDeviation="2" flood-color="#ff5361" flood-opacity="0.14"/>
    </filter>
    <radialGradient id="clockAura" cx="50%" cy="48%" r="50%">
      <stop offset="0" stop-color="#193142" stop-opacity="0.2"/>
      <stop offset="0.56" stop-color="#0e2333" stop-opacity="0.08"/>
      <stop offset="1" stop-color="#061018" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="bezelFace" cx="36%" cy="24%" r="78%">
      <stop offset="0" stop-color="#263a46"/>
      <stop offset="0.28" stop-color="#132431"/>
      <stop offset="0.58" stop-color="#07121b"/>
      <stop offset="0.78" stop-color="#050a11"/>
      <stop offset="1" stop-color="#1d3441"/>
    </radialGradient>
    <linearGradient id="bezelEdge" x1="83" y1="74" x2="430" y2="438" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#a6ecff" stop-opacity="0.72"/>
      <stop offset="0.23" stop-color="#2b80ac" stop-opacity="0.26"/>
      <stop offset="0.52" stop-color="#e7f8ff" stop-opacity="0.18"/>
      <stop offset="0.82" stop-color="#276d8b" stop-opacity="0.34"/>
      <stop offset="1" stop-color="#0f2230" stop-opacity="0.56"/>
    </linearGradient>
    <radialGradient id="dialTrack" cx="46%" cy="42%" r="66%">
      <stop offset="0" stop-color="#112531"/>
      <stop offset="0.52" stop-color="#07141d"/>
      <stop offset="0.82" stop-color="#03080d"/>
      <stop offset="1" stop-color="#000307"/>
    </linearGradient>
    <radialGradient id="blueRound" cx="256" cy="256" r="210" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#02070c"/>
      <stop offset="0.39" stop-color="#07121b"/>
      <stop offset="0.47" stop-color="#0b3852"/>
      <stop offset="0.65" stop-color="#167cc1"/>
      <stop offset="0.86" stop-color="#37c7f8"/>
      <stop offset="1" stop-color="#9cf4ff"/>
    </radialGradient>
    <radialGradient id="greenRound" cx="256" cy="256" r="210" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#02070c"/>
      <stop offset="0.39" stop-color="#06151b"/>
      <stop offset="0.47" stop-color="#0d3828"/>
      <stop offset="0.65" stop-color="#239d5d"/>
      <stop offset="0.86" stop-color="#63e990"/>
      <stop offset="1" stop-color="#b4ffc8"/>
    </radialGradient>
    <radialGradient id="dialGloss" cx="35%" cy="23%" r="76%">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.26"/>
      <stop offset="0.16" stop-color="#ffffff" stop-opacity="0.1"/>
      <stop offset="0.5" stop-color="#ffffff" stop-opacity="0"/>
      <stop offset="0.82" stop-color="#000000" stop-opacity="0.2"/>
      <stop offset="1" stop-color="#000000" stop-opacity="0.38"/>
    </radialGradient>
    <radialGradient id="centerWell" cx="48%" cy="40%" r="64%">
      <stop offset="0" stop-color="#112632"/>
      <stop offset="0.44" stop-color="#07131c"/>
      <stop offset="0.78" stop-color="#02070c"/>
      <stop offset="1" stop-color="#000205"/>
    </radialGradient>
    <radialGradient id="centerLip" cx="50%" cy="45%" r="62%">
      <stop offset="0" stop-color="#000307" stop-opacity="0.9"/>
      <stop offset="0.7" stop-color="#02070c" stop-opacity="0.86"/>
      <stop offset="0.86" stop-color="#11212a" stop-opacity="0.74"/>
      <stop offset="1" stop-color="#5dc2d8" stop-opacity="0.2"/>
    </radialGradient>
    <radialGradient id="hubRing" cx="35%" cy="30%" r="72%">
      <stop offset="0" stop-color="#56636a"/>
      <stop offset="0.34" stop-color="#26333a"/>
      <stop offset="0.72" stop-color="#11191f"/>
      <stop offset="1" stop-color="#020509"/>
    </radialGradient>
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
    <linearGradient id="electionArc" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0" stop-color="#fff5a2"/>
      <stop offset="0.5" stop-color="#ffd462"/>
      <stop offset="1" stop-color="#d59a38"/>
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
  return path;
}

function drawArcStroke(svg, cx, cy, radius, startAngle, sweepAngle, color, strokeWidth, filter = null) {
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
  if (filter) {
    path.setAttribute("filter", filter);
  }
  svg.appendChild(path);
  return path;
}

function drawGaugeTicks(svg, cx, cy, outerRadius, innerRadius) {
  for (let index = 0; index < 96; index += 1) {
    const angle = -Math.PI / 2 + (index / 96) * Math.PI * 2;
    const isMajor = index % 12 === 0;
    const isHalf = index % 6 === 0;
    const startRadius = innerRadius + (isMajor ? 24 : isHalf ? 28 : 32);
    const endRadius = innerRadius + (isMajor ? 56 : isHalf ? 51 : 46);
    const start = polar(cx, cy, startRadius, angle);
    const end = polar(cx, cy, endRadius, angle);
    drawLine(svg, start, end, isMajor ? "rgba(238, 252, 255, 0.48)" : "rgba(226, 249, 255, 0.22)", isMajor ? 1.35 : isHalf ? 0.95 : 0.62);
  }
}

function drawSeam(svg, center, innerRadius, outerRadius) {
  for (const angle of [-Math.PI / 2, Math.PI / 2]) {
    const start = polar(center, center, innerRadius - 1, angle);
    const end = polar(center, center, outerRadius + 1, angle);
    const seam = document.createElementNS("http://www.w3.org/2000/svg", "line");
    seam.setAttribute("x1", start.x);
    seam.setAttribute("y1", start.y);
    seam.setAttribute("x2", end.x);
    seam.setAttribute("y2", end.y);
    seam.setAttribute("stroke", "rgba(3, 8, 13, 0.82)");
    seam.setAttribute("stroke-width", 2);
    seam.setAttribute("stroke-linecap", "round");
    svg.appendChild(seam);
  }
}

function drawArcEndpoint(svg, cx, cy, radius, angle, active) {
  const point = polar(cx, cy, radius, angle);
  const glow = active ? 14 : 11;
  const core = active ? 5.1 : 4.4;
  drawCircle(svg, point.x, point.y, glow, "rgba(255, 214, 98, 0.13)", "none", 0);
  drawCircle(svg, point.x, point.y, core, "#ffe58a", "rgba(255, 255, 255, 0.92)", 1);
  drawSpark(svg, point.x, point.y, angle, active);
}

function drawSpark(svg, x, y, angle, active) {
  const tangent = angle + Math.PI / 2;
  const radialSize = active ? 13 : 9;
  const tangentSize = active ? 10 : 7;
  drawCenteredLine(svg, x, y, angle, radialSize, active ? "rgba(255, 244, 174, 0.62)" : "rgba(255, 232, 142, 0.34)", active ? 1.8 : 1.2);
  drawCenteredLine(svg, x, y, tangent, tangentSize, active ? "rgba(255, 244, 174, 0.52)" : "rgba(255, 232, 142, 0.28)", active ? 1.5 : 1);
}

function drawNeedle(svg, cx, cy, radius, angle) {
  const tip = polar(cx, cy, radius - 7, angle);
  const tipBase = polar(cx, cy, radius - 25, angle);
  const tail = polar(cx, cy, 62, angle + Math.PI);
  const inner = polar(cx, cy, 17, angle);

  const shadow = drawLine(svg, tail, tip, "rgba(0, 3, 8, 0.58)", 6.4);
  shadow.setAttribute("filter", "url(#needleShadow)");
  drawLine(svg, tail, tip, "rgba(184, 38, 51, 0.82)", 3.6);
  drawLine(svg, inner, polar(cx, cy, radius - 31, angle), "rgba(255, 122, 134, 0.88)", 1.35);
  drawNeedlePolygon(svg, tipBase, tip, angle, 8, "url(#needle)", "url(#needleShadow)");
  drawLine(svg, polar(cx, cy, 15, angle + Math.PI), tail, "rgba(116, 26, 36, 0.58)", 2.6);
}

function drawNeedlePolygon(svg, back, tip, angle, width, fill, filter = null) {
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
  if (filter) {
    needle.setAttribute("filter", filter);
  }
  svg.appendChild(needle);
}

function drawCenteredLine(svg, x, y, angle, length, stroke, strokeWidth) {
  const half = length / 2;
  drawLine(
    svg,
    { x: x - half * Math.cos(angle), y: y - half * Math.sin(angle) },
    { x: x + half * Math.cos(angle), y: y + half * Math.sin(angle) },
    stroke,
    strokeWidth
  );
}

function drawLine(svg, start, end, stroke, strokeWidth) {
  const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
  line.setAttribute("x1", start.x);
  line.setAttribute("y1", start.y);
  line.setAttribute("x2", end.x);
  line.setAttribute("y2", end.y);
  line.setAttribute("stroke", stroke);
  line.setAttribute("stroke-width", strokeWidth);
  line.setAttribute("stroke-linecap", "round");
  svg.appendChild(line);
  return line;
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
