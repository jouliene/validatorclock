let validatorHoverTooltip = null;
let validatorHoverTooltipTarget = null;
const VALIDATOR_TOOLTIP_DANGER_PREFIX = "[danger]";

function validatorTooltipDangerLine(text) {
  return `${VALIDATOR_TOOLTIP_DANGER_PREFIX}${text}`;
}

function setValidatorTooltip(element, content) {
  const tooltip = normalizeValidatorTooltip(content);
  if (!tooltip) {
    return;
  }

  element.removeAttribute("title");
  element.dataset.validatorTooltip = tooltip;
  if (!element.classList.contains("has-validator-tooltip")) {
    element.classList.add("has-validator-tooltip");
    element.addEventListener("mouseenter", handleValidatorTooltipEnter);
    element.addEventListener("focus", handleValidatorTooltipEnter);
    element.addEventListener("pointerdown", handleValidatorTooltipPointerDown);
    element.addEventListener("mouseleave", hideValidatorTooltip);
    element.addEventListener("blur", hideValidatorTooltip);
  }
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

function handleValidatorTooltipPointerDown(event) {
  if (!isTouchLikePointer(event) || isTooltipButton(event.currentTarget)) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();

  if (validatorHoverTooltipTarget === event.currentTarget) {
    hideValidatorTooltip();
    return;
  }

  showValidatorTooltip(event.currentTarget);
}

function isTouchLikePointer(event) {
  return event.pointerType === "touch" || event.pointerType === "pen";
}

function isTooltipButton(target) {
  return target instanceof HTMLButtonElement;
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
  document.addEventListener("pointerdown", handleValidatorTooltipOutsidePointerDown, true);
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
  document.removeEventListener("pointerdown", handleValidatorTooltipOutsidePointerDown, true);
}

function handleValidatorTooltipOutsidePointerDown(event) {
  if (validatorHoverTooltipTarget?.contains(event.target)) {
    return;
  }
  hideValidatorTooltip();
}
