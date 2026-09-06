import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/state.js", "app/map.js");

test("map nodes are only ever read as the selected chain's", () => {
  state.selectedChainId = "ton";
  validatorMapNodes = [{ peer: "aa", ip: "203.0.113.1" }];
  validatorMapNodesChainId = "ton";
  assert.deepEqual(currentChainMapNodes(), validatorMapNodes);

  // What a chain switch leaves behind for the moment before the new chain's map arrives:
  // nodes belonging to the chain the reader just left.
  state.selectedChainId = "everscale";
  assert.equal(
    currentChainMapNodes(),
    null,
    "the previous chain's dots are not the new chain's, however recently they were drawn",
  );

  validatorMapNodes = null;
  validatorMapNodesChainId = "everscale";
  assert.equal(currentChainMapNodes(), null, "and nothing loaded yet is nothing to draw");
});
