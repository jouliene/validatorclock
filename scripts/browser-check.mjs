// Loads the page in a real browser and checks the things a unit test cannot: that
// nothing throws while it starts up, that the tables and the dial are actually drawn,
// and that hovering a validator opens its tooltip.
//
// It speaks the DevTools protocol directly - node has a WebSocket client, and the
// alternative is a browser automation dependency for four assertions.
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const BASE_URL = process.argv[2] || "http://127.0.0.1:18787/";
const BROWSERS = ["chromium", "chromium-browser", "google-chrome", "google-chrome-stable", "brave-browser"];
const DEADLINE_MS = 60000;

const userDataDir = mkdtempSync(join(tmpdir(), "validatorclock-browser-check-"));
const failures = [];
let browser;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function main() {
  browser = launchBrowser();
  const wsUrl = await browserWebSocket();
  const session = await attachToPage(wsUrl);

  const problems = [];
  session.on("Runtime.exceptionThrown", (params) => {
    problems.push(`uncaught: ${params.exceptionDetails?.exception?.description || params.exceptionDetails?.text}`);
  });
  session.on("Log.entryAdded", (params) => {
    if (params.entry?.level === "error") {
      problems.push(`console error: ${params.entry.text}`);
    }
  });
  await session.send("Runtime.enable");
  await session.send("Log.enable");
  await session.send("Page.enable");

  await session.send("Page.navigate", { url: BASE_URL });
  await waitFor(session, () => evaluate(session, `Boolean(document.querySelector(".validator-row"))`), "the validator tables to be drawn");

  await check(session, "the dial is drawn", `document.querySelectorAll("#clockSvg g").length >= 4`);
  await check(session, "both round panels have rows", `document.querySelectorAll(".round-panel .validator-row").length > 1`);
  await check(session, "the election phase is one of the three", `["Before elections","Elections open","After elections"].includes(document.getElementById("metricStatus").textContent.trim())`);
  await check(session, "no round shows a dash where a total belongs", `![...document.querySelectorAll(".round-stat-value")].some((node) => node.textContent.trim() === "")`);

  // The tooltips are delegated: nothing is bound to the row itself, so this is the only
  // way to know they still open.
  await check(
    session,
    "hovering a validator opens its tooltip",
    `(() => {
       const target = document.querySelector(".validator-row .has-validator-tooltip");
       if (!target) return "no tooltip-bearing element in the tables";
       target.dispatchEvent(new PointerEvent("pointerover", { bubbles: true, pointerType: "mouse" }));
       const tooltip = document.querySelector(".validator-hover-tooltip");
       if (!tooltip) return "pointerover opened nothing";
       if (!tooltip.textContent.trim()) return "the tooltip opened empty";
       target.dispatchEvent(new PointerEvent("pointerout", { bubbles: true, pointerType: "mouse", relatedTarget: document.body }));
       return document.querySelector(".validator-hover-tooltip") ? "the tooltip stayed after the pointer left" : true;
     })()`,
  );

  await check(
    session,
    "switching chain redraws without leaving the previous chain's rows",
    `(() => {
       const tabs = [...document.querySelectorAll("#chainTabs button")];
       if (tabs.length < 2) return "fewer than two chains to switch between";
       const before = document.querySelector(".validator-row")?.textContent || "";
       tabs.find((tab) => tab.getAttribute("aria-selected") !== "true")?.click();
       return { before };
     })()`,
  );
  await sleep(2500);
  await check(session, "the page survives a chain switch", `Boolean(document.getElementById("metricStatus").textContent.trim())`);

  if (problems.length) {
    failures.push(...problems);
  }
}

async function check(session, what, expression) {
  const value = await evaluate(session, expression);
  if (value === true || (value && typeof value === "object")) {
    console.log(`  ok  ${what}`);
    return;
  }
  failures.push(`${what}: ${value === false ? "false" : value}`);
  console.log(`  FAIL ${what}: ${value === false ? "false" : value}`);
}

async function waitFor(session, probe, what) {
  const until = Date.now() + DEADLINE_MS;
  while (Date.now() < until) {
    if (await probe()) {
      return;
    }
    await sleep(250);
  }
  failures.push(`timed out waiting for ${what}`);
}

async function evaluate(session, expression) {
  const result = await session.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) {
    return `threw: ${result.exceptionDetails.exception?.description || result.exceptionDetails.text}`;
  }
  return result.result?.value;
}

function launchBrowser() {
  for (const candidate of BROWSERS) {
    const child = spawn(candidate, [
      "--headless=new",
      "--disable-gpu",
      "--no-sandbox",
      "--hide-scrollbars",
      "--remote-debugging-port=9333",
      `--user-data-dir=${userDataDir}`,
      "about:blank",
    ], { stdio: "ignore" });
    child.on("error", () => {});
    if (child.pid) {
      return child;
    }
  }
  throw new Error(`no browser found; tried ${BROWSERS.join(", ")}`);
}

async function browserWebSocket() {
  const until = Date.now() + 20000;
  while (Date.now() < until) {
    try {
      const response = await fetch("http://127.0.0.1:9333/json/version");
      const body = await response.json();
      if (body.webSocketDebuggerUrl) {
        return body.webSocketDebuggerUrl;
      }
    } catch {
      // not up yet
    }
    await sleep(200);
  }
  throw new Error("the browser never opened its debugging port");
}

async function attachToPage(wsUrl) {
  const socket = new WebSocket(wsUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });

  let nextId = 0;
  const pending = new Map();
  const listeners = new Map();
  let sessionId = null;

  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id !== undefined) {
      pending.get(message.id)?.(message);
      pending.delete(message.id);
      return;
    }
    listeners.get(message.method)?.forEach((handler) => handler(message.params));
  });

  const call = (method, params = {}, withSession = true) =>
    new Promise((resolve, reject) => {
      const id = (nextId += 1);
      pending.set(id, (message) => (message.error ? reject(new Error(`${method}: ${message.error.message}`)) : resolve(message.result)));
      socket.send(JSON.stringify({ id, method, params, ...(withSession && sessionId ? { sessionId } : {}) }));
    });

  const { targetId } = await call("Target.createTarget", { url: "about:blank" }, false);
  ({ sessionId } = await call("Target.attachToTarget", { targetId, flatten: true }, false));

  return {
    send: call,
    on: (method, handler) => {
      const handlers = listeners.get(method) || [];
      handlers.push(handler);
      listeners.set(method, handlers);
    },
  };
}

try {
  await main();
} catch (error) {
  failures.push(String(error?.message || error));
} finally {
  browser?.kill();
  // The browser writes as it shuts down, so the profile is swept after it has gone -
  // and a profile left behind in the temp directory is not worth failing a check over.
  await sleep(300);
  try {
    rmSync(userDataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  } catch {
    // left for the temp directory to reap
  }
}

if (failures.length) {
  console.error(`\n${failures.length} problem(s) in the browser:`);
  for (const failure of failures) {
    console.error(`  - ${failure}`);
  }
  process.exit(1);
}
console.log("\nthe page loads, draws and responds without errors");
