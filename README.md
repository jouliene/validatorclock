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
profile.

Example production settings for `104.238.222.200`, keeping runtime state outside
the git checkout:

```json
{
  "listen": "127.0.0.1:8787",
  "refresh_seconds": 60,
  "cache_path": "/home/admin/validators_clock_state/validators_clock_cache.json",
  "security": {
    "allowed_hosts": ["104.238.222.200"],
    "allow_force_refresh": false,
    "max_connections": 128
  },
  "tls": {
    "enabled": true,
    "http_listen": "0.0.0.0:80",
    "https_listen": "0.0.0.0:443",
    "public_url": "https://104.238.222.200",
    "cert_path": "/home/admin/validators_clock_state/acme/fullchain.pem",
    "key_path": "/home/admin/validators_clock_state/acme/privkey.pem",
    "acme": {
      "enabled": true,
      "staging": true,
      "identifier": "104.238.222.200",
      "extra_identifiers": [],
      "account_path": "/home/admin/validators_clock_state/acme/account.json",
      "profile": "shortlived",
      "renew_after_seconds": 172800,
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

Example service file at `/etc/systemd/system/validators-clock.service`:

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

# Optional. The default is equivalent to warn,validators_clock=info.
# Environment=RUST_LOG=validators_clock=info

[Install]
WantedBy=multi-user.target
```

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
curl -I http://104.238.222.200
curl -I https://104.238.222.200
curl https://104.238.222.200/api/health
```

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
