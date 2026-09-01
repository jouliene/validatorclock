#!/usr/bin/env bash
# Installs the offline basemap: one pmtiles archive plus the fonts its style
# needs. Everything already in place is left alone, so running this again after
# an update costs nothing.
set -euo pipefail

BASEMAP_DIR="${1:-${HOME}/.validatorclock/basemap}"
MAX_ZOOM="${VALIDATORCLOCK_BASEMAP_MAX_ZOOM:-10}"
PMTILES_VERSION="${VALIDATORCLOCK_PMTILES_VERSION:-1.31.2}"
BUILD_METADATA_URL="https://build-metadata.protomaps.dev/builds.json"
BUILD_BASE_URL="https://build.protomaps.com"
ASSETS_URL="https://codeload.github.com/protomaps/basemaps-assets/tar.gz/refs/heads/main"
FONT_STACKS=("Noto Sans Regular" "Noto Sans Medium" "Noto Sans Italic")
# Each stack covers the whole range in 256 files. A directory holding fewer is
# a copy that was interrupted, not an install.
FONT_RANGES_PER_STACK=256

TILES_PATH="${BASEMAP_DIR}/tiles.pmtiles"
FONTS_DIR="${BASEMAP_DIR}/fonts"
TOOLS_DIR="${BASEMAP_DIR}/.tools"
PMTILES_BIN="${TOOLS_DIR}/pmtiles"
# Written only after the tool is fully in place, and named for the version it
# holds, so a bumped version is fetched instead of quietly kept.
PMTILES_MARKER="${TOOLS_DIR}/.pmtiles-${PMTILES_VERSION}"
FONTS_MARKER="${FONTS_DIR}/.installed"

log() {
  echo "[basemap] $*"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "[basemap] $1 is required" >&2
    exit 1
  }
}

require_command curl
require_command tar
require_command python3

mkdir -p "$BASEMAP_DIR" "$TOOLS_DIR"

install_pmtiles_cli() {
  # A download that was interrupted leaves a truncated file that is still
  # executable, and judging by the mode alone made that failure permanent:
  # every later run skipped the download and then failed to extract with it.
  if [[ -x "$PMTILES_BIN" && -f "$PMTILES_MARKER" ]]; then
    return
  fi
  rm -f "$PMTILES_BIN" "${TOOLS_DIR}"/.pmtiles-*

  log "downloading the pmtiles tool ${PMTILES_VERSION}"
  local url="https://github.com/protomaps/go-pmtiles/releases/download/v${PMTILES_VERSION}/go-pmtiles_${PMTILES_VERSION}_Linux_x86_64.tar.gz"
  local tmp
  tmp="$(mktemp -d)"
  curl -fsSL --retry 3 -o "${tmp}/pmtiles.tar.gz" "$url"
  tar xzf "${tmp}/pmtiles.tar.gz" -C "$tmp" pmtiles
  install -m 0755 "${tmp}/pmtiles" "$PMTILES_BIN"
  touch "$PMTILES_MARKER"
  rm -rf "$tmp"
}

# The archive header carries its zoom range, so an archive that already covers
# the requested zooms is kept as it is.
installed_max_zoom() {
  [[ -s "$TILES_PATH" ]] || return 1
  "$PMTILES_BIN" show "$TILES_PATH" 2>/dev/null | awk '/^max zoom:/ { print $3; found = 1 } END { exit !found }'
}

install_tiles() {
  local current
  if current="$(installed_max_zoom)"; then
    if [[ "$current" -ge "$MAX_ZOOM" ]]; then
      log "tiles already installed (zooms 0-${current}), skipping the download"
      return
    fi
    log "tiles cover zooms 0-${current}, rebuilding for 0-${MAX_ZOOM}"
  fi

  # An extract that was interrupted leaves this behind.
  rm -f "${TILES_PATH}.part"

  local build
  build="$(curl -fsS --retry 3 "$BUILD_METADATA_URL" | python3 -c 'import json,sys; print(json.load(sys.stdin)[-1]["key"])')"
  log "extracting zooms 0-${MAX_ZOOM} from ${build} (only the needed ranges are downloaded)"
  "$PMTILES_BIN" extract "${BUILD_BASE_URL}/${build}" "${TILES_PATH}.part" --maxzoom="$MAX_ZOOM"
  mv "${TILES_PATH}.part" "$TILES_PATH"
  log "tiles installed: $(du -h "$TILES_PATH" | cut -f1)"
}

fonts_installed() {
  if [[ -f "$FONTS_MARKER" ]]; then
    return 0
  fi

  # An install made before the marker existed is accepted once every stack is
  # complete, so an update does not re-download what is already on disk.
  local stack
  for stack in "${FONT_STACKS[@]}"; do
    [[ -d "${FONTS_DIR}/${stack}" ]] || return 1
    [[ "$(find "${FONTS_DIR}/${stack}" -name '*.pbf' | wc -l)" -ge "$FONT_RANGES_PER_STACK" ]] ||
      return 1
  done
  touch "$FONTS_MARKER"
}

install_fonts() {
  if fonts_installed; then
    log "fonts already installed, skipping the download"
    return
  fi

  log "downloading fonts"
  local tmp
  tmp="$(mktemp -d)"
  curl -fsSL --retry 3 -o "${tmp}/assets.tar.gz" "$ASSETS_URL"
  tar xzf "${tmp}/assets.tar.gz" -C "$tmp"
  local root="${tmp}/basemaps-assets-main"

  mkdir -p "$FONTS_DIR"
  local stack
  for stack in "${FONT_STACKS[@]}"; do
    rm -rf "${FONTS_DIR:?}/${stack}"
    cp -r "${root}/fonts/${stack}" "${FONTS_DIR}/${stack}"
  done
  rm -rf "$tmp"
  # Written last: a copy that was interrupted leaves no marker, so the next run
  # does the work again instead of trusting a half-filled directory.
  touch "$FONTS_MARKER"
  log "fonts installed: $(du -sh "$FONTS_DIR" | cut -f1)"
}

install_pmtiles_cli
install_tiles
install_fonts
log "basemap ready in ${BASEMAP_DIR}"
