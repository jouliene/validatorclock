let validatorTypeGlossaryPopover = null;
let validatorTypeGlossaryAnchor = null;
let validatorFakePhaseTimer = null;

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

function validatorContractDisplayName(validator) {
  const typeName = validator?.contract_type || "";
  if (typeName && typeName !== "Unknown") {
    return VALIDATOR_CONTRACT_TYPE_NAMES[typeName] || typeName;
  }

  const displayed = displayedValidatorType(validator);
  const entry = validatorTypeGlossaryEntry(displayed.label);
  return entry?.name || "Unknown";
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
