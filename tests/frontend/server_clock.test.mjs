import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/state.js");

test("ages are measured against the server's clock, not the browser's", () => {
  const browserNow = Math.trunc(Date.now() / 1000);

  // A machine five minutes fast: every age on the page was five minutes out, and the
  // needle was drawn five minutes further round the dial.
  noteServerClock({ started_at: browserNow - 3600 - 300, uptime_seconds: 3600 });
  assert.equal(nowSeconds(), browserNow - 300);

  noteServerClock({ started_at: browserNow - 3600, uptime_seconds: 3600 });
  assert.equal(nowSeconds(), browserNow, "a clock that agrees changes nothing");
});

test("a status without a clock in it leaves the offset alone", () => {
  const browserNow = Math.trunc(Date.now() / 1000);
  noteServerClock({ started_at: browserNow - 60, uptime_seconds: 60 });
  const before = nowSeconds();

  noteServerClock({ status: "degraded", chains: [], error: "no answer" });

  assert.equal(nowSeconds(), before, "a failed status is not a statement about the time");
});
