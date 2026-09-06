import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/state.js", "app/map.js", "app/map_data.js");

const CHAIN = "ton";
const snapshotFor = (roundId) => ({
  chain: { id: CHAIN },
  current_set: { round_id: roundId, round_color: "blue", utime_since: 1000, validators: [] },
});

function withCachedNodes(roundId, nodes, fetchedAt) {
  state.selectedChainId = CHAIN;
  state.refreshSeconds = 60;
  state.snapshot = snapshotFor(roundId);
  state.snapshotsByChain = new Map([[CHAIN, state.snapshot]]);
  state.validatorMapNodesByChain = new Map([[CHAIN, nodes]]);
  state.validatorMapNodeCacheKeysByChain = new Map([
    [CHAIN, validatorMapSnapshotCacheKey(state.snapshot)],
  ]);
  state.validatorMapFetchedAtByChain = new Map([[CHAIN, fetchedAt]]);
}

test("the map is asked for again within a round, but not on every poll", () => {
  const now = Math.trunc(Date.now() / 1000);
  const key = validatorMapSnapshotCacheKey(snapshotFor(27_291));

  withCachedNodes(27_291, [{ peer: "aa" }], now - 5);
  assert.equal(
    validatorMapNodesAreRecent(CHAIN, key),
    true,
    "five seconds after a fetch there is nothing new to fetch: the resolver publishes every few minutes",
  );

  withCachedNodes(27_291, [{ peer: "aa" }], now - 120);
  assert.equal(validatorMapNodesAreRecent(CHAIN, key), false, "two minutes on, ask again");

  withCachedNodes(27_291, [{ peer: "aa" }], now - 5);
  assert.equal(
    validatorMapNodesAreRecent(CHAIN, validatorMapSnapshotCacheKey(snapshotFor(27_292))),
    false,
    "a new round is a different set of validators, whatever the clock says",
  );
});

test("a chain with no validator set yet is not asked about at all", async () => {
  state.selectedChainId = CHAIN;
  state.chains = [{ id: CHAIN, has_map: true }];
  state.snapshot = null;
  state.snapshotsByChain = new Map();
  state.validatorMapNodesByChain = new Map();
  globalThis.fetch = () => assert.fail("asked for a map with nothing to match it against");

  assert.deepEqual(await refreshValidatorMapNodesForSnapshot(CHAIN), []);
});
