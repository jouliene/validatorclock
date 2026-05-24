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

function validatorCell(text, className = "", title = text) {
  const cell = document.createElement("div");
  cell.className = `validator-cell ${className}`.trim();
  cell.textContent = text;
  if (title && title !== text && title !== "-") {
    setValidatorTooltip(cell, title);
  }
  return cell;
}

function emptyState(text) {
  const item = document.createElement("div");
  item.className = "empty-state";
  item.textContent = text;
  return item;
}
