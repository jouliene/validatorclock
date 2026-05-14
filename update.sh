#!/usr/bin/env bash
set -euo pipefail

UPDATE_BRANCH="${VALIDATORS_CLOCK_UPDATE_BRANCH:-main}"

usage() {
  cat <<'USAGE'
Usage: ./update.sh

Updates this production checkout from GitHub, updates the Rust toolchain,
rebuilds validators_clock, installs the new binary, and restarts systemd.

Environment overrides:
  VALIDATORS_CLOCK_UPDATE_BRANCH default: main
  all install.sh environment overrides are also supported
USAGE
}

while (($#)); do
  case "$1" in
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "${EUID}" -eq 0 ]]; then
  echo "Run ./update.sh as the deployment user, not with sudo." >&2
  echo "The installer asks sudo only for systemd operations." >&2
  exit 1
fi

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$REPO_DIR"

load_cargo_env() {
  if [[ -f "${HOME}/.cargo/env" ]]; then
    # shellcheck disable=SC1091
    source "${HOME}/.cargo/env"
  fi
}

ensure_rust() {
  load_cargo_env

  if command -v cargo >/dev/null 2>&1; then
    if command -v rustup >/dev/null 2>&1; then
      echo "Updating Rust toolchain"
      rustup update
    else
      echo "Rust/Cargo found, but rustup was not found; skipping Rust toolchain update"
    fi
    return
  fi

  if ! command -v curl >/dev/null 2>&1; then
    echo "curl is required to install Rust with rustup" >&2
    exit 1
  fi

  echo "Rust/Cargo not found; installing Rust with rustup"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal

  # shellcheck disable=SC1091
  source "${HOME}/.cargo/env"

  if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo is still not available after rustup install" >&2
    exit 1
  fi

  echo "Updating Rust toolchain"
  rustup update
}

current_branch="$(git branch --show-current)"
if [[ "$current_branch" != "$UPDATE_BRANCH" ]]; then
  echo "This checkout is on branch '$current_branch', but update branch is '$UPDATE_BRANCH'." >&2
  echo "Run: git checkout $UPDATE_BRANCH" >&2
  exit 1
fi

if [[ -n "$(git status --porcelain)" ]]; then
  echo "Working tree has local changes. Commit, stash, or remove them before updating." >&2
  git status --short >&2
  exit 1
fi

ensure_rust

echo "Updating Git checkout with fast-forward only"
git pull --ff-only origin "$UPDATE_BRANCH"

echo "Installing updated release"
VALIDATORS_CLOCK_RUST_ALREADY_UPDATED=1 ./install.sh
