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
  const glow = active ? 11 : 8;
  const core = active ? 5.1 : 4.4;
  drawCircle(svg, point.x, point.y, glow, "rgba(255, 214, 98, 0.09)", "none", 0);
  drawCircle(svg, point.x, point.y, core, "#ffe58a", "rgba(255, 255, 255, 0.92)", 1);
  drawSpark(svg, point.x, point.y, angle, active);
}

function drawSpark(svg, x, y, angle, active) {
  const tangent = angle + Math.PI / 2;
  const radialSize = active ? 13 : 9;
  const tangentSize = active ? 10 : 7;
  drawCenteredLine(svg, x, y, angle, radialSize, active ? "rgba(255, 244, 174, 0.46)" : "rgba(255, 232, 142, 0.24)", active ? 1.8 : 1.2);
  drawCenteredLine(svg, x, y, tangent, tangentSize, active ? "rgba(255, 244, 174, 0.36)" : "rgba(255, 232, 142, 0.2)", active ? 1.5 : 1);
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
