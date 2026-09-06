let validatorHoverTooltip = null;
let validatorHoverTooltipTarget = null;
const VALIDATOR_TOOLTIP_DANGER_PREFIX = "[danger]";

function validatorTooltipDangerLine(text) {
  return `${VALIDATOR_TOOLTIP_DANGER_PREFIX}${text}`;
}

// What each element would say, kept beside it rather than on it. A TON round puts about
// two thousand of these on the page; as `data-` attributes that was 185 KB of text
// serialised into the DOM, and a WeakMap lets each one go with the row it belongs to.
const validatorTooltipContent = new WeakMap();

function setValidatorTooltip(element, content) {
  const tooltip = normalizeValidatorTooltip(content);
  if (!tooltip) {
    // Clearing matters for the elements that outlive a render - the status widget keeps
    // its node and only changes what it says - which is how a message that no longer
    // applies used to stay hoverable.
    validatorTooltipContent.delete(element);
    element.classList.remove("has-validator-tooltip");
    if (validatorHoverTooltipTarget === element) {
      hideValidatorTooltip();
    }
    return;
  }

  element.removeAttribute("title");
  validatorTooltipContent.set(element, tooltip);
  element.classList.add("has-validator-tooltip");
  wireValidatorTooltips();
}

// One set of listeners for the page instead of five on every element that carries a
// tooltip - about nine and a half thousand of them on a TON round, attached again on
// every render of the tables. The element under the pointer is found by looking upwards
// from whatever the event landed on.
let validatorTooltipsWired = false;

function wireValidatorTooltips() {
  if (validatorTooltipsWired) {
    return;
  }
  validatorTooltipsWired = true;
  document.addEventListener("pointerover", handleValidatorTooltipPointerOver);
  document.addEventListener("pointerout", handleValidatorTooltipPointerOut);
  document.addEventListener("focusin", handleValidatorTooltipFocusIn);
  document.addEventListener("focusout", handleValidatorTooltipFocusOut);
  document.addEventListener("pointerdown", handleValidatorTooltipPointerDown, true);
}

function validatorTooltipTarget(node) {
  return node instanceof Element ? node.closest(".has-validator-tooltip") : null;
}

function handleValidatorTooltipPointerOver(event) {
  // A touch shows a tooltip by tapping, below; pointerover fires for it too, and would
  // open the tooltip a tap was about to close.
  if (isTouchLikePointer(event)) {
    return;
  }
  const target = validatorTooltipTarget(event.target);
  if (target && target !== validatorHoverTooltipTarget) {
    showValidatorTooltip(target);
  }
}

function handleValidatorTooltipPointerOut(event) {
  if (!validatorHoverTooltipTarget || isTouchLikePointer(event)) {
    return;
  }
  if (validatorTooltipTarget(event.target) !== validatorHoverTooltipTarget) {
    return;
  }
  // Moving between two children of the same element is not leaving it.
  if (event.relatedTarget && validatorHoverTooltipTarget.contains(event.relatedTarget)) {
    return;
  }
  hideValidatorTooltip();
}

function handleValidatorTooltipFocusIn(event) {
  const target = validatorTooltipTarget(event.target);
  if (target) {
    showValidatorTooltip(target);
  }
}

function handleValidatorTooltipFocusOut(event) {
  if (validatorTooltipTarget(event.target) === validatorHoverTooltipTarget) {
    hideValidatorTooltip();
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

// A press does two jobs: outside an open tooltip it closes it, and on a touch screen it
// is how a tooltip is opened at all.
function handleValidatorTooltipPointerDown(event) {
  const target = validatorTooltipTarget(event.target);
  if (!target) {
    if (validatorHoverTooltipTarget && !validatorHoverTooltipTarget.contains(event.target)) {
      hideValidatorTooltip();
    }
    return;
  }

  if (!isTouchLikePointer(event) || isTooltipButton(target)) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();

  if (validatorHoverTooltipTarget === target) {
    hideValidatorTooltip();
    return;
  }

  showValidatorTooltip(target);
}

function isTouchLikePointer(event) {
  return event.pointerType === "touch" || event.pointerType === "pen";
}

function isTooltipButton(target) {
  return target instanceof HTMLButtonElement;
}

function showValidatorTooltip(target) {
  const content = validatorTooltipContent.get(target) || "";
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

