import test from "node:test";
import assert from "node:assert/strict";
import { load, stubBrowser } from "./harness.mjs";

stubBrowser();
load("app/api.js");

const answering = ({ ok = true, status = 200, statusText = "OK", body }) => {
  globalThis.fetch = async () => ({
    ok,
    status,
    statusText,
    json: async () => {
      if (body === "not json") {
        throw new SyntaxError("Unexpected token < in JSON at position 0");
      }
      return body;
    },
  });
};

test("a JSON answer is handed back as it is", async () => {
  answering({ body: { chains: [{ id: "ton" }] } });
  assert.deepEqual(await fetchJson("/api/chains"), { chains: [{ id: "ton" }] });
});

test("a success that is not JSON is a failure, not an empty object", async () => {
  answering({ body: "not json" });
  await assert.rejects(
    fetchJson("/api/chains"),
    /not JSON/,
    "returning {} sent the caller off to read fields from nothing",
  );
});

test("an error is reported with what the server said, or with its status", async () => {
  answering({ ok: false, status: 503, statusText: "Service Unavailable", body: { error: "chain is warming up", code: "cold" } });
  await assert.rejects(fetchJson("/api/x"), /chain is warming up/);

  answering({ ok: false, status: 502, statusText: "Bad Gateway", body: "not json" });
  await assert.rejects(fetchJson("/api/x"), /502 Bad Gateway/, "an error body is a courtesy, not a requirement");
});

test("a request that ran out of time says so in words", async () => {
  globalThis.fetch = async () => {
    const error = new Error("signal timed out");
    error.name = "TimeoutError";
    throw error;
  };
  await assert.rejects(fetchJson("/api/chains"), /took too long/);
});
