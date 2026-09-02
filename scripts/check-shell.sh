#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# The installer and the updater live at the repo root, not under scripts/, so
# scanning only that directory left the two files an operator actually runs
# unchecked - including every edit made to them.
while IFS= read -r file; do
  bash -n "${file}"
done < <(
  find "${ROOT_DIR}" -maxdepth 1 -type f -name '*.sh' -print
  find "${ROOT_DIR}/scripts" -type f -name '*.sh' -print
)
