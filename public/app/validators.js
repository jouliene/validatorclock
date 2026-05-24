let validatorTypeGlossaryPopover = null;
let validatorTypeGlossaryAnchor = null;
let validatorHoverTooltip = null;
let validatorHoverTooltipTarget = null;
let validatorFakePhaseTimer = null;
const VALIDATOR_TOOLTIP_DANGER_PREFIX = "[danger]";

function renderValidators(container, validators, options = {}) {
  const table = document.createElement("div");
  table.className = "validator-table";
  table.appendChild(validatorHeader(VALIDATOR_ROUND_HEADERS, options));

  validators.forEach((validator, index) => {
    const row = document.createElement("div");
    const sourceKind = validatorSourceKind(validator, options);
    row.className = `validator-row has-source-${sourceKind}`;
    if (isFakeMapValidator(validator, options)) {
      row.classList.add("is-map-fake");
    }

    row.append(
      validatorCell(String(index + 1), "validator-index"),
      validatorSourceTypeCell(validator, options),
      validatorSourceCell(validator, options),
      validatorIdentityCell(validator, options),
      validatorHistoryCell(validator.history),
      validatorCell(formatStakeAmount(validator.stake || "0"), "validator-number validator-stake", validator.stake || ""),
      validatorCell(options.rewards && validator.reward ? formatRewardCellAmount(validator.reward) : "-", "validator-number validator-rewards", validator.reward || ""),
      validatorCell(validator.weight_percent == null ? "-" : `${formatPercent(validator.weight_percent)}`, "validator-number validator-weight", validator.weight || "")
    );
    table.appendChild(row);
  });

  container.appendChild(table);
}

function renderRecentAbsentValidators(container, validators, options = {}) {
  if (!Array.isArray(validators) || validators.length === 0) {
    return;
  }

  const table = document.createElement("div");
  table.className = "validator-table is-absent";
  table.appendChild(validatorHeader(VALIDATOR_ABSENT_HEADERS, options));

  validators.forEach((validator, index) => {
    const row = document.createElement("div");
    const sourceKind = validatorSourceKind(validator, options);
    row.className = `validator-row has-source-${sourceKind}`;
    if (isFakeMapValidator(validator, options)) {
      row.classList.add("is-map-fake");
    }
    row.append(
      validatorCell(String(index + 1), "validator-index"),
      validatorSourceTypeCell(validator, options),
      validatorSourceCell(validator, options),
      validatorIdentityCell(validator, options, true),
      validatorHistoryCell(validator.history),
      validatorCell(formatSeenRounds(validator), "validator-number validator-seen-rounds validator-seen", String(validator.last_seen_round || ""))
    );
    table.appendChild(row);
  });

  container.appendChild(table);
}

function validatorHeader(labels, options = {}) {
  const header = document.createElement("div");
  header.className = "validator-header";
  for (const label of labels) {
    header.appendChild(validatorHeaderCell(label, options));
  }
  return header;
}

function validatorHeaderCell(label, options = {}) {
  const cell = document.createElement("div");
  const classes = ["validator-cell"];
  const semanticClass = VALIDATOR_HEADER_CLASSES[label];
  if (semanticClass) {
    classes.push(semanticClass);
  }
  if (VALIDATOR_NUMBER_HEADERS.has(label)) {
    classes.push("validator-number");
  }
  cell.className = classes.join(" ");

  if (label === "Type") {
    cell.classList.add("validator-type-heading");
    const name = document.createElement("span");
    name.textContent = "Type";
    const help = document.createElement("button");
    help.type = "button";
    help.className = "validator-type-help";
    help.setAttribute("aria-label", "Show type glossary");
    help.setAttribute("aria-expanded", "false");
    help.validatorGlossaryLabels = normalizedGlossaryLabels(options.glossaryLabels);
    setValidatorTooltip(help, "Type glossary");
    help.innerHTML = `
      <svg viewBox="0 0 24 24" focusable="false" aria-hidden="true">
        <circle cx="12" cy="12" r="8.5"></circle>
        <path d="M12 11.5v5"></path>
        <path d="M12 8h.01"></path>
      </svg>
    `;
    help.addEventListener("click", (event) => {
      event.stopPropagation();
      toggleValidatorTypeGlossary(help);
    });
    cell.append(name, help);
    return cell;
  }

  if (label === "Source" && options.chainId === "ton") {
    cell.classList.add("validator-source-heading");
    const name = document.createElement("span");
    name.textContent = "Source";
    const toggle = document.createElement("span");
    toggle.className = "validator-source-mode-toggle";
    toggle.setAttribute("role", "group");
    toggle.setAttribute("aria-label", "Source display");
    toggle.append(
      validatorSourceModeButton("meta", "META", options),
      validatorSourceModeButton("addr", "ADDR", options)
    );
    cell.append(name, toggle);
    return cell;
  }

  if (label === "Validator" && options.chainId === "ton") {
    cell.classList.add("validator-address-heading");
    const name = document.createElement("span");
    name.textContent = "Validator";
    const toggle = document.createElement("span");
    toggle.className = "validator-source-mode-toggle validator-address-mode-toggle";
    toggle.setAttribute("role", "group");
    toggle.setAttribute("aria-label", "Validator address type");
    toggle.append(
      validatorAddressModeButton("ever", "HASH", options),
      validatorAddressModeButton("ton", "BASE64", options)
    );
    cell.append(name, toggle);
    return cell;
  }

  if (label !== "History") {
    cell.textContent = label;
    return cell;
  }

  cell.classList.add("validator-history-heading");
  const name = document.createElement("span");
  name.textContent = "History";
  const direction = document.createElement("span");
  direction.className = "history-direction";
  direction.textContent = "Older -> Latest";
  cell.append(name, direction);
  return cell;
}

function validatorSourceModeButton(mode, label, options = {}) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "validator-source-mode-button";
  button.textContent = label;
  button.title = mode === "meta" ? "Show TON source metadata" : "Show TON source address";
  button.setAttribute("aria-pressed", String((options.sourceDisplayMode || "meta") === mode));
  button.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    if (typeof options.onSourceDisplayModeChange === "function") {
      options.onSourceDisplayModeChange(mode);
    }
  });
  return button;
}

function validatorAddressModeButton(type, label, options = {}) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "validator-source-mode-button validator-address-mode-button";
  button.textContent = label;
  button.title = type === "ever" ? "Show raw workchain:hash address" : "Show TON user-friendly base64 address";
  button.setAttribute("aria-pressed", String((options.addressType || "ton") === type));
  button.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    if (typeof options.onAddressTypeChange === "function") {
      options.onAddressTypeChange(type);
    }
  });
  return button;
}

function formatSeenRounds(validator) {
  const rounds = Array.isArray(validator?.history)
    ? validator.history
      .filter((point) => point.status === "participated" && point.round != null)
      .map((point) => Number(point.round))
      .filter((round) => Number.isFinite(round))
    : [];

  if (rounds.length === 0 && validator?.last_seen_round != null) {
    rounds.push(Number(validator.last_seen_round));
  }

  return [...new Set(rounds)]
    .sort((left, right) => right - left)
    .join(", ");
}

function validatorHistoryCell(history) {
  const cell = document.createElement("div");
  cell.className = "validator-cell validator-history";
  const dots = document.createElement("span");
  dots.className = "validator-history-dots";
  const points = Array.isArray(history) && history.length > 0
    ? history
    : Array.from({ length: 5 }, () => ({ status: "unknown" }));

  for (const point of points.slice(0, 5)) {
    const dot = document.createElement("span");
    const status = point.status || "unknown";
    const fakeNode = status === "participated" && point.fake_node === true;
    dot.className = `validator-history-dot is-${status}${fakeNode ? " is-fake-node" : ""}`;
    if (fakeNode) {
      dot.setAttribute("aria-label", "Fake Node participation");
    }
    setValidatorTooltip(
      dot,
      point.round == null
        ? "Round: unknown"
        : [
            `Round: ${point.round}`,
            `Status: ${historyStatusLabel(status)}`,
            ...(fakeNode ? ["Node: Fake Node"] : []),
          ]
    );
    dots.appendChild(dot);
  }

  cell.appendChild(dots);
  return cell;
}

function historyStatusLabel(status) {
  if (status === "participated") {
    return "participated";
  }
  if (status === "missed") {
    return "missed";
  }
  return "unknown";
}

function validatorSourceTypeCell(validator, options = {}) {
  const cell = document.createElement("div");
  cell.className = "validator-cell validator-source-type";
  const type = displayedValidatorType(validator);
  const hash = type.hash;
  const fake = isFakeMapValidator(validator, options);

  const badges = document.createElement("span");
  badges.className = "validator-type-badges";

  const badge = document.createElement("span");
  badge.className = `validator-type-badge is-${type.className}`;
  if (fake) {
    ensureValidatorFakePhaseTicker();
    badge.classList.add("is-map-fake");
    badge.append(
      validatorBadgeText(type.label, "validator-type-badge-primary"),
      validatorBadgeText("FAKE NODE", "validator-type-badge-fake")
    );
  } else {
    badge.appendChild(validatorBadgeText(type.label));
  }

  const contractHash = hash || validator?.contract_type_hash || "";
  const missingLocationLine = fake
    ? validatorTooltipDangerLine(options.fakeSourceTooltip || "Validator node IP not detected.")
    : "";
  const tooltipLines = withValidatorLocationTooltipLines(
    validator,
    validatorTypeTooltipLines(type.label, contractHash),
    options,
    missingLocationLine
  );
  setValidatorTooltip(
    badge,
    fake ? fakeValidatorTypeTooltipLines(type.label, tooltipLines, options) : tooltipLines
  );
  badges.appendChild(badge);

  if (fake) {
    badges.appendChild(validatorFakeNodeBadge(type.label, tooltipLines, options));
  }

  cell.appendChild(badges);
  return cell;
}

function validatorFakeNodeBadge(typeLabel, tooltipLines, options = {}) {
  const badge = document.createElement("span");
  badge.className = "validator-map-fake-badge";
  badge.setAttribute("aria-label", "FAKE NODE");

  const primary = document.createElement("span");
  primary.textContent = "FAKE";
  const detail = document.createElement("span");
  detail.className = "validator-map-fake-detail";
  detail.textContent = "NODE";

  badge.append(primary, detail);
  setValidatorTooltip(badge, fakeValidatorTypeTooltipLines(typeLabel, tooltipLines, options));
  return badge;
}

function ensureValidatorFakePhaseTicker() {
  if (validatorFakePhaseTimer != null) {
    return;
  }

  validatorFakePhaseTimer = window.setInterval(() => {
    document.documentElement.classList.toggle("validator-fake-phase");
  }, 1800);
}

function validatorBadgeText(label, className = "") {
  const text = document.createElement("span");
  text.className = `validator-type-badge-text ${className}`.trim();
  text.textContent = label;
  return text;
}

function validatorSourceType(hash) {
  return VALIDATOR_SOURCE_TYPES[String(hash || "").toLowerCase()] || UNKNOWN_VALIDATOR_TYPE;
}

function validatorContractType(typeName) {
  return VALIDATOR_CONTRACT_TYPES[typeName] || UNKNOWN_VALIDATOR_TYPE;
}

function displayedValidatorType(validator) {
  const hash = validator && validator.source && validator.source.contract_type_hash;
  if (hash) {
    return { ...validatorSourceType(hash), hash };
  }
  return { ...validatorContractType(validator && validator.contract_type), hash: "" };
}

function validatorGlossaryLabelsForSnapshot(snapshot) {
  const labels = new Set();
  collectValidatorGlossaryLabels(labels, snapshot?.current_set?.validators);
  collectValidatorGlossaryLabels(labels, snapshot?.previous_set?.validators);
  collectValidatorGlossaryLabels(labels, snapshot?.next_set?.validators);
  collectValidatorGlossaryLabels(labels, snapshot?.election?.candidates);
  collectValidatorGlossaryLabels(labels, snapshot?.current_set?.recent_absent_validators);
  collectValidatorGlossaryLabels(labels, snapshot?.previous_set?.recent_absent_validators);
  collectValidatorGlossaryLabels(labels, snapshot?.next_set?.recent_absent_validators);
  return labels;
}

function collectValidatorGlossaryLabels(labels, validators) {
  if (!Array.isArray(validators)) {
    return;
  }
  for (const validator of validators) {
    labels.add(displayedValidatorType(validator).label);
  }
}

function validatorSourceCell(validator, options = {}) {
  const cell = document.createElement("div");
  const sourceKind = validatorSourceKind(validator, options);
  cell.className = `validator-cell validator-source is-${sourceKind}`;

  const source = validator && validator.source;
  if (source && source.address) {
    const formatted = formatDisplayAddress(source.address, options);
    if (shouldDisplayTonSourceMetadata(validator, options)) {
      const meta = tonSourceMetadata(validator);
      const metadata = copyableValue(
        meta.label,
        formatted.value,
        `validator-source-address validator-source-meta is-${meta.className}`,
        "validator source address"
      );
      const contractHash = source.contract_type_hash || validator?.contract_type_hash;
      setValidatorTooltip(metadata, validatorSourceMetadataTooltipLines(validator, meta, contractHash));
      cell.appendChild(metadata);
      return cell;
    }

    const address = copyableValue(
      formatted.text,
      formatted.value,
      "validator-source-address",
      "validator source address"
    );
    const contractHash = source.contract_type_hash || validator?.contract_type_hash;
    setValidatorTooltip(address, validatorSourceTooltipLines(validator, contractHash));
    cell.appendChild(address);
    return cell;
  }

  if (isDirectValidatorContract(validator)) {
    const direct = document.createElement("span");
    direct.className = "validator-source-direct";
    direct.textContent = "Direct";
    setValidatorTooltip(direct, directValidatorTooltipLines(validator));
    cell.appendChild(direct);
    return cell;
  }

  const tonHash = tonValidatorContractHash(validator, options);
  if (tonHash) {
    const hash = copyableValue(
      shortenContractHash(tonHash),
      tonHash,
      "validator-source-address",
      "validator contract repr hash"
    );
    setValidatorTooltip(hash, `Contract HASH: ${tonHash}`);
    cell.appendChild(hash);
    return cell;
  }

  const unknown = document.createElement("span");
  unknown.className = "validator-source-unknown";
  unknown.textContent = "Unknown";
  cell.appendChild(unknown);
  return cell;
}

function validatorSourceKind(validator, options = {}) {
  const source = validator && validator.source;
  if (source && source.address) {
    return "detail";
  }
  if (isDirectValidatorContract(validator)) {
    return "direct";
  }
  if (tonValidatorContractHash(validator, options)) {
    return "detail";
  }
  return "unknown";
}

function isFakeMapValidator(validator, options = {}) {
  if (!validatorMapAvailableForChain(options.chainId)) {
    return false;
  }
  if (!(options.fakeValidatorPeers instanceof Set)) {
    return false;
  }

  const publicKey = String(validator?.public_key || "").toLowerCase();
  return Boolean(publicKey) && options.fakeValidatorPeers.has(publicKey);
}

function fakeValidatorTypeTooltipLines(typeLabel, tooltipLines) {
  return Array.isArray(tooltipLines) && tooltipLines.length > 0
    ? tooltipLines
    : validatorTypeTooltipLines(typeLabel);
}

function withValidatorLocationTooltipLines(validator, tooltipLines, options = {}, missingLocationLine = "") {
  return [
    ...(Array.isArray(tooltipLines) ? tooltipLines : []),
    ...validatorLocationTooltipLines(validator, options, missingLocationLine),
  ];
}

function validatorLocationTooltipLines(validator, options = {}, missingLocationLine = "") {
  const node = validatorMapNode(validator, options);
  if (!node) {
    const line = String(missingLocationLine || "").trim();
    return line ? ["Location:", line] : [];
  }

  const lines = ["Location:"];
  const ip = String(node.ip || "").trim();
  const isp = String(node.isp || "").trim();
  const place = [node.city, node.country]
    .map((part) => String(part || "").trim())
    .filter(Boolean)
    .join(", ");

  if (ip) {
    lines.push(`IP: ${ip}`);
  }
  if (isp) {
    lines.push(`ISP: ${isp}`);
  }
  if (place) {
    lines.push(`Place: ${place}`);
  }

  if (lines.length > 1) {
    return lines;
  }
  const line = String(missingLocationLine || "").trim();
  return line ? ["Location:", line] : [];
}

function validatorMapNode(validator, options = {}) {
  if (validator?.map_node && typeof validator.map_node === "object") {
    return validator.map_node;
  }

  const peer = String(validator?.public_key || "").toLowerCase();
  if (!peer || !(options.mapNodesByPeer instanceof Map)) {
    return null;
  }
  return options.mapNodesByPeer.get(peer) || null;
}

function validatorTooltipDangerLine(text) {
  return `${VALIDATOR_TOOLTIP_DANGER_PREFIX}${text}`;
}

function validatorMapAvailableForChain(chainId) {
  if (typeof mapAvailableForChain === "function") {
    return mapAvailableForChain(chainId);
  }
  return chainId === TYCHO_MAP_CHAIN_ID;
}

function shouldDisplayTonSourceMetadata(validator, options = {}) {
  return options.chainId === "ton" && options.sourceDisplayMode !== "addr" && Boolean(validator?.source?.address);
}

function tonSourceMetadata(validator) {
  const sourceAddress = normalizeSourceAddress(validator?.source?.address);
  const explicit = TON_SOURCE_METADATA_BY_ADDRESS[sourceAddress];
  if (explicit) {
    return {
      className: sourceMetadataClass(explicit.label),
      ...explicit,
    };
  }

  if (validator?.contract_type === "WhalesPoolProxy") {
    return {
      label: "TON Whales",
      name: "TON Whales",
      detail: "Nominator pool source for a TON Whales masterchain proxy.",
      className: "whales",
    };
  }
  if (validator?.contract_type === "HipoValidatorProxy") {
    return {
      label: "Hipo",
      name: "Hipo Finance",
      detail: "Hipo liquid-staking treasury source.",
      className: "hipo",
    };
  }
  if (validator?.contract_type === "ValidatorController") {
    return {
      label: "LST Pool",
      name: "Liquid staking pool",
      detail: "tonstake_pool source. No public owner name was found for this address.",
      className: "lst",
    };
  }
  if (validator?.contract_type === "TonNominatorPool") {
    return {
      label: "Nominator",
      name: "Nominator pool validator",
      detail: "Validator address configured in a TON nominator pool.",
      className: "nominator",
    };
  }
  if (validator?.contract_type === "TonSingleNominatorPool") {
    return {
      label: "Owner",
      name: "Single nominator pool owner",
      detail: "Owner address configured in a TON single nominator pool.",
      className: "owner",
    };
  }
  if (validator?.contract_type === "SingleNominatorV1_0" || validator?.contract_type === "SingleNominatorV1_1") {
    return {
      label: "Owner",
      name: "Single nominator owner",
      detail: "Owner wallet configured in a TON Single Nominator contract.",
      className: "owner",
    };
  }
  if (isDirectValidatorContract(validator)) {
    return {
      label: "Direct",
      name: "Direct validation wallet",
      detail: "Validator wallet participates directly through Elector.",
      className: "direct",
    };
  }
  return {
    label: "Unknown",
    name: "Unknown source owner",
    detail: "No source metadata is known for this address yet.",
    className: "unknown",
  };
}

function normalizeSourceAddress(address) {
  return formatMasterchainAddress(address || "").toLowerCase();
}

function sourceMetadataClass(label) {
  return String(label || "unknown")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "") || "unknown";
}

function validatorSourceTooltipLines(validator, contractHash = "") {
  const role = validatorSourceRole(validator);
  const lines = [];
  if (contractHash) {
    lines.push(`Contract HASH: ${contractHash}`);
  }
  if (role) {
    lines.push(`Source: ${role}`);
  }
  return lines;
}

function validatorSourceMetadataTooltipLines(validator, meta, contractHash = "") {
  const role = validatorSourceRole(validator);
  const lines = role ? [`Source: ${role}`] : [];
  lines.push(`Owner: ${meta.name || meta.label}`);
  if (meta.detail) {
    lines.push(`Metadata: ${meta.detail}`);
  }
  if (contractHash) {
    lines.push(`Contract HASH: ${contractHash}`);
  }
  return lines;
}

function validatorSourceRole(validator) {
  if (!validator) {
    return "";
  }
  if (validator.contract_type === "TonNominatorPool") {
    return "Validator address";
  }
  if (validator.contract_type === "ValidatorController") {
    return "Liquid staking pool";
  }
  if (validator.contract_type === "WhalesPoolProxy") {
    return "Whales pool";
  }
  if (validator.contract_type === "HipoValidatorProxy") {
    return "Hipo Treasury";
  }
  if (validator.contract_type === "DePoolProxy") {
    return "DePool address";
  }
  if (validator.contract_type === "StEverDePoolProxy") {
    return "Staked EVER DePool address";
  }
  if (validator.contract_type === "SingleNominatorV1_0" || validator.contract_type === "SingleNominatorV1_1" || validator.contract_type === "TonSingleNominatorPool") {
    return "Owner address";
  }
  return "";
}

function isDirectValidatorContract(validator) {
  return (
    validator?.contract_type === "EverWallet"
    || validator?.contract_type === "TonWalletV1R3"
    || validator?.contract_type === "TonVestingWallet"
  );
}

function directValidatorTooltipLines(validator) {
  const lines = ["Source: Direct validation wallet"];
  if (validator?.contract_type_hash) {
    lines.push(`Contract HASH: ${validator.contract_type_hash}`);
  }
  return lines;
}

function tonValidatorContractHash(validator, options = {}) {
  if (options.chainId !== "ton") {
    return "";
  }
  if (validator?.contract_type && validator.contract_type !== "Unknown") {
    return "";
  }
  return validator?.contract_type_hash || "";
}

function shortenContractHash(hash) {
  return hash && hash.length > 12 ? `${hash.slice(0, 6)}...${hash.slice(-6)}` : (hash || "-");
}

function validatorTypeGlossaryEntry(label) {
  return VALIDATOR_TYPE_GLOSSARY.find((entry) => entry.label === label);
}

function validatorTypeTooltipLines(label, contractHash = "") {
  const entry = validatorTypeGlossaryEntry(label);
  const hash = String(contractHash || "").trim();
  const normalizedLabel = String(label || "").trim();
  const name = entry?.name || (normalizedLabel && normalizedLabel !== UNKNOWN_VALIDATOR_TYPE.label
    ? normalizedLabel
    : "Unknown");
  return [
    `Contract HASH: ${hash || "Unknown"}`,
    `Name: ${name}`,
  ];
}

function toggleValidatorTypeGlossary(anchor) {
  if (validatorTypeGlossaryPopover && validatorTypeGlossaryAnchor === anchor) {
    closeValidatorTypeGlossary();
    return;
  }

  closeValidatorTypeGlossary();
  validatorTypeGlossaryAnchor = anchor;
  validatorTypeGlossaryPopover = buildValidatorTypeGlossary(anchor.validatorGlossaryLabels);
  document.body.appendChild(validatorTypeGlossaryPopover);
  positionValidatorTypeGlossary();
  setValidatorTypeHelpExpanded(anchor);

  document.addEventListener("click", handleValidatorTypeGlossaryOutsideClick);
  document.addEventListener("keydown", handleValidatorTypeGlossaryKeydown);
  window.addEventListener("resize", closeValidatorTypeGlossary);
  window.addEventListener("scroll", closeValidatorTypeGlossary, true);
}

function buildValidatorTypeGlossary(labels) {
  const popover = document.createElement("div");
  popover.className = "validator-type-glossary";
  popover.setAttribute("role", "dialog");
  popover.setAttribute("aria-label", "Validator type glossary");

  const title = document.createElement("div");
  title.className = "validator-type-glossary-title";
  title.textContent = "Type glossary";
  popover.appendChild(title);

  for (const entry of validatorTypeGlossaryEntries(labels)) {
    const row = document.createElement("div");
    row.className = "validator-type-glossary-row";

    const badge = document.createElement("span");
    const badgeType = { label: entry.label, className: glossaryBadgeClass(entry.label) };
    badge.className = `validator-type-badge is-${badgeType.className}`;
    badge.appendChild(validatorBadgeText(entry.label));

    const details = document.createElement("span");
    details.className = "validator-type-glossary-details";
    const name = document.createElement("strong");
    name.textContent = entry.name;
    const description = document.createElement("span");
    description.textContent = entry.description;
    details.append(name, description);

    row.append(badge, details);
    popover.appendChild(row);
  }

  return popover;
}

function normalizedGlossaryLabels(labels) {
  return labels instanceof Set ? labels : new Set(Array.isArray(labels) ? labels : []);
}

function validatorTypeGlossaryEntries(labels) {
  if (!(labels instanceof Set) || labels.size === 0) {
    return VALIDATOR_TYPE_GLOSSARY;
  }
  const entries = VALIDATOR_TYPE_GLOSSARY.filter((entry) => labels.has(entry.label));
  return entries.length > 0 ? entries : VALIDATOR_TYPE_GLOSSARY;
}

function glossaryBadgeClass(label) {
  if (label === "EVER") return "ever";
  if (label === "PROXY" || label === "DEPOOL") return "proxy";
  if (label === "StPROXY" || label === "StDEPOOL") return "stproxy";
  if (label === "SNOMv1.0" || label === "SNOMv1.1") return "snom";
  if (label === "SNPOOL") return "snpool";
  if (label === "NOMPOOL") return "nompool";
  if (label === "LSTCTRL") return "lstctrl";
  if (label === "V1R3") return "v1r3";
  if (label === "VEST") return "vest";
  if (label === "WHALES") return "whales";
  if (label === "HIPO") return "hipo";
  return "unknown";
}

function positionValidatorTypeGlossary() {
  if (!validatorTypeGlossaryPopover || !validatorTypeGlossaryAnchor) {
    return;
  }

  const anchorRect = validatorTypeGlossaryAnchor.getBoundingClientRect();
  const width = Math.min(320, Math.max(260, window.innerWidth - 24));
  validatorTypeGlossaryPopover.style.width = `${width}px`;

  const popoverRect = validatorTypeGlossaryPopover.getBoundingClientRect();
  const left = Math.min(
    Math.max(12, anchorRect.left + anchorRect.width / 2 - width / 2),
    window.innerWidth - width - 12
  );
  const belowTop = anchorRect.bottom + 8;
  const aboveTop = anchorRect.top - popoverRect.height - 8;
  const top = belowTop + popoverRect.height <= window.innerHeight - 12
    ? belowTop
    : Math.max(12, aboveTop);

  validatorTypeGlossaryPopover.style.left = `${left}px`;
  validatorTypeGlossaryPopover.style.top = `${top}px`;
}

function handleValidatorTypeGlossaryOutsideClick(event) {
  if (
    validatorTypeGlossaryPopover
    && !validatorTypeGlossaryPopover.contains(event.target)
    && !validatorTypeGlossaryAnchor?.contains(event.target)
  ) {
    closeValidatorTypeGlossary();
  }
}

function handleValidatorTypeGlossaryKeydown(event) {
  if (event.key === "Escape") {
    closeValidatorTypeGlossary();
  }
}

function closeValidatorTypeGlossary() {
  validatorTypeGlossaryPopover?.remove();
  validatorTypeGlossaryPopover = null;
  validatorTypeGlossaryAnchor = null;
  setValidatorTypeHelpExpanded(null);

  document.removeEventListener("click", handleValidatorTypeGlossaryOutsideClick);
  document.removeEventListener("keydown", handleValidatorTypeGlossaryKeydown);
  window.removeEventListener("resize", closeValidatorTypeGlossary);
  window.removeEventListener("scroll", closeValidatorTypeGlossary, true);
}

function setValidatorTypeHelpExpanded(activeAnchor) {
  document.querySelectorAll(".validator-type-help").forEach((button) => {
    button.setAttribute("aria-expanded", String(button === activeAnchor));
  });
}

function setValidatorTooltip(element, content) {
  const tooltip = normalizeValidatorTooltip(content);
  if (!tooltip) {
    return;
  }

  element.dataset.validatorTooltip = tooltip;
  element.addEventListener("mouseenter", handleValidatorTooltipEnter);
  element.addEventListener("focus", handleValidatorTooltipEnter);
  element.addEventListener("mouseleave", hideValidatorTooltip);
  element.addEventListener("blur", hideValidatorTooltip);
}

function normalizeValidatorTooltip(content) {
  const lines = Array.isArray(content)
    ? content
    : String(content || "").split("\n");
  return lines
    .map((line) => String(line || "").trim())
    .filter(Boolean)
    .join("\n");
}

function handleValidatorTooltipEnter(event) {
  showValidatorTooltip(event.currentTarget);
}

function showValidatorTooltip(target) {
  const content = target?.dataset?.validatorTooltip || "";
  if (!content) {
    return;
  }

  hideValidatorTooltip();
  validatorHoverTooltipTarget = target;
  validatorHoverTooltip = buildValidatorTooltip(content);
  document.body.appendChild(validatorHoverTooltip);
  positionValidatorTooltip();
  window.addEventListener("resize", hideValidatorTooltip);
  window.addEventListener("scroll", hideValidatorTooltip, true);
}

function buildValidatorTooltip(content) {
  const tooltip = document.createElement("div");
  tooltip.className = "validator-hover-tooltip";
  tooltip.setAttribute("role", "tooltip");

  for (const line of content.split("\n")) {
    const row = document.createElement("div");
    row.className = "validator-hover-tooltip-row";
    const isDanger = line.startsWith(VALIDATOR_TOOLTIP_DANGER_PREFIX);
    const displayLine = isDanger ? line.slice(VALIDATOR_TOOLTIP_DANGER_PREFIX.length).trim() : line;
    if (isDanger) {
      row.classList.add("is-danger");
    }
    const separatorIndex = displayLine.indexOf(":");
    if (separatorIndex > 0) {
      const label = document.createElement("span");
      label.className = "validator-hover-tooltip-label";
      label.textContent = displayLine.slice(0, separatorIndex + 1);
      const value = document.createElement("span");
      value.className = "validator-hover-tooltip-value";
      value.textContent = displayLine.slice(separatorIndex + 1).trim();
      row.append(label, value);
    } else {
      const value = document.createElement("span");
      value.className = "validator-hover-tooltip-value";
      value.textContent = displayLine;
      row.append(value);
    }
    tooltip.appendChild(row);
  }

  return tooltip;
}

function positionValidatorTooltip() {
  if (!validatorHoverTooltip || !validatorHoverTooltipTarget) {
    return;
  }

  const targetRect = validatorHoverTooltipTarget.getBoundingClientRect();
  const tooltipRect = validatorHoverTooltip.getBoundingClientRect();
  const left = Math.min(
    Math.max(12, targetRect.left + targetRect.width / 2 - tooltipRect.width / 2),
    window.innerWidth - tooltipRect.width - 12
  );
  const aboveTop = targetRect.top - tooltipRect.height - 9;
  const belowTop = targetRect.bottom + 9;
  const top = aboveTop >= 12
    ? aboveTop
    : Math.min(belowTop, window.innerHeight - tooltipRect.height - 12);

  validatorHoverTooltip.style.left = `${left}px`;
  validatorHoverTooltip.style.top = `${Math.max(12, top)}px`;
}

function hideValidatorTooltip() {
  validatorHoverTooltip?.remove();
  validatorHoverTooltip = null;
  validatorHoverTooltipTarget = null;
  window.removeEventListener("resize", hideValidatorTooltip);
  window.removeEventListener("scroll", hideValidatorTooltip, true);
}

function validatorCell(text, className = "", title = text) {
  const cell = document.createElement("div");
  cell.className = `validator-cell ${className}`.trim();
  cell.textContent = text;
  if (title && title !== text && title !== "-") {
    setValidatorTooltip(cell, title);
  }
  return cell;
}

function validatorIdentityCell(validator, options = {}, fallbackToPublicKey = false) {
  const cell = document.createElement("div");
  cell.className = "validator-cell validator-id";
  const identity = validatorIdentityValue(validator, fallbackToPublicKey);
  const formatted = formatDisplayAddress(identity, options);
  const address = copyableValue(formatted.text, formatted.value, "validator-address", "validator wallet address");
  setValidatorTooltip(address, validatorIdentityTooltipLines(validator));
  cell.append(address);
  return cell;
}

function validatorIdentityValue(validator, fallbackToPublicKey = false) {
  if (validator?.wallet) {
    return validatorWalletAddress(validator);
  }
  return fallbackToPublicKey ? (validator?.public_key || "-") : "-";
}

function validatorIdentityTooltipLines(validator) {
  return [
    `Validator Pubkey: ${validator?.public_key || "Unknown"}`,
    `Contract HASH: ${validator?.contract_type_hash || "Unknown"}`,
    `Type: ${validatorContractDisplayName(validator)}`,
  ];
}

function validatorContractDisplayName(validator) {
  const typeName = validator?.contract_type || "";
  if (typeName && typeName !== "Unknown") {
    return VALIDATOR_CONTRACT_TYPE_NAMES[typeName] || typeName;
  }

  const displayed = displayedValidatorType(validator);
  const entry = validatorTypeGlossaryEntry(displayed.label);
  return entry?.name || "Unknown";
}

function emptyState(text) {
  const item = document.createElement("div");
  item.className = "empty-state";
  item.textContent = text;
  return item;
}
