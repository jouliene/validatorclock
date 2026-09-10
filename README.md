# Validator Clock

Web dashboard for Everscale, Tycho, and TON validator rounds, elections,
stakes, rewards, wallet types, and recent validator history.

![Validator Clock screenshot](docs/validatorclock-screenshot.png)

## Run Locally

```bash
cd ~
git clone https://github.com/jouliene/validatorclock.git validatorclock
cd validatorclock
cargo run
```

Open:

```text
http://127.0.0.1:8787
```

The default TON config keeps TON Center as the primary RPC and uses Broxus as a
fallback. If you have a TON Center key, set it before starting the app:

```bash
export VALIDATORCLOCK_TONCENTER_API_KEY=your-key
```

Chain endpoints can be JRPC (`https://host`), TON Center
(`https://toncenter.com/api/v2/jsonRPC`), or GraphQL — any URL whose last path
segment is `graphql`, such as an Evercloud endpoint. The bundled config uses the
keyless Evercloud endpoint, which is rate limited; a project endpoint carries
its id in the URL path.

Because such a URL is a credential, any chain can take its endpoint from the
environment instead of a config file, under `VALIDATORCLOCK_RPC_` plus the chain
id in upper case (`-` becomes `_`):

```bash
export VALIDATORCLOCK_RPC_EVERSCALE=https://mainnet.evercloud.dev/your-project-id/graphql
export VALIDATORCLOCK_RPC_TYCHO_TESTNET=https://rpc-testnet.tychoprotocol.com
```

The override replaces the chain's `rpc`; fallbacks stay as configured, and the
startup log names the variable, never the URL. On a server, keep these in the
systemd `EnvironmentFile` next to `IPINFO_TOKEN`. Endpoints that authenticate
with a bearer token instead of a project id in the URL can read it from:

```bash
export VALIDATORCLOCK_GRAPHQL_API_KEY=your-key
```

If Rust is missing:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

## Install On Ubuntu Server

Point DNS to the server first. Ports `80` and `443` must be open.

Install packages:

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev curl git
```

Clone, migrate existing production data if present, and install:

```bash
cd ~
git clone https://github.com/jouliene/validatorclock.git validatorclock
cd validatorclock
./scripts/migrate_to_validatorclock.sh
./install.sh
```

For another domain:

```bash
VALIDATORCLOCK_PUBLIC_URL=https://your-domain.example \
VALIDATORCLOCK_ACME_IDENTIFIER=your-domain.example \
VALIDATORCLOCK_ACME_EXTRA_IDENTIFIERS=www.your-domain.example \
./install.sh
```

`install.sh` checks Rust. If Rust is missing, it installs Rust with `rustup`.
If Rust is already managed by `rustup`, it updates Rust before building.

The script asks for `sudo` only for systemd work: installing the service file,
reloading systemd, enabling the service, and restarting the service.

## Update Production

```bash
cd ~/validatorclock
./update.sh
```

`update.sh` checks/updates Rust, runs:

```bash
git pull --ff-only origin main
```

then builds and installs the new binary. It does not recreate the systemd
service. For normal updates, it restarts the already-existing service without
sudo by stopping the current app process and letting systemd start it again.

`--ff-only` is intentional. It updates production only when Git can move
straight to the GitHub version. Plain `git pull` can create a merge commit on
the server if there are local changes.

## Node Map

The basemap is served by this app from disk: a pmtiles archive plus the fonts
and sprite its style needs. No tile service, no key, no watermark. CARTO, which
served the basemap before, now stamps "API KEY REQUIRED" across keyless tiles
and sells no free tier.

`install.sh` installs it, so `./update.sh` keeps it in place too. Everything
already present is left alone, so a second run costs a second and downloads
nothing:

```bash
scripts/install_basemap.sh ~/.validatorclock/basemap
```

The archive holds zooms 0-10 and takes about 3.5 GB; the extract pulls only
those zooms out of the 128 GB planet build, so it transfers 3.5 GB, not 128.
`VALIDATORCLOCK_BASEMAP_MAX_ZOOM` picks a different depth,
`VALIDATORCLOCK_SKIP_BASEMAP=1` skips the step entirely. Without the archive the
map still draws the nodes, on an empty canvas.

The style ships inside the binary and takes its zoom range from the installed
archive, so the two cannot drift: a style promising zooms the archive lacks
makes MapLibre request tiles that are not there, and the map goes blank in
patches. `scripts/build_basemap_style.py` regenerates the style from the
Protomaps dark theme in the dashboard palette.

A label layer must ask for a font the style serves. A missing font answers 404,
and that failure takes down every layer sharing the tile, so the node circles
vanish with the labels; a missing glyph range is therefore answered with no
glyphs instead of 404, and a test fails the build if a layer names a font
inline.

Full all-network comparison and known issues: [10 September 2026 report](docs/geolocation-full-comparison-2026-09-10/README.md). The experimental resolver has not been promoted to main.

This experimental branch uses a persistent, keyless geolocation researcher. The
five-minute loop only checks locally resolved IPs; completed locations are reused
for 2 days, across validator-set changes and restarts. New and late-arriving IPs
enter a bounded queue. Failed lookups back off (1h, 6h, 1d, 3d, then weekly), and
fully researched disagreements are checked weekly rather than treated as facts.

The primary source is ip-api's free batch endpoint. DB-IP City Lite is downloaded
automatically into a local MMDB (about 122 MiB unpacked for September 2026), with
at most one successful update per month and only when there is research to do.
The resolver also uses reviewed operator geofeeds and bounded, direct-target
Globalping checks independent of the target operator. Candidates come from the
current probe catalogue and conflicting database locations, not an ASN allowlist.
A location over 100 km from the nearest available probe city in its country also
triggers research; this distance is not proof that the database is wrong.
A metro is accepted only with <=3 ms RTT (>=3 replies) from at least two distinct
probe ASNs within 100 km, with no incompatible low-RTT alternative. No accounts, API
keys, paid plans, Python runtime or extra setup are required for this component.
DB-IP data is [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/); the footer
includes the required [DB-IP attribution](https://db-ip.com).

Answers and decisions live separately in `geo_cache.research.json` alongside the
configured geo cache; downloaded files live in `geo_cache.research-data/`. Keep
these paths writable and persistent. Corrupt research state stops new queries
instead of resetting quotas. Existing map points remain available during service
failures, and uncertainty is shown in map/validator tooltips.

Defaults (these need not be added to an existing enabled node-location config):

```json
"node_locations": {
  "research": {
    "enabled": true,
    "reuse_days": 2,
    "max_ips_per_cycle": 100,
    "daily_requests": 0,
    "daily_measurements": 0,
    "download_database": true,
    "network_measurements": true
  }
}
```

Globalping uses no API key. There is no application-imposed daily ceiling by
default: `daily_requests: 0` and `daily_measurements: 0` disable optional local
caps. Positive values enable an administrator-selected cap on HTTP calls or
measurement jobs respectively; existing explicit values remain effective.
Provider throttling and Retry-After still apply. Each job uses at most 6 probe
tests to compare its candidate cities; that is not a daily quota. Catalogue reads
(once/day while work exists), job creation and result reads remain accounted for. Job IDs survive restarts; results are collected on later retry
cycles (first normally after one hour), without submitting duplicate jobs. No
answer or insufficient independent probes leaves the location uncertain. The old
`operator_measurements` setting remains a deserialization alias. Operator geofeeds
currently cover only Hetzner/Latitude; they are optional extra evidence and do not
limit the universal measurement engine. See the [universal measurement report](docs/geolocation-experiment/universal-measurements.md).

Limits are shared across chains and persisted before sending requests. The
request limit counts HTTP calls, not the number of IPs in a batch. Research only
starts after the existing DHT/external resolver supplies an IP; configuring that
resolver is unchanged. `geo_cache_ttl_seconds`, IPinfo credentials and
`auto_resolve_conflicts` apply to the old strategy; select it explicitly with
`"research": {"enabled": false}`. Existing manual location files retain priority.

See the [implementation comparison](docs/geolocation-experiment/README.md) for
measured results, limits, and reproduction commands. These are improvements to
specific cases and request scheduling, not a claim of universally accurate IP
geolocation. ip-api's free endpoint permits non-commercial use; see its
[terms and limits](https://ip-api.com/docs/api:batch).

## Visitor Stats

The footer of the main page shows aggregate traffic (today, last 30 days, all
time) and stays public. A password-protected page breaks the same traffic down
per IP address:

```text
https://validatorclock.xyz/stats
```

Each row shows the address, its country, city, and provider (resolved through
ip-api.com), visits today, visits over the last 30 days, visits all time, when
the address was last seen, and whether it is on the site right now. A visit is a
session from one address; a new visit starts after 30 minutes without activity,
and days are counted in UTC.

### Password

`/stats`, `/stats/app.js`, and `/stats/visitors` are behind HTTP Basic auth. Set
the password in the production config, which survives `install.sh` re-runs:

```json
"security": {
  "allowed_hosts": ["validatorclock.xyz", "www.validatorclock.xyz"],
  "stats_auth": {
    "username": "admin",
    "password": "your-long-random-password"
  }
}
```

Generate one with `openssl rand -base64 24`, keep the config file at mode `600`,
and restart the service. Instead of the config field the password can come from
the `VALIDATORCLOCK_STATS_PASSWORD` environment variable (set
`security.stats_auth.password_env` to read a different name), which suits a
systemd `EnvironmentFile`.

Without a password the page and its API return `404`, so a fresh install never
exposes visitor addresses by accident. The startup log says which of the three
states is active. Setting `security.stats_auth.enabled` to `false` makes the
page public again.

### Storage

Visitor addresses live in `validatorclock_visitors.json` next to the other state
files. Per-address day counters are kept for 31 days, records for addresses that
stop visiting are dropped after a year, and the store holds at most 5000
addresses (the least recently seen are evicted first).

## Check Production

```bash
sudo systemctl status validatorclock.service --no-pager
curl -I https://validatorclock.xyz/
curl -I https://validatorclock.xyz/api/status
curl -I https://www.validatorclock.xyz/
```

Logs:

```bash
sudo journalctl -u validatorclock.service -n 100 --no-pager
sudo journalctl -u validatorclock.service -f
```

## Files

Installed binary:

```text
~/.cargo/bin/validatorclock
```

Production data:

```text
~/.validatorclock
```

Important data files:

```text
validatorclock.production.json
validatorclock_cache_everscale.json
validatorclock_cache_tycho-testnet.json
validatorclock_cache_ton.json
validatorclock_history_everscale.json
validatorclock_history_tycho-testnet.json
validatorclock_history_ton.json
validatorclock_validator_types.json
validatorclock_visitors.json
acme/
```

The snapshot cache and the round history keep one file per chain, so a refresh
rewrites only the chain it refreshed. A `validatorclock_cache.json` left by an
earlier release is split into per-chain files on the next start and removed.
