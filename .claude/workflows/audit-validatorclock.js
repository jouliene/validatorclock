export const meta = {
  name: 'audit-validatorclock',
  description: 'Full-code audit: 10 finder lenses, adversarial verification, completeness critic',
  phases: [
    { title: 'Поиск', detail: '10 направлений по всему коду' },
    { title: 'Проверка', detail: 'состязательная верификация каждой находки' },
    { title: 'Полнота', detail: 'критик пропущенных углов + добор' },
  ],
}

const ROOT = '/home/neo/validatorclock'

const PREAMBLE = `You are one finder in a multi-agent audit of the codebase at ${ROOT} (Rust + axum single-binary server "validatorclock" with embedded frontend assets; TON/Everscale validator dashboard, public site vlckr.com). READ-ONLY: never modify, create, or delete any file. Read the actual code thoroughly with Read/Grep/Bash(cat,grep); do not rely on assumptions.

Report ONLY concrete defects you can substantiate by pointing at code: bugs, security holes, data loss, races, resource leaks, DoS vectors, broken error paths, wrong arithmetic, spec violations. NOT style, naming, or hypothetical "could be cleaner". Each finding needs: repo-relative file, exact 1-based line, severity, and a concrete failure scenario (inputs/state -> wrong outcome). If you find nothing real in your scope, return an empty list — do not pad.

Known deliberate design decisions — do NOT report these:
- /stats page is intentionally unlinked from the main page and protected by HTTP Basic auth over TLS.
- "A visitor is an IP address"; analytics keep no cookies/sessions by design.
- The Evercloud GraphQL project id lives only in the private production config, not in the repo.
- The basemap is a locally served pmtiles archive capped at zoom 10; the sprite was removed on purpose; basemap labels are intentionally dim.
- Single binary, embedded assets, no reverse proxy; ACME TLS built in.
- The recent map commit made node dots brighter than labels on purpose.
Severity guide: critical = remote crash/compromise/data corruption in default config; high = wrong results or outage under realistic conditions; medium = realistic edge-case failure or meaningful resource issue; low = minor but real defect.`

const FINDINGS = {
  type: 'object', required: ['findings'],
  properties: {
    findings: {
      type: 'array', maxItems: 12,
      items: {
        type: 'object',
        required: ['file', 'line', 'title', 'severity', 'description', 'failure_scenario'],
        properties: {
          file: { type: 'string' },
          line: { type: 'integer' },
          title: { type: 'string' },
          severity: { enum: ['critical', 'high', 'medium', 'low'] },
          description: { type: 'string' },
          failure_scenario: { type: 'string' },
          fix_sketch: { type: 'string' },
        },
      },
    },
  },
}

const VERDICT = {
  type: 'object', required: ['refuted', 'reasoning'],
  properties: {
    refuted: { type: 'boolean' },
    reasoning: { type: 'string' },
    severity_should_be: { enum: ['critical', 'high', 'medium', 'low', 'unchanged'] },
  },
}

const ANGLES = {
  type: 'object', required: ['angles'],
  properties: {
    angles: {
      type: 'array', maxItems: 4,
      items: {
        type: 'object', required: ['title', 'prompt'],
        properties: { title: { type: 'string' }, prompt: { type: 'string' } },
      },
    },
  },
}

const DIMENSIONS = [
  {
    key: 'http-security',
    prompt: `Lens: HTTP surface security. Scope: src/server/security.rs, src/server/basemap.rs, src/server/routes.rs, src/server/responses.rs, src/server/assets/mod.rs, src/server/api/*.rs, src/config/security.rs. Hunt: auth bypass on /stats (route coverage — is EVERY stats-ish path guarded? try to construct an unguarded alias), Basic-auth parsing flaws, timing side channels that actually matter, header injection, path traversal in basemap asset serving (percent_decode edge cases: double-encoding, %2e%2e, %2f, backslash, NUL, non-UTF8), CSP/security-header gaps that enable a real attack given the embedded pages, cache-poisoning via Vary/ETag interplay, information leaks in API responses (do public APIs leak visitor IPs or internal paths?).`,
  },
  {
    key: 'serving-dos',
    prompt: `Lens: resource exhaustion and blocking in the serving path. Scope: src/server/basemap.rs, src/server/connection.rs, src/server/conditional.rs, src/server/mod.rs, src/server/acme.rs, src/http.rs. Hunt: synchronous file I/O inside async handlers (the pmtiles archive is 3.5 GB — what does a Range request do to the tokio runtime?), per-request memory (read_to_end of how big a file? MAX_RANGE_BYTES semantics; what happens on a request with no Range for tiles.pmtiles — does the server try to load 3.5 GB into RAM?), unbounded concurrent connections, slowloris beyond header_read_timeout, gzip of already-compressed bodies, ETag computed over how large a body and how often, timeout coverage (which handlers can outlive the 10 s request timeout?).`,
  },
  {
    key: 'state-persistence',
    prompt: `Lens: state persistence and counter integrity. Scope: src/state/*.rs (especially json_store.rs, visitors.rs, analytics.rs, cache.rs, history.rs, runtime.rs, acme.rs), src/fsutil.rs. Hunt: non-atomic writes / torn files on crash, snapshot-take vs write race (take_snapshot returns then writes outside the lock — can two snapshots race and an older one win?), counter drift between visitors and analytics (both derive from VisitorEvent — verify), prune logic dropping live data, MAX_VISITOR_RECORDS eviction correctness, load-error handling (corrupt JSON on disk: does the store silently start empty and later overwrite good data?), file permission of files that hold IPs or secrets.`,
  },
  {
    key: 'concurrency',
    prompt: `Lens: async/concurrency hygiene across the whole Rust codebase (src/). Hunt: a tokio Mutex held across an await that does I/O (visitors lock across geo lookups? refresh locks?), std::sync locks in async context, blocking calls (std::fs, reqwest::blocking, heavy CPU) on the runtime without spawn_blocking, deadlock orderings between the several stores, tasks spawned without shutdown handling, refresh scheduler races (two refreshes of the same chain concurrently?), signal handling and graceful shutdown gaps (is state flushed on SIGTERM?), OnceLock caching things that must not be cached forever (e.g. basemap style caches archive maxzoom at first request — what if the archive is installed AFTER server start, as install.sh ordering might allow?).`,
  },
  {
    key: 'time-arithmetic',
    prompt: `Lens: time and arithmetic correctness. Scope: src/timeutil.rs, src/state/visitors.rs (prune/window logic), src/history/retention.rs, src/history/window.rs, src/chain/round_stats.rs, src/decimal.rs, any day_index/day_string/parse_day_index users. Hunt: off-by-one in 30-day windows and retention floors, day-boundary behaviour around UTC midnight, u64/i64 casts that can wrap on weird clocks (system clock before epoch, clock jumping backwards — last_seen in the future makes saturating_sub zero: consequences?), leap seconds/DST assumptions, division by zero in stats/APR math, precision loss in decimal.rs, integer overflow in stake/reward aggregation (values are nano-tokens — do sums fit u64/u128?).`,
  },
  {
    key: 'geo-pipeline',
    prompt: `Lens: geo pipeline correctness and trust boundaries. Scope: src/node_locations/*.rs, src/geoip.rs, src/visitor_geo.rs. Hunt: untrusted upstream data (ip-api, ipinfo, ipwho.is) flowing unvalidated into stored state and then into JSON APIs (country/city/isp strings — length limits? control characters? are they later placed into the frontend DOM anywhere via non-text sinks?), lat/lon validity (NaN/inf filtered everywhere or only in tiebreak?), batch lookup error handling that poisons the cache, TTL logic that hammers external APIs when a lookup keeps failing (retry storm against ip-api rate limits), conflict resolution writing wrong country when sources disagree in a new way, manual_review file handling races.`,
  },
  {
    key: 'chain-upstream',
    prompt: `Lens: chain refresh and upstream parsing. Scope: src/chain/**/*.rs (refresh.rs, scheduler, elector/*, validator_sources/**, graphql_client.rs, toncenter_client.rs, rpc_retry.rs, dto.rs, util.rs). Hunt: panics on malformed upstream JSON/BOC (unwrap/expect/indexing on network data), unbounded response bodies read into memory, missing timeouts on any request path, retry logic that can stampede a dying endpoint, fallback logic (broxus -> toncenter) that can serve stale or mixed-round data as fresh, elector stack parsing edge cases (empty validator set, duplicate keys, absurd stake values), wallet_index correctness, round id/time math errors that would show a wrong countdown on the public clock.`,
  },
  {
    key: 'frontend-xss',
    prompt: `Lens: frontend injection and DOM safety. Scope: public/shared/dom.js, public/shared/analytics_client.js, public/stats.js, public/app/map_popups.js, public/app/validator_tooltips.js, public/app/validator_copy.js, public/app/format_addresses.js, and grep ALL of public/ for innerHTML/outerHTML/insertAdjacentHTML/document.write/setAttribute with dynamic values/javascript: URLs/on* attributes. Untrusted inputs to consider: geo strings (country/city/ISP from third-party geo APIs), validator metadata from chain, jokes.json, query params, location.hash. Also: does stats.js render IPs/ISP strings via text nodes only? Any URL built from data (href/src) without validation? Clipboard API misuse?`,
  },
  {
    key: 'frontend-logic',
    prompt: `Lens: frontend logic, races and lifecycle. Scope: public/app/*.js (especially map_render.js, map_controls.js, map_data.js, map_events.js, map_features.js, state.js, runtime*.js, api.js, rounds*.js, node_stats*.js), public/stats.js, public/app.js ordering (APP_JS_PARTS). Hunt: concurrent loadValidatorMap calls racing (double addSource/addLayer -> maplibre throws), chain switch while map loading (features of chain A drawn on chain B), setInterval/poll loops that stack after errors or tab sleep, event listeners leaking on re-render, unhandled promise rejections that stop polling forever, stale closure over state, error paths that leave the UI stuck on "Loading", fetch without timeout/abort, JSON parse of failed responses, race between injected cluster source updates and refresh timer.`,
  },
  {
    key: 'ops-config',
    prompt: `Lens: ops scripts and configuration. Scope: install.sh, update.sh, scripts/install_basemap.sh, scripts/build_basemap_style.py, scripts/migrate_to_validatorclock.sh, src/config/*.rs, src/main.rs, src/logging.rs, src/tls/*.rs. Hunt: shell quoting/word-splitting bugs with paths containing spaces, curl without failure handling leaving partial files that later pass the "already installed" check (pmtiles .part rename — what if extract is interrupted mid-write after some bytes? does installed_max_zoom accept a truncated archive?), update.sh failure modes that leave the service down, config parsing that silently drops unknown/misspelled keys (a typo in "basemap_dir" — noticed or ignored?), env override handling, secrets/permissions (stats password hash storage, ACME account key, config file modes), systemd/service assumptions, TLS cert renewal edge cases (expiry during downtime, ACME rate limits on restart loop).`,
  },
]

const LENSES = [
  'CLAIM ACCURACY: read the cited code and its callers. Does the code actually behave as the finding claims? Check the exact types, guards, and control flow.',
  'REACHABILITY: can the failure scenario actually occur in a real deployment? Consider default config values, how the function is called in production, what an external attacker/user can actually control, and whether tests or invariants elsewhere prevent the state.',
  'MITIGATION & INTENT: is this already handled or mitigated elsewhere in the codebase, or an explicitly documented design decision (comments, docs, tests that assert this behaviour on purpose)? Would the "fix" break something intended?',
]

const VOTE_COUNT = { critical: 3, high: 3, medium: 2, low: 1 }
const RANK = { critical: 0, high: 1, medium: 2, low: 3 }

function verifyPrompt(f, lens) {
  return `You are an adversarial verifier in a code audit of ${ROOT}. READ-ONLY: never modify any file. A finder reported this:

FILE: ${f.file}
LINE: ${f.line}
SEVERITY CLAIMED: ${f.severity}
TITLE: ${f.title}
DESCRIPTION: ${f.description}
FAILURE SCENARIO: ${f.failure_scenario}

Your job is to REFUTE it. Lens: ${lens}

Read the actual code (the cited location, its callers, related tests) before deciding. Rules: refuted=true if the claimed defect does not exist, cannot actually happen, is an explicitly intended behaviour, or the finding materially misdescribes the code. refuted=false ONLY if you verified the defect is real by reading the code. If you cannot confirm it, default to refuted=true and say why. Also judge severity: set severity_should_be if the claimed severity is wrong (e.g. real but exotic -> lower).`
}

function dedupKey(f) {
  return `${f.file}:${Math.floor((f.line || 0) / 15)}`
}

function dedupe(findings, seen) {
  const fresh = []
  for (const f of findings) {
    const k = dedupKey(f)
    const prev = seen.get(k)
    if (prev) {
      if (RANK[f.severity] < RANK[prev.severity]) { prev.severity = f.severity }
      continue
    }
    seen.set(k, f)
    fresh.push(f)
  }
  return fresh
}

async function verifyBatch(findings, phaseName) {
  const enriched = await parallel(findings.map(f => () =>
    parallel(LENSES.slice(0, VOTE_COUNT[f.severity] || 1).map(lens => () =>
      agent(verifyPrompt(f, lens), {
        phase: phaseName,
        label: `verify:${(f.file || '').split('/').pop()}:${f.line}`,
        schema: VERDICT,
      })
    )).then(votes => ({ ...f, votes: votes.filter(Boolean) }))
  ))
  const survivors = []
  const killed = []
  for (const f of enriched.filter(Boolean)) {
    const total = f.votes.length
    const refutes = f.votes.filter(v => v.refuted).length
    if (total === 0 || refutes * 2 >= total + 1) {
      killed.push({ file: f.file, line: f.line, title: f.title, severity: f.severity,
        reasons: f.votes.filter(v => v.refuted).map(v => v.reasoning) })
      continue
    }
    const adjust = f.votes.map(v => v.severity_should_be).filter(s => s && s !== 'unchanged')
    if (adjust.length) {
      adjust.sort((a, b) => RANK[b] - RANK[a])
      f.adjusted_severity = adjust[0]
    }
    f.verdict = refutes === 0 ? 'CONFIRMED' : 'PLAUSIBLE'
    f.vote_summary = f.votes.map(v => (v.refuted ? 'REFUTE: ' : 'CONFIRM: ') + v.reasoning)
    delete f.votes
    survivors.push(f)
  }
  return { survivors, killed }
}

// ---- Round 1: ten finder lenses -------------------------------------------
const found = await parallel(DIMENSIONS.map(d => () =>
  agent(`${PREAMBLE}\n\n${d.prompt}`, { phase: 'Поиск', label: `find:${d.key}`, schema: FINDINGS })
))
const seen = new Map()
const round1 = dedupe(
  found.filter(Boolean).flatMap(r => r.findings || []).filter(f => f && f.file),
  seen,
)
round1.sort((a, b) => RANK[a.severity] - RANK[b.severity])
log(`Найдено ${round1.length} уникальных находок (${found.filter(Boolean).length}/10 файндеров ответили)`)

const CAP = 48
let toVerify = round1
if (round1.length > CAP) {
  toVerify = round1.slice(0, CAP)
  log(`ВНИМАНИЕ: проверяются только ${CAP} самых серьёзных из ${round1.length}; отброшено ${round1.length - CAP} (низший приоритет)`)
}
const r1 = await verifyBatch(toVerify, 'Проверка')
log(`Проверка: ${r1.survivors.length} выжило, ${r1.killed.length} опровергнуто`)

// ---- Round 2: completeness critic + targeted follow-up finders ------------
const criticPrompt = `${PREAMBLE}

You are the completeness critic of this audit. Ten finders already swept these lenses: ${DIMENSIONS.map(d => d.key).join(', ')}.
Surviving findings so far (title @ file:line):
${r1.survivors.map(f => `- [${f.adjusted_severity || f.severity}] ${f.title} @ ${f.file}:${f.line}`).join('\n') || '- none'}

Look at the repository structure yourself and name what the sweep most plausibly MISSED: subsystems no lens read carefully, failure classes nobody hunted (e.g. TLS/ACME renewal, embedded-asset/version pipeline, history participation math, validator_map matching, logging, main.rs bootstrap, the stats frontend, CSS-level clickjacking, dependency pinning), or cross-cutting interactions between subsystems. Return up to 4 additional finder prompts, each naming concrete files and a sharp defect-hunting lens. Only propose angles genuinely likely to yield real defects; fewer is fine.`

const critic = await agent(criticPrompt, { phase: 'Полнота', label: 'critic', schema: ANGLES })
let r2 = { survivors: [], killed: [] }
const extraAngles = (critic && critic.angles) || []
if (extraAngles.length) {
  log(`Критик предложил ${extraAngles.length} доп. углов: ${extraAngles.map(a => a.title).join('; ')}`)
  const extraFound = await parallel(extraAngles.map((a, i) => () =>
    agent(`${PREAMBLE}\n\n${a.prompt}`, { phase: 'Полнота', label: `find-extra:${i}:${a.title.slice(0, 30)}`, schema: FINDINGS })
  ))
  const round2 = dedupe(
    extraFound.filter(Boolean).flatMap(r => r.findings || []).filter(f => f && f.file),
    seen,
  )
  round2.sort((a, b) => RANK[a.severity] - RANK[b.severity])
  log(`Добор: ${round2.length} новых находок`)
  if (round2.length) {
    r2 = await verifyBatch(round2.slice(0, 24), 'Полнота')
    log(`Проверка добора: ${r2.survivors.length} выжило, ${r2.killed.length} опровергнуто`)
  }
}

const survivors = [...r1.survivors, ...r2.survivors]
survivors.sort((a, b) => RANK[a.adjusted_severity || a.severity] - RANK[b.adjusted_severity || b.severity])

return {
  survivors,
  killed: [...r1.killed, ...r2.killed],
  stats: {
    finders: DIMENSIONS.length + extraAngles.length,
    raw_findings: round1.length,
    extra_findings: r2.survivors.length + r2.killed.length,
    confirmed: survivors.filter(f => f.verdict === 'CONFIRMED').length,
    plausible: survivors.filter(f => f.verdict === 'PLAUSIBLE').length,
    refuted: r1.killed.length + r2.killed.length,
  },
}