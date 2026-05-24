# Validators Clock Optimization Plan

This branch is for cleanup and modularization only. Keep behavior changes out of the
cleanup commits unless a bug is found during verification.

## Completed On `cleaning`

- Split server API handlers, static asset serving, and chain config handling into focused modules.
- Split history/state tests into module-level files and added shared test helpers where useful.
- Split frontend runtime, clock, round rendering, validator rendering, validator type/source helpers, map rendering, map feature grouping, map popup state, and formatting helpers into smaller files.
- Split chain refresh background scheduling/logging out of the cache-first snapshot path.
- Split validator source provider implementations into separate JRPC and TON Center modules.
- Split TON Center stack parsing helpers out of election/round data assembly.
- Split fake-validator map annotation policy out of map-node annotation.
- Removed stale validator source fake styles after fake-node rendering moved to `is-map-fake`.
- Added section markers to `public/styles.css` so future CSS changes can stay scoped.

## Working Rules

- Use small commits with one refactor topic per commit.
- Run `cargo fmt`, `cargo test`, `cargo clippy`, JS syntax checks, and `scripts/visual-check.sh` when behavior or layout can be affected.
- Do not hard-code resolver timing assumptions into backend logic. TON/Everscale/Tycho resolver cadence is deployment configuration and may change.
- Preserve `/styles.css` and `/app.js` public URLs unless the deployment pipeline is updated at the same time.
- Avoid touching tracked runtime data such as `validators_clock.json` unless the task explicitly requires it.

## Remaining Work

1. Finish CSS audit with targeted cleanup only:
   - keep `styles.css` behavior stable;
   - remove only verified-dead selectors;
   - consider CSS modularization later if the asset embedding path is updated safely.
2. Re-audit Rust backend hotspots:
   - `src/history/store.rs`.
3. Revisit scripts only after frontend/backend code is stable:
   - `scripts/collect-tycho-map.sh`;
   - `scripts/visual-check.sh`;
   - deployment/install scripts only with extra caution.
4. Improve CI coverage:
   - add JS syntax checking for all `public/app/*.js`;
   - consider a non-browser static check for embedded asset order;
   - keep full visual checks local unless CI has a reliable browser environment.
