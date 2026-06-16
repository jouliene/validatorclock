#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${VALIDATORCLOCK_VISUAL_PORT:-18787}"
HOST="127.0.0.1"
BASE_URL="http://${HOST}:${PORT}"
OUT_DIR="${VALIDATORCLOCK_VISUAL_OUT:-${ROOT_DIR}/target/visual-check}"
BROWSER="${VALIDATORCLOCK_BROWSER:-}"

find_browser() {
  if [[ -n "${BROWSER}" ]]; then
    command -v "${BROWSER}" >/dev/null 2>&1 || {
      echo "Configured browser not found: ${BROWSER}" >&2
      exit 1
    }
    echo "${BROWSER}"
    return
  fi

  for candidate in brave-browser chromium chromium-browser google-chrome google-chrome-stable; do
    if command -v "${candidate}" >/dev/null 2>&1; then
      echo "${candidate}"
      return
    fi
  done

  echo "No supported browser found. Install Brave, Chromium, or Chrome." >&2
  exit 1
}

wait_for_server() {
  local attempts=80
  for _ in $(seq 1 "${attempts}"); do
    if curl -fsS "${BASE_URL}/api/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done

  echo "Server did not become ready at ${BASE_URL}" >&2
  return 1
}

wait_for_data() {
  local attempts=120
  local body=""
  for _ in $(seq 1 "${attempts}"); do
    body="$(curl -fsS "${BASE_URL}/api/chains/everscale/clock" 2>/dev/null || true)"
    if [[ "${body}" == *'"current_set"'* && "${body}" == *'"validators"'* ]]; then
      return 0
    fi
    sleep 0.5
  done

  echo "Clock data did not become ready at ${BASE_URL}" >&2
  return 1
}

capture() {
  local name="$1"
  local size="$2"
  local height="${size#*,}"
  local output="${OUT_DIR}/${name}.png"

  "${BROWSER_BIN}" \
    --headless=new \
    --disable-gpu \
    --hide-scrollbars \
    --force-device-scale-factor=1 \
    "--window-size=${size}" \
    --virtual-time-budget=6000 \
    "--screenshot=${output}" \
    "${BASE_URL}/" >/dev/null

  if [[ ! -s "${output}" ]]; then
    echo "Screenshot was not written: ${output}" >&2
    return 1
  fi

  echo "Captured ${name}: ${output} (${height}px high)"
}

run_geometry_check() {
  if ! command -v node >/dev/null 2>&1; then
    echo "Node.js not found; skipped geometry checks."
    return 0
  fi

  node - "${BASE_URL}" <<'NODE'
const http = require("http");
const baseUrl = process.argv[2];

function getJson(url) {
  return new Promise((resolve, reject) => {
    http.get(url, (response) => {
      let body = "";
      response.on("data", (chunk) => body += chunk);
      response.on("end", () => {
        try {
          resolve(JSON.parse(body));
        } catch (error) {
          reject(error);
        }
      });
    }).on("error", reject);
  });
}

async function evaluateForWidth(wsUrl, width) {
  const ws = new WebSocket(wsUrl);
  const expression = `new Promise((resolve) => {
    const rect = (el) => {
      const r = el.getBoundingClientRect();
      return {
        left: +r.left.toFixed(1),
        right: +r.right.toFixed(1),
        top: +r.top.toFixed(1),
        bottom: +r.bottom.toFixed(1),
        width: +r.width.toFixed(1)
      };
    };
    let attempts = 0;
    const collect = () => {
      const detailSource = document.querySelector(".validator-row .validator-source.is-detail");
      const row = detailSource?.closest(".validator-row") || document.querySelector(".validator-row");
      const source = row?.querySelector(".validator-source");
      const validator = row?.querySelector(".validator-id");
      const history = row?.querySelector(".validator-history");
      const badge = row?.querySelector(".validator-type-badge");
      const metrics = Array.from(row?.querySelectorAll(".validator-stake, .validator-rewards, .validator-weight") || []).map(rect);
      const sourceRect = source && rect(source);
      const sourceVisible = Boolean(sourceRect && sourceRect.width > 0);
      if (badge && validator && history && metrics.length === 3 && (source?.classList.contains("is-detail") ? sourceVisible : true)) {
        const focusTarget = validator.querySelector(".validator-copy:not([disabled])") || validator;
        focusTarget.focus();
        badge.dispatchEvent(new PointerEvent("pointerdown", {
          bubbles: true,
          cancelable: true,
          pointerType: "touch"
        }));
        const touchTooltip = document.querySelector(".validator-hover-tooltip");
        resolve({
          ready: true,
          innerWidth,
          scrollWidth: document.documentElement.scrollWidth,
          roundsGrid: getComputedStyle(document.querySelector(".rounds-grid")).gridTemplateColumns,
          focusedRowShadow: getComputedStyle(row).boxShadow,
          touchTooltipText: touchTooltip?.textContent || "",
          source: sourceVisible ? sourceRect : null,
          validator: rect(validator),
          history: rect(history),
          metrics
        });
        return;
      }
      if (attempts++ >= 40) {
        resolve({
          ready: false,
          innerWidth,
          scrollWidth: document.documentElement.scrollWidth,
          rowCount: document.querySelectorAll(".validator-row").length,
          sourceFound: Boolean(source),
          sourceVisible,
          badgeFound: Boolean(badge),
          validatorFound: Boolean(validator),
          historyFound: Boolean(history),
          metricCount: metrics.length
        });
        return;
      }
      setTimeout(collect, 250);
    };
    collect();
  })`;

  let step = "metrics";
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      ws.close();
      reject(new Error(`Timed out checking ${width}px`));
    }, 10000);

    ws.onopen = () => {
      ws.send(JSON.stringify({
        id: 1,
        method: "Emulation.setDeviceMetricsOverride",
        params: { width, height: 1000, deviceScaleFactor: 1, mobile: true }
      }));
    };

    ws.onmessage = (event) => {
      const message = JSON.parse(event.data);
      if (message.id === 1 && step === "metrics") {
        step = "evaluate";
        setTimeout(() => {
          ws.send(JSON.stringify({
            id: 2,
            method: "Runtime.evaluate",
            params: { expression, returnByValue: true, awaitPromise: true }
          }));
        }, 700);
      }
      if (message.id === 2) {
        clearTimeout(timeout);
        ws.close();
        resolve(message.result.result.value);
      }
    };

    ws.onerror = (error) => {
      clearTimeout(timeout);
      reject(error);
    };
  });
}

function closeEnough(a, b, tolerance = 1.5) {
  return Math.abs(a - b) <= tolerance;
}

function assertAligned(name, a, b) {
  if (!a || !b || !closeEnough(a.left, b.left) || !closeEnough(a.right, b.right)) {
    throw new Error(`${name} is not aligned: ${JSON.stringify({ a, b })}`);
  }
}

function assertSameRow(name, a, b) {
  if (!a || !b || !closeEnough(a.top, b.top) || !closeEnough(a.bottom, b.bottom)) {
    throw new Error(`${name} is not on one row: ${JSON.stringify({ a, b })}`);
  }
}

function assertSplitRow(name, left, right) {
  assertSameRow(name, left, right);
  if (left.right > right.left || !closeEnough(left.width, right.width)) {
    throw new Error(`${name} is not split evenly: ${JSON.stringify({ left, right })}`);
  }
}

(async () => {
  const port = 9223;
  const pages = await getJson(`http://127.0.0.1:${port}/json`);
  const page = pages.find((entry) => entry.type === "page");
  if (!page) {
    throw new Error("No browser page found for geometry check");
  }

  for (const width of [390, 360, 320]) {
    const result = await evaluateForWidth(page.webSocketDebuggerUrl, width);
    if (!result.ready) {
      throw new Error(`Validator rows did not render at ${width}px: ${JSON.stringify(result)}`);
    }
    if (result.scrollWidth !== result.innerWidth) {
      throw new Error(`Horizontal overflow at ${width}px: ${result.scrollWidth} > ${result.innerWidth}`);
    }
    if (result.focusedRowShadow !== "none") {
      throw new Error(`Validator row focus shadow is visible at ${width}px: ${result.focusedRowShadow}`);
    }
    if (!String(result.touchTooltipText || "").trim()) {
      throw new Error(`Validator type touch tooltip did not open at ${width}px`);
    }
    if (result.source) {
      assertSplitRow(`source/validator at ${width}px`, result.source, result.validator);
      assertAligned(`source+validator/history at ${width}px`, {
        left: result.source.left,
        right: result.validator.right
      }, result.history);
    } else {
      console.log(`Geometry ${width}px note: no visible source detail row in current data`);
      assertAligned(`validator/history at ${width}px`, result.validator, result.history);
    }
    const metricWidths = result.metrics.map((metric) => metric.width);
    if (new Set(metricWidths).size !== 1) {
      throw new Error(`Metric cards are uneven at ${width}px: ${metricWidths.join(", ")}`);
    }
    console.log(`Geometry ${width}px ok: grid=${result.roundsGrid}`);
  }
})().catch((error) => {
  console.error(error.message || error);
  process.exit(1);
});
NODE
}

cd "${ROOT_DIR}"
BROWSER_BIN="$(find_browser)"
mkdir -p "${OUT_DIR}"

TMP_DIR="$(mktemp -d)"
SERVER_PID=""
GEOMETRY_BROWSER_PID=""
cleanup() {
  if [[ -n "${SERVER_PID}" ]]; then
    kill "${SERVER_PID}" >/dev/null 2>&1 || true
  fi
  if [[ -n "${GEOMETRY_BROWSER_PID}" ]]; then
    kill "${GEOMETRY_BROWSER_PID}" >/dev/null 2>&1 || true
  fi
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

cat > "${TMP_DIR}/visual-check.json" <<JSON
{
  "listen": "${HOST}:${PORT}",
  "refresh_seconds": 60,
  "refresh_timeout_seconds": 90,
  "cache_path": "${TMP_DIR}/validatorclock_cache.json",
  "history_path": "${TMP_DIR}/validatorclock_history.json",
  "validator_type_cache_path": "${ROOT_DIR}/validatorclock_validator_types.json",
  "chains": [
    {
      "id": "everscale",
      "name": "Everscale",
      "rpc": "https://jrpc.everwallet.net",
      "color": "#6347F5",
      "token_symbol": "EVER",
      "rpc_label": "jrpc.everwallet.net"
    },
    {
      "id": "tycho-testnet",
      "name": "Tycho Testnet",
      "rpc": "https://rpc-testnet.tychoprotocol.com",
      "color": "#2ECC71",
      "token_symbol": "TYCHO",
      "rpc_label": "rpc-testnet.tychoprotocol.com"
    },
    {
      "id": "ton",
      "name": "TON",
      "rpc": "https://toncenter.com/api/v2/jsonRPC",
      "rpc_fallbacks": [
        "https://jrpc-ton.broxus.com"
      ],
      "color": "#4DB8FF",
      "token_symbol": "TON",
      "rpc_label": "toncenter.com + jrpc-ton.broxus.com"
    }
  ]
}
JSON

echo "Building validatorclock..."
cargo build

echo "Starting preview server at ${BASE_URL}..."
"${ROOT_DIR}/target/debug/validatorclock" "${TMP_DIR}/visual-check.json" >"${OUT_DIR}/server.log" 2>&1 &
SERVER_PID="$!"
wait_for_server
wait_for_data

echo "Capturing screenshots with ${BROWSER_BIN}..."
capture "mobile-390" "390,2400"
capture "desktop-1440" "1440,1400"

echo "Running mobile geometry checks..."
"${BROWSER_BIN}" \
  --headless=new \
  --disable-gpu \
  --remote-debugging-address=127.0.0.1 \
  --remote-debugging-port=9223 \
  --user-data-dir="${TMP_DIR}/browser-profile" \
  --force-device-scale-factor=1 \
  --window-size=500,1000 \
  "${BASE_URL}/" >"${OUT_DIR}/browser.log" 2>&1 &
GEOMETRY_BROWSER_PID="$!"
sleep 2
run_geometry_check

echo "Visual check complete. Artifacts: ${OUT_DIR}"
