const VALIDATOR_SELECTION_MAX_TAP_DISTANCE = 8;
const VALIDATOR_SELECTION_MAX_TAP_MS = 650;

let validatorSelectionWired = false;
let validatorSelectionPointer = null;

function renderValidators(container, validators, options = {}) {
  const table = document.createElement("div");
  table.className = "validator-table";
  table.appendChild(validatorHeader(VALIDATOR_ROUND_HEADERS, options));

  validators.forEach((validator, index) => {
    const row = document.createElement("div");
    const sourceKind = validatorSourceKind(validator, options);
    row.className = `validator-row has-source-${sourceKind}`;
    setValidatorRowMetadata(row, validator, index, options);
    if (isFakeMapValidator(validator, options)) {
      row.classList.add("is-map-fake");
    }

    row.append(
      validatorCell(String(index + 1), "validator-index"),
      validatorSourceTypeCell(validator, options),
      validatorSourceCell(validator, options),
      validatorIdentityCell(validator, options),
      validatorHistoryCell(validator.history),
      validatorCell(formatStakeAmount(validator.stake || "0"), "validator-number validator-stake", exactValueTooltip("Stake", validator.stake)),
      validatorCell(options.rewards && validator.reward ? formatRewardCellAmount(validator.reward) : "-", "validator-number validator-rewards", exactValueTooltip("Rewards", validator.reward)),
      validatorCell(validator.weight_percent == null ? "-" : `${formatPercent(validator.weight_percent)}`, "validator-number validator-weight", exactValueTooltip("Weight", validator.weight))
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
    setValidatorRowMetadata(row, validator, index, options);
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

function setValidatorRowMetadata(row, validator, index, options = {}) {
  const peer = String(validator?.public_key || "").toLowerCase();
  if (peer) {
    row.dataset.validatorPeer = peer;
    row.dataset.validatorSelectionKey = validatorSelectionKey(peer, options);
  }
  row.dataset.validatorRow = String(index + 1);
  row.tabIndex = -1;
  syncValidatorSelectionForRow(row);
}

function validatorSelectionKey(peer, options = {}) {
  const scope = String(options.validatorSelectionScope || "round").toLowerCase();
  const color = String(options.validatorSelectionColor || "").toLowerCase();
  return `${scope}:${color}:${peer}`;
}

function setupValidatorSelection() {
  if (validatorSelectionWired) {
    return;
  }

  validatorSelectionWired = true;
  document.addEventListener("pointerdown", handleValidatorSelectionPointerDown);
  document.addEventListener("pointerup", handleValidatorSelectionPointerUp);
  document.addEventListener("pointercancel", clearValidatorSelectionPointer);
}

function handleValidatorSelectionPointerDown(event) {
  if (!validatorSelectionCanTrack(event)) {
    return;
  }

  const target = event.target instanceof Element ? event.target : null;
  const row = target?.closest(".validator-row[data-validator-selection-key]");
  if (!row) {
    startValidatorSelectionClear(event, target);
    return;
  }

  const selectionKey = row.dataset.validatorSelectionKey;
  if (isValidatorSelectionInteractiveTarget(target, event)) {
    if (selectionKey !== state.selectedValidatorKey) {
      startValidatorSelectionClear(event, target);
    }
    return;
  }

  if (!validatorSelectionCanStart(event, selectionKey)) {
    startValidatorSelectionClear(event, target);
    return;
  }

  validatorSelectionPointer = {
    pointerId: event.pointerId,
    mode: "toggle",
    selectionKey,
    startX: event.clientX,
    startY: event.clientY,
    startedAt: Date.now(),
    chainId: state.selectedChainId,
  };
}

function startValidatorSelectionClear(event, target) {
  if (!state.selectedValidatorKey || target?.closest(".validator-row.is-validator-selected")) {
    return;
  }

  validatorSelectionPointer = {
    pointerId: event.pointerId,
    mode: "clear",
    selectionKey: state.selectedValidatorKey,
    startX: event.clientX,
    startY: event.clientY,
    startedAt: Date.now(),
    chainId: state.selectedChainId,
  };
}

function handleValidatorSelectionPointerUp(event) {
  const pending = validatorSelectionPointer;
  if (!pending || pending.pointerId !== event.pointerId) {
    return;
  }

  clearValidatorSelectionPointer();
  if (pending.chainId !== state.selectedChainId || !pending.selectionKey) {
    return;
  }
  if (Date.now() - pending.startedAt > VALIDATOR_SELECTION_MAX_TAP_MS) {
    return;
  }
  if (validatorSelectionDistance(pending, event) > VALIDATOR_SELECTION_MAX_TAP_DISTANCE) {
    return;
  }

  const target = event.target instanceof Element ? event.target : null;
  if (pending.mode === "clear") {
    setSelectedValidatorKey(null);
    return;
  }

  if (target && isValidatorSelectionInteractiveTarget(target, event)) {
    return;
  }

  setSelectedValidatorKey(state.selectedValidatorKey === pending.selectionKey ? null : pending.selectionKey);
}

function validatorSelectionCanTrack(event) {
  return event.isPrimary !== false && (event.pointerType !== "mouse" || event.button === 0);
}

// A click on a row selects that validator, which is how it is found on the map. With a
// mouse this used to work only on a row that was already selected - so the table could
// clear a selection and never make one, and the only way to pick a validator was to find
// its dot on the map first.
function validatorSelectionCanStart(event, selectionKey) {
  return event.isPrimary !== false;
}

// Controls take the press: a copy button copies rather than selects. A tooltip is not a
// control for a mouse - it opens on hover and the click underneath it belongs to the row.
// On a touch screen it is: tapping is the only way to read a tooltip there, and almost
// every cell in a row carries one.
function isValidatorSelectionInteractiveTarget(target, event) {
  if (target?.closest("button, a, input, select, textarea, summary, [role='button']")) {
    return true;
  }
  const touchLike = event?.pointerType === "touch" || event?.pointerType === "pen";
  return touchLike && Boolean(target?.closest(".has-validator-tooltip"));
}

function validatorSelectionDistance(start, event) {
  return Math.hypot(event.clientX - start.startX, event.clientY - start.startY);
}

function clearValidatorSelectionPointer() {
  validatorSelectionPointer = null;
}

function setSelectedValidatorKey(key) {
  const normalizedKey = String(key || "").toLowerCase();
  state.selectedValidatorKey = normalizedKey || null;
  syncSelectedValidatorRows();
}

function syncSelectedValidatorRows(options = {}) {
  let hasSelectedRow = false;
  document.querySelectorAll(".validator-row[data-validator-peer]").forEach((row) => {
    syncValidatorSelectionForRow(row);
    hasSelectedRow = hasSelectedRow || row.classList.contains("is-validator-selected");
  });
  if (options.clearMissing && state.selectedValidatorKey && !hasSelectedRow) {
    state.selectedValidatorKey = null;
  }
}

function syncValidatorSelectionForRow(row) {
  row.classList.toggle(
    "is-validator-selected",
    Boolean(state.selectedValidatorKey && row.dataset.validatorSelectionKey === state.selectedValidatorKey)
  );
}

// The cells round what they show; the tooltip is where the number itself belongs. It
// used to be handed the bare string - eighteen digits of weight with nothing to say what
// they were.
function exactValueTooltip(label, value) {
  return value ? `${label}: ${formatWeight(value)}` : "";
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
