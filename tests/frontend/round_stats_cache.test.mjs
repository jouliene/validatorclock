import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/state.js", "app/round_stats_data.js");

test("a cache that is fresh for one poll interval is fresh at a poll", () => {
  state.refreshSeconds = 60;
  // The page polls every refreshSeconds / 2. A window of exactly that is never open when
  // the tick arrives, so both the prefetch timer and the clock handler refetched each time.
  assert.ok(
    roundStatsCacheMaxAgeSeconds() > Math.floor(state.refreshSeconds / 2),
    "the window has to outlast the interval between two polls",
  );
});

test("an answer older than the one already held is not an update", () => {
  // Storing a snapshot for the selected chain also repaints its APR badges, which is the
  // round panels' business and not this test's.
  globalThis.renderRoundAprBadges = () => {};
  state.selectedChainId = "ton";
  state.roundStatsByChain = new Map();
  state.roundStatsCachedAtByChain = new Map();

  storeRoundStatsSnapshot("ton", { fetched_at: 200, rounds: ["new"] });
  storeRoundStatsSnapshot("ton", { fetched_at: 100, rounds: ["old"] });

  assert.deepEqual(
    state.roundStatsByChain.get("ton").rounds,
    ["new"],
    "the cache request and the live one answer in either order; the older is not news",
  );

  storeRoundStatsSnapshot("ton", { fetched_at: 300, rounds: ["newer"] });
  assert.deepEqual(state.roundStatsByChain.get("ton").rounds, ["newer"]);
});
