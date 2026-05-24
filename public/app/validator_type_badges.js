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
