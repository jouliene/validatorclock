#!/usr/bin/env bash
set -euo pipefail

# Collect Tycho overlay peers, keep only active validators when validators_clock
# is reachable, enrich IPs with geo data, and write a map node JSON/JS file.
#
# Required commands: tycho, jq, curl.
#
# Common production usage:
#   TYCHO_MAP_OUTPUT=/var/lib/validators-clock/tycho_nodes.json \
#   VALIDATORS_CLOCK_URL=http://127.0.0.1:8787 \
#   scripts/collect-tycho-map.sh
#
# Useful env vars:
#   TYCHO_BIN                  tycho binary path, default: tycho
#   VALIDATORS_CLOCK_URL       validators_clock base URL, default: http://127.0.0.1:8787
#   TYCHO_MAP_CHAIN_ID         chain id for clock API, default: tycho-testnet
#   TYCHO_MAP_OUTPUT           output path, default: ./target/tycho_nodes.json
#   TYCHO_MAP_FORMAT           json or js, default inferred from output extension
#   TYCHO_MAP_GEO_CACHE        geo cache path, default: output dir/tycho_geo_cache.json
#   TYCHO_MAP_GEO_PROVIDER     ip-api or cache-only, default: ip-api
#   TYCHO_MAP_OVERLAYS         optional space/comma separated overlay IDs
#   TYCHO_MAP_OVERLAY_SCOPE    private, public, or all when overlays are auto-discovered
#   TYCHO_MAP_ACTIVE_ONLY      1 to require current validator filter, 0 for all peers, default: 1

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TYCHO_BIN="${TYCHO_BIN:-tycho}"
VALIDATORS_CLOCK_URL="${VALIDATORS_CLOCK_URL:-http://127.0.0.1:8787}"
TYCHO_MAP_CHAIN_ID="${TYCHO_MAP_CHAIN_ID:-tycho-testnet}"
TYCHO_MAP_OUTPUT="${TYCHO_MAP_OUTPUT:-${ROOT_DIR}/target/tycho_nodes.json}"
TYCHO_MAP_OVERLAY_SCOPE="${TYCHO_MAP_OVERLAY_SCOPE:-private}"
TYCHO_MAP_GEO_PROVIDER="${TYCHO_MAP_GEO_PROVIDER:-ip-api}"
TYCHO_MAP_ACTIVE_ONLY="${TYCHO_MAP_ACTIVE_ONLY:-1}"

OUTPUT_DIR="$(dirname "${TYCHO_MAP_OUTPUT}")"
mkdir -p "${OUTPUT_DIR}"

if [[ -z "${TYCHO_MAP_FORMAT:-}" ]]; then
  case "${TYCHO_MAP_OUTPUT}" in
    *.js) TYCHO_MAP_FORMAT="js" ;;
    *) TYCHO_MAP_FORMAT="json" ;;
  esac
fi

TYCHO_MAP_GEO_CACHE="${TYCHO_MAP_GEO_CACHE:-${OUTPUT_DIR}/tycho_geo_cache.json}"

require_command() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    echo "Missing required command: ${name}" >&2
    exit 1
  fi
}

require_command jq
require_command curl
if ! command -v "${TYCHO_BIN}" >/dev/null 2>&1; then
  echo "Missing required command: ${TYCHO_BIN}" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

overlay_filter() {
  case "${TYCHO_MAP_OVERLAY_SCOPE}" in
    private) jq -r '.private_overlays[]?' ;;
    public) jq -r '.public_overlays[]?' ;;
    all) jq -r '.private_overlays[]?, .public_overlays[]?' ;;
    *)
      echo "Invalid TYCHO_MAP_OVERLAY_SCOPE=${TYCHO_MAP_OVERLAY_SCOPE}; expected private, public, or all" >&2
      exit 1
      ;;
  esac
}

discover_overlays() {
  if [[ -n "${TYCHO_MAP_OVERLAYS:-}" ]]; then
    tr ', ' '\n\n' <<<"${TYCHO_MAP_OVERLAYS}" | sed '/^$/d'
    return
  fi

  "${TYCHO_BIN}" node overlay list | overlay_filter
}

collect_overlay_peers() {
  local overlays_file="$1"
  local peers_file="$2"
  : >"${peers_file}"

  while IFS= read -r overlay; do
    [[ -n "${overlay}" ]] || continue
    echo "Collecting overlay peers: ${overlay}" >&2
    "${TYCHO_BIN}" node overlay peers "${overlay}" \
      | jq -c --arg overlay "${overlay}" '
          .peers[]?
          | select(.info != null)
          | .peer_id as $peer
          | (.info.address_list // [])[]? as $address
          | ($address | capture("^(?<host>\\[[^\\]]+\\]|[^:]+)(?::(?<port>[0-9]+))?$")?) as $parsed
          | select($parsed != null)
          | {
              peer: $peer,
              ip: ($parsed.host | sub("^\\["; "") | sub("\\]$"; "")),
              address: $address,
              overlay: $overlay,
              created_at: .info.created_at,
              expires_at: .info.expires_at
            }
        ' >>"${peers_file}"
  done <"${overlays_file}"
}

fetch_active_validators() {
  local output_file="$1"
  local clock_url="${VALIDATORS_CLOCK_URL%/}/api/chains/${TYCHO_MAP_CHAIN_ID}/clock"

  if [[ "${TYCHO_MAP_ACTIVE_ONLY}" != "1" ]]; then
    jq -n '[]' >"${output_file}"
    return 1
  fi

  if curl -fsS "${clock_url}" \
    | jq '[.current_set.validators[]?.public_key | ascii_downcase] | unique' >"${output_file}"; then
    return 0
  fi

  echo "Warning: unable to fetch active validators from ${clock_url}; keeping all overlay peers" >&2
  jq -n '[]' >"${output_file}"
  return 1
}

filter_active_peers() {
  local peers_file="$1"
  local active_file="$2"
  local output_file="$3"

  if [[ "${TYCHO_MAP_ACTIVE_ONLY}" == "1" ]] && [[ "$(jq 'length' "${active_file}")" -gt 0 ]]; then
    jq --slurpfile active "${active_file}" '
      [ .[]
        | select((.peer | ascii_downcase) as $peer | ($active[0] | index($peer)))
      ]
      | unique_by(.peer + "|" + .ip)
      | sort_by(.peer, .ip)
    ' "${peers_file}" >"${output_file}"
  else
    jq 'unique_by(.peer + "|" + .ip) | sort_by(.peer, .ip)' "${peers_file}" >"${output_file}"
  fi
}

ensure_geo_cache() {
  if [[ ! -s "${TYCHO_MAP_GEO_CACHE}" ]]; then
    mkdir -p "$(dirname "${TYCHO_MAP_GEO_CACHE}")"
    printf '{}\n' >"${TYCHO_MAP_GEO_CACHE}"
  fi
}

refresh_geo_cache() {
  local peers_file="$1"
  local ips_file="${TMP_DIR}/ips.txt"
  local missing_file="${TMP_DIR}/missing_ips.txt"
  local response_file="${TMP_DIR}/geo_response.json"
  local new_cache="${TMP_DIR}/geo_cache.json"

  jq -r '.[].ip' "${peers_file}" | sort -u >"${ips_file}"
  : >"${missing_file}"

  while IFS= read -r ip; do
    [[ -n "${ip}" ]] || continue
    if ! jq -e --arg ip "${ip}" 'has($ip)' "${TYCHO_MAP_GEO_CACHE}" >/dev/null; then
      printf '%s\n' "${ip}" >>"${missing_file}"
    fi
  done <"${ips_file}"

  if [[ ! -s "${missing_file}" ]]; then
    return
  fi

  if [[ "${TYCHO_MAP_GEO_PROVIDER}" == "cache-only" ]]; then
    echo "Warning: missing geo cache entries for $(wc -l <"${missing_file}") IPs; cache-only mode will skip them" >&2
    return
  fi

  if [[ "${TYCHO_MAP_GEO_PROVIDER}" != "ip-api" ]]; then
    echo "Invalid TYCHO_MAP_GEO_PROVIDER=${TYCHO_MAP_GEO_PROVIDER}; expected ip-api or cache-only" >&2
    exit 1
  fi

  echo "Fetching geo data for $(wc -l <"${missing_file}") IPs" >&2
  jq -Rn '[inputs]' <"${missing_file}" \
    | curl -fsS \
        -H 'Content-Type: application/json' \
        --data-binary @- \
        'http://ip-api.com/batch?fields=status,message,query,country,city,lat,lon,isp' \
    >"${response_file}"

  jq -s '
    .[0] * (
      .[1]
      | map(select(.status == "success" and .lat != null and .lon != null))
      | map({
          key: .query,
          value: {
            city: (.city // "Unknown"),
            country: (.country // "Unknown"),
            isp: (.isp // "Unknown"),
            lat: .lat,
            lon: .lon,
            updated_at: (now | floor)
          }
        })
      | from_entries
    )
  ' "${TYCHO_MAP_GEO_CACHE}" "${response_file}" >"${new_cache}"

  mv "${new_cache}" "${TYCHO_MAP_GEO_CACHE}"
}

build_nodes() {
  local peers_file="$1"
  local output_file="$2"

  jq --slurpfile geo "${TYCHO_MAP_GEO_CACHE}" '
    [ .[] as $peer
      | ($geo[0][$peer.ip] // null) as $location
      | select($location != null and $location.lat != null and $location.lon != null)
      | {
          peer: $peer.peer,
          ip: $peer.ip,
          city: $location.city,
          country: $location.country,
          isp: $location.isp,
          lat: $location.lat,
          lon: $location.lon
        }
    ]
    | unique_by(.peer + "|" + .ip)
    | sort_by(.country, .city, .ip, .peer)
  ' "${peers_file}" >"${output_file}"
}

report_summary() {
  local active_file="$1"
  local peers_file="$2"
  local nodes_file="$3"

  local active_count
  local peer_count
  local mapped_count
  active_count="$(jq 'length' "${active_file}")"
  peer_count="$(jq 'length' "${peers_file}")"
  mapped_count="$(jq 'length' "${nodes_file}")"

  if [[ "${active_count}" -gt 0 ]]; then
    echo "Active validators: ${active_count}; active peers with address: ${peer_count}; mapped nodes: ${mapped_count}" >&2
    jq -r --slurpfile nodes "${nodes_file}" '
      [ $nodes[0][].peer | ascii_downcase ] as $mapped
      | .[] as $peer
      | select(($mapped | index($peer)) | not)
      | $peer
    ' "${active_file}" \
      | sed 's/^/Unmapped active validator: /' >&2
  else
    echo "Overlay peers with address: ${peer_count}; mapped nodes: ${mapped_count}" >&2
  fi
}

write_output() {
  local nodes_file="$1"
  local tmp_output="${TYCHO_MAP_OUTPUT}.tmp"

  case "${TYCHO_MAP_FORMAT}" in
    json)
      cp "${nodes_file}" "${tmp_output}"
      ;;
    js)
      {
        printf 'window.TYCHO_NODES = '
        jq -c '.' "${nodes_file}"
        printf ';\n'
      } >"${tmp_output}"
      ;;
    *)
      echo "Invalid TYCHO_MAP_FORMAT=${TYCHO_MAP_FORMAT}; expected json or js" >&2
      exit 1
      ;;
  esac

  mv "${tmp_output}" "${TYCHO_MAP_OUTPUT}"
}

main() {
  local overlays_file="${TMP_DIR}/overlays.txt"
  local peers_ndjson="${TMP_DIR}/peers.ndjson"
  local peers_json="${TMP_DIR}/peers.json"
  local active_json="${TMP_DIR}/active_validators.json"
  local filtered_json="${TMP_DIR}/filtered_peers.json"
  local nodes_json="${TMP_DIR}/nodes.json"

  discover_overlays | sort -u >"${overlays_file}"
  if [[ ! -s "${overlays_file}" ]]; then
    echo "No Tycho overlays found" >&2
    exit 1
  fi

  collect_overlay_peers "${overlays_file}" "${peers_ndjson}"
  jq -s 'unique_by(.peer + "|" + .ip) | sort_by(.peer, .ip)' "${peers_ndjson}" >"${peers_json}"

  fetch_active_validators "${active_json}" || true
  filter_active_peers "${peers_json}" "${active_json}" "${filtered_json}"

  ensure_geo_cache
  refresh_geo_cache "${filtered_json}"
  build_nodes "${filtered_json}" "${nodes_json}"
  report_summary "${active_json}" "${filtered_json}" "${nodes_json}"
  write_output "${nodes_json}"

  echo "Wrote $(jq 'length' "${nodes_json}") Tycho map nodes to ${TYCHO_MAP_OUTPUT}" >&2
}

main "$@"
