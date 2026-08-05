# Devloop Output: Negative-path browser E2E specs + pipeline integration (R-45, R-48)

**Date**: 2026-08-04
**Task**: Add three negative-path Playwright specs (auth-rejection, meeting-not-found, mc-token-rejection) and wire browser E2E into Layer 7 + verify-completion.sh
**Specialist**: test (paired with operations)
**Mode**: Agent Teams (v2)
**Branch**: `feature/user-story-run-test`
**Duration**: 2 sessions (2026-08-04 interrupted mid-implementation; resumed headless 2026-08-05, ~2.5h from resume to commit incl. four full-pipeline runs)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `e29aa615318f1f681c3cfcd49da6099d4551f128` |
| Branch | `feature/user-story-run-test` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |

**Gate 3 (Final Approval)** — PASSED 2026-08-05. All seven verdicts RESOLVED-FIXED (zero
deferrals, zero spin-outs, zero escalations):

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | token-value echo in auth-rejection sanity assert → boolean compare (same site as Semantic Guard) |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | stale-series flake-class tracking (TODO §Env-Test Resilience) + central churn hint in `pollUntilSumAbove` |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | mcMetrics header accuracy (churn-exposure caveat) + below-baseline triage hint |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 | import order; ADR-0033/0024/0028/0030 compliant; Ownership Lens clean, no GSA |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | `captureResponse` extraction; wire-path consts hoisted (fixtures.ts:83-88) |
| Operations (paired) | RESOLVED-FIXED | 2 | 2 | 0 | **Gate-3 owner hunk-ACK for all operations-owned Domain-judgment rows + Minor-judgment verify-completion.sh; §6.7 cite re-anchor explicitly ACKed**; env-tests follow-ups + SC2034 dead-args TODO-tracked (pre-existing debt, not diff findings) |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | credential-leak item 8 at auth-rejection.spec.ts:52 (deduplicated with Security) |

**Final validation (post-review-fixes tree)** — PASSED 2026-08-05: rerun of
`./scripts/layer-all.sh` against the tree including all review-round fixes. L1-5 OK, L6
N/A (documented proto gap; cargo-audit + pnpm-audit OK), L7 OK (env-tests passed, browser
E2E 6/6 `browser-e2e-passed`). Exit 0. No staged-vs-validated drift at commit.

**Lead close-out note**: user-story Devloop Tracking flipped for #19 (both status tables,
avoiding two-places drift); the factually stale #18 row in §Devloop Tracking (completed
2026-08-03, row still Pending) was also corrected — one-line factual fix per
fix-don't-defer.

**Gate 2 (Validation)** — PASSED 2026-08-05, attempt 3. Attempt 1: L3 FAIL (scope-drift
row + unresolvable bash-array cite; both fixed). Attempt 2: L1-6 green, L7 FAIL
(`test_mc_media_connection_update_increments_participant_mh_status_metric` — diagnosed
stale-series churn from cluster re-creation mid-rollout, 3/3 isolation passes, not the
diff; consumed L7 attempt 1 of 2). Attempt 3 (final L7 attempt): all layers OK — L6 N/A
is the documented proto intentional-gap (`not-applicable-to-this-lang`; cargo-audit +
pnpm-audit OK); L7 env-tests passed AND browser E2E 6/6 passed (`browser-e2e-passed`).

**Resume note (2026-08-05, headless)**: session interrupted mid-implementation; roster
respawned per SKILL.md §Recovery. Partial work found in tree at resume: `fixtures.ts`,
`mcMetrics.ts`, `layer7.sh`, `layer7.test.sh` modified; `auth-rejection.spec.ts` created.
Remaining per plan: `meeting-not-found.spec.ts`, `mc-token-rejection.spec.ts`, e2e
`README.md`, `verify-completion.sh`, SKILL.md Layer-7 docs, runbook rows, `docs/TODO.md`
note. Gate 1 remains valid (no re-plan).

**Gate 1 (Plan Approval)** — all reviewers confirmed 2026-08-04; classification-sanity guard `STATUS=OK REASON=cross-boundary-classification-clean-1-files`; "Plan approved" issued.

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations (paired) | confirmed (Gate-1 owner confirmation for ops-owned hunks per ADR-0024 §6.3) |
| Semantic Guard | confirmed |
| Implementer | `done` (2026-08-05) |
| Implementing Specialist | `test` |
| Iteration | `2` (validation attempts: L1-6 ×2, L7 flake-retry ×1, final post-review rerun ×1) |
| Security | RESOLVED-FIXED |
| Test | RESOLVED-FIXED |
| Observability | RESOLVED-FIXED |
| Code Quality | RESOLVED-FIXED |
| DRY | RESOLVED-FIXED |
| Operations (paired) | RESOLVED-FIXED (owner hunk-ACK) |
| Semantic Guard | RESOLVED-FIXED |

---

## Task Overview

### Objective
Story task #19 (headless run): land the secondary negative-path browser E2E specs (R-45) and integrate the browser E2E into the devloop validation pipeline (R-48).

1. Three new spec files in `packages/web-app/e2e/`, reusing the task #18 harness/fixtures:
   - `auth-rejection.spec.ts` — unauthenticated meeting-join surfaces a typed AuthError in the demo UI
   - `meeting-not-found.spec.ts` — GC 404 for unknown meeting code surfaces a typed error in the DOM
   - `mc-token-rejection.spec.ts` — real meeting token, garbage meeting_id to MC → Unauthorized → typed SignalingError
2. Pipeline: `scripts/layer7.sh` runs `pnpm --filter @darktower/web-app test:e2e` sequentially AFTER `cargo test -p env-tests --features all` against the same live Kind cluster (shared setup, shared single-attempt budget). Browser E2E triggered only when `packages/**` or `proto/**` changed or backend diff touches MC/AC/GC contract surface. `.claude/skills/devloop/SKILL.md` Layer 7 documentation updated accordingly (operations-owned; cross-boundary, hence --paired-with=operations).
3. `scripts/verify-completion.sh`: gate client `pnpm test:unit` + `pnpm test:component` under LAYER >= standard; browser E2E under LAYER == full.
4. STATUS= contract per `docs/runbooks/devloop-validation.md`: cluster-side precondition failure surfaces as PRECONDITION_FAILURE, never test FAIL, never silent skip.

### Scope
- **Service(s)**: client (web-app E2E), validation pipeline scripts, devloop skill doc
- **Schema**: No
- **Cross-cutting**: Yes — operations-owned SKILL.md + layer7.sh/verify-completion.sh pipeline scripts

### Debate Decision
NOT NEEDED — design already fixed by story plan, ADR-0033 (layer renumbering), and task #18's landed harness.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2 — one row per planned file change. `--paired-with=operations`: the
paired @operations teammate covers Domain-judgment rows on operations-owned surfaces
(their Gate-1 "Plan confirmed" + Gate-3 verdict are the hunk-ACKs). No GSA surfaces
touched.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `packages/web-app/e2e/auth-rejection.spec.ts` (new) | Mine | — |
| `packages/web-app/e2e/meeting-not-found.spec.ts` (new) | Mine | — |
| `packages/web-app/e2e/mc-token-rejection.spec.ts` (new) | Mine | — |
| `packages/web-app/e2e/fixtures.ts` (add shared negative-path helpers) | Mine | — |
| `packages/web-app/e2e/mcMetrics.ts` (add jwt_validation counter helper; extract shared poll) | Mine | — |
| `packages/web-app/e2e/join-happy-path.spec.ts` (comment-only: stale rate-limit claim fix found in resume audit) | Mine | — |
| `packages/web-app/e2e/README.md` (pipeline note, budget math, spec docs) | Mine | — |
| `scripts/layer7.sh` (Phase-1(g) + Phase-2 browser lane) | Domain-judgment | operations (paired) |
| `scripts/layer7.test.sh` (new hermetic browser-lane cases) | Domain-judgment (ops surface; test co-signs test quality) | operations (paired) |
| `scripts/verify-completion.sh` (comment/usage text only) | Minor-judgment | operations (paired) |
| `.claude/skills/devloop/SKILL.md` (Layer 7 docs + Lead-policy bullet) | Domain-judgment | operations (paired) |
| `docs/runbooks/devloop-validation.md` (§3/§6.7/§8 rows + changelog) | Domain-judgment | operations (paired) |
| `docs/TODO.md` (one-line #19 adjudication note on the sdk-core wire-path entry) | Minor-judgment | dry-reviewer (pre-adjudicated in their Gate-1 lens) |
| `docs/user-stories/2026-05-02-browser-client-join.md` (task #19 status flip at close) | Mechanical | team-lead |
| `docs/devloop-outputs/2026-08-04-e2e-negative-specs-pipeline/main.md` (this document) | Mine | — |

---

## Planning

### Problem restated in mechanism language

Task instance-language: three named negative-path specs; wire `pnpm test:e2e` into Layer 7
after the Rust env-tests; trigger on `packages/**`/`proto/**`/"MC/AC/GC contract surface";
gate client tests in verify-completion.sh.

Mechanism restatement produces a WIDER class than the task names in two places — both
surfaced for reviewer ruling:

1. **Trigger**: the mechanism is "run the browser E2E whenever the diff can change
   behavior the browser client observes". That class is wider than "MC/AC/GC": the
   happy-path spec asserts MH WebTransport handshakes (assertion (b)), so
   `crates/mh-service/` changes are observable; `crates/common/` (JWT/meeting-token shared
   types) and `crates/media-protocol/` (42-byte frame the client mirrors) are
   contract-carrying too. Proposal: include them — fail-toward-RUN (over-triggering costs
   minutes; under-triggering silently un-gates the browser tier). **Q4 below.**
2. **auth-rejection's "typed AuthError"**: the mechanism is "each hop's credential
   rejection surfaces that hop's typed error in the demo UI". Post task #58, join is
   token-based — an unauthenticated JOIN is rejected by **GC** with 401 →
   `MeetingUnauthorizedError` (code `MEETING`), whose demo-UI surface is the
   typed-error-driven session-drop (`isSessionRejection` fires ONLY on
   `MeetingUnauthorizedError`). The literal "`AuthError` in the demo UI" surface lives at
   the **AC** hop (sign-in rejection → `AuthUnauthorizedError`, code `AUTH`, rendered in
   `last-error`). The R-45 wording predates #58's token-join contract. The spec file
   covers BOTH hops. **Q6 below.**

### Part 1 — three spec files (`packages/web-app/e2e/`)

All specs reuse the #18 harness (`fixtures.ts`, `env.ts`, `mcMetrics.ts`, global-setup
preconditions); no hardcoded hosts/ports; no new wire-path literals outside `fixtures.ts`;
`retries: 0` and existing ceilings unchanged; all waits bounded and assertion-meaningful.

**Key observability constraint discovered up front** (drives the assertion design): a
join-time MC `ErrorMessage` rejects the pending `join()` promise and does NOT emit a bus
`error` event (`SignalingClient.#raise` pre-settle path settles the join instead of
emitting; `MeetingSession` re-emits only post-join signaling errors). So the negative-path
observables are: the `last-error` DOM surface (`errorText` renders `${code}: ${message}`),
HTTP response status captured via narrowly-scoped `waitForResponse`, typed-error-driven UI
behavior (session drop), and MC-side Prometheus counters. Each spec pins the *typed*
discriminant, not free text, and each is hardened against the "any error present"
false-pass — a dead cluster cannot green these specs (global-setup health probes gate
entry; each spec's discriminant is unreachable from a cluster-down state).

**`auth-rejection.spec.ts`** — 0 registrations:
- Test A, unauthenticated meeting-join (GC hop): `page.route`-**fulfill**
  `/api/v1/auth/register` with a synthetic `RegisterResponse` whose `accessToken` is a
  randomly generated, well-formed-but-invalid JWT-shaped token68 string (three
  `crypto.randomUUID()`-derived base64url segments — passes the SDK's local
  `validateUserToken` charset guard, never hardcoded, never hits AC, costs 0
  registrations). Drive `signUpViaUi` (works against the fulfilled route), then
  `joinAsUser` with a random well-formed 12-alnum meeting code. Assert:
  (1) the GC `GET /api/v1/meetings/<code>` response is exactly **401** via
  `waitForResponse` (discriminates from 429 rate-limit, 404, and dead-cluster);
  (2) the auth nav (`nav-signin`) reappears — the session-drop policy fires ONLY on
  `MeetingUnauthorizedError`, so this asserts the typed-error path, not generic failure;
  (3) no `joined` bus event (single point-in-time bus scan after the terminal condition —
  never a wait-for-absence).
- Test B, AC credential rejection (the literal typed-`AuthError`-in-demo-UI surface):
  sign-in with `randomCredentials` that were never registered → assert AC
  `/api/v1/auth/user/token` responded exactly **401** (not 429) AND `last-error` renders
  with the `AUTH:` code prefix (`AuthUnauthorizedError` via `errorText`). 0 registrations
  (sign-in only).

**`meeting-not-found.spec.ts`** — 1 registration:
- `signUpViaUi` (real user), `joinAsUser` with a random well-formed 12-alnum code that
  passes client `validateMeetingCode` AND GC's pre-DB-lookup regex, so the rejection is a
  real 404 lookup miss, not a 400. Assert: (1) GC join response exactly **404**;
  (2) `last-error` renders with the `MEETING:` prefix and STAYS rendered
  (`MeetingNotFoundError` is NOT a session rejection — additionally assert `nav-signin`
  does NOT appear, discriminating the 404 path from the 401 session-drop path);
  (3) no `joined` bus event.

**`mc-token-rejection.spec.ts`** — 1 registration:
- `signUpViaUi` → `bootstrapMeeting(token)` (Node-side GC create). `page.route` the GC
  join `GET /api/v1/meetings/<code>`: `route.fetch()` the real response, rewrite ONLY
  `meetingId` to `crypto.randomUUID()` (random garbage), fulfill. The SDK holds a REAL
  meeting token and real `mcAssignment` but sends MC a garbage `meeting_id`; MC's binding
  check (`crates/mc-service/src/webtransport/connection.rs` step 6) replies
  `ErrorCode::Unauthorized` with the fail-closed generic message and closes (auth-class).
  Assert: (1) `last-error` renders with the `SIGNALING:` prefix (the join promise rejects
  with a typed `SignalingError`); (2) **MC-side proof**:
  `mc_session_join_failures_total{error_type="jwt_validation"}` rises above a baseline
  captured immediately before the join, via a new `mcMetrics.ts` helper (same poll shape /
  60s budget as the existing helper; monotonic-counter `> baseline` is concurrency-safe).
  This is the false-pass killer: a dead/unreachable MC also yields `SIGNALING:` in the
  DOM (Transport class) but CANNOT increment MC's server-side rejection counter — the
  delta proves MC actively rejected the join at its auth gate; (3) no `joined` bus event;
  (4) only the bounded generic client surface is asserted (MC's client message is the
  generic "Invalid or expired token") — no dependence on rejection detail leaking
  (fail-closed contract preserved).
- Token hygiene: the real meeting token stays inside the routed response body (passed by
  value through `route.fulfill`); never logged, never interpolated into test titles or
  assertion messages (semantic-guard credential-leak item 8; #18 redaction conventions).

**`fixtures.ts` additions** (shared, not per-spec inlined): `randomMeetingCode()`,
`garbageToken68()`, `expectNoJoinedEvent(page)` (single point-in-time bus scan),
`expectLastErrorCode(page, codePrefix)` (bounded read reusing the `busFailureContext`
diagnostic on failure). `mcMetrics.ts` gains `mcSessionJoinFailureSum(errorType)` /
`waitForMcSessionJoinFailureAbove(...)` with the poll loop refactored into one internal
helper shared with the existing pair (no second copy of the poll idiom; PromQL literals
stay in this one module).

**Registration budget**: new specs add 0 + 1 + 1 = **2 registrations/run**; full suite
total = 4/run (with #18's 2). CORRECTION from @operations (Gate 1, verified against the
tree): the "5/hour" figure the #18 README hardcoded is AC's PRODUCTION default
(`crates/ac-service/src/config.rs` `DEFAULT_REGISTRATION_RATE_LIMIT_*`: 5 per 60 min);
the Kind cluster this suite actually targets ships
`infra/services/ac-service/configmap.yaml` `AC_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS:
"100"` per 1-minute window ("relaxed for Kind dev/test") — so there is no same-hour
re-run hazard and the 2-attempt Layer-7 retry math is safe. README fix folded in: replace
the drifted hardcoded 5/hour claim with a pointer at the configmap keys as SSoT, keep the
single-workstation caveat, and add one sentence that a prod-configured AC target gets the
5/hour behavior. The negative specs' 401-exact assertions cannot mistake a 429 in any
case. Alternative considered and NOT chosen: cross-spec shared-user via worker module
state (saves 1 registration but breaks on worker restart after a failing test — the
red-run case). **Q2: confirmed by @operations (4/run, isolated identities).**

### Part 2 — pipeline integration

**`scripts/layer7.sh`** (operations-owned; design pre-agreed with @operations' planning
message, their items 1-4):
- **Phase 1 gains step (g)** — browser-E2E trigger + preconditions, evaluated after the
  existing (a)-(f) steps, so AC/GC/Prometheus readiness is NOT re-checked (covered by
  1e/1f; cross-ref comments both directions):
  - `__browser_e2e_triggered()`: reuses `lang/ts/changed.sh` (the existing SSoT for
    "packages/ or TS root files touched") `|| diff_touches_path` on `proto/`,
    `crates/ac-service/`, `crates/gc-service/`, `crates/mc-service/`,
    `crates/mh-service/`, `crates/common/`, `crates/media-protocol/` (Q4). No second
    git-diff derivation — the existing changed-files cache is the single source.
  - Triggered → deterministic pre-suite checks, each `precondition_fail` (exit 2,
    operator lane): fingerprints JSON present with both `MC_CERT_SHA256` +
    `MH_CERT_SHA256` → `dev-certs-missing`; Chromium present under
    `PLAYWRIGHT_BROWSERS_PATH` (default `/opt/ms-playwright`, baked into the devloop
    image) → `playwright-browser-missing`. NO grepping of Playwright output ever — after
    Phase 1, any suite non-zero is FAIL (implementer lane), same retired-log-grep rule as
    env-tests. `global-setup.ts` keeps its own fail-loud checks unchanged (defense in
    depth; a residual throw lands as FAIL — the load-bearing uncertain→loud asymmetry).
- **Phase 2**: env-tests unchanged (`env-tests-passed`/`env-tests-failed`), then:
  - env-tests FAILED + browser triggered → browser suite NOT run; loud stderr note
    ("browser E2E not run: env-tests failed first — shared single-attempt; runs on the
    retry"); no browser STATUS line (layer already FAIL; attribution noise avoided —
    @operations' lean, adopted; **Q1**).
  - Not triggered → explicit child line
    `STATUS=SKIPPED-NO-DIFF REASON=browser-e2e-no-diff` (existing enum, ranks below OK —
    no ladder edits) + stderr note naming the trigger paths. Never an invisible
    if-branch.
  - Triggered + env-tests green → export `E2E_AC_URL`/`E2E_GC_URL`/`E2E_PROMETHEUS_URL`
    and `VITE_AC_PROXY_TARGET`/`VITE_GC_PROXY_TARGET` from the already-exported
    `ENV_TEST_*` values (one ports.json read, zero drift), then
    `timeout "$DEVLOOP_BROWSER_E2E_TIMEOUT"` (default **600s**, separate from
    `ENV_TEST_TIMEOUT` — the Rust budget is never eaten) run
    `pnpm --filter @darktower/web-app test:e2e`, teed to
    `${DEVLOOP_TMP}/layer-7-browser-e2e.log` (the `layer-*` prefix keeps it under
    layer-all.sh's per-run token-hygiene cleanup). OK → `browser-e2e-passed`; non-zero →
    stderr names the log AND the Playwright artifact dir
    (`packages/web-app/test-results/` — retained-on-failure traces, gitignored,
    local-only per the #18 README hygiene note; they persist OUTSIDE DEVLOOP_TMP cleanup,
    called out in the runbook) → `browser-e2e-failed`. Per-step durations via
    `emit_step_duration`.
- **Test seams** (DEVLOOP_TEST=1-gated, same trust boundary as the existing ones):
  `DEVLOOP_BROWSER_E2E_CMD`, `DEVLOOP_FINGERPRINTS_JSON`,
  `DEVLOOP_PLAYWRIGHT_BROWSERS_DIR`.
- **`scripts/layer7.test.sh`**: new hermetic cases — trigger-off → SKIPPED-NO-DIFF child
  line + layer OK; trigger-on + fake browser cmd fails → FAIL `browser-e2e-failed`
  (exit 1); fingerprints missing → PRECONDITION_FAILURE `dev-certs-missing` (exit 2);
  browsers dir missing → PRECONDITION_FAILURE `playwright-browser-missing` (exit 2);
  env-tests red + trigger-on → no browser STATUS line + the stderr note; both green → OK
  with both `-passed` REASONs collected. Trigger controlled by pre-writing the
  changed-files cache.

**`scripts/verify-completion.sh`** — near-zero diff: the requirement is already
structurally satisfied by delegation — `standard` = L1-4 and L4's `lang/ts/test.sh` runs
`nx affected -t test:unit test:component`; `full` = `layer-all.sh` = L1-7, which includes
the browser E2E once layer7.sh is wired. Only the header comment + usage text change,
naming where client unit/component (standard, via the L4 ts wrapper, affected-gated per
ADR-0033 §3) and browser E2E (full, via L7, diff-triggered) live. No parallel test
invocations added (SSoT: the layer scripts are the one encoding).

**`.claude/skills/devloop/SKILL.md`** (operations-owned): layer-table row 7 (stale
"Playwright `@smoke`: pending #18/#19" note replaced); the "Today Layer 7 runs only the
Rust env-tests" paragraph rewritten to describe the two sequential Phase-2 suites, with
`scripts/layer7.sh` remaining the mechanism SSoT (prose points at the script + runbook
§6.7; does not re-encode trigger paths/timeouts); Lead-policy bullet extended: the two
suites share the single Layer-7 attempt — browser FAIL consumes it, browser
PRECONDITION_FAILURE does not, browser suite is skipped when env-tests fail and runs on
the retry.

**`docs/runbooks/devloop-validation.md`** (operations-owned): §3 REASON-examples rows
(`browser-e2e-failed`, `browser-e2e-no-diff`, `dev-certs-missing`,
`playwright-browser-missing`); §6.7 Phase-2 description (two suites, trigger predicate,
env-tests-red skip note, artifact locations incl. the outside-DEVLOOP_TMP retention of
Playwright traces); §8 symptom-catalogue rows; changelog entry.

**`packages/web-app/e2e/README.md`**: "Manual invocation only" paragraph replaced with
the Layer 7 wiring description; budget math 2 → 4; the three negative specs added to the
what-is-asserted section.

**Known risk (flagged, not resolvable pre-run)**: the first in-container full-layer run
must confirm (a) the Vite dev server Playwright spawns honors the exported
`VITE_*_PROXY_TARGET` container URLs, and (b) in-container Chromium reaches the MC/MH
WebTransport advertise addresses (UDP/QUIC via the host gateway). Task #22
(infrastructure finalization) is the designated alignment pass; a reachability failure
surfaces loudly as spec timeouts naming the handshake, not silently.

### DRY adjudications

sdk-core wire-path constants (`docs/TODO.md` §Cross-Service Duplication, #18 entry): NOT
extracted here — same constraint as #18 (requires a client-owned `packages/sdk-core/**`
barrel edit; no client reviewer on this roster). This task adds NO new wire-path
literals: the specs observe paths only through `fixtures.ts` helpers and `waitForResponse`
predicates already centralized there. TODO entry gets a one-line note recording the #19
adjudication (defer trigger updated to "next sdk-core http-surface change").

### Reviewer asks accepted at Gate 1 (implementation checklist)

- **@operations**: rationale comment ON `__browser_e2e_triggered()` naming fail-toward-RUN;
  README rate-limit drift fix (point at `infra/services/ac-service/configmap.yaml`
  `AC_REGISTRATION_RATE_LIMIT_*` as SSoT — Kind relaxes to 100/min; one sentence on
  prod-configured AC targets showing 5/hour behavior).
- **@dry-reviewer**: per-crate WHY comment on the trigger list (mh = assertion (b)
  handshakes, common = shared token types, media-protocol = client-mirrored frame);
  cross-ref comments on `randomMeetingCode()`/`garbageToken68()` pointing at
  `packages/sdk-core/src/validation/limits.ts` (`validateMeetingCode`/`validateUserToken`)
  + GC's pre-DB-lookup regex as the format authorities (coupled-grep comment convention).
- **@observability**: one-line comment in mcMetrics.ts anchoring the `jwt_validation`
  label choice to the MC step-6 emission site (SessionBinding maps to a DIFFERENT label —
  a future reclassification breaks this spec knowingly); the env-red browser-not-run
  stderr note carries a stable greppable token (`Layer7: browser-e2e-not-run:` prefix) +
  matching runbook §8 symptom row.
- **@security** (checked at review): no token/garbage-token interpolation into failure
  messages/titles/console anywhere incl. new helpers (busFailureContext dumps bus events —
  confirmed safe: the bus projects whitelisted non-secret fields only, but re-verify no
  token-bearing shapes can land in it); meeting-not-found asserts 404-EXACT so a code
  collision fails loudly; URL echoes in layer7.sh logs stay host:port only.
- **@code-reviewer**: (1) encode the browser-trigger path list ONCE as a bash array
  (`__BROWSER_E2E_TRIGGER_PATHS=(...)`) iterated by BOTH `__browser_e2e_triggered()` and
  the SKIPPED-NO-DIFF stderr note — no second hand-written path list to drift;
  (2) `expectLastErrorCode` takes a literal union derived from sdk-core's barrel-exported
  `SdkErrorCode` (verified exported: `packages/sdk-core/src/index.ts:59` — value/const
  import, no new wire-path literals) so a typo'd prefix is a compile error; same
  literal-typing discipline for the new mcMetrics `errorType` param; (3) shellcheck-clean,
  `local` vars in new bash functions, retired-log-grep rule kept exactly.
- **@semantic-guard**: the runbook/README retention note for
  `packages/web-app/test-results/` must state explicitly that retain-on-failure traces
  capture network request/response bodies and therefore CAN contain live credentials
  (real meeting/user tokens) — "local-only, gitignored, treat as sensitive" is the
  load-bearing sentence, not merely "persists outside DEVLOOP_TMP".
- **@test** (checked at review): waitForResponse predicates match on PATH only, status
  asserted exactly afterwards (#18 captureAccessToken pattern — a status-matching
  predicate would convert wrong-status into an opaque timeout); auth-rejection Test A
  reuses signUpViaUi UNMODIFIED against the fulfilled route (synthetic RegisterResponse
  satisfies response.ok + accessToken shape); expectNoJoinedEvent sequenced strictly
  AFTER each spec's terminal condition; layer7.test.sh trigger-off case asserts
  SKIPPED-NO-DIFF as a CHILD status with aggregate remaining OK (pins the
  no-ladder-edits claim behaviorally).

### Open questions for reviewers

- **Q1 (@operations)**: env-tests-FAIL → browser suite skipped with stderr note, no
  STATUS line (your lean, adopted). Confirm.
- **Q2 (@test/@security/@operations)**: registration budget — 4/run simple-and-isolated
  (proposed) vs 3/run shared-user (fragile on worker restart). Confirm 4/run.
- **Q4 (all)**: trigger includes `crates/mh-service/`, `crates/common/`,
  `crates/media-protocol/` beyond the task's literal "MC/AC/GC" (mechanism-wider class,
  fail-toward-RUN). Confirm.
- **Q6 (@test/@security)**: auth-rejection covers both hops (GC 401 session-drop for the
  unauthenticated JOIN + AC 401 typed `AuthError` for the literal R-45 surface). Confirm.

---

## Pre-Work

None

---

## Implementation Summary

Resumed session audit (2026-08-05): the five files touched before the interruption
(`fixtures.ts`, `mcMetrics.ts`, `auth-rejection.spec.ts`, `layer7.sh`, `layer7.test.sh`)
were verified complete against the plan + the Gate-1 reviewer checklist — every checklist
item was already honored in them (trigger-array single encoding, fail-toward-RUN rationale,
per-crate WHY comments, `jwt_validation` label anchor, `browser-e2e-not-run:` greppable
token, SdkErrorCode literal typing, path-only waitForResponse predicates, token hygiene).
Two stale-comment drifts were found and fixed: the "5/hour / 2 registrations" claims in
`fixtures.ts::randomCredentials` and `join-happy-path.spec.ts` (now pointing at the README
budget section + configmap SSoT).

New this session:
- `meeting-not-found.spec.ts` — real user, random well-formed code → 404-EXACT via
  `joinCapturingGcStatus`, typed `MEETING:` in `last-error`, nav-signin NOT visible
  (discriminates 404 from the 401 session-drop), no `joined` bus event.
- `mc-token-rejection.spec.ts` — real user + `bootstrapMeeting`, GC join response
  `meetingId` route-rewritten to garbage → typed `SIGNALING:` in the DOM AND
  `mc_session_join_failures_total{error_type="jwt_validation"}` rises above a pre-join
  baseline (the MC-side false-pass killer; verified against `connection.rs` step 6 →
  `McError::JwtValidation` → `errors.rs::error_type_label()` → `jwt_validation`), no
  `joined` event, no message-detail dependence (fail-closed contract).
- `e2e/README.md` — title #18→#18/#19; negative-spec assertion section; "Manual
  invocation only" replaced with the Layer-7 wiring description; budget math 2→4 with the
  rate-limit SSoT fix (configmap 100/min for Kind, prod default 5/60min from
  `ac-service/src/config.rs`); semantic-guard's trace-sensitivity sentence (traces capture
  request/response bodies, CAN contain live tokens — local-only, gitignored, sensitive).
- `verify-completion.sh` — header comment + usage text only (delegation note: client
  unit/component under standard via L4 ts wrapper; browser E2E under full via L7).
- `SKILL.md` — layer-table row 7, the "Today Layer 7 runs only…" paragraph rewritten to
  the two-sequential-suites description (script stays mechanism SSoT), Lead-policy bullet
  extended (shared single attempt; browser FAIL consumes, PRECONDITION does not, env-red
  skip runs on retry).
- Runbook — §3 REASON examples; §6.7 two-suite Phase-2 paragraph + six new lane-table
  rows; "no new lanes / no ladder edits" note; §8 symptom rows; changelog entry.
- `docs/TODO.md` — #19 adjudication sentence on the sdk-core wire-path entry (defer
  trigger now "next sdk-core http-surface change").

NOT touched (per plan): `docs/user-stories/2026-05-02-browser-client-join.md`
(team-lead flips status at close); no `packages/sdk-core/**` edits; no production code.

---

## Files Modified

| File | Change |
|------|--------|
| `packages/web-app/e2e/meeting-not-found.spec.ts` | NEW — 404 negative spec |
| `packages/web-app/e2e/mc-token-rejection.spec.ts` | NEW — MC binding-rejection spec w/ Prometheus proof |
| `packages/web-app/e2e/auth-rejection.spec.ts` | NEW (pre-resume) — both credential-rejection hops |
| `packages/web-app/e2e/fixtures.ts` | Negative-path helpers (pre-resume) + rate-limit comment drift fix |
| `packages/web-app/e2e/mcMetrics.ts` | join-failure counter helpers + shared poll extraction (pre-resume) |
| `packages/web-app/e2e/join-happy-path.spec.ts` | Comment-only: stale 5/hour claim → README/configmap pointer |
| `packages/web-app/e2e/README.md` | Pipeline wiring, budget 2→4 + SSoT fix, negative-spec docs, trace-sensitivity |
| `scripts/layer7.sh` | Phase-1(g) + Phase-2 browser lane (pre-resume) |
| `scripts/layer7.test.sh` | Hermetic browser-lane cases B1-B7 (pre-resume) |
| `scripts/verify-completion.sh` | Comment/usage text only |
| `.claude/skills/devloop/SKILL.md` | Layer-7 docs + Lead-policy bullet |
| `docs/runbooks/devloop-validation.md` | §3/§6.7/§8 rows + changelog |
| `docs/TODO.md` | One-line #19 adjudication on the sdk-core wire-path entry |

---

## Devloop Verification Steps

Local (in-container) checks this session:
- `bash -n` clean on `layer7.sh` / `layer7.test.sh` / `verify-completion.sh`
  (shellcheck binary not present in this container — covered by the pipeline's
  artifact-specific shell lint on the Lead's `layer-all.sh` run).
- `bash scripts/layer7.test.sh` → **70 passed, 0 failed** (incl. the B1-B7 browser-lane
  cases and the direct `__browser_e2e_triggered()` predicate cases).
- `pnpm exec svelte-check --tsconfig ./tsconfig.json` (web-app, `e2e/**/*` in scope) →
  687 files, 0 errors; `tsc --noEmit -p tsconfig.json` → rc 0.
- `pnpm exec playwright test --list` → 6 tests in 4 files (all three negative specs
  discovered alongside the happy path).

Deferred to the Lead's full run: `./scripts/layer-all.sh` (live cluster; first
in-container full-layer run carries the known VITE-proxy/UDP-reachability risk flagged in
the plan — surfaces loudly as named handshake timeouts if it bites; task #22 is the
alignment pass).

---

## Code Review Results

**@security** — one finding, FIXED: `auth-rejection.spec.ts` harness-sanity assertion
compared token VALUES via `.toBe(garbage)`; on the route-not-intercepting failure mode a
REAL AC token would have been printed into the failure message. Changed to a boolean
compare (`retained === garbage` → `.toBe(true)`) with a redaction-by-design message —
same discipline as `expectLastErrorCode`/`assertTokenOnlyJoinTraffic`. All other Gate-1
security asks verified by the reviewer (404-exact, token-free busFailureContext,
host:port-only layer7 echoes, by-value token routing, runtime-generated garbage token,
700-perm log hygiene, DEVLOOP_TEST trust boundary).

**@semantic-guard** — same finding ([credential-leak], same site/same fix), independently
confirmed both Gate-1 asks landed (README/runbook trace-sensitivity sentence; item-8
token flow in mc-token-rejection fetch→json→fulfill only). FIXED (one edit satisfies
both reviewers).

**@observability** — all three Gate-1 asks verified landed; one comment-accuracy fix,
FIXED: mcMetrics.ts header's "concurrency-safe … never a false negative" claim
disproved by the Gate-2 attempt-2 stale-series flake — header now distinguishes
concurrent-increment safety from series-churn exposure (false-negative-only, loud,
references the 26_mh_quic diagnosis; the deferred env-tests-crate hardening must sweep
the TS mirrors). The below-baseline triage hint was implemented CENTRALLY in
`pollUntilSumAbove` (appended to any timeout message when `lastObserved < baseline`), so
BOTH counter-delta consumers inherit it — satisfies @observability's per-message ask and
@test's both-builders ask with one encoding.

**@test** — all four Gate-1 asks + Q2/Q6 verified landed (independent 70/70 self-test
re-run). Finding FIXED in both parts (re-review 2026-08-05: verified in tree, verdict
RESOLVED-FIXED to Lead): (1) the stale-series baseline-capture follow-up is
now TRACKED in `docs/TODO.md` §Env-Test Resilience naming BOTH siblings (Rust
counter-delta helpers incl. the `Err(_) => 0.0` masking sub-item as its own entry, AND
`mcMetrics.ts::pollUntilSumAbove`), with the Layer-7 retry-attempt scenario recorded as
the concrete exposure; (2) the below-baseline churn hint landed centrally in
`pollUntilSumAbove` (see @observability row).

**@dry-reviewer** — all four Gate-1 asks verified landed. Both findings FIXED
(re-review 2026-08-05: verified in tree, verdict RESOLVED-FIXED to coordinator):
(1) response-capture idiom collapsed to ONE internal `captureResponse(page, path,
action)` (path-only predicate + 15s timeout single-encoded); `captureAccessToken`,
`joinCapturingGcStatus`, `signInExpectingRejection` all consume it — net-negative LoC;
(2) wire-path literals hoisted to module-local `REGISTER_PATH`/`LOGIN_PATH`/
`meetingPath(code)` consumed at all six driver sites (predicates + route globs);
`assertTokenOnlyJoinTraffic`'s prefixes left deliberately independent with the
requested do-not-clean-up comment citing TODO nuance (b).

**@code-reviewer** — all three Gate-1 asks verified landed (incl. their independent
shellcheck 0.11.0 run: layer7.sh fully clean; remaining hits pre-existing). Finding
FIXED: `joinAsUser` alphabetized in mc-token-rejection.spec.ts's fixtures import. The
pre-existing SC2034 `--verbose`/`[path]` dead-args issue they flagged on
verify-completion.sh is tracked in `docs/TODO.md` §Code Quality (ops-owned, out of this
task's comment-only classification for that file).

**@operations** — all owned surfaces verified landed exactly; §6.7 cite re-anchor
ACKed. Finding FIXED (re-review 2026-08-05: verdict RESOLVED-FIXED to team-lead,
INCLUDING the ADR-0024 §6.3 owner hunk-ACK for the operations-owned Domain-judgment
rows — layer7.sh / layer7.test.sh / SKILL.md / runbook / verify-completion.sh): both env-tests follow-up candidates promoted from this transcript
to tracked `- [ ]` entries in `docs/TODO.md` §Env-Test Resilience (surfacing context,
owners, defer triggers); addendum bullet for the verify-completion.sh dead-args issue
added under §Code Quality verbatim.

Post-fix verification: `tsc --noEmit` clean; `playwright test --list` still 6 tests in
4 files; `validate-todo-tracking` OK; wire-path literals now single-encoded at
fixtures.ts:83-88 (remaining occurrences are the deliberate guard independents + one doc
comment).

---

## Accepted Deferrals

None — all seven Gate-3 verdicts are RESOLVED-FIXED; every finding on this diff was fixed in this PR. The bullets below are PRE-EXISTING debt surfaced during review (Gate-2 flake diagnosis + one out-of-diff reviewer observation), not deferrals of findings on this change — see §Code Review Results.

- `docs/TODO.md` §Env-Test Resilience — stale-series baseline capture in both Rust and TS counter-delta helpers
- `docs/TODO.md` §Env-Test Resilience — `Err(_) => 0.0` Prometheus-error masking in `26_mh_quic.rs` helpers
- `docs/TODO.md` §Code Quality — `verify-completion.sh` dead `--verbose`/`[path]` args

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `e29aa615318f1f681c3cfcd49da6099d4551f128`
2. Review all changes: `git diff e29aa615..HEAD`
3. Soft reset (preserves changes): `git reset --soft e29aa615`
4. Hard reset (clean revert): `git reset --hard e29aa615`

---

## Issues Encountered & Resolutions

**Gate-2 attempt 1: Layer 3 guard failures (2026-08-05)** — everything else green,
incl. Layer 7 live (env-tests OK + all 6 browser specs passed against the cluster).

1. `validate-cross-boundary-scope` `[scope_drift_inbound]` on
   `join-happy-path.spec.ts`: the resume-audit stale-comment fix touched a file with no
   classification-table row. Resolution: row added (Mine, comment-only).
2. `validate-doc-citations-symbol-resolves` on the runbook's
   `layer7.sh::__BROWSER_E2E_TRIGGER_PATHS` cite: dt-guard's sh resolver
   (`crates/dt-guard/src/cite_extract.rs` `SH_FN_PAREN_RESOLVER`/`SH_FN_KEYWORD_RESOLVER`)
   recognizes only `name()`-at-line-start and `function name` — bash ARRAY assignments are
   not citable symbols by design (code-reviewer ruling 2026-05-19: six static per-language
   resolvers, Python-kernel parity). Adjudication: fix the cite, not the guard — extending
   dt-guard is a task-sized cross-boundary edit (Rust + parity + tests, `crates/dt-guard`
   not in this task's classification), and §10's preferred form is a function anchor
   anyway. The runbook now cites `scripts/layer7.sh::__browser_e2e_triggered` (resolvable)
   with the array named in prose — the Gate-1 "array is the one encoding" pointer is
   preserved verbatim in meaning.

Local re-verification: cite guard OK (35 cites / 24 docs), scope guard OK (no drift),
classification guard OK (37 files).

**Gate-2 attempt 2, Layer-7 attempt 1: env-test flake (2026-08-05)** — Layers 1-6 green;
`26_mh_quic.rs::test_mc_media_connection_update_increments_participant_mh_status_metric`
failed; browser suite correctly not run (env-red skip). Diagnosis (targeted run + cluster
forensics, not a pipeline attempt):

- Panic (from the retained `/tmp/devloop/layer-7-env-test.log`):
  `did not increase above baseline 5 within 60s (last observed: 1)` — **observed <
  baseline**: the monotonic counter SUM went DOWN during the assertion window, which is
  only possible when constituent series vanish from the instant vector.
- Mechanism: **stale-series inflation across the pipeline's own rebuild rollout.**
  Prometheus range data shows the sum dropping 7→5→1 across 19:08:44-19:09:14 — exactly
  bracketing the test's baseline read. Attempt 2 ran against a freshly RE-CREATED cluster
  (kube-system pods 22 min old ⇒ full Phase-1a setup ~18:51; Prometheus TSDB uptime ~18.5
  min) and Phase-1c's `rebuild-all` rollout was still draining old MC pods (service pods
  5 min old ⇒ final roll ~19:07) when this test — last in the 73s `26_mh_quic` binary —
  captured its baseline: the old pods' series were still inside Prometheus's staleness
  window (baseline 5), expired mid-assertion, and the fresh pods' post-rollout counters
  (the test's own join = 1) could never exceed the inflated baseline.
- Not this diff: zero Rust/production code touched; stale-series inflation needs
  pre-existing counter history + a pod rollout — both cluster-lifecycle facts. The
  browser specs only add monotonic increments to series whose baselines every consumer
  (Rust and TS) captures at its own start.
- Isolation runs: **3/3 pass** (16.4s / 6.3s / 14.4s), cluster now settled and healthy
  (all pods Running, 0 restarts, `dev-cluster status` green).
- Follow-up candidates surfaced (env-tests crate — out of this task's classification, not
  fixed here): (a) the counter-delta helpers' baseline capture is not robust to
  stale-series inflation across a rollout (a settle-wait or per-current-pod query shape
  would close it) — covering BOTH the Rust helpers and this task's TS mirrors in
  `mcMetrics.ts`; (b) `mc_participant_mh_status_counter`'s `Err(_) => return 0.0`
  silently masks Prometheus unreachability as a zero reading (the TS mirror in
  `mcMetrics.ts` deliberately throws instead). **Both now TRACKED as `- [ ]` entries in
  `docs/TODO.md` §"Env-Test Resilience & Runbook Validation"** (@operations/@test review
  ask, 2026-08-05) — that is the tracking home; this transcript is the diagnosis record.

---

## Lessons Learned

1. **Resume-audit discipline pays**: treating the interrupted session's five files as
   untrusted and re-auditing them against the Gate-1 checklist caught two stale-comment
   drifts — but the audit fix itself then tripped the scope-drift guard (a file touched
   that wasn't in the plan table). When a resume audit edits a file outside the plan,
   add its classification row in the same breath.
2. **Cite anchors: prefer functions**: dt-guard's sh symbol resolver recognizes shell
   functions only, not bash array assignments. Runbook cites should anchor on a function
   and name arrays in prose (runbook §10 convention) — fixing the cite beats extending
   the resolver mid-devloop.
3. **Counter-delta `> baseline` assertions are rollout-fragile**: a monotonic sum can
   DROP when pod churn expires old series inside Prometheus's staleness window,
   inflating a just-captured baseline (observed live in Gate-2 attempt 2:
   sum 7→5→1 bracketing the baseline read). "Observed < baseline" is the smoking gun —
   both the Rust and TS helpers now emit that hint; hardening is TODO-tracked.
4. **The env-red browser-skip lane was validated for free**: the Gate-2 attempt-2
   flake exercised the exact Q1 design path (browser not run, stderr note, no STATUS
   line) against a real failure before the code ever shipped.
