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

function emptyState(text) {
  const item = document.createElement("div");
  item.className = "empty-state";
  item.textContent = text;
  return item;
}
