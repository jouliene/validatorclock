#!/usr/bin/env bash
set -euo pipefail

UPDATE_BRANCH="${VALIDATORCLOCK_UPDATE_BRANCH:-main}"
SERVICE_NAME="${VALIDATORCLOCK_SERVICE_NAME:-validatorclock.service}"

usage() {
  cat <<'USAGE'
Usage: ./update.sh

Updates this production checkout from GitHub, updates the Rust toolchain,
rebuilds validatorclock, installs the new binary, and restarts the existing
service without sudo.

Environment overrides:
  VALIDATORCLOCK_UPDATE_BRANCH default: main
  VALIDATORCLOCK_SERVICE_NAME  default: validatorclock.service
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

restart_existing_service_without_sudo() {
  if ! command -v systemctl >/dev/null 2>&1; then
    echo "systemctl was not found; binary is updated but service was not restarted." >&2
    return 1
  fi

  local old_pid
  old_pid="$(systemctl show "$SERVICE_NAME" --property MainPID --value 2>/dev/null || true)"

  if [[ ! "$old_pid" =~ ^[0-9]+$ || "$old_pid" -le 1 ]]; then
    echo "No running MainPID found for $SERVICE_NAME." >&2
    echo "Binary is updated, but starting the system service requires:" >&2
    echo "  sudo systemctl start $SERVICE_NAME" >&2
    return 1
  fi

  echo "Restarting $SERVICE_NAME without sudo"
  echo "Stopping process $old_pid; systemd Restart=always will start the new binary"
  if ! kill "$old_pid"; then
    echo "Could not stop process $old_pid without sudo." >&2
    echo "Binary is updated, but applying it now requires:" >&2
    echo "  sudo systemctl restart $SERVICE_NAME" >&2
    return 1
  fi

  local attempt
  local new_pid
  local state
  for attempt in {1..20}; do
    sleep 0.5
    state="$(systemctl is-active "$SERVICE_NAME" 2>/dev/null || true)"
    new_pid="$(systemctl show "$SERVICE_NAME" --property MainPID --value 2>/dev/null || true)"
    if [[ "$state" == "active" && "$new_pid" =~ ^[0-9]+$ && "$new_pid" -gt 1 && "$new_pid" != "$old_pid" ]]; then
      echo "$SERVICE_NAME restarted with PID $new_pid"
      systemctl --no-pager --lines=12 status "$SERVICE_NAME" || true
      return 0
    fi
  done

  echo "Timed out waiting for $SERVICE_NAME to restart." >&2
  systemctl --no-pager --lines=20 status "$SERVICE_NAME" || true
  return 1
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
VALIDATORCLOCK_RUST_ALREADY_UPDATED=1 VALIDATORCLOCK_NO_SYSTEMD=1 ./install.sh

restart_existing_service_without_sudo
