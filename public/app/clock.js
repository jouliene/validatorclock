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
