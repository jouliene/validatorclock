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
