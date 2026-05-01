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
