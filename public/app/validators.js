function renderValidators(container, validators, options) {
  const table = document.createElement("div");
  table.className = "validator-table";

  const header = document.createElement("div");
  header.className = "validator-header";
  for (const label of ["#", "Type", "Source", "Validator", "History", "Stake", "Rewards", "Weight"]) {
    header.appendChild(validatorHeaderCell(label));
  }
  table.appendChild(header);

  validators.forEach((validator, index) => {
    const row = document.createElement("div");
    row.className = `validator-row has-source-${validatorSourceKind(validator)}`;

    row.append(
      validatorCell(String(index + 1), "validator-index"),
      validatorSourceTypeCell(validator),
      validatorSourceCell(validator),
      validatorIdentityCell(validatorWalletAddress(validator)),
      validatorHistoryCell(validator.history),
      validatorCell(formatStakeAmount(validator.stake || "0"), "validator-number validator-stake", validator.stake || ""),
      validatorCell(options.rewards && validator.reward ? formatRewardCellAmount(validator.reward) : "-", "validator-number validator-rewards", validator.reward || ""),
      validatorCell(validator.weight_percent == null ? "-" : `${formatPercent(validator.weight_percent)}`, "validator-number validator-weight", validator.weight || "")
    );
    table.appendChild(row);
  });

  container.appendChild(table);
}

function renderRecentAbsentValidators(container, validators) {
  if (!Array.isArray(validators) || validators.length === 0) {
    return;
  }

  const table = document.createElement("div");
  table.className = "validator-table is-absent";

  const header = document.createElement("div");
  header.className = "validator-header";
  for (const label of ["#", "Type", "Source", "Validator", "History", "Seen"]) {
    header.appendChild(validatorHeaderCell(label));
  }
  table.appendChild(header);

  validators.forEach((validator, index) => {
    const row = document.createElement("div");
    row.className = `validator-row has-source-${validatorSourceKind(validator)}`;
    row.append(
      validatorCell(String(index + 1), "validator-index"),
      validatorSourceTypeCell(validator),
      validatorSourceCell(validator),
      validatorIdentityCell(validator.wallet || validator.public_key),
      validatorHistoryCell(validator.history),
      validatorCell(formatSeenRounds(validator), "validator-number validator-seen-rounds validator-seen", String(validator.last_seen_round || ""))
    );
    table.appendChild(row);
  });

  container.appendChild(table);
}

function validatorHeaderCell(label) {
  const cell = document.createElement("div");
  cell.className = `validator-cell${["Stake", "Rewards", "Weight", "Seen"].includes(label) ? " validator-number" : ""}`;

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
    dot.title = point.round == null
      ? "Round unknown"
      : `Round ${point.round}: ${historyStatusLabel(status)}`;
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

function validatorTypeCell(typeName, hash) {
  const name = typeof typeName === "string" ? typeName : "";
  const value = typeof hash === "string" ? hash : "";
  const cell = document.createElement("div");
  cell.className = "validator-cell validator-type";

  const badge = document.createElement("span");
  badge.className = `validator-type-badge is-${validatorTypeClass(name)}`;
  badge.appendChild(validatorBadgeText(validatorTypeLabel(name)));
  badge.title = value ? `${name || "Unknown"} · ${value}` : "Type unknown";
  cell.appendChild(badge);
  return cell;
}

function validatorSourceTypeCell(validator) {
  const cell = document.createElement("div");
  cell.className = "validator-cell validator-source-type";
  const hash = validator && validator.source && validator.source.contract_type_hash;
  const label = hash ? validatorSourceTypeLabel(hash) : validatorTypeLabel(validator && validator.contract_type);
  const className = hash ? validatorSourceTypeClass(hash) : validatorTypeClass(validator && validator.contract_type);

  const badge = document.createElement("span");
  badge.className = `validator-type-badge is-${className}`;
  badge.appendChild(validatorBadgeText(label));
  if (hash) {
    badge.title = `${label} · ${hash}`;
  } else if (validator && validator.contract_type_hash) {
    badge.title = `${validator.contract_type || "Unknown"} · ${validator.contract_type_hash}`;
  } else {
    badge.title = label === "UNKNOWN" ? "Type unknown" : label;
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

function validatorSourceTypeLabel(hash) {
  if (sameHash(hash, "533adf8a5680849177b9f213f61c48dfd8d730597078670d2367a5eef77251fe")) {
    return "StDEPOOL";
  }
  if (sameHash(hash, "14e20e304f53e6da152eb95fffc993dbd28245a775d847eed043f7c78a503885")) {
    return "DEPOOL";
  }
  return "UNKNOWN";
}

function validatorSourceTypeClass(hash) {
  if (sameHash(hash, "533adf8a5680849177b9f213f61c48dfd8d730597078670d2367a5eef77251fe")) {
    return "stdepool";
  }
  if (sameHash(hash, "14e20e304f53e6da152eb95fffc993dbd28245a775d847eed043f7c78a503885")) {
    return "depool";
  }
  return "unknown";
}

function sameHash(left, right) {
  return typeof left === "string" && left.toLowerCase() === right;
}

function validatorSourceCell(validator) {
  const cell = document.createElement("div");
  const sourceKind = validatorSourceKind(validator);
  cell.className = `validator-cell validator-source is-${sourceKind}`;
  const source = validator && validator.source;
  if (source && source.address) {
    const address = copyableValue(
      shortenAddress(source.address),
      source.address,
      "validator-source-address",
      "validator source address"
    );
    if (source.contract_type_hash) {
      address.title = source.contract_type_hash;
    }
    cell.appendChild(address);
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

function validatorSourceKind(validator) {
  const source = validator && validator.source;
  if (source && source.address) {
    return "detail";
  }
  if (validator && validator.contract_type === "EverWallet") {
    return "direct";
  }
  return "unknown";
}

function validatorTypeLabel(typeName) {
  if (typeName === "EverWallet") {
    return "EVER";
  }
  if (typeName === "DePoolProxy") {
    return "PROXY";
  }
  if (typeName === "StEverDePoolProxy") {
    return "StPROXY";
  }
  return "UNKNOWN";
}

function validatorTypeClass(typeName) {
  if (typeName === "EverWallet") {
    return "ever";
  }
  if (typeName === "DePoolProxy") {
    return "proxy";
  }
  if (typeName === "StEverDePoolProxy") {
    return "stproxy";
  }
  return "unknown";
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

function validatorIdentityCell(wallet) {
  const cell = document.createElement("div");
  cell.className = "validator-cell validator-id";
  const address = copyableValue(shortenAddress(wallet), wallet, "validator-address", "validator wallet address");
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
