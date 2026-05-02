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

Example production settings for `104.238.222.200`:

```json
{
  "listen": "127.0.0.1:8787",
  "refresh_seconds": 60,
  "cache_path": "/var/lib/validators_clock/validators_clock_cache.json",
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
    "cert_path": "/var/lib/validators_clock/acme/fullchain.pem",
    "key_path": "/var/lib/validators_clock/acme/privkey.pem",
    "acme": {
      "enabled": true,
      "staging": true,
      "identifier": "104.238.222.200",
      "account_path": "/var/lib/validators_clock/acme/account.json",
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
and HTTPS traffic. If systemd runs the service as a non-root user, add:

```ini
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
NoNewPrivileges=true
ReadWritePaths=/var/lib/validators_clock
```

The HTTP listener only serves ACME challenges and redirects all other requests to
`tls.public_url`. HTTPS responses include basic browser security headers, and
`security.allowed_hosts` rejects unexpected Host headers when configured.
