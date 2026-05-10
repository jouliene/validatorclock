const VALIDATOR_HEADER_CLASSES = {
  "#": "validator-index",
  Type: "validator-source-type",
  Source: "validator-source",
  Validator: "validator-id",
  History: "validator-history",
  Stake: "validator-stake",
  Rewards: "validator-rewards",
  Weight: "validator-weight",
  Seen: "validator-seen",
};

const VALIDATOR_NUMBER_HEADERS = new Set(["Stake", "Rewards", "Weight", "Seen"]);
const VALIDATOR_ROUND_HEADERS = ["#", "Type", "Source", "Validator", "History", "Stake", "Rewards", "Weight"];
const VALIDATOR_ABSENT_HEADERS = ["#", "Type", "Source", "Validator", "History", "Seen"];

const UNKNOWN_VALIDATOR_TYPE = { label: "UNKNOWN", className: "unknown" };

const VALIDATOR_CONTRACT_TYPES = {
  EverWallet: { label: "EVER", className: "ever" },
  DePoolProxy: { label: "PROXY", className: "proxy" },
  StEverDePoolProxy: { label: "StPROXY", className: "stproxy" },
  SingleNominatorV1_1: { label: "SNOMv1.1", className: "snom" },
  SingleNominatorV1_0: { label: "SNOMv1.0", className: "snom" },
  TonNominatorPool: { label: "NOMPOOL", className: "nompool" },
  ValidatorController: { label: "VCTRL", className: "vctrl" },
};

const VALIDATOR_SOURCE_TYPES = {
  "533adf8a5680849177b9f213f61c48dfd8d730597078670d2367a5eef77251fe": {
    label: "StDEPOOL",
    className: "stdepool",
  },
  "14e20e304f53e6da152eb95fffc993dbd28245a775d847eed043f7c78a503885": {
    label: "DEPOOL",
    className: "depool",
  },
};

const VALIDATOR_TYPE_GLOSSARY = [
  { label: "EVER", name: "Ever Wallet", description: "Default Broxus wallet for Tycho TVM networks. Can be deployed in the masterchain and used directly for validation." },
  { label: "DEPOOL", name: "DePool", description: "Staking pool contract where many users can stake into one shared pool. The pool participates in validation through a proxy contract deployed in the masterchain." },
  { label: "StDEPOOL", name: "Staked EVER DePool", description: "Specialized DePool that uses liquid-staking funds for validation. It validates through a masterchain proxy contract, the same way as a regular DePool." },
  { label: "SNOMv1.1", name: "Single Nominator v1.1", description: "TON validator contract with a cold owner and hot validator role." },
  { label: "SNOMv1.0", name: "Single Nominator v1.0", description: "TON validator contract with a cold owner and hot validator role." },
  { label: "NOMPOOL", name: "TON Nominator Pool", description: "Multi-user TON staking pool where nominators delegate stake to a validator. The pool participates in validation and distributes rewards by pool settings." },
  { label: "VCTRL", name: "Validator Controller", description: "TON controller contract that manages validation for a pool or operator and points to the basechain pool address used as the funding source." },
  { label: "UNKNOWN", name: "Unknown", description: "Contract type has not been identified yet." },
];

let validatorTypeGlossaryPopover = null;
let validatorTypeGlossaryAnchor = null;
let validatorHoverTooltip = null;
let validatorHoverTooltipTarget = null;

function renderValidators(container, validators, options = {}) {
  const table = document.createElement("div");
  table.className = "validator-table";
  table.appendChild(validatorHeader(VALIDATOR_ROUND_HEADERS));

  validators.forEach((validator, index) => {
    const row = document.createElement("div");
    row.className = `validator-row has-source-${validatorSourceKind(validator, options)}`;

    row.append(
      validatorCell(String(index + 1), "validator-index"),
      validatorSourceTypeCell(validator),
      validatorSourceCell(validator, options),
      validatorIdentityCell(validatorWalletAddress(validator), options),
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
  table.appendChild(validatorHeader(VALIDATOR_ABSENT_HEADERS));

  validators.forEach((validator, index) => {
    const row = document.createElement("div");
    row.className = `validator-row has-source-${validatorSourceKind(validator, options)}`;
    row.append(
      validatorCell(String(index + 1), "validator-index"),
      validatorSourceTypeCell(validator),
      validatorSourceCell(validator, options),
      validatorIdentityCell(validator.wallet || validator.public_key, options),
      validatorHistoryCell(validator.history),
      validatorCell(formatSeenRounds(validator), "validator-number validator-seen-rounds validator-seen", String(validator.last_seen_round || ""))
    );
    table.appendChild(row);
  });

  container.appendChild(table);
}

function validatorHeader(labels) {
  const header = document.createElement("div");
  header.className = "validator-header";
  for (const label of labels) {
    header.appendChild(validatorHeaderCell(label));
  }
  return header;
}

function validatorHeaderCell(label) {
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
    dot.className = `validator-history-dot is-${status}`;
    setValidatorTooltip(
      dot,
      point.round == null
        ? "Round: unknown"
        : [`Round: ${point.round}`, `Status: ${historyStatusLabel(status)}`]
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

function validatorSourceTypeCell(validator) {
  const cell = document.createElement("div");
  cell.className = "validator-cell validator-source-type";
  const hash = validator && validator.source && validator.source.contract_type_hash;
  const type = hash ? validatorSourceType(hash) : validatorContractType(validator && validator.contract_type);

  const badge = document.createElement("span");
  badge.className = `validator-type-badge is-${type.className}`;
  badge.appendChild(validatorBadgeText(type.label));
  if (hash) {
    setValidatorTooltip(badge, validatorTypeTooltipLines(type.label, hash));
  } else if (validator && validator.contract_type_hash) {
    setValidatorTooltip(badge, validatorTypeTooltipLines(type.label, validator.contract_type_hash));
  } else {
    setValidatorTooltip(badge, validatorTypeTooltipLines(type.label));
  }
  cell.appendChild(badge);
  return cell;
}

function validatorBadgeText(label) {
  const text = document.createElement("span");
  text.className = "validator-type-badge-text";
  text.textContent = label;
  return text;
}

function validatorSourceType(hash) {
  return VALIDATOR_SOURCE_TYPES[String(hash || "").toLowerCase()] || UNKNOWN_VALIDATOR_TYPE;
}

function validatorContractType(typeName) {
  return VALIDATOR_CONTRACT_TYPES[typeName] || UNKNOWN_VALIDATOR_TYPE;
}

function validatorSourceCell(validator, options = {}) {
  const cell = document.createElement("div");
  const sourceKind = validatorSourceKind(validator, options);
  cell.className = `validator-cell validator-source is-${sourceKind}`;
  const source = validator && validator.source;
  if (source && source.address) {
    const formatted = formatDisplayAddress(source.address, options);
    const address = copyableValue(
      formatted.text,
      formatted.value,
      "validator-source-address",
      "validator source address"
    );
    const contractHash = source.contract_type_hash || validator?.contract_type_hash;
    setValidatorTooltip(address, validatorSourceTooltipLines(validator, formatted, contractHash));
    cell.appendChild(address);
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

  if (validator && validator.contract_type === "EverWallet") {
    const direct = document.createElement("span");
    direct.className = "validator-source-direct";
    direct.textContent = "Direct";
    cell.appendChild(direct);
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
  if (tonValidatorContractHash(validator, options)) {
    return "detail";
  }
  if (validator && validator.contract_type === "EverWallet") {
    return "direct";
  }
  return "unknown";
}

function validatorSourceTooltipLines(validator, formatted, contractHash = "") {
  const role = validatorSourceRole(validator);
  const lines = role ? [`Source: ${role}`] : [];
  lines.push(...formatted.tooltip);
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
    return "Pool address";
  }
  if (validator.contract_type === "SingleNominatorV1_0" || validator.contract_type === "SingleNominatorV1_1") {
    return "Owner address";
  }
  return "";
}

function tonValidatorContractHash(validator, options = {}) {
  if (options.chainId !== "ton") {
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
  const lines = [`Type: ${label}`];
  if (entry) {
    lines.push(`Name: ${entry.name}`);
  }
  if (contractHash) {
    lines.push(`Contract HASH: ${contractHash}`);
  }
  return lines;
}

function toggleValidatorTypeGlossary(anchor) {
  if (validatorTypeGlossaryPopover && validatorTypeGlossaryAnchor === anchor) {
    closeValidatorTypeGlossary();
    return;
  }

  closeValidatorTypeGlossary();
  validatorTypeGlossaryAnchor = anchor;
  validatorTypeGlossaryPopover = buildValidatorTypeGlossary();
  document.body.appendChild(validatorTypeGlossaryPopover);
  positionValidatorTypeGlossary();
  setValidatorTypeHelpExpanded(anchor);

  document.addEventListener("click", handleValidatorTypeGlossaryOutsideClick);
  document.addEventListener("keydown", handleValidatorTypeGlossaryKeydown);
  window.addEventListener("resize", closeValidatorTypeGlossary);
  window.addEventListener("scroll", closeValidatorTypeGlossary, true);
}

function buildValidatorTypeGlossary() {
  const popover = document.createElement("div");
  popover.className = "validator-type-glossary";
  popover.setAttribute("role", "dialog");
  popover.setAttribute("aria-label", "Validator type glossary");

  const title = document.createElement("div");
  title.className = "validator-type-glossary-title";
  title.textContent = "Type glossary";
  popover.appendChild(title);

  for (const entry of VALIDATOR_TYPE_GLOSSARY) {
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

function glossaryBadgeClass(label) {
  if (label === "EVER") return "ever";
  if (label === "PROXY" || label === "DEPOOL") return "proxy";
  if (label === "StPROXY" || label === "StDEPOOL") return "stproxy";
  if (label === "SNOMv1.0" || label === "SNOMv1.1") return "snom";
  if (label === "NOMPOOL") return "nompool";
  if (label === "VCTRL") return "vctrl";
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
    const separatorIndex = line.indexOf(":");
    if (separatorIndex > 0) {
      const label = document.createElement("span");
      label.className = "validator-hover-tooltip-label";
      label.textContent = line.slice(0, separatorIndex + 1);
      const value = document.createElement("span");
      value.className = "validator-hover-tooltip-value";
      value.textContent = line.slice(separatorIndex + 1).trim();
      row.append(label, value);
    } else {
      const value = document.createElement("span");
      value.className = "validator-hover-tooltip-value";
      value.textContent = line;
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

function validatorIdentityCell(wallet, options = {}) {
  const cell = document.createElement("div");
  cell.className = "validator-cell validator-id";
  const formatted = formatDisplayAddress(wallet, options);
  const address = copyableValue(formatted.text, formatted.value, "validator-address", "validator wallet address");
  setValidatorTooltip(address, formatted.tooltip);
  cell.append(address);
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
  wireCopyButton(button, feedback, value);
  return button;
}

function wireCopyButton(button, feedback, value) {
  button.addEventListener("click", async (event) => {
    event.preventDefault();
    event.stopPropagation();
    hideValidatorTooltip();
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
