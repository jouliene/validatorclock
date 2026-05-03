const state = {
  chains: [],
  selectedChainId: null,
  refreshSeconds: 60,
  runtimeStatus: null,
  snapshot: null,
  pollTimer: null,
  statusTimer: null,
  drawTimer: null,
  clockLoading: false,
  clockRequestSeq: 0,
  lastClockRefreshAttempt: 0,
  roundRenderKey: null,
};

const palette = {
  blue: "#2f93dc",
  green: "#32af68",
  yellow: "#ead06a",
  gold: "#caa85c",
  red: "#dc3f4d",
  seam: "#07080c",
  center: "#080a0f",
};

const scriptUrl = document.currentScript?.src ? new URL(document.currentScript.src) : null;
const assetVersion = scriptUrl?.searchParams.get("v") || "";
const assetPath = (path) => assetVersion ? `${path}?v=${encodeURIComponent(assetVersion)}` : path;

const chainLogos = {
  everscale: assetPath("/brands/everscale.svg"),
  "tycho-testnet": assetPath("/brands/tycho.svg"),
};

const $ = (id) => document.getElementById(id);

async function fetchJson(url) {
  const response = await fetch(url, { headers: { Accept: "application/json" } });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(body.error || `${response.status} ${response.statusText}`);
  }
  return body;
}

function setError(message) {
  const banner = $("errorBanner");
  banner.hidden = !message;
  banner.textContent = message || "";
}

async function loadChains() {
  const data = await fetchJson("/api/chains");
  state.chains = data.chains;
  state.refreshSeconds = data.refresh_seconds || 60;
  state.selectedChainId = state.selectedChainId || state.chains[0]?.id;
  renderChainTabs();
}

async function loadRuntimeStatus() {
  try {
    state.runtimeStatus = await fetchJson("/api/status");
    renderRuntimeStatus(Math.trunc(Date.now() / 1000));
  } catch (error) {
    state.runtimeStatus = {
      status: "degraded",
      chains: [],
      error: error.message,
    };
    renderRuntimeStatus(Math.trunc(Date.now() / 1000));
  }
}

function renderChainTabs() {
  const tabs = $("chainTabs");
  tabs.replaceChildren();

  for (const chain of state.chains) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "chain-tab";
    button.setAttribute("role", "tab");
    button.setAttribute("aria-selected", String(chain.id === state.selectedChainId));
    button.style.setProperty("--chain-color", chain.color || palette.blue);

    const main = document.createElement("span");
    main.className = "chain-tab-main";
    const mark = document.createElement("span");
    mark.className = "chain-mark";

    const logoSrc = chainLogos[chain.id];
    if (logoSrc) {
      const logo = document.createElement("img");
      logo.src = logoSrc;
      logo.alt = "";
      logo.decoding = "async";
      mark.append(logo);
    } else {
      mark.classList.add("chain-swatch");
    }

    main.append(mark, document.createTextNode(chain.name));

    button.append(main);

    button.addEventListener("click", () => selectChain(chain.id));
    tabs.appendChild(button);
  }
}

async function selectChain(chainId) {
  state.selectedChainId = chainId;
  state.snapshot = null;
  state.roundRenderKey = null;
  renderChainTabs();
  clearClock();
  renderRuntimeStatus(Math.trunc(Date.now() / 1000));
  await loadClock(true);
  await loadRuntimeStatus();
}

async function loadClock(force = false) {
  const chainId = state.selectedChainId;
  if (!chainId) {
    return;
  }
  if (state.clockLoading && !force) {
    return;
  }

  const suffix = force ? "?refresh=1" : "";
  const requestSeq = state.clockRequestSeq + 1;
  state.clockRequestSeq = requestSeq;
  state.clockLoading = true;
  state.lastClockRefreshAttempt = Math.trunc(Date.now() / 1000);
  try {
    const snapshot = await fetchJson(`/api/chains/${encodeURIComponent(chainId)}/clock${suffix}`);
    if (requestSeq !== state.clockRequestSeq || chainId !== state.selectedChainId) {
      return;
    }
    state.snapshot = snapshot;
    state.roundRenderKey = null;
    setError(snapshot.warning || "");
    renderNow();
  } finally {
    if (requestSeq === state.clockRequestSeq) {
      state.clockLoading = false;
    }
  }
}

function startTimers() {
  window.clearInterval(state.pollTimer);
  window.clearInterval(state.statusTimer);
  window.clearInterval(state.drawTimer);

  const pollSeconds = refreshPollSeconds();

  state.pollTimer = window.setInterval(() => {
    loadClock(false).catch((error) => setError(error.message));
  }, pollSeconds * 1000);

  state.statusTimer = window.setInterval(() => {
    loadRuntimeStatus();
  }, pollSeconds * 1000);

  state.drawTimer = window.setInterval(renderNow, 1000);
}

function refreshPollSeconds() {
  return Math.max(10, Math.floor(Math.max(10, state.refreshSeconds) / 2));
}

function renderNow() {
  const now = Math.trunc(Date.now() / 1000);
  renderRuntimeStatus(now);
  refreshStaleSnapshot(now);

  if (!state.snapshot) {
    return;
  }

  const model = buildClockModel(state.snapshot, now);
  drawClock(model);
  renderMetrics(state.snapshot, model, now);
  renderRoundPanelsIfNeeded(state.snapshot, model);
}

function refreshStaleSnapshot(now) {
  if (!state.snapshot || state.clockLoading) {
    return;
  }

  const refreshSeconds = Math.max(10, state.refreshSeconds);
  const age = now - state.snapshot.fetched_at;
  const attemptAge = now - state.lastClockRefreshAttempt;
  if (age >= refreshSeconds && attemptAge >= 5) {
    loadClock(false).catch((error) => setError(error.message));
  }
}

function renderRuntimeStatus(now) {
  const container = $("runtimeStatus");
  const label = $("runtimeState");
  const detail = $("runtimeFreshness");
  if (!container || !label || !detail) {
    return;
  }

  const status = state.runtimeStatus;
  const chain = status?.chains?.find((item) => item.id === state.selectedChainId);
  container.className = "runtime-status is-starting";
  container.title = "Runtime status";

  if (!status) {
    label.textContent = "Starting";
    detail.textContent = "checking";
    return;
  }

  if (status.error) {
    container.className = "runtime-status is-bad";
    container.title = status.error;
    label.textContent = "Status error";
    detail.textContent = "retrying";
    return;
  }

  if (!chain) {
    container.className = status.status === "degraded" ? "runtime-status is-warn" : "runtime-status is-starting";
    label.textContent = status.status === "degraded" ? "Degraded" : "Starting";
    detail.textContent = "warming cache";
    return;
  }

  const age = chain.fetched_at ? Math.max(0, now - chain.fetched_at) : null;
  if (chain.stale) {
    container.className = "runtime-status is-bad";
    container.title = chain.last_error || "Cached data is stale";
    label.textContent = "Stale";
    detail.textContent = age == null ? "no cache" : `${formatDuration(age)} old`;
    return;
  }

  if (chain.last_error) {
    container.className = "runtime-status is-warn";
    container.title = chain.last_error;
    label.textContent = "Retrying";
    detail.textContent = age == null ? "no cache" : `${formatDuration(age)} old`;
    return;
  }

  if (chain.cached) {
    container.className = "runtime-status is-ok";
    label.textContent = "Data fresh";
    detail.textContent = age == null ? "cached" : `${formatDuration(age)} old`;
    return;
  }

  label.textContent = "Starting";
  detail.textContent = "warming cache";
}

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

function renderMetrics(snapshot, model, now) {
  $("metricChain").textContent = snapshot.chain.name;
  $("metricRpc").textContent = snapshot.chain.rpc_label;
  $("metricGlobalId").textContent = snapshot.global_id;
  $("metricSeqno").textContent = snapshot.seqno;
  $("metricStatus").textContent = model.status;
  $("activeRoundWindowCard").style.setProperty("--card-accent", roundAccentColor(snapshot.current_set.round_color));
  renderDateStack($("metricRound"), snapshot.current_set.utime_since, snapshot.current_set.utime_until);
  $("metricRoundEndsIn").textContent = formatDurationPrecise(Math.max(0, snapshot.current_set.utime_until - now));
  renderDateStack($("metricElections"), model.electionsStart, model.electionsEnd);
  $("metricElectionsCountdownLabel").textContent = model.inElections ? "Elections end in" : "Elections start in";
  $("metricElectionsStartIn").textContent = model.inElections
    ? formatDurationPrecise(Math.max(0, model.electionsEnd - now))
    : formatDurationPrecise(Math.max(0, model.electionsStart - now));
  renderInfoUpdated($("metricFetched"), snapshot.fetched_at, now);
}

function roundAccentColor(color) {
  return color === "green" ? "rgba(50, 175, 104, 0.78)" : "rgba(47, 147, 220, 0.78)";
}

function renderRoundPanelsIfNeeded(snapshot, model) {
  const key = [
    snapshot.chain.id,
    snapshot.fetched_at,
    snapshot.current_set.utime_since,
    snapshot.previous_set?.utime_since || "",
    snapshot.next_set?.utime_since || "",
    model.inElections ? "election" : "closed",
  ].join("|");
  if (state.roundRenderKey === key) {
    return;
  }
  state.roundRenderKey = key;
  renderRoundPanels(snapshot, model);
}

function renderRoundPanels(snapshot, model) {
  renderRoundPanel("blue", snapshot, model);
  renderRoundPanel("green", snapshot, model);
}

function renderRoundPanel(color, snapshot, model) {
  const current = snapshot.current_set.round_color === color ? snapshot.current_set : null;
  const next = snapshot.next_set?.round_color === color ? snapshot.next_set : null;
  const previous = model.beforeElections && snapshot.previous_set?.round_color === color ? snapshot.previous_set : null;
  const candidates = model.inElections ? snapshot.election.candidates : [];
  const isActive = Boolean(current);
  const isNext = Boolean(next);
  const list = $(`${color}Validators`);
  const meta = $(`${color}RoundMeta`);
  const badge = $(`${color}RoundBadge`);
  const stats = $(`${color}RoundStats`);
  list.replaceChildren();
  stats.replaceChildren();
  meta.replaceChildren();
  badge.className = "round-badge";

  if (isActive) {
    renderRoundMeta(meta, current, snapshot);
    badge.textContent = "active";
    badge.classList.add("is-active");
    renderRoundStats(stats, current);
    renderValidators(list, current.validators, { rewards: true });
    return;
  }

  if (isNext) {
    renderRoundMeta(meta, next, snapshot);
    badge.textContent = "elected";
    renderRoundStats(stats, next);
    renderValidators(list, next.validators, { rewards: true });
    return;
  }

  if (model.inElections && candidates.length > 0) {
    renderRoundMeta(meta, electionRoundMeta(snapshot), snapshot);
    badge.textContent = "elections open";
    badge.classList.add("is-election");
    renderCandidateStats(stats, candidates);
    renderValidators(list, candidates, { rewards: false });
    return;
  }

  if (previous) {
    renderRoundMeta(meta, previous, snapshot);
    badge.textContent = "previous";
    badge.classList.add("is-previous");
    renderRoundStats(stats, previous);
    renderValidators(list, previous.validators, { rewards: true });
    return;
  }

  renderWaitingMeta(meta, color);
  badge.textContent = "waiting";
  renderEmptyStats(stats);
  list.appendChild(emptyState("No validators announced for this round yet."));
}

function renderRoundMeta(container, round, snapshot) {
  container.replaceChildren(
    roundMetaItem("Round_Id", String(round.utime_since)),
    roundMetaItem("Round", String(calculatedRoundNumber(round, snapshot)))
  );
}

function electionRoundMeta(snapshot) {
  return {
    utime_since: snapshot.current_set.utime_until,
  };
}

function calculatedRoundNumber(round, snapshot) {
  const period = Math.max(1, Number(snapshot.params15.validators_elected_for || 1));
  return Math.floor(Number(round.utime_since) / period);
}

function renderStatusMeta(container, status) {
  container.replaceChildren(roundMetaItem(status, null, true));
}

function renderWaitingMeta(container, color) {
  container.replaceChildren(roundMetaItem("Status", "Waiting"));
}

function roundMetaItem(label, value, strong = false) {
  const item = document.createElement("span");
  item.className = `round-meta-item${strong ? " round-meta-strong" : ""}`;
  if (value == null) {
    item.textContent = label;
    return item;
  }

  const labelNode = document.createElement("span");
  labelNode.className = "round-meta-label";
  labelNode.textContent = label;
  const valueNode = document.createElement("span");
  valueNode.textContent = value;
  item.append(labelNode, valueNode);
  return item;
}

function renderRoundStats(container, round) {
  const totalStake = round.total_stake || sumTokenValues(round.validators, "stake");
  const totalReward = round.total_reward || sumTokenValues(round.validators, "reward");
  const stats = [
    ["Validators", String(round.total)],
    ["Total stake", formatStakeAmount(totalStake)],
    ["Total rewards", totalReward ? formatRewardAmount(totalReward) : "-"],
  ];
  renderStats(container, stats);
}

function renderCandidateStats(container, candidates) {
  const stats = [
    ["Candidates", String(candidates.length)],
    ["Total stake", formatStakeAmount(sumTokenValues(candidates, "stake"))],
    ["Total rewards", "-"],
  ];
  renderStats(container, stats);
}

function renderEmptyStats(container) {
  renderStats(container, [
    ["Validators", "-"],
    ["Total stake", "-"],
    ["Total rewards", "-"],
  ]);
}

function renderStats(container, stats) {
  for (const [label, value] of stats) {
    const item = document.createElement("div");
    item.className = "round-stat";
    const labelNode = document.createElement("span");
    labelNode.textContent = label;
    const valueNode = document.createElement("strong");
    valueNode.textContent = value;
    valueNode.title = value;
    item.append(labelNode, valueNode);
    container.appendChild(item);
  }
}

function renderValidators(container, validators, options) {
  const table = document.createElement("div");
  table.className = "validator-table";

  const header = document.createElement("div");
  header.className = "validator-header";
  for (const label of ["Rank", "Validator", "Public key", "Stake", "Rewards", "Weight"]) {
    const cell = document.createElement("div");
    cell.className = `validator-cell${["Stake", "Rewards", "Weight"].includes(label) ? " validator-number" : ""}`;
    cell.textContent = label;
    header.appendChild(cell);
  }
  table.appendChild(header);

  validators.forEach((validator, index) => {
    const row = document.createElement("div");
    row.className = "validator-row";

    row.append(
      validatorCell(String(index + 1)),
      validatorIdentityCell(validatorWalletAddress(validator), validator.public_key),
      validatorCopyCell(shortenHash(validator.public_key, 5, 5), validator.public_key, "validator-pubkey", "validator public key"),
      validatorCell(formatStakeAmount(validator.stake || "0"), "validator-number", validator.stake || ""),
      validatorCell(options.rewards && validator.reward ? formatRewardAmount(validator.reward) : "-", "validator-number", validator.reward || ""),
      validatorCell(validator.weight_percent == null ? "-" : `${formatPercent(validator.weight_percent)}`, "validator-number", validator.weight || "")
    );
    table.appendChild(row);
  });

  container.appendChild(table);
}

function validatorCell(text, className = "", title = text) {
  const cell = document.createElement("div");
  cell.className = `validator-cell ${className}`.trim();
  cell.textContent = text;
  cell.title = title;
  return cell;
}

function validatorCopyCell(text, value, className, label) {
  const cell = document.createElement("div");
  cell.className = `validator-cell ${className}`.trim();
  cell.appendChild(copyableValue(text, value, className, label));
  return cell;
}

function validatorIdentityCell(wallet, publicKey) {
  const cell = document.createElement("div");
  cell.className = "validator-cell validator-id";
  const avatar = document.createElement("span");
  avatar.className = "validator-avatar";
  avatar.style.background = validatorGradient(publicKey || wallet);
  const address = copyableValue(shortenAddress(wallet), wallet, "validator-address", "validator wallet address");
  cell.append(avatar, address);
  return cell;
}

function copyableValue(text, value, className, label) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `validator-copy ${className}`.trim();
  button.setAttribute("aria-label", `Copy ${label}`);
  const textNode = document.createElement("span");
  textNode.className = "validator-copy-text";
  textNode.textContent = text;
  const feedback = document.createElement("span");
  feedback.className = "validator-copy-feedback";
  feedback.textContent = "Copied";
  button.append(textNode, feedback);
  if (!value || value === "-") {
    button.disabled = true;
    return button;
  }
  button.addEventListener("click", async (event) => {
    event.preventDefault();
    event.stopPropagation();
    try {
      await copyText(value);
      markCopied(button);
    } catch (error) {
      button.classList.add("is-failed");
      feedback.textContent = "Copy failed";
      window.setTimeout(() => {
        button.classList.remove("is-failed");
        feedback.textContent = "Copied";
      }, 1200);
    }
  });
  return button;
}

async function copyText(value) {
  if (navigator.clipboard && window.isSecureContext) {
    await navigator.clipboard.writeText(value);
    return;
  }

  const textarea = document.createElement("textarea");
  textarea.value = value;
  textarea.setAttribute("readonly", "");
  textarea.style.position = "fixed";
  textarea.style.left = "-9999px";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.select();
  const copied = document.execCommand("copy");
  textarea.remove();
  if (!copied) {
    throw new Error("copy failed");
  }
}

function markCopied(button) {
  button.classList.add("is-copied");
  if (button.dataset.copyTimer) {
    window.clearTimeout(Number(button.dataset.copyTimer));
  }
  button.dataset.copyTimer = String(window.setTimeout(() => {
    button.classList.remove("is-copied");
    delete button.dataset.copyTimer;
  }, 1000));
}

function emptyState(text) {
  const item = document.createElement("div");
  item.className = "empty-state";
  item.textContent = text;
  return item;
}

function renderDateStack(container, start, end) {
  container.replaceChildren();
  container.append(dateRow("Start", start), dateRow("End", end));
}

function dateRow(label, unixSeconds) {
  const row = document.createElement("div");
  row.className = "date-row";
  const labelNode = document.createElement("span");
  labelNode.className = "date-label";
  labelNode.textContent = label;
  const valueNode = document.createElement("span");
  valueNode.className = "date-value";
  valueNode.textContent = formatDateTime(unixSeconds);
  row.append(labelNode, valueNode);
  return row;
}

function renderInfoUpdated(container, fetchedAt, now) {
  const ageSeconds = Math.max(0, now - fetchedAt);
  container.textContent = `${ageSeconds}s ago`;
}

function validatorWalletAddress(validator) {
  const hash = validator.wallet;
  if (!hash) {
    return "-";
  }
  return formatMasterchainAddress(hash);
}

function shortenAddress(address) {
  if (!address || address === "-") {
    return "-";
  }
  const [workchain, hash] = address.includes(":") ? address.split(":") : ["-1", address];
  return `${workchain}:${hash.slice(0, 4)}...${hash.slice(-4)}`;
}

function shortenHash(value, head = 5, tail = 5) {
  if (!value) {
    return "-";
  }
  return value.length <= head + tail + 3 ? value : `${value.slice(0, head)}...${value.slice(-tail)}`;
}

function validatorGradient(seed) {
  let hash = 0;
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * 31 + seed.charCodeAt(i)) >>> 0;
  }
  const gradients = [
    "linear-gradient(135deg, #67b7c7 0%, #2e6f8f 100%)",
    "linear-gradient(135deg, #75bd91 0%, #2f7655 100%)",
    "linear-gradient(135deg, #caa85c 0%, #806a36 100%)",
    "linear-gradient(135deg, #8f98c9 0%, #536093 100%)",
    "linear-gradient(135deg, #9d80ae 0%, #654a73 100%)",
    "linear-gradient(135deg, #c48771 0%, #81503f 100%)",
  ];
  return gradients[hash % gradients.length];
}

function sumTokenValues(items, key) {
  const total = items.reduce((sum, item) => {
    const value = Number(item[key] || 0);
    return Number.isFinite(value) ? sum + value : sum;
  }, 0);
  return total ? String(total) : "";
}

function formatMasterchainAddress(hash) {
  return hash.includes(":") ? hash : `-1:${hash}`;
}

function formatWeight(value) {
  return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

function formatPercent(value) {
  return `${Number(value || 0).toFixed(2)}%`;
}

function formatStakeAmount(value) {
  return formatTokenAmount(value, 0, 0);
}

function formatRewardAmount(value) {
  return formatTokenAmount(value, 9, 9);
}

function formatTokenAmount(value, minimumFractionDigits = 0, maximumFractionDigits = 3) {
  if (!value && value !== 0) {
    return "-";
  }
  const number = Number(value);
  if (!Number.isFinite(number)) {
    return value;
  }
  return number.toLocaleString(undefined, { minimumFractionDigits, maximumFractionDigits });
}

function formatDateTime(unixSeconds) {
  if (!unixSeconds) {
    return "-";
  }
  const date = new Date(unixSeconds * 1000);
  const pad = (value) => String(value).padStart(2, "0");
  return [
    date.getFullYear(),
    pad(date.getMonth() + 1),
    pad(date.getDate()),
  ].join("-") + ` ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

function formatDuration(totalSeconds) {
  const seconds = Math.max(0, Math.trunc(totalSeconds));
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainder = seconds % 60;

  if (days > 0) {
    return `${days}d ${hours}h`;
  }
  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  if (minutes > 0) {
    return `${minutes}m ${remainder}s`;
  }
  return `${remainder}s`;
}

function formatDurationClock(totalSeconds) {
  const seconds = Math.max(0, Math.trunc(totalSeconds));
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainder = seconds % 60;
  const pad = (value) => String(value).padStart(2, "0");

  if (days > 0) {
    return `${days}d ${pad(hours)}h ${pad(minutes)}m ${pad(remainder)}s`;
  }
  return `${pad(hours)}h ${pad(minutes)}m ${pad(remainder)}s`;
}

function formatDurationPrecise(totalSeconds) {
  const seconds = Math.max(0, Math.trunc(totalSeconds));
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainder = seconds % 60;
  const parts = [];

  if (days > 0) {
    parts.push(`${days}d`);
  }
  if (hours > 0 || days > 0) {
    parts.push(`${hours}h`);
  }
  if (minutes > 0 || hours > 0 || days > 0) {
    parts.push(`${minutes}m`);
  }
  parts.push(`${remainder}s`);
  return parts.join(" ");
}

async function boot() {
  try {
    await loadChains();
    await loadRuntimeStatus();
    await loadClock(true);
    await loadRuntimeStatus();
    startTimers();
  } catch (error) {
    setError(error.message);
  }
}

boot();
