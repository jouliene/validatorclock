import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/format_addresses.js", "app/validator_identity.js");

const PUBLIC_KEY = "ab".repeat(32);
const WALLET = `-1:${"cd".repeat(32)}`;

test("a validator with a wallet is named by its wallet", () => {
  const identity = validatorIdentityDisplay({ wallet: WALLET, public_key: PUBLIC_KEY }, {});
  assert.equal(identity.value, WALLET);
  assert.equal(identity.label, "validator wallet address");
});

test("a validator with no wallet is named by its key, not by an address made out of it", () => {
  const validator = { wallet: null, public_key: PUBLIC_KEY };

  const everscale = validatorIdentityDisplay(validator, {}, true);
  assert.equal(everscale.value, PUBLIC_KEY, "the copy button hands over the key itself");
  assert.ok(!everscale.value.includes(":"), "and not a masterchain address made out of it");
  assert.equal(everscale.label, "validator public key");

  const ton = validatorIdentityDisplay(validator, { chainId: "ton", addressType: "ton" }, true);
  assert.equal(ton.value, PUBLIC_KEY, "least of all a base64 TON address that exists nowhere");
  assert.ok(!ton.value.startsWith("EQ"));
});

test("without the fallback a validator with no wallet has nothing to show", () => {
  const identity = validatorIdentityDisplay({ wallet: null, public_key: PUBLIC_KEY }, {});
  assert.equal(identity.text, "-");
  assert.equal(identity.value, "-");
});
