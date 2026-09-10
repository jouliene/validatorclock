import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";
stubBrowser();
load("app/state.js", "app/map.js", "app/map_data.js", "app/validator_locations.js");

test("uncertainty reaches the validator tooltip", () => {
  const node = { ip: "67.213.125.125", city: "Sydney", country: "Australia" };
  const uncertain = mapNodeTooltipLines({ ...node, geo_confidence: "disputed" });
  assert.ok(uncertain.some((line) => line.includes("sources disagree")));
  const measured = mapNodeTooltipLines({ ...node, geo_confidence: "measured_metro" });
  assert.ok(measured.some((line) => line.includes("approximate metro")));
  const stale = mapNodeTooltipLines({ ...node, geo_confidence: "stale" });
  assert.ok(stale.some((line) => line.includes("refresh pending")));
  assert.ok(mapNodeTooltipLines(node).includes("Place: Sydney, Australia"));
});
