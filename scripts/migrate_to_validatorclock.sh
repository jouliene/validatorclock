#!/usr/bin/env bash
set -euo pipefail

OLD_STATE_DIR="${HOME}/.validators_clock"
NEW_STATE_DIR="${HOME}/.validatorclock"
OLD_SERVICE="validators-clock.service"
NEW_SERVICE="validatorclock.service"
OLD_CONFIG="${OLD_STATE_DIR}/validators_clock.production.json"
NEW_CONFIG="${NEW_STATE_DIR}/validatorclock.production.json"

log() {
  printf '%s\n' "$*"
}

warn() {
  printf 'Warning: %s\n' "$*" >&2
}

copy_file_if_missing() {
  local source_path="$1"
  local dest_path="$2"

  if [[ ! -f "$source_path" ]]; then
    return
  fi

  if [[ -e "$dest_path" ]]; then
    log "Keeping existing file: $dest_path"
    return
  fi

  cp -a "$source_path" "$dest_path"
  log "Copied file: $source_path -> $dest_path"
}

copy_dir_contents_if_exists() {
  local name="$1"
  local source_dir="${OLD_STATE_DIR}/${name}"
  local dest_dir="${NEW_STATE_DIR}/${name}"

  if [[ ! -d "$source_dir" ]]; then
    return
  fi

  mkdir -p "$dest_dir"
  cp -a -n "${source_dir}/." "$dest_dir/"
  log "Copied directory contents: $source_dir -> $dest_dir"
}

stop_old_service() {
  if ! command -v systemctl >/dev/null 2>&1; then
    warn "systemctl was not found; skipping old service stop"
    return
  fi

  if systemctl cat "$OLD_SERVICE" >/dev/null 2>&1; then
    sudo systemctl stop "$OLD_SERVICE" || true
  else
    log "Old service is not installed: $OLD_SERVICE"
  fi
}

copy_state_data() {
  if [[ ! -d "$OLD_STATE_DIR" ]]; then
    warn "old state directory does not exist: $OLD_STATE_DIR"
    return
  fi

  copy_dir_contents_if_exists "acme"
  copy_dir_contents_if_exists "tycho_map"
  copy_dir_contents_if_exists "ton_map"
  copy_dir_contents_if_exists "everscale_map"

  copy_file_if_missing \
    "${OLD_STATE_DIR}/validators_clock_cache.json" \
    "${NEW_STATE_DIR}/validatorclock_cache.json"
  copy_file_if_missing \
    "${OLD_STATE_DIR}/validators_clock_validator_types.json" \
    "${NEW_STATE_DIR}/validatorclock_validator_types.json"
  copy_file_if_missing \
    "${OLD_STATE_DIR}/validators_clock_acme_account.json" \
    "${NEW_STATE_DIR}/validatorclock_acme_account.json"

  local source_path
  local source_name
  local dest_name
  for source_path in "${OLD_STATE_DIR}"/validators_clock_history*.json; do
    [[ -e "$source_path" ]] || continue
    source_name="$(basename "$source_path")"
    dest_name="${source_name/validators_clock_history/validatorclock_history}"
    copy_file_if_missing "$source_path" "${NEW_STATE_DIR}/${dest_name}"
  done
}

convert_config() {
  if ! command -v python3 >/dev/null 2>&1; then
    warn "python3 is required to convert and validate the production config"
    return 1
  fi

  if [[ ! -f "$OLD_CONFIG" && ! -f "$NEW_CONFIG" ]]; then
    warn "no production config found at $OLD_CONFIG or $NEW_CONFIG"
    return
  fi

  python3 - "$OLD_CONFIG" "$NEW_CONFIG" "$HOME" <<'PY'
import json
import os
import sys
from pathlib import Path

old_config = Path(sys.argv[1])
new_config = Path(sys.argv[2])
home = Path(sys.argv[3])
source = new_config if new_config.exists() else old_config

with source.open("r", encoding="utf-8") as handle:
    config = json.load(handle)

def rewrite(value):
    if isinstance(value, dict):
        return {key: rewrite(item) for key, item in value.items()}
    if isinstance(value, list):
        return [rewrite(item) for item in value]
    if isinstance(value, str):
        return (
            value.replace(".validators_clock", ".validatorclock")
            .replace("validators_clock", "validatorclock")
            .replace("validatorsclock.xyz", "validatorclock.xyz")
        )
    return value

config = rewrite(config)

security = config.setdefault("security", {})
if not isinstance(security, dict):
    security = {}
    config["security"] = security
security["allowed_hosts"] = ["validatorclock.xyz", "www.validatorclock.xyz"]

tls = config.setdefault("tls", {})
if not isinstance(tls, dict):
    tls = {}
    config["tls"] = tls
tls["public_url"] = "https://validatorclock.xyz"
tls["cert_path"] = str(home / ".validatorclock" / "acme" / "fullchain.pem")
tls["key_path"] = str(home / ".validatorclock" / "acme" / "privkey.pem")

acme = tls.setdefault("acme", {})
if not isinstance(acme, dict):
    acme = {}
    tls["acme"] = acme
acme["identifier"] = "validatorclock.xyz"
acme["extra_identifiers"] = ["www.validatorclock.xyz"]
acme["account_path"] = str(home / ".validatorclock" / "acme" / "account.json")

new_config.parent.mkdir(parents=True, exist_ok=True)
mode = 0o600
if new_config.exists():
    mode = new_config.stat().st_mode & 0o777
elif source.exists():
    mode = source.stat().st_mode & 0o777

tmp_config = new_config.with_name(f".{new_config.name}.tmp")
with tmp_config.open("w", encoding="utf-8") as handle:
    json.dump(config, handle, indent=2)
    handle.write("\n")
os.chmod(tmp_config, mode)
os.replace(tmp_config, new_config)
PY

  python3 -m json.tool "$NEW_CONFIG" >/dev/null
  log "Converted and validated config: $NEW_CONFIG"
}

disable_old_service_if_new_is_active() {
  if ! command -v systemctl >/dev/null 2>&1; then
    warn "systemctl was not found; old service was not disabled"
    return
  fi

  if systemctl is-active --quiet "$NEW_SERVICE"; then
    sudo systemctl disable "$OLD_SERVICE" || true
    log "New service is active; disabled old service: $OLD_SERVICE"
  else
    warn "new service is not active yet; leaving old service enabled: $OLD_SERVICE"
  fi
}

print_next_steps() {
  cat <<NEXT

Next deployment commands:
  ./install.sh
  sudo systemctl status validatorclock.service --no-pager
  curl -I https://validatorclock.xyz/
  curl -I https://validatorclock.xyz/api/status
  curl -I https://www.validatorclock.xyz/

After validatorclock.service is active, rerun this script or run:
  sudo systemctl disable validators-clock.service || true

Manual cleanup can happen later after verification. This script does not remove:
  /etc/systemd/system/validators-clock.service
  ${OLD_STATE_DIR}
NEXT
}

log "Old state: $OLD_STATE_DIR"
log "New state: $NEW_STATE_DIR"

stop_old_service

mkdir -p "$NEW_STATE_DIR"
chmod 700 "$NEW_STATE_DIR"

copy_state_data
convert_config
disable_old_service_if_new_is_active
print_next_steps
