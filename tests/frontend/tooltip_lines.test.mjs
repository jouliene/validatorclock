import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/validator_tooltips.js");

test("a label is words before a colon, and an address is not a label", () => {
  assert.equal(validatorTooltipLabelEnd("Validator Pubkey: abc"), "Validator Pubkey".length);
  assert.equal(validatorTooltipLabelEnd("Last seen: 4 min ago"), "Last seen".length);
  assert.equal(
    validatorTooltipLabelEnd(`-1:${"ab".repeat(32)}`),
    -1,
    "a masterchain address is one value; splitting it left '-1:' standing as a label",
  );
  assert.equal(validatorTooltipLabelEnd("no colon here"), -1);
});

// Selection is how a validator in the table is found on the map, so the table has to be
// able to make one - with a mouse it could only clear one.
load("app/validators.js");

test("a click on a row selects it; a press on a control does not", () => {
  const mouse = { isPrimary: true, pointerType: "mouse", button: 0 };
  assert.equal(validatorSelectionCanStart(mouse, "abc"), true);

  const control = { closest: (selector) => (selector.includes("button") ? {} : null) };
  assert.equal(isValidatorSelectionInteractiveTarget(control, mouse), true, "a copy button copies");

  const tooltipCell = { closest: (selector) => (selector.includes("has-validator-tooltip") ? {} : null) };
  assert.equal(
    isValidatorSelectionInteractiveTarget(tooltipCell, mouse),
    false,
    "a tooltip opens on hover, so the click underneath it belongs to the row",
  );
  assert.equal(
    isValidatorSelectionInteractiveTarget(tooltipCell, { pointerType: "touch", isPrimary: true }),
    true,
    "on a touch screen the tap is how the tooltip is read at all",
  );
});
