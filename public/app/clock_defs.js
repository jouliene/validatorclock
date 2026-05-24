function drawDefs(svg) {
  const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
  defs.innerHTML = `
    <filter id="roundGlow" x="-18%" y="-18%" width="136%" height="136%">
      <feDropShadow dx="0" dy="12" stdDeviation="14" flood-color="#000712" flood-opacity="0.36"/>
      <feDropShadow dx="0" dy="0" stdDeviation="4" flood-color="#36bdf6" flood-opacity="0.035"/>
    </filter>
    <filter id="arcGlow" x="-28%" y="-28%" width="156%" height="156%">
      <feDropShadow dx="0" dy="0" stdDeviation="3" flood-color="#ffd66a" flood-opacity="0.3"/>
      <feDropShadow dx="0" dy="0" stdDeviation="7" flood-color="#d69835" flood-opacity="0.11"/>
    </filter>
    <filter id="bezelShadow" x="-18%" y="-18%" width="136%" height="136%">
      <feDropShadow dx="0" dy="16" stdDeviation="14" flood-color="#00040a" flood-opacity="0.46"/>
      <feDropShadow dx="0" dy="0" stdDeviation="4" flood-color="#6fd4ff" flood-opacity="0.04"/>
    </filter>
    <filter id="needleShadow" x="-26%" y="-26%" width="152%" height="152%">
      <feDropShadow dx="0" dy="7" stdDeviation="6" flood-color="#02050a" flood-opacity="0.5"/>
      <feDropShadow dx="0" dy="0" stdDeviation="2" flood-color="#ff5361" flood-opacity="0.14"/>
    </filter>
    <radialGradient id="clockAura" cx="50%" cy="48%" r="50%">
      <stop offset="0" stop-color="#193142" stop-opacity="0.11"/>
      <stop offset="0.56" stop-color="#0e2333" stop-opacity="0.035"/>
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
    </radialGradient>
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
