import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/round_stats_charts.js");

test("a request that ran out of time says so, whoever timed it out", () => {
  // What the server sends (src/server/api/round_stats.rs) once its own deadline passes.
  assert.equal(
    roundStatsErrorMessage(new Error("chain round statistics request timed out")),
    "Statistics request timed out.",
  );
  // What the browser throws when the client-side deadline in api.js fires.
  const aborted = new Error("signal timed out");
  aborted.name = "TimeoutError";
  assert.equal(roundStatsErrorMessage(aborted), "Statistics request timed out.");
});

test("anything else is still just unavailable", () => {
  assert.equal(roundStatsErrorMessage(new Error("chain not found")), "Statistics are unavailable.");
  assert.equal(roundStatsErrorMessage(undefined), "Statistics are unavailable.");
});
