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
  const inner = 76;
  const electionRadius = 229;

  drawDefs(svg);

  drawCircle(svg, center, center, 242, "url(#clockAura)", "none", 0);
  drawCircle(svg, center, center, 223, "rgba(6, 13, 20, 0.68)", "rgba(86, 191, 243, 0.24)", 1);
  drawCircle(svg, center, center, 214, "none", "url(#outerRim)", 3);

  for (const segment of model.baseSegments) {
    const slice = drawDonutSlice(svg, center, center, outer, inner, segment.startAngle, segment.sweepAngle, segment.color, 0);
    slice.setAttribute("filter", "url(#roundGlow)");
  }

  drawDonutSlice(svg, center, center, outer + 1, inner - 2, Math.PI / 2, Math.PI * 2 - 0.0001, "url(#dialGloss)", 0);
  drawCircle(svg, center, center, outer + 6, "none", "rgba(140, 213, 255, 0.18)", 1);
  drawCircle(svg, center, center, inner + 1, "url(#centerWell)", "rgba(255, 255, 255, 0.04)", 1);
  drawClockTicks(svg, center, center, outer, inner);
  drawSeam(svg, center, inner, outer);
  drawArcStroke(svg, center, center, electionRadius, model.electionArc.startAngle, model.electionArc.sweepAngle, "url(#electionArc)", 11, "url(#arcGlow)");
  drawArcEndpoint(svg, center, center, electionRadius, model.electionArc.startAngle);
  drawArcEndpoint(svg, center, center, electionRadius, model.electionArc.startAngle + model.electionArc.sweepAngle);
  drawNeedle(svg, center, center, electionRadius + 4, model.angle);
  drawCircle(svg, center, center, 17, "rgba(255, 255, 255, 0.08)", "rgba(255, 255, 255, 0.1)", 1);
  drawCircle(svg, center, center, 11, "url(#hub)", "rgba(255, 255, 255, 0.28)", 1);
  drawCircle(svg, center, center, 5, "rgba(255, 185, 190, 0.9)", "none", 0);
}

function drawDefs(svg) {
  const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
  defs.innerHTML = `
    <filter id="roundGlow" x="-18%" y="-18%" width="136%" height="136%">
      <feDropShadow dx="0" dy="18" stdDeviation="20" flood-color="#000712" flood-opacity="0.46"/>
      <feDropShadow dx="0" dy="0" stdDeviation="7" flood-color="#36bdf6" flood-opacity="0.16"/>
    </filter>
    <filter id="arcGlow" x="-28%" y="-28%" width="156%" height="156%">
      <feDropShadow dx="0" dy="0" stdDeviation="4" flood-color="#ffd66a" flood-opacity="0.74"/>
      <feDropShadow dx="0" dy="0" stdDeviation="10" flood-color="#d69835" flood-opacity="0.38"/>
    </filter>
    <filter id="needleShadow" x="-26%" y="-26%" width="152%" height="152%">
      <feDropShadow dx="0" dy="7" stdDeviation="6" flood-color="#02050a" flood-opacity="0.5"/>
      <feDropShadow dx="0" dy="0" stdDeviation="2" flood-color="#ff5361" flood-opacity="0.28"/>
    </filter>
    <radialGradient id="clockAura" cx="50%" cy="48%" r="50%">
      <stop offset="0" stop-color="#193142" stop-opacity="0.38"/>
      <stop offset="0.56" stop-color="#0e2333" stop-opacity="0.18"/>
      <stop offset="1" stop-color="#061018" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="outerRim" x1="92" y1="76" x2="424" y2="436" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#57caff" stop-opacity="0.56"/>
      <stop offset="0.5" stop-color="#d7f7ff" stop-opacity="0.16"/>
      <stop offset="1" stop-color="#2a6f94" stop-opacity="0.44"/>
    </linearGradient>
    <linearGradient id="blueRound" x1="104" y1="82" x2="236" y2="430" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#66e7ff"/>
      <stop offset="0.42" stop-color="#26aef0"/>
      <stop offset="0.74" stop-color="#1d85cc"/>
      <stop offset="1" stop-color="#155f9e"/>
    </linearGradient>
    <linearGradient id="greenRound" x1="304" y1="80" x2="420" y2="430" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#7df09d"/>
      <stop offset="0.45" stop-color="#37c873"/>
      <stop offset="0.76" stop-color="#269b5e"/>
      <stop offset="1" stop-color="#1d714b"/>
    </linearGradient>
    <radialGradient id="dialGloss" cx="35%" cy="23%" r="76%">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.2"/>
      <stop offset="0.2" stop-color="#ffffff" stop-opacity="0.08"/>
      <stop offset="0.62" stop-color="#ffffff" stop-opacity="0"/>
      <stop offset="1" stop-color="#000000" stop-opacity="0.22"/>
    </radialGradient>
    <radialGradient id="centerWell" cx="48%" cy="40%" r="64%">
      <stop offset="0" stop-color="#162b36"/>
      <stop offset="0.54" stop-color="#09151e"/>
      <stop offset="1" stop-color="#03070b"/>
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
    <linearGradient id="electionArc" x1="344" y1="136" x2="430" y2="394" gradientUnits="userSpaceOnUse">
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

function drawClockTicks(svg, cx, cy, outerRadius, innerRadius) {
  for (let index = 0; index < 80; index += 1) {
    const angle = -Math.PI / 2 + (index / 80) * Math.PI * 2;
    const isMajor = index % 10 === 0;
    const isHalf = index % 5 === 0;
    const start = polar(cx, cy, outerRadius - (isMajor ? 45 : isHalf ? 41 : 36), angle);
    const end = polar(cx, cy, outerRadius - 28, angle);
    const tick = document.createElementNS("http://www.w3.org/2000/svg", "line");
    tick.setAttribute("x1", start.x);
    tick.setAttribute("y1", start.y);
    tick.setAttribute("x2", end.x);
    tick.setAttribute("y2", end.y);
    tick.setAttribute("stroke", isMajor ? "rgba(232, 247, 255, 0.44)" : "rgba(232, 247, 255, 0.22)");
    tick.setAttribute("stroke-width", isMajor ? 1.5 : isHalf ? 1.1 : 0.8);
    tick.setAttribute("stroke-linecap", "round");
    svg.appendChild(tick);
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

function drawArcEndpoint(svg, cx, cy, radius, angle) {
  const point = polar(cx, cy, radius, angle);
  drawCircle(svg, point.x, point.y, 4.8, "#ffe58a", "rgba(255, 255, 255, 0.88)", 1);
}

function drawNeedle(svg, cx, cy, radius, angle) {
  const back = polar(cx, cy, 13, angle + Math.PI);
  const tip = polar(cx, cy, radius, angle);
  const counter = polar(cx, cy, 28, angle + Math.PI);

  drawNeedlePolygon(svg, counter, tip, angle, 7.5, "url(#needle)", "url(#needleShadow)");
  drawNeedlePolygon(svg, back, polar(cx, cy, 76, angle + Math.PI), angle + Math.PI, 5, "rgba(151, 39, 49, 0.72)");
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
