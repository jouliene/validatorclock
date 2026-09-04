export const meta = {
  name: 'audit-validatorclock-full',
  description: 'Full audit: 12 lenses (double depth on concurrency and HTTP safety), adversarial verification, completeness critic',
  phases: [
    { title: 'Поиск', detail: '12 направлений; concurrency и HTTP — по две линзы' },
    { title: 'Проверка', detail: 'состязательная верификация каждой находки' },
    { title: 'Полнота', detail: 'критик пропущенных углов' },
  ],
}

const ROOT = '/home/neo/validatorclock'
const BASE = '473b1dd'

const PREAMBLE = `You are one finder in a multi-agent audit of the codebase at ${ROOT}: a Rust + axum single-binary server ("validatorclock", ~20k lines of Rust, ~6.9k lines of browser JS) that serves a public TON/Everscale validator dashboard at https://validatorclock.xyz. Assets are embedded in the binary; TLS is built in via ACME; there is no reverse proxy.

READ-ONLY. Never modify, create or delete a file. Never start a long-running server. Read the real code with Read/Grep/Bash(cat, grep, git). Do not answer from assumption - open the file.

Report ONLY concrete defects you can point at in code: bugs, security holes, data loss, races, deadlocks, resource leaks, denial of service, broken error paths, wrong arithmetic, protocol violations, panics on hostile input. NOT style, naming, or "could be cleaner". Every finding needs a repo-relative file, an exact 1-based line, a severity, and a failure scenario naming concrete inputs or interleavings and the wrong outcome they produce. Find nothing rather than pad the list.

An earlier audit was cut short: several lenses never ran and NOTHING was ever verified. Assume nothing has been covered. Do not trust a comment that claims a problem is handled - check it.

ALREADY FIXED in the last six commits (do not re-report these as open, but DO look for mistakes in how they were fixed):
- Unbounded read of the tile archive; large basemap files are streamed now, small ones buffered.
- The entity-tag middleware buffered every body and emptied one it could not hold.
- Store files that do not parse are moved aside instead of overwritten; snapshots are numbered so an older one cannot land after a newer one; store writes moved to a blocking thread.
- Round history: one bad chain file no longer empties the others or blocks that chain's saves.
- TLS handshake timeout; ipinfo token kept out of logs.
- Front end: request deadlines, retryable CDN load, one map per container, nodes read at draw time, cluster click survives a vanished cluster.
- Geo: negative caching for ipinfo, settled conflicts stay settled, ISO-code agreement is not a conflict, third-party strings folded to one line and capped, coordinates range-checked.
- Installer: completion markers, basemap_dir no longer force-rewritten, systemd paths quoted.

DELIBERATE, not defects:
- /stats is unlinked from the site and behind HTTP Basic auth over TLS.
- "A visitor is an IP address"; no cookies, no sessions.
- The Evercloud project id lives only in the private production config.
- The basemap is a local pmtiles archive capped at zoom 10, with no sprite, and its labels are deliberately dim.

Severity: critical = remote crash, compromise or data corruption in the default configuration; high = wrong results or an outage under realistic conditions; medium = a realistic edge case or a meaningful resource problem; low = minor but real.`

const FINDINGS = {
  type: 'object', required: ['findings'],
  properties: {
    findings: {
      type: 'array', maxItems: 10,
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
    key: 'concurrency-locks',
    prompt: `Lens: locks, await points and cancellation. This lens NEVER RAN in the earlier audit - treat the whole Rust tree as unexamined. Scope: src/state/*.rs (runtime.rs, json_store.rs, visitors.rs, analytics.rs, cache.rs, history.rs, acme.rs, map_annotations.rs, validator_types.rs), src/server/connection.rs, src/chain/refresh.rs and refresh/scheduler.rs.

Hunt: a lock held across an await that does I/O; two locks taken in different orders anywhere in the tree (build the lock-order graph and look for a cycle - AppState holds several); a tokio Mutex held across spawn_blocking (json_store::Snapshot::write now does exactly this - is it safe, and can a slow disk stall every other writer of that store?); std::sync primitives used in async context; cancellation safety - a request future dropped mid-way (the per-request timeout in connection.rs DOES drop futures) leaving state half-updated, a snapshot taken but never written, a counter incremented without its pair; RwLock write starvation; anything that can deadlock if two chains refresh at once.`,
  },
  {
    key: 'concurrency-tasks',
    prompt: `Lens: task lifecycle, shutdown and cross-subsystem races. This lens NEVER RAN in the earlier audit. Scope: src/main.rs, src/server/mod.rs, src/chain/refresh.rs and refresh/scheduler.rs, src/tls/acme.rs, src/node_locations/mod.rs, src/state/runtime.rs.

Hunt: tokio::spawn with no handle and no shutdown path - what happens to in-flight state writes on SIGTERM, is anything flushed?; the refresh scheduler starting a second refresh of the same chain while the first is still running (prove it can or cannot); unbounded task spawning driven by request volume or by node count; JoinSet usage that drops results or swallows panics; a panic inside a spawned task killing only that task and leaving the system in a half state; the ACME renewal loop racing the serving path when it swaps the TLS acceptor; geo refresh and the map publication racing each other; work that keeps running after the thing that wanted it is gone.`,
  },
  {
    key: 'http-surface',
    prompt: `Lens: HTTP request surface and authorisation. Scope: src/server/routes.rs, security.rs, responses.rs, api/*.rs, assets/mod.rs, config/security.rs.

Hunt: any path that reaches stats data without passing require_stats_auth (enumerate every route and every middleware layer, and check the layer ORDER in app_router - a layer attached to the wrong router or after with_state may not run); Basic auth comparison and parsing (padding, unicode, absurdly long header, missing colon); enforce_allowed_host bypass (Host vs :authority, port, trailing dot, absolute-form request target); DefaultBodyLimit coverage - which POST bodies are unbounded?; CORS/OPTIONS handling; header injection through any value the caller controls; what the public JSON endpoints expose (do they leak visitor addresses, internal paths, or the geo cache?); the analytics POST endpoint as an unauthenticated write into state.`,
  },
  {
    key: 'http-protocol',
    prompt: `Lens: HTTP protocol handling and the connection layer. Scope: src/server/connection.rs, conditional.rs, basemap.rs, mod.rs, acme.rs, and the tower-http compression layer wired in routes.rs.

Hunt: the newly streamed basemap body - a client that disconnects mid-stream, a Content-Length set by hand that disagrees with what is actually sent, interaction between the manual Content-Length and CompressionLayer (which changes the body length), chunked vs length framing, HEAD requests on a streamed route; Range parsing against RFC 7233 (multiple ranges, suffix ranges "bytes=-500", start beyond EOF, overflow in start+MAX_RANGE_BYTES); entity tags and 304s (does a 304 keep headers it must not, can two different bodies collide in a 64-bit FNV tag, is the tag stable across restarts); connection limits, keep-alive lifetime, header read timeout, TLS handshake timeout - can any of them be held open cheaply; compression applied to a gigabyte-scale body as a CPU amplifier.`,
  },
  {
    key: 'chain-elector',
    prompt: `Lens: elector and RPC parsing under hostile or broken upstream data. This lens NEVER RAN in the earlier audit - treat it as unexamined. Scope: src/chain/elector.rs, elector/*.rs (election, frozen, graphql, snapshot, toncenter, toncenter_stack, toncenter_stack/parse), graphql_client.rs, toncenter_client.rs, rpc_retry.rs, dto.rs, util.rs.

Hunt: any unwrap/expect/panic/slice-index/integer-cast reachable from network data (the upstream is a third party and can return anything); unbounded response bodies read into memory, missing timeouts on any request path; BOC/cell parsing errors that panic rather than return; empty or absurd validator sets (zero validators, duplicate public keys, stake values at u64::MAX); retry logic that stampedes a failing endpoint or retries a non-idempotent call; the fallback from one provider to another serving mixed or stale data as if fresh; election timing arithmetic that could show a wrong countdown on the public clock.`,
  },
  {
    key: 'chain-sources',
    prompt: `Lens: validator sources and the refresh pipeline. This lens NEVER RAN in the earlier audit. Scope: src/chain/validator_sources/**/*.rs (mod, provider and its graphql/jrpc/toncenter backends, wallet_index, wallet_tasks, nominator_pool_sources, single_nominator_sources, proxy_sources, whales_pool_proxy_sources, hipo_validator_proxy_sources, st_ever_strategy_sources, validator_controller_sources, contract_types), src/chain/refresh.rs, src/chain/round_stats.rs, src/validator_map/*.rs, src/validator_types.rs.

Hunt: address parsing and matching that can attribute a stake or a reward to the wrong validator; wallet_index lookups that are O(n^2) or unbounded in node count; contract type detection by code hash that can misclassify; a source that fails silently and leaves a validator with wrong or missing data presented as fact; per-refresh work that grows with the validator set and could outrun the refresh interval; caching that mixes chains.`,
  },
  {
    key: 'new-code',
    prompt: `Lens: the six commits of fixes just landed, reviewed as new code. Read the diff yourself: \`git -C ${ROOT} diff ${BASE}..HEAD\` and \`git -C ${ROOT} log --oneline ${BASE}..HEAD\`.

These changes are the freshest and least exercised code in the tree, so a defect introduced here is the most likely one to be real. Look hard at: json_store's new numbered snapshots and blocking-thread writes (ordering under real concurrency, a snapshot dropped because a newer one was taken but never written, the write mutex as a bottleneck or a stall); basemap streaming (the hand-set Content-Length, the buffered/streamed threshold, error paths); the conditional middleware's new size check (does it read the size hint correctly for every body shape, can a body now escape tagging that used to be tagged and matter); history's split between "does not parse" and "cannot be read"; geo's new fields and their migration from existing on-disk cache files (old files lack them - what do they deserialise to, and does that change behaviour?); the front-end changes (the in-flight guard in loadValidatorMap, the boot finally block, AbortSignal support, the cluster-click fallback).`,
  },
  {
    key: 'state-integrity',
    prompt: `Lens: state on disk and counter integrity. Scope: src/state/*.rs, src/history/**/*.rs, src/fsutil.rs, src/node_locations/geo_cache.rs and manual_review.rs.

Hunt: any remaining path that can lose or corrupt data on crash, restart or concurrent write; write_file_atomic's temp-file naming and cleanup, permissions on files holding IP addresses; the round-history file lock (its stale-lock removal is a TOCTOU - can two processes hold it, and what does that do to the data?); retention and pruning arithmetic that can drop live rounds or keep dead ones forever; counters that can drift between the visitor store and the analytics summary; unbounded growth of any file (geo cache, manual review directory, visitor records); migration of on-disk formats written by an older version of this binary.`,
  },
  {
    key: 'time-money',
    prompt: `Lens: time and value arithmetic. Scope: src/timeutil.rs, src/decimal.rs, src/chain/round_stats.rs, src/history/window.rs, retention.rs, participation.rs, stats.rs, src/state/visitors.rs, src/state/analytics.rs, and the round/election timing in src/chain/elector*.

Hunt: overflow or truncation in stake and reward sums (values are nano-tokens; do they fit the types used, and are u64/i64/f64 casts lossy?); division by zero or by a near-zero denominator in APR and percentage math; precision loss in decimal formatting that misreports a balance; off-by-one in day and round windows; behaviour when the system clock jumps backwards or forwards (records dated in the future, saturating_sub hiding it); UTC day boundaries; election countdown arithmetic that can go negative or wrap; anything where a wrong number would be shown to the public as fact.`,
  },
  {
    key: 'geo-trust',
    prompt: `Lens: the geo pipeline as a trust boundary. Scope: src/geoip.rs, src/visitor_geo.rs, src/node_locations/*.rs.

Hunt: what a hostile or broken third-party geo API can still do - the strings are folded and capped now, but check every field that bypasses that path, and check the JSON shapes accepted (arrays, nested objects, numbers as strings); the manual review and manual resolved directories as an input the server trusts from disk (path handling, unbounded file count, a file an operator wrote by hand); ip-api answers keyed by an address the server did not ask about; unbounded concurrency in lookups against node count; the geo cache growing without limit; addresses that should never be sent to a third party (private, reserved, the server's own) reaching a lookup; whether a visitor's address can end up in a file or an API response that is not behind auth.`,
  },
  {
    key: 'frontend',
    prompt: `Lens: the browser code, both injection and logic. Scope: public/shared/dom.js, public/shared/analytics_client.js, public/stats.js, public/app.js and all of public/app/*.js. Grep the whole of public/ for innerHTML, outerHTML, insertAdjacentHTML, document.write, setAttribute with a dynamic name or value, href/src built from data, and eval-like calls.

Untrusted inputs: geo strings from third-party APIs, validator metadata from the chain, jokes.json, the URL (query and hash), and anything the /stats table renders. Hunt: any sink that is not a text node; a URL built from data without a scheme check; state that gets stuck (a flag set before the work it guards succeeds, a promise cached after rejection, a poll that stops after an error, a listener added on every render); a chain switch racing an in-flight request so one chain's data lands on another's view; timers that stack or leak; unhandled rejections that break a later interaction; the analytics beacon firing in ways that inflate counts.`,
  },
  {
    key: 'ops-tls',
    prompt: `Lens: deployment, configuration and TLS. Scope: install.sh, update.sh, scripts/*.sh, scripts/build_basemap_style.py, src/main.rs, src/logging.rs, src/config/*.rs, src/tls/*.rs, src/server/acme.rs, src/state/acme.rs.

Hunt: shell quoting and word splitting with paths that contain spaces; a script that leaves the service down or half-installed on failure; update.sh failure modes; config parsing that silently drops a misspelled key, or accepts a value that makes the binary refuse to start; file permissions and ownership for the config, the ACME account key and the certificate; ACME correctness - renewal timing, what happens when Let's Encrypt is unavailable or rate-limits, the challenge store's lifetime, whether a failed renewal can take the site down or spin a restart loop; certificate parsing and the acceptor swap; anything logged that should not be (tokens, keys, visitor addresses).`,
  },
]

const LENSES = [
  'CLAIM ACCURACY: open the cited code and its callers. Does it actually behave as the finding says? Check the exact types, the guards, the control flow, and any test that pins the behaviour.',
  'REACHABILITY: can this failure actually happen in a real deployment? Consider the default configuration, how the function is reached in production, what an outsider can actually control, and whether an invariant elsewhere already prevents the state.',
  'MITIGATION AND INTENT: is this already handled elsewhere, or an explicitly intended behaviour documented in a comment, a doc or a test that asserts it on purpose? Would the proposed fix break something that is deliberate?',
]

const RANK = { critical: 0, high: 1, medium: 2, low: 3 }
const VERIFY_CAP = 16

function verifyPrompt(f, lens) {
  return `You are an adversarial verifier in a code audit of ${ROOT}. READ-ONLY: never modify a file. A finder reported this:

FILE: ${f.file}
LINE: ${f.line}
SEVERITY CLAIMED: ${f.severity}
TITLE: ${f.title}
DESCRIPTION: ${f.description}
FAILURE SCENARIO: ${f.failure_scenario}

Your job is to REFUTE it. Lens: ${lens}

Read the actual code - the cited lines, the callers, the related tests - before deciding. Set refuted=true if the defect does not exist, cannot happen in practice, is intended behaviour, or the finding materially misdescribes the code. Set refuted=false ONLY where you confirmed the defect is real by reading the code, and say what convinced you. If you cannot confirm it either way, refute it and say why. Judge the severity too: set severity_should_be when the claim is real but rated wrong.`
}

function dedupe(findings, seen) {
  const fresh = []
  for (const f of findings) {
    const key = `${f.file}:${Math.floor((f.line || 0) / 15)}`
    const prev = seen.get(key)
    if (prev) {
      if (RANK[f.severity] < RANK[prev.severity]) prev.severity = f.severity
      continue
    }
    seen.set(key, f)
    fresh.push(f)
  }
  return fresh
}

// Votes by rank, not by claimed severity: the loudest claims get the most scrutiny
// without letting a wave of "high" ratings blow the agent budget.
function votesFor(index) {
  if (index < 5) return 3
  if (index < 11) return 2
  return 1
}

async function verifyBatch(findings, phaseName) {
  const enriched = await parallel(findings.map((f, index) => () =>
    parallel(LENSES.slice(0, votesFor(index)).map(lens => () =>
      agent(verifyPrompt(f, lens), {
        phase: phaseName,
        label: `verify:${(f.file || '').split('/').pop()}:${f.line}`,
        schema: VERDICT,
      })
    )).then(votes => ({ ...f, votes: votes.filter(Boolean) }))
  ))

  const survivors = []
  const refuted = []
  for (const f of enriched.filter(Boolean)) {
    const total = f.votes.length
    const against = f.votes.filter(v => v.refuted).length
    if (total === 0) {
      f.verdict = 'UNVERIFIED'
      f.vote_summary = ['no verifier returned a verdict']
      delete f.votes
      survivors.push(f)
      continue
    }
    if (against * 2 >= total + 1) {
      refuted.push({
        file: f.file, line: f.line, title: f.title, severity: f.severity,
        reasons: f.votes.filter(v => v.refuted).map(v => v.reasoning),
      })
      continue
    }
    const adjust = f.votes.map(v => v.severity_should_be).filter(s => s && s !== 'unchanged')
    if (adjust.length) {
      adjust.sort((a, b) => RANK[b] - RANK[a])
      f.adjusted_severity = adjust[0]
    }
    f.verdict = against === 0 ? 'CONFIRMED' : 'PLAUSIBLE'
    f.vote_summary = f.votes.map(v => (v.refuted ? 'REFUTE: ' : 'CONFIRM: ') + v.reasoning)
    delete f.votes
    survivors.push(f)
  }
  return { survivors, refuted }
}

phase('Поиск')
const found = await parallel(DIMENSIONS.map(d => () =>
  agent(`${PREAMBLE}\n\n${d.prompt}`, { phase: 'Поиск', label: `find:${d.key}`, schema: FINDINGS })
))
const answered = found.filter(Boolean).length
const seen = new Map()
const all = dedupe(
  found.filter(Boolean).flatMap(r => r.findings || []).filter(f => f && f.file && f.severity),
  seen,
)
all.sort((a, b) => RANK[a.severity] - RANK[b.severity])
log(`${answered}/${DIMENSIONS.length} направлений ответили, ${all.length} уникальных находок`)

let toVerify = all
if (all.length > VERIFY_CAP) {
  toVerify = all.slice(0, VERIFY_CAP)
  const dropped = all.slice(VERIFY_CAP)
  log(`ВНИМАНИЕ: проверяются ${VERIFY_CAP} самых серьёзных из ${all.length}. НЕ проверены (${dropped.length}): ${dropped.map(f => `[${f.severity}] ${f.file}:${f.line} ${f.title}`).join(' | ')}`)
}

phase('Проверка')
const checked = await verifyBatch(toVerify, 'Проверка')
log(`Проверка: ${checked.survivors.length} выжило, ${checked.refuted.length} опровергнуто`)

phase('Полнота')
const critic = await agent(`${PREAMBLE}

You are the completeness critic. Twelve lenses swept the tree: ${DIMENSIONS.map(d => d.key).join(', ')}.

Findings that survived verification:
${checked.survivors.map(f => `- [${f.adjusted_severity || f.severity}] ${f.title} @ ${f.file}:${f.line}`).join('\n') || '- none'}

Findings that were refuted:
${checked.refuted.map(f => `- [${f.severity}] ${f.title} @ ${f.file}:${f.line}`).join('\n') || '- none'}

Look at the repository yourself - the module tree, the files nobody cited, the tests - and name what this sweep most plausibly MISSED: a subsystem no lens read carefully, a failure class nobody hunted, an interaction between two subsystems that only shows up when both are considered at once, or a place where the tests look like they cover something but do not. Be concrete about files. Return up to 4 finder prompts worth running next, or fewer - an empty list is a real answer if the sweep looks complete.`,
  { phase: 'Полнота', label: 'critic', schema: ANGLES })

const survivors = checked.survivors
survivors.sort((a, b) => RANK[a.adjusted_severity || a.severity] - RANK[b.adjusted_severity || b.severity])

return {
  survivors,
  refuted: checked.refuted,
  unverified: all.length > VERIFY_CAP ? all.slice(VERIFY_CAP) : [],
  next_angles: (critic && critic.angles) || [],
  stats: {
    lenses_run: answered,
    lenses_total: DIMENSIONS.length,
    raw_findings: all.length,
    verified: toVerify.length,
    confirmed: survivors.filter(f => f.verdict === 'CONFIRMED').length,
    plausible: survivors.filter(f => f.verdict === 'PLAUSIBLE').length,
    refuted: checked.refuted.length,
  },
}