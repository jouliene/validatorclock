import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/format_numbers.js");

test("a total of zero is a total of zero, not a missing value", () => {
  assert.equal(sumTokenValues([{ stake: "2" }, { stake: "3" }], "stake"), "5");
  assert.equal(
    sumTokenValues([{ stake: "0" }, { stake: "0" }], "stake"),
    "0",
    "a round whose validators staked nothing has a total of 0; the dash means unknown",
  );
  assert.equal(sumTokenValues([], "stake"), "", "nothing to sum is unknown, and that is the dash");
  assert.equal(sumTokenValues([{ stake: "x" }], "stake"), "", "and so is a value that is not a number");
});

test("a value that is not there is not a zero", () => {
  assert.equal(sumTokenValues([{}, {}], "stake"), "", "no item carried a stake at all");
  assert.equal(sumTokenValues([{ stake: null }, { stake: "" }], "stake"), "");
  assert.equal(
    sumTokenValues([{ stake: "0" }, { stake: null }], "stake"),
    "0",
    "one item said zero and the other said nothing: the total is what was said",
  );
});
