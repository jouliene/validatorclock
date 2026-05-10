#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME="${VALIDATORS_CLOCK_SERVICE_NAME:-validators-clock.service}"
APP_NAME="validators_clock"

usage() {
  cat <<'USAGE'
Usage: ./install.sh [--no-restart]

Builds validators_clock, installs the binary to $HOME/.cargo/bin, creates a
production runtime directory at $HOME/.validators_clock, installs/updates the
systemd service, and restarts it.

If cargo is missing, the script installs Rust with rustup first.

This script does not run git pull. Production update flow:

  git pull --ff-only
  ./install.sh

Environment overrides:
  VALIDATORS_CLOCK_STATE_DIR              default: $HOME/.validators_clock
  VALIDATORS_CLOCK_PUBLIC_URL             default: https://validatorsclock.xyz
  VALIDATORS_CLOCK_ACME_IDENTIFIER        default: host from public URL
  VALIDATORS_CLOCK_ACME_EXTRA_IDENTIFIERS default: www.<identifier>
  VALIDATORS_CLOCK_ACME_STAGING           default: false
  VALIDATORS_CLOCK_RUSTFLAGS              default: -C target-cpu=native
  VALIDATORS_CLOCK_NO_RESTART             set to 1 to skip restart
USAGE
}

NO_RESTART="${VALIDATORS_CLOCK_NO_RESTART:-0}"

while (($#)); do
  case "$1" in
    --help|-h)
      usage
      exit 0
      ;;
    --no-restart)
      NO_RESTART=1
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [[ "${EUID}" -eq 0 ]]; then
  echo "Run ./install.sh as the deployment user, not with sudo." >&2
  echo "The script asks sudo only for systemd operations." >&2
  exit 1
fi

if ! command -v systemctl >/dev/null 2>&1; then
  echo "systemctl is required but was not found in PATH" >&2
  exit 1
fi

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUN_USER="$(id -un)"
RUN_GROUP="$(id -gn)"
BIN_DIR="${HOME}/.cargo/bin"
BIN_PATH="${BIN_DIR}/${APP_NAME}"
STATE_DIR="${VALIDATORS_CLOCK_STATE_DIR:-${HOME}/.validators_clock}"
CONFIG_PATH="${VALIDATORS_CLOCK_CONFIG:-${STATE_DIR}/validators_clock.production.json}"
ACME_DIR="${STATE_DIR}/acme"
PUBLIC_URL="${VALIDATORS_CLOCK_PUBLIC_URL:-https://validatorsclock.xyz}"
PUBLIC_HOST="${PUBLIC_URL#https://}"
PUBLIC_HOST="${PUBLIC_HOST%%/*}"
PUBLIC_HOST="${PUBLIC_HOST%%:*}"
ACME_IDENTIFIER="${VALIDATORS_CLOCK_ACME_IDENTIFIER:-${PUBLIC_HOST}}"
ACME_EXTRA_IDENTIFIERS="${VALIDATORS_CLOCK_ACME_EXTRA_IDENTIFIERS:-www.${ACME_IDENTIFIER}}"
ACME_STAGING="${VALIDATORS_CLOCK_ACME_STAGING:-false}"
PRODUCTION_RUSTFLAGS_DEFAULT="-C target-cpu=native"
PRODUCTION_RUSTFLAGS="${VALIDATORS_CLOCK_RUSTFLAGS-$PRODUCTION_RUSTFLAGS_DEFAULT}"
SERVICE_PATH="/etc/systemd/system/${SERVICE_NAME}"

case "$ACME_STAGING" in
  true|false) ;;
  *)
    echo "VALIDATORS_CLOCK_ACME_STAGING must be true or false" >&2
    exit 1
    ;;
esac

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  printf '%s' "$value"
}

trim() {
  local value="$1"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  printf '%s' "$value"
}

json_array_from_csv() {
  local csv="$1"
  local output="["
  local first=1
  local item
  local -a items=()
  IFS=',' read -r -a items <<<"$csv"
  for item in "${items[@]}"; do
    item="$(trim "$item")"
    [[ -z "$item" ]] && continue
    if [[ "$first" -eq 0 ]]; then
      output+=", "
    fi
    output+="\"$(json_escape "$item")\""
    first=0
  done
  output+="]"
  printf '%s' "$output"
}

sync_existing_config_defaults() {
  if ! command -v python3 >/dev/null 2>&1; then
    echo "python3 is required to merge built-in chains into an existing config." >&2
    echo "Install python3 or add the missing chain entries manually in: $CONFIG_PATH" >&2
    exit 1
  fi

  python3 - "$CONFIG_PATH" <<'PY'
import json
import shutil
import sys
import time
from pathlib import Path

config_path = Path(sys.argv[1])
default_chains = [
    {
        "id": "everscale",
        "name": "Everscale",
        "rpc": "https://jrpc.everwallet.net",
        "color": "#6347F5",
        "token_symbol": "EVER",
        "rpc_label": "jrpc.everwallet.net",
    },
    {
        "id": "tycho-testnet",
        "name": "Tycho Testnet",
        "rpc": "https://rpc-testnet.tychoprotocol.com",
        "color": "#2ECC71",
        "token_symbol": "TYCHO",
        "rpc_label": "rpc-testnet.tychoprotocol.com",
    },
    {
        "id": "ton",
        "name": "TON",
        "rpc": "https://jrpc-ton.broxus.com",
        "color": "#4DB8FF",
        "token_symbol": "TON",
        "rpc_label": "jrpc-ton.broxus.com",
    },
]

with config_path.open("r", encoding="utf-8") as fh:
    config = json.load(fh)

chains = config.setdefault("chains", [])
if not isinstance(chains, list):
    raise SystemExit(f"{config_path}: chains must be an array")

by_id = {
    chain.get("id"): chain
    for chain in chains
    if isinstance(chain, dict) and isinstance(chain.get("id"), str)
}
added = []
updated = []

for default in default_chains:
    chain = by_id.get(default["id"])
    if chain is None:
        chains.append(default.copy())
        added.append(default["id"])
        continue

    for key in ("name", "color", "token_symbol"):
        if chain.get(key) != default[key]:
            chain[key] = default[key]
            updated.append(f"{default['id']}.{key}")

    if not chain.get("rpc_label"):
        chain["rpc_label"] = default["rpc_label"]
        updated.append(f"{default['id']}.rpc_label")

if not added and not updated:
    print(f"Existing config already has built-in chains: {config_path}")
    raise SystemExit(0)

backup_path = config_path.with_name(
    f"{config_path.name}.bak-{time.strftime('%Y%m%d%H%M%S')}"
)
shutil.copy2(config_path, backup_path)

tmp_path = config_path.with_name(f".{config_path.name}.tmp")
with tmp_path.open("w", encoding="utf-8") as fh:
    json.dump(config, fh, indent=2)
    fh.write("\n")

tmp_path.chmod(config_path.stat().st_mode & 0o777)
tmp_path.replace(config_path)

if added:
    print(f"Added built-in chains to existing config: {', '.join(added)}")
if updated:
    print(f"Updated built-in chain metadata: {', '.join(updated)}")
print(f"Config backup: {backup_path}")
PY
}

ensure_cargo() {
  if command -v cargo >/dev/null 2>&1; then
    return
  fi

  if [[ -f "${HOME}/.cargo/env" ]]; then
    # shellcheck disable=SC1091
    source "${HOME}/.cargo/env"
  fi

  if command -v cargo >/dev/null 2>&1; then
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
}

build_release() {
  local rustflags="$PRODUCTION_RUSTFLAGS"
  if [[ -n "${RUSTFLAGS:-}" && -n "$rustflags" ]]; then
    rustflags="${RUSTFLAGS} ${rustflags}"
  elif [[ -z "$rustflags" ]]; then
    rustflags="${RUSTFLAGS:-}"
  fi

  if [[ -n "$rustflags" ]]; then
    echo "Production RUSTFLAGS: $rustflags"
    RUSTFLAGS="$rustflags" cargo build --release --locked
  else
    echo "Production RUSTFLAGS: <none>"
    cargo build --release --locked
  fi
}

write_config_if_missing() {
  if [[ -f "$CONFIG_PATH" ]]; then
    echo "Keeping existing config: $CONFIG_PATH"
    sync_existing_config_defaults
    return
  fi

  local allowed_hosts_csv="$PUBLIC_HOST"
  if [[ -n "$ACME_EXTRA_IDENTIFIERS" ]]; then
    allowed_hosts_csv+=",${ACME_EXTRA_IDENTIFIERS}"
  fi

  local allowed_hosts_json
  local extra_identifiers_json
  allowed_hosts_json="$(json_array_from_csv "$allowed_hosts_csv")"
  extra_identifiers_json="$(json_array_from_csv "$ACME_EXTRA_IDENTIFIERS")"

  local tmp_config
  tmp_config="$(mktemp)"
  cat >"$tmp_config" <<EOF
{
  "listen": "127.0.0.1:8787",
  "refresh_seconds": 60,
  "refresh_timeout_seconds": 90,
  "cache_path": "$(json_escape "$STATE_DIR")/validators_clock_cache.json",
  "history_path": "$(json_escape "$STATE_DIR")/validators_clock_history.json",
  "security": {
    "allowed_hosts": ${allowed_hosts_json},
    "allow_force_refresh": false,
    "max_connections": 128
  },
  "tls": {
    "enabled": true,
    "http_listen": "0.0.0.0:80",
    "https_listen": "0.0.0.0:443",
    "public_url": "$(json_escape "$PUBLIC_URL")",
    "cert_path": "$(json_escape "$ACME_DIR")/fullchain.pem",
    "key_path": "$(json_escape "$ACME_DIR")/privkey.pem",
    "acme": {
      "enabled": true,
      "staging": ${ACME_STAGING},
      "identifier": "$(json_escape "$ACME_IDENTIFIER")",
      "extra_identifiers": ${extra_identifiers_json},
      "account_path": "$(json_escape "$ACME_DIR")/account.json",
      "renew_after_seconds": 2592000,
      "retry_timeout_seconds": 60
    }
  },
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
      "rpc": "https://jrpc-ton.broxus.com",
      "color": "#4DB8FF",
      "token_symbol": "TON",
      "rpc_label": "jrpc-ton.broxus.com"
    }
  ]
}
EOF
  install -m 0600 "$tmp_config" "$CONFIG_PATH"
  rm -f "$tmp_config"
  echo "Created config: $CONFIG_PATH"
}

write_systemd_service() {
  local tmp_service
  tmp_service="$(mktemp)"
  cat >"$tmp_service" <<EOF
[Unit]
Description=Validators Clock
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${RUN_USER}
Group=${RUN_GROUP}
WorkingDirectory=${REPO_DIR}
ExecStart=${BIN_PATH} --config ${CONFIG_PATH}
Restart=always
RestartSec=5

AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
NoNewPrivileges=true
LimitNOFILE=1048576
UMask=0077

PrivateTmp=true
PrivateDevices=true
ProtectSystem=strict
ReadOnlyPaths=${REPO_DIR} ${BIN_DIR}
ReadWritePaths=${STATE_DIR}
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
LockPersonality=true
RestrictRealtime=true
RestrictSUIDSGID=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
SystemCallArchitectures=native

Environment=RUST_LOG=warn,validators_clock=info

[Install]
WantedBy=multi-user.target
EOF
  sudo install -m 0644 "$tmp_service" "$SERVICE_PATH"
  rm -f "$tmp_service"
  echo "Installed systemd unit: $SERVICE_PATH"
}

echo "Repository: $REPO_DIR"
echo "Runtime state: $STATE_DIR"
echo "Binary: $BIN_PATH"
echo "Config: $CONFIG_PATH"

mkdir -p "$BIN_DIR" "$STATE_DIR" "$ACME_DIR"
chmod 700 "$STATE_DIR" "$ACME_DIR"

write_config_if_missing

ensure_cargo

echo "Building release binary"
build_release

tmp_binary="${BIN_PATH}.new"
install -m 0755 "${REPO_DIR}/target/release/${APP_NAME}" "$tmp_binary"
mv -f "$tmp_binary" "$BIN_PATH"
echo "Installed binary: $BIN_PATH"

write_systemd_service

sudo systemctl daemon-reload
sudo systemctl enable "$SERVICE_NAME"

if [[ "$NO_RESTART" == "1" ]]; then
  echo "Skipping restart because --no-restart or VALIDATORS_CLOCK_NO_RESTART=1 was set"
else
  sudo systemctl restart "$SERVICE_NAME"
  sudo systemctl status "$SERVICE_NAME" --no-pager --lines=20
fi
