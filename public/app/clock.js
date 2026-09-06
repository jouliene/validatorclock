// The dial is drawn once and then only the parts that move are redrawn.
//
// It used to be cleared and rebuilt whole on every tick: about a hundred and fifty
// elements plus the block of definitions - eight drop-shadow filters among them, which
// the browser re-rasterises when they reappear - once a second, for a picture in which
// only the needle had moved. The face is fixed, the election arc changes when the round
// or its phase does, and the needle changes every second, so each of them lives in a
// layer of its own and is touched when it has a reason to be.
let clockLayers = null;
let clockElectionArcKey = null;

function clearClock() {
  $("clockSvg").replaceChildren();
  clockLayers = null;
  clockElectionArcKey = null;
}

function drawClock(model) {
  const svg = $("clockSvg");
  const layers = clockFaceLayers(svg);
  drawElectionArc(layers.electionArc, model);
  replaceChildren(layers.needle, []);
  drawNeedle(layers.needle, CLOCK_CENTER, CLOCK_CENTER, CLOCK_ELECTION_RADIUS + 4, model.angle);
}

const CLOCK_CENTER = 256;
const CLOCK_ROUND_HALVES = [
  { startAngle: Math.PI / 2, sweepAngle: Math.PI, color: "url(#blueRound)", highlight: "rgba(134, 233, 255, 0.42)" },
  { startAngle: Math.PI * 1.5, sweepAngle: Math.PI, color: "url(#greenRound)", highlight: "rgba(135, 244, 169, 0.4)" },
];
const CLOCK_ELECTION_RADIUS = 229;

function clockFaceLayers(svg) {
  // The layers are looked for on the element rather than trusted from last time: the
  // page can have cleared the dial between two ticks, and drawing into a group that is
  // no longer in the document is a picture nobody sees.
  if (clockLayers && clockLayers.svg === svg && clockLayers.face.isConnected) {
    return clockLayers;
  }

  svg.replaceChildren();
  drawDefs(svg);
  const face = appendClockLayer(svg);
  const electionArc = appendClockLayer(svg);
  const needle = appendClockLayer(svg);
  const hub = appendClockLayer(svg);
  drawClockFace(face);
  drawClockHub(hub);
  clockLayers = { svg, face, electionArc, needle, hub };
  clockElectionArcKey = null;
  return clockLayers;
}

function appendClockLayer(svg) {
  const layer = document.createElementNS("http://www.w3.org/2000/svg", "g");
  svg.appendChild(layer);
  return layer;
}

// Everything that is the same in every round: bezel, dial, the two round halves, the
// ticks and the seam between them. The halves come from the model, and the model always
// says the same thing about them - a blue half and a green half, fixed to the dial.
function drawClockFace(layer) {
  const center = CLOCK_CENTER;
  const outer = 208;
  const inner = 88;
  const bezelOuter = 242;
  const bezelInner = 214;

  drawCircle(layer, center, center, 250, "url(#clockAura)", "none", 0);
  drawDonutSlice(layer, center, center, bezelOuter, bezelInner, -Math.PI / 2, Math.PI * 2 - 0.0001, "url(#bezelFace)", 0).setAttribute("filter", "url(#bezelShadow)");
  drawCircle(layer, center, center, bezelOuter - 1, "none", "url(#bezelEdge)", 2);
  drawCircle(layer, center, center, bezelOuter - 8, "none", "rgba(198, 242, 255, 0.12)", 8);
  drawCircle(layer, center, center, bezelInner + 1, "rgba(5, 12, 18, 0.72)", "rgba(220, 248, 255, 0.12)", 1);
  drawCircle(layer, center, center, bezelInner - 3, "none", "rgba(0, 2, 5, 0.78)", 5);
  drawDonutSlice(layer, center, center, outer + 2, inner - 2, -Math.PI / 2, Math.PI * 2 - 0.0001, "url(#dialTrack)", 0);

  for (const segment of CLOCK_ROUND_HALVES) {
    const slice = drawDonutSlice(layer, center, center, outer, inner, segment.startAngle, segment.sweepAngle, segment.color, 0);
    slice.setAttribute("filter", "url(#roundGlow)");
    drawArcStroke(layer, center, center, outer - 4, segment.startAngle, segment.sweepAngle, segment.highlight, 4.2);
  }

  drawDonutSlice(layer, center, center, outer + 1, inner - 1, Math.PI / 2, Math.PI * 2 - 0.0001, "url(#dialGloss)", 0);
  drawArcStroke(layer, center, center, outer - 13, Math.PI * 1.03, Math.PI * 0.42, "rgba(232, 253, 255, 0.14)", 6);
  drawArcStroke(layer, center, center, outer - 13, Math.PI * 1.55, Math.PI * 0.32, "rgba(226, 255, 235, 0.1)", 5);
  drawCircle(layer, center, center, outer + 5, "none", "rgba(0, 7, 12, 0.72)", 5);
  drawCircle(layer, center, center, outer + 5, "none", "url(#bezelEdge)", 1.8);
  drawCircle(layer, center, center, outer - 2, "none", "rgba(255, 255, 255, 0.08)", 1);
  drawGaugeTicks(layer, center, center, outer, inner);
  drawCircle(layer, center, center, inner + 14, "url(#centerLip)", "rgba(255, 255, 255, 0.06)", 1);
  drawCircle(layer, center, center, inner + 6, "none", "rgba(147, 226, 244, 0.08)", 2);
  drawCircle(layer, center, center, inner + 1, "url(#centerWell)", "rgba(255, 255, 255, 0.07)", 1);
  drawCircle(layer, center, center, inner - 13, "none", "rgba(90, 160, 185, 0.13)", 1);
  drawSeam(layer, center, inner, outer);
}

// Above the needle, so that it turns under the hub rather than over it.
function drawClockHub(layer) {
  drawCircle(layer, CLOCK_CENTER, CLOCK_CENTER, 21, "url(#hubRing)", "rgba(255, 255, 255, 0.14)", 1);
  drawCircle(layer, CLOCK_CENTER, CLOCK_CENTER, 13, "url(#hub)", "rgba(255, 255, 255, 0.3)", 1);
  drawCircle(layer, CLOCK_CENTER, CLOCK_CENTER, 5.5, "rgba(255, 190, 194, 0.94)", "none", 0);
}

// The arc moves when the election window does, which is once a round, not once a second.
function drawElectionArc(layer, model) {
  const arc = model.electionArc;
  const key = `${arc.startAngle}|${arc.sweepAngle}|${model.inElections}`;
  if (clockElectionArcKey === key && layer.firstChild) {
    return;
  }
  clockElectionArcKey = key;
  replaceChildren(layer, []);

  const center = CLOCK_CENTER;
  const radius = CLOCK_ELECTION_RADIUS;
  drawArcStroke(layer, center, center, radius, arc.startAngle, arc.sweepAngle, "rgba(0, 4, 7, 0.72)", 17);
  drawArcStroke(layer, center, center, radius, arc.startAngle, arc.sweepAngle, "url(#electionArc)", 12, "url(#arcGlow)");
  drawArcStroke(layer, center, center, radius, arc.startAngle, arc.sweepAngle, "rgba(255, 250, 181, 0.62)", 2.4);
  drawArcEndpoint(layer, center, center, radius, arc.startAngle, model.inElections);
  drawArcEndpoint(layer, center, center, radius, arc.startAngle + arc.sweepAngle, model.inElections);
}
