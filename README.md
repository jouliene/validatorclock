# validators_clock

Validator election clock for TVM blockchains.

The server reads chain RPC endpoints from `validators_clock.json`, fetches config
params and elector state directly through `minik2`, and serves a browser UI that
draws the clock.
It starts with Everscale via `https://jrpc.everwallet.net`; more TVM chains can
be added by appending entries to the `chains` array.

Active validator sets expose public keys, ADNL addresses, and validator weights.
The server parses frozen elector round data to derive validator wallets, stakes,
total round stake, total rewards, and per-validator rewards from on-chain state.
The live server treats elector/full-round data as authoritative for round
history, so recorded rounds can prove both participation and absence.
Configured chains are refreshed in the background, so normal browser requests
usually read a warm in-memory snapshot instead of waiting for an RPC round trip.
The UI keeps small per-chain round history files and uses them to mark whether a
validator appeared in the current and previous same-color rounds. The server
prunes each file to the visible history windows after each successful refresh so
they do not grow without bound.
Validator wallet contract code hashes are cached next to `cache_path` in
`validators_clock_validator_types.json`; the round tables map known hashes to
contract names and show `Unknown` for other known-but-unmapped hashes.

Run:

```bash
cargo run
```

Then open:

```text
http://127.0.0.1:8787
```

Useful checks:

```bash
cargo run -- --once everscale
cargo run -- --config validators_clock.json
```

## Built-in HTTPS

The server can terminate TLS itself and request Let's Encrypt certificates through
ACME HTTP-01. This works with a bare IP address when Let's Encrypt IP
certificates are available, but the certificate must use the `shortlived`
profile. Domain certificates do not need a profile; when `tls.acme.profile` is
omitted, Let's Encrypt chooses its default profile.

The recommended production layout keeps the git checkout at
`~/validators_clock`, the installed binary at `~/.cargo/bin/validators_clock`,
and runtime state in the hidden directory `~/.validators_clock`.

Fresh install:

```bash
git clone https://github.com/jouliene/validators_clock.git ~/validators_clock
cd ~/validators_clock
./install.sh
```

Update an existing install without touching runtime data:

```bash
cd ~/validators_clock
git pull --ff-only
./install.sh
```

`install.sh` creates `~/.validators_clock` and `~/.validators_clock/acme`,
copies any legacy `~/validators_clock_state` files into the hidden directory
without overwriting existing files, creates
`~/.validators_clock/validators_clock.production.json` only when it is missing,
builds the release binary, installs it to `~/.cargo/bin`, installs the systemd
unit, and restarts `validators-clock.service`.

The script does not overwrite history, cache, validator type, ACME account, TLS
certificate, TLS key, or an existing production config in `~/.validators_clock`.

Environment overrides for install:

```bash
VALIDATORS_CLOCK_PUBLIC_URL=https://validatorsclock.xyz ./install.sh
VALIDATORS_CLOCK_ACME_STAGING=true ./install.sh
VALIDATORS_CLOCK_STATE_DIR=/home/admin/.validators_clock ./install.sh
```

Example generated production settings for `validatorsclock.xyz`:

```json
{
  "listen": "127.0.0.1:8787",
  "refresh_seconds": 60,
  "refresh_timeout_seconds": 90,
  "cache_path": "/home/admin/.validators_clock/validators_clock_cache.json",
  "history_path": "/home/admin/.validators_clock/validators_clock_history.json",
  "security": {
    "allowed_hosts": ["validatorsclock.xyz", "www.validatorsclock.xyz"],
    "allow_force_refresh": false,
    "max_connections": 128
  },
  "tls": {
    "enabled": true,
    "http_listen": "0.0.0.0:80",
    "https_listen": "0.0.0.0:443",
    "public_url": "https://validatorsclock.xyz",
    "cert_path": "/home/admin/.validators_clock/acme/fullchain.pem",
    "key_path": "/home/admin/.validators_clock/acme/privkey.pem",
    "acme": {
      "enabled": true,
      "staging": false,
      "identifier": "validatorsclock.xyz",
      "extra_identifiers": ["www.validatorsclock.xyz"],
      "account_path": "/home/admin/.validators_clock/acme/account.json",
      "renew_after_seconds": 2592000,
      "retry_timeout_seconds": 60
    }
  },
  "chains": [
    {
      "id": "everscale",
      "name": "Everscale",
      "rpc": "https://jrpc.everwallet.net",
      "color": "#38bdf8",
      "token_symbol": "EVER",
      "rpc_label": "jrpc.everwallet.net"
    },
    {
      "id": "tycho-testnet",
      "name": "Tycho Testnet",
      "rpc": "https://rpc-testnet.tychoprotocol.com",
      "color": "#35d07f",
      "token_symbol": "TYCHO",
      "rpc_label": "rpc-testnet.tychoprotocol.com"
    }
  ]
}
```

By default `install.sh` creates a production ACME config with `"staging": false`.
To test issuance safely before requesting a trusted certificate, run the first
install with `VALIDATORS_CLOCK_ACME_STAGING=true ./install.sh`, then edit
`~/.validators_clock/validators_clock.production.json` and set `"staging": false`
before restarting.

`history_path` is a base path. Runtime history is written per chain by adding the
chain id before the extension, for example
`validators_clock_history_everscale.json` and
`validators_clock_history_tycho-testnet.json`.

On startup and during the renewal loop, the app reuses an existing certificate
only if the key loads, the certificate is valid outside the renewal window, and
the certificate covers every configured ACME identifier. For normal 90-day domain
certificates, `renew_after_seconds: 2592000` renews when less than 30 days remain.
For direct IP certificates, set `"profile": "shortlived"` and use a short renewal
window such as `172800`. Adding a name such as `www.validatorsclock.xyz` to
`tls.acme.extra_identifiers` will cause the next start or renewal check to
request a replacement certificate.

Ports 80 and 443 must be reachable from the public internet for ACME validation
and HTTPS traffic. If `ufw` is enabled:

```bash
sudo ufw allow 80/tcp comment 'validators-clock HTTP ACME'
sudo ufw allow 443/tcp comment 'validators-clock HTTPS'
```

## Production systemd service

`install.sh` writes `/etc/systemd/system/validators-clock.service` for the
current deployment user. An example service file is kept in
`deploy/validators-clock.service`:

```ini
[Unit]
Description=Validators Clock
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=admin
Group=admin
WorkingDirectory=/home/admin/validators_clock
ExecStart=/home/admin/.cargo/bin/validators_clock --config /home/admin/.validators_clock/validators_clock.production.json
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
ReadOnlyPaths=/home/admin/validators_clock /home/admin/.cargo/bin
ReadWritePaths=/home/admin/.validators_clock
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
```

The hardening assumes production state is outside the git checkout in
`/home/admin/.validators_clock`. If `cache_path`, `history_path`, ACME account,
or TLS keys are placed elsewhere, add that directory to `ReadWritePaths`.

Manual reload and start if you are not using `install.sh`:

```bash
sudo systemctl daemon-reload
sudo systemctl enable validators-clock.service
sudo systemctl restart validators-clock.service
sudo systemctl status validators-clock.service --no-pager
```

The HTTP listener only serves ACME challenges and redirects all other requests to
`tls.public_url`. HTTPS responses include basic browser security headers, and
`security.allowed_hosts` rejects unexpected Host headers when configured.

For a domain that should work with and without `www`, keep `public_url` on the
canonical host and add the other names to `tls.acme.extra_identifiers`, for
example `"identifier": "validatorsclock.xyz"` and
`"extra_identifiers": ["www.validatorsclock.xyz"]`.

## Production updates

From the server checkout:

```bash
cd /home/admin/validators_clock
git pull --ff-only origin main
./install.sh
```

Basic checks:

```bash
curl -I http://validatorsclock.xyz
curl -I https://validatorsclock.xyz
curl https://validatorsclock.xyz/api/health
curl https://validatorsclock.xyz/api/status
```

`/api/status` reports the app version, uptime, configured refresh interval,
refresh timeout, and per-chain cache freshness or last refresh error. If a chain
RPC stalls longer than `refresh_timeout_seconds`, the app records the timeout and
continues serving the last good cached snapshot when one exists.

## Logs

Show recent service logs:

```bash
sudo journalctl -u validators-clock.service -n 100 --no-pager
```

Follow live logs:

```bash
sudo journalctl -u validators-clock.service -f
```

Show logs since a recent restart:

```bash
sudo journalctl -u validators-clock.service --since "10 minutes ago" --no-pager
```

The default log filter is `warn,validators_clock=info`. TLS handshakes from
internet scanners are logged at `debug`, so normal logs stay quiet. To inspect
debug details temporarily, add or override:

```ini
Environment=RUST_LOG=validators_clock=debug
```

then run `sudo systemctl daemon-reload` and restart the service.
