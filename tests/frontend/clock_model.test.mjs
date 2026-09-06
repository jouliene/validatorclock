import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/state.js", "app/clock_model.js");

// TON's numbers: an 18.2 h round, elections open 9.1 h before it ends and close 2.3 h before.
const ROUND = 65536;
const START_BEFORE = 32768;
const END_BEFORE = 8192;
const snapshotAt = (utimeSince, { next = false } = {}) => ({
  params15: {
    validators_elected_for: ROUND,
    elections_start_before: START_BEFORE,
    elections_end_before: END_BEFORE,
    stake_held_for: ROUND / 2,
  },
  current_set: { utime_since: utimeSince, utime_until: utimeSince + ROUND, round_color: "blue" },
  next_set: next
    ? { utime_since: utimeSince + ROUND, utime_until: utimeSince + 2 * ROUND, round_color: "green" }
    : null,
});

test("a round passes through all three phases", () => {
  const snapshot = snapshotAt(0);
  const electionsStart = ROUND - START_BEFORE;
  const electionsEnd = ROUND - END_BEFORE;

  assert.equal(buildClockModel(snapshot, electionsStart - 1).status, "Before elections");
  assert.equal(buildClockModel(snapshot, electionsStart).status, "Elections open");
  assert.equal(buildClockModel(snapshot, electionsEnd - 1).status, "Elections open");
  assert.equal(
    buildClockModel(snapshot, electionsEnd).status,
    "After elections",
    "the hours between the vote and the round change are after the vote, not before the next one",
  );
});

test("the phase does not change when the elected set becomes known", () => {
  const now = ROUND - END_BEFORE + 60;
  assert.equal(
    buildClockModel(snapshotAt(0, { next: true }), now).status,
    buildClockModel(snapshotAt(0), now).status,
    "next_set arriving is the result of the vote, not a different phase",
  );
});

test("the window shown after the vote is the next round's", () => {
  const snapshot = snapshotAt(0);
  const after = buildClockModel(snapshot, ROUND - END_BEFORE + 60);

  assert.equal(after.electionsStart, ROUND - START_BEFORE + ROUND);
  assert.equal(after.electionsEnd, ROUND - END_BEFORE + ROUND);
  assert.ok(after.electionsStart > snapshot.current_set.utime_until, "which is the half of the dial ahead");

  const during = buildClockModel(snapshot, ROUND - START_BEFORE + 60);
  assert.equal(during.electionsStart, ROUND - START_BEFORE, "while they are open, the window is this round's");
  assert.equal(during.electionsEnd, ROUND - END_BEFORE);
});
