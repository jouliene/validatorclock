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
If a chain/RPC cannot expose that frozen data, the server falls back to scanning
elector `participate_in_elections` messages and saves that fallback mapping to
`cache_path` so restarts do not repeat the full scan.
Configured chains are refreshed in the background, so normal browser requests
usually read a warm in-memory snapshot instead of waiting for an RPC round trip.
The UI also keeps a small local round history file and uses it to mark whether a
validator appeared in the previous same-color rounds.

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

Example production settings for `validatorsclock.xyz`, keeping runtime state outside
the git checkout:

```json
{
  "listen": "127.0.0.1:8787",
  "refresh_seconds": 60,
  "refresh_timeout_seconds": 90,
  "cache_path": "/home/admin/validators_clock_state/validators_clock_cache.json",
  "history_path": "/home/admin/validators_clock_state/validators_clock_history.json",
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
    "cert_path": "/home/admin/validators_clock_state/acme/fullchain.pem",
    "key_path": "/home/admin/validators_clock_state/acme/privkey.pem",
    "acme": {
      "enabled": true,
      "staging": true,
      "identifier": "validatorsclock.xyz",
      "extra_identifiers": ["www.validatorsclock.xyz"],
      "account_path": "/home/admin/validators_clock_state/acme/account.json",
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

First run with `"staging": true`; after issuance works, switch it to `false` for
a trusted production certificate.

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

Create the state directory once:

```bash
mkdir -p /home/admin/validators_clock_state/acme
```

## Production systemd service

Example service file at `/etc/systemd/system/validators-clock.service` is kept
in `deploy/validators-clock.service`:

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
ExecStart=/home/admin/validators_clock/target/release/validators_clock --config /home/admin/validators_clock/validators_clock.production.json
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
ReadOnlyPaths=/home/admin/validators_clock
ReadWritePaths=/home/admin/validators_clock_state
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
LockPersonality=true
RestrictRealtime=true
RestrictSUIDSGID=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
SystemCallArchitectures=native

# Optional. The default is equivalent to warn,validators_clock=info.
# Environment=RUST_LOG=validators_clock=info

[Install]
WantedBy=multi-user.target
```

The hardening assumes production state is outside the git checkout, for example
`/home/admin/validators_clock_state`. If `cache_path`, `history_path`, ACME
account, or TLS keys are placed elsewhere, add that directory to
`ReadWritePaths`.

Reload and start:

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
cargo build --release
sudo systemctl restart validators-clock.service
sudo systemctl status validators-clock.service --no-pager
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
