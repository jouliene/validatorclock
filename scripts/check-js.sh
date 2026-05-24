#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v node >/dev/null 2>&1; then
  echo "Node.js is required for frontend syntax checks." >&2
  exit 1
fi

while IFS= read -r file; do
  node --check "${file}"
done < <(
  find "${ROOT_DIR}/public/app" -type f -name '*.js' -print
  printf '%s\n' "${ROOT_DIR}/public/app.js"
)
