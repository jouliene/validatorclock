import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/state.js", "app/runtime.js", "app/runtime_clock.js", "app/format_dates.js");

let monotonicNow = 100_000;
Object.defineProperty(globalThis, "performance", { value: { now: () => monotonicNow }, configurable: true });
const timers = new Map();
let timerId = 0;
window.setTimeout = (callback, delay) => { timers.set(++timerId, { callback, delay }); return timerId; };
window.clearTimeout = (id) => timers.delete(id);
globalThis.requestIsCurrent = (seq, current, chain) => seq === current && chain === state.selectedChainId;
globalThis.renderNow = () => {};
globalThis.setError = () => {};
globalThis.renderChainTabs = () => {};
globalThis.handleRoundStatsClockSnapshot = () => {};
globalThis.mapAvailableForChain = () => true;
globalThis.applyCachedValidatorMapNodesForChain = () => null;
globalThis.refreshValidatorMapNodesForSnapshot = () => new Promise(() => {});
const snapshot = { chain: { id: "ton" }, fetched_at: 1, refreshing: true };

function reset() {
  timers.clear();
  state.selectedChainId = "ton";
  state.clockRequestSeq = 0;
  state.clockReceivedAtByChain = new Map();
  state.snapshot = null;
}

test("an old server snapshot is shown immediately without waiting for a slow map", async () => {
  reset();
  globalThis.fetchClockSnapshot = async () => snapshot;
  await loadClock(false);
  assert.equal(state.snapshot, snapshot);
  assert.equal(state.clockReceivedAtByChain.get("ton"), monotonicNow);
  assert.equal(state.clockLoading, false);
  assert.equal(timers.get(state.pollTimer).delay, 60_000);
  assert.equal([...timers.values()].some(t => t.delay === 5000), false,
    "a server refresh in progress does not create an extra five-second page poll");
});

test("counter uses local receipt time: now, 0, 1, 58, 59, then a new receipt", () => {
  const label = {}, value = {};
  for (const [elapsed, expected] of [[0, "now"], [250, "0s"], [1000, "1s"], [58000, "58s"], [59000, "59s"]]) {
    renderInfoUpdated(label, value, 100_000, 100_000 + elapsed);
    assert.equal(value.textContent, expected);
  }
  renderInfoUpdated(label, value, 160_000, 160_000);
  assert.equal(value.textContent, "now");
  renderInfoUpdated(label, value, 160_000, 160_250);
  assert.equal(value.textContent, "0s");
});

test("failed responses do not reset the counter or claim a successful update", async () => {
  reset();
  state.clockReceivedAtByChain.set("ton", 40_000);
  globalThis.fetchClockSnapshot = async () => { throw new Error("offline"); };
  await assert.rejects(loadClock(false), /offline/);
  assert.equal(state.clockReceivedAtByChain.get("ton"), 40_000);
  const label = {}, value = {};
  renderInfoUpdated(label, value, 40_000, 101_000);
  assert.equal(value.textContent, "61s");
  assert.equal(timers.get(state.pollTimer).delay, 60_000);
});

test("starting timers preserves the minute relative to the successful response", () => {
  reset();
  state.clockReceivedAtByChain.set("ton", monotonicNow - 2000);
  scheduleClockRefresh();
  assert.equal(timers.get(state.pollTimer).delay, 58_000);
  assert.equal(refreshPollSeconds(), 60);
});

test("a late response from the previous chain cannot reset the selected clock", async () => {
  reset();
  let answer;
  globalThis.fetchClockSnapshot = () => new Promise(resolve => { answer = resolve; });
  const loading = loadClock(false);
  state.selectedChainId = "everscale";
  answer(snapshot);
  await loading;
  assert.equal(state.snapshot, null);
  assert.equal(state.clockReceivedAtByChain.size, 0);
});
