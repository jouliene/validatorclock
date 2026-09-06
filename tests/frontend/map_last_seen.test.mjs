import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/state.js", "app/map.js", "app/map_data.js");

const at = (secondsAgo) => ({ last_seen_at: Math.trunc(Date.now() / 1000) - secondsAgo });

test("a node is called remembered by the clock, not by the rest of the file", () => {
  // The sweep that produces these takes minutes on a large chain, so its first answers
  // are minutes older than its last. Judging one against the other made almost every TON
  // validator look stale and made a small chain's validators never look stale at all.
  assert.equal(validatorMapNodeIsRemembered(at(60)), false);
  // TON's cycle is a nine-minute sweep and a five-minute pause, so a node answering every
  // time is still a quarter of an hour old just before its next turn.
  assert.equal(validatorMapNodeIsRemembered(at(14 * 60)), false, "a healthy node on the slowest chain");
  assert.equal(validatorMapNodeIsRemembered(at(40 * 60)), true, "two cycles with no answer is missing");
  assert.equal(validatorMapNodeIsRemembered({}), false, "a node with no timestamp says nothing");
});

test("the label says how long ago, in the units a reader thinks in", () => {
  assert.equal(validatorMapLastSeenLabel(at(300)), null, "nothing to say about a node just seen");
  assert.equal(validatorMapLastSeenLabel(at(40 * 60)), "40 min ago");
  assert.equal(validatorMapLastSeenLabel(at(3 * 3600)), "3 h ago");
});
