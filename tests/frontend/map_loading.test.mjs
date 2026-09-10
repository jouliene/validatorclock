import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/state.js", "app/map.js", "app/map_style.js", "app/map_render.js");
window.setTimeout = () => 1;
document.getElementById = () => ({});

test("both independent libraries start together and a second caller shares the load", async () => {
  const pending = new Map();
  globalThis.loadMapScript = (id) => new Promise(resolve => pending.set(id, resolve));
  let registrations = 0;
  globalThis.maplibregl = { addProtocol: () => registrations++ };
  globalThis.pmtiles = { Protocol: class { tile() {} } };
  const first = ensureMapLibre();
  const second = ensureMapLibre();
  assert.equal(first, second);
  assert.deepEqual([...pending.keys()], ["maplibreJs", "pmtilesJs"]);
  pending.get("maplibreJs")();
  pending.get("pmtilesJs")();
  await first;
  assert.equal(registrations, 1);
});

test("style warmup is shared with opening the map; failed fetch can be retried", async () => {
  let calls = 0;
  const style = { version: 8, sources: {}, layers: [] };
  globalThis.fetchJson = async () => { calls++; if (calls === 1) throw new Error("offline"); return style; };
  await assert.rejects(loadValidatorMapStyle(), /offline/);
  const first = loadValidatorMapStyle();
  assert.equal(first, loadValidatorMapStyle());
  assert.equal(await first, style);
  assert.equal(await loadValidatorMapStyle(), style);
  assert.equal(calls, 2);
});

test("map resources start while node lookup is still pending", async () => {
  let releaseNodes;
  const calls = [];
  const style = { version: 8 };
  globalThis.loadValidatorMapNodes = () => { calls.push("nodes"); return new Promise(r => { releaseNodes = r; }); };
  globalThis.ensureMapLibre = async () => { calls.push("libraries"); };
  globalThis.loadValidatorMapStyle = async () => { calls.push("style"); return style; };
  globalThis.showValidatorMapStatus = () => {};
  globalThis.renderValidatorMap = (s) => { assert.equal(s, style); calls.push("render"); return true; };
  const built = buildValidatorMap();
  assert.deepEqual(calls, ["nodes", "libraries", "style"]);
  releaseNodes([]);
  await built;
  assert.equal(calls.at(-1), "render");
});
