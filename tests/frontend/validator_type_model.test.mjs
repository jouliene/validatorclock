import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/validator_metadata.js", "app/validator_type_model.js");

const DEPOOL_HASH = "14e20e304f53e6da152eb95fffc993dbd28245a775d847eed043f7c78a503885";
const proxy = (contract_type_hash) => ({
  contract_type: "DePoolProxy",
  source: contract_type_hash === undefined ? null : { address: "-1:aa", contract_type_hash },
});

test("a proxy whose pool is known is named by that pool", () => {
  assert.equal(displayedValidatorType(proxy(DEPOOL_HASH)).label, "DEPOOL");
});

test("a proxy whose pool is not known is still a proxy, never UNKNOWN", () => {
  const unknownPool = displayedValidatorType(proxy("ff".repeat(32)));
  const noSourceHash = displayedValidatorType(proxy(null));
  const noSourceAtAll = displayedValidatorType(proxy(undefined));

  assert.equal(unknownPool.label, "PROXY", "an uncatalogued pool does not make the contract unknown");
  assert.equal(noSourceHash.label, "PROXY");
  assert.equal(noSourceAtAll.label, "PROXY");
});

test("every label a validator can be badged with is in the glossary", () => {
  const described = new Set(VALIDATOR_TYPE_GLOSSARY.map((entry) => entry.label));
  for (const type of Object.values(VALIDATOR_CONTRACT_TYPES)) {
    assert.ok(described.has(type.label), `${type.label} has no glossary entry`);
  }
  for (const type of Object.values(VALIDATOR_SOURCE_TYPES)) {
    assert.ok(described.has(type.label), `${type.label} has no glossary entry`);
  }
});
