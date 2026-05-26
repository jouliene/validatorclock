let validatorTypeGlossaryPopover = null;
let validatorTypeGlossaryAnchor = null;

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
  if (label === "StEVER") return "stever";
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
