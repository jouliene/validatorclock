#!/usr/bin/env bash
# Loads the page in a real browser against a server serving a fixed snapshot, and fails
# if anything throws, if the page does not draw, or if a tooltip no longer opens.
#
# The snapshot is a fixture rather than a live chain: this has to give the same answer on
# a machine with no network, and a page drawn from four validators exercises the same
# code as one drawn from four hundred.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIXTURES="${ROOT_DIR}/tests/fixtures/browser-check"
PORT=18788
BASE_URL="http://127.0.0.1:${PORT}/"
WORK_DIR="$(mktemp -d)"
BINARY="${VALIDATORCLOCK_BINARY:-${ROOT_DIR}/target/debug/validatorclock}"

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    kill "${SERVER_PID}" 2>/dev/null || true
    wait "${SERVER_PID}" 2>/dev/null || true
  fi
  rm -rf "${WORK_DIR}"
}
trap cleanup EXIT

if [[ ! -x "${BINARY}" ]]; then
  echo "Building the server first: ${BINARY} is not there." >&2
  cargo build --manifest-path "${ROOT_DIR}/Cargo.toml"
fi

# The cache is copied because the server splits and rewrites it in place.
cp "${FIXTURES}"/cache_*.json "${WORK_DIR}/"
sed "s#tests/fixtures/browser-check/cache.json#${WORK_DIR}/cache.json#" \
  "${FIXTURES}/config.json" > "${WORK_DIR}/config.json"

RUST_LOG=warn "${BINARY}" --config "${WORK_DIR}/config.json" > "${WORK_DIR}/server.log" 2>&1 &
SERVER_PID=$!

for _ in $(seq 1 60); do
  if curl -fsS "${BASE_URL}api/health" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

if ! curl -fsS "${BASE_URL}api/chains/everscale/clock" >/dev/null 2>&1; then
  echo "The fixture server never served a clock. Its log:" >&2
  cat "${WORK_DIR}/server.log" >&2
  exit 1
fi

node "${ROOT_DIR}/scripts/browser-check.mjs" "${BASE_URL}"
