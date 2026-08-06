# Devloop Output: Close four browser E2E coverage gaps (pre-close manual test-plan review)

**Date**: 2026-08-06
**Task**: Close four browser E2E coverage gaps identified in the pre-close manual test-plan review (distinct-user two-party join, participantLeft/teardown/rejoin, join-after-error recovery, rendered-roster DOM assertion)
**Specialist**: test
**Mode**: Agent Teams (full) — HEADLESS
**Branch**: `feature/user-story-run-test`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `ad165aefc9348193c57c18780730b1277294f278` |
| Branch | `feature/user-story-run-test` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` (RESUMED 2026-08-06 — FINDING-1 fixed by task #65 / `fb9e0c9`; Gate 2 GREEN: L1-5 OK, L6 N/A audit-aggregate, L7 env-tests + browser-e2e PASS 8/8; Gate 3 all CLEAR) |
| Implementer | `implementer` |
| Implementing Specialist | `test` |
| Iteration | `1` (Gate 2 attempt 2 — first attempt blocked on FINDING-1, now fixed upstream) |
| Security | `CLEAR` |
| Test | `n/a (implementer is the test specialist)` |
| Observability | `CLEAR` |
| Code Quality | `CLEAR` |
| DRY | `CLEAR` |
| Operations | `CLEAR` |
| Semantic Guard | `CLEAR` |

---

## Task Overview

### Objective
Close the four "Yes" promote-to-E2E gaps from
`docs/user-stories/2026-05-02-browser-client-join-manual-test-plan.md` §Promote-to-E2E verdicts,
BEFORE story close. All work in `packages/web-app/e2e/` plus (only if actually needed) one
small SDK-side bus addition in `packages/web-app/src/lib/e2eBus.ts`.

### Scope
- **Service(s)**: web-app E2E suite (test); possibly `packages/web-app/src/lib/e2eBus.ts` (client-owned)
- **Schema**: No
- **Cross-cutting**: No (test-tier only)

### Hard constraints (from task prompt — must hold)
- `retries: 0` (ADR-0028: a flaky test is fixed or deleted)
- `workers: 1`
- Counter assertions via `mcMetrics.ts` baseline-delta pattern
- Credentials as per-run throwaways; no cert-bypass flags
- **4-registrations-per-run budget across the WHOLE suite** after these additions

### Lead pre-scout findings (fed to implementer — verify, do not assume)
- **Gap (4) roster testid ALREADY EXISTS**: `JoinMeeting.svelte:81-84` renders
  `data-testid="participant-list"` + per-participant `data-testid="participant-${participantId}"`.
  So (4) is assertion-only; no DOM change should be needed. Confirm before adding a testid.
- **Gap (2) `participantLeft` bus event ALREADY EXISTS**: `e2eBus.ts:100-106` projects
  `{ type: 'participantLeft', participantId, reason }` (since commit 16fb5e6). The task's
  "SDK-side bus addition" premise appears already satisfied — VERIFY it emits correctly rather
  than re-adding. If it is truly already present and correct, that is the finding; record it.
- **Gap (2) MC counter**: `mc_connections_active` exists as a **gauge**
  (`crates/mc-service/src/actors/metrics.rs:375`, `prom::set_connections_active`). Choose the most
  robustly-assertable MC signal for a departure; a gauge decrement under concurrency/churn is
  weaker than a monotonic counter — pick per the mcMetrics.ts staleness caveat and justify.
- **Budget math**: current suite = 4 registrations (happy-path 2 [sign-up primary + bootstrap
  sign-up], meeting-not-found 1, mc-token-rejection 1; auth-rejection 0). Gap (1) adds a genuinely
  distinct 2nd user (+1 registration). To stay at 4, one existing registration must be rebalanced
  to fixture bootstrap / shared sign-in. Implementer owns this decomposition; state the final
  per-spec registration tally in the plan.

---

## Cross-Boundary Classification

**Verdict: TEST-TIER ONLY. Zero client-owned production source is touched.** Both pre-scout
"already exists" findings are CONFIRMED against the code (see §Planning "Verification"), so the
`e2eBus.ts` addition and the `JoinMeeting.svelte` testid the task anticipated are both unnecessary.
Every file below lives in `packages/web-app/e2e/` (Playwright test tier) or is docs.

| File | Change | Classification |
|------|--------|----------------|
| `packages/web-app/e2e/fixtures.ts` | Add `SHARED_USER` + `authAsSharedUser`, `waitForParticipantLeft`, `expectRosterShows`/`expectRosterMissing`, `recoverByJoining`, `clearJoinResponseRewrite` (unroute) helpers | Mechanical (test harness, reuses existing idioms) |
| `packages/web-app/e2e/mcMetrics.ts` | Add `mcParticipantLeavesSum` + `waitForMcParticipantLeavesAbove` (reuses `pollUntilSumAbove`; label-anchored to `record_participant_leave`) | Minor-judgment (metric-signal choice; justified below) |
| `packages/web-app/e2e/join-happy-path.spec.ts` | Restructure to shared valid user; add distinct-user two-party + leave/counter + rejoin + roster-DOM test (gaps 1,2,4) | Mechanical (test) |
| `packages/web-app/e2e/auth-rejection.spec.ts` | Append recovery tail (gap 3) | Mechanical (test) |
| `packages/web-app/e2e/meeting-not-found.spec.ts` | Append recovery tail (gap 3) | Mechanical (test) |
| `packages/web-app/e2e/mc-token-rejection.spec.ts` | Append recovery tail (gap 3, unroute-before-rejoin) | Mechanical (test) |
| `packages/web-app/e2e/README.md` | Update §Budgets + §asserts + doc-drift note | Mechanical (docs) |
| `packages/web-app/playwright.config.ts` | `workers=1`↔register-once coupling comment + rate-limit-drift reconciliation (operations concern C) | Mine (test config) |

**NOT touched** (pre-scout findings confirmed): `packages/web-app/src/lib/e2eBus.ts` (participantLeft
event already correct), `packages/web-app/src/views/JoinMeeting.svelte` (roster testids already present).
No client-domain semantic edit is required; the client specialist is NOT needed as a reviewer.

---

## Planning

### Verification of the two "already exists" pre-scout findings (both CONFIRMED)

1. **Gap (4) roster testid — EXISTS, assertion-only.** `JoinMeeting.svelte:81-84` renders
   `<ul data-testid="participant-list">` with `<li data-testid={`participant-${participantId}`}>{name}</li>`.
   The roster store (`MeetingStore.svelte.ts:69-83`) is seeded from `joined.existingParticipants` and
   mutated by `participantJoined`/`participantLeft` — it **excludes self**. Consequence for gap (4):
   "both participants with correct names" is satisfied by asserting **each context renders its PEER**
   (context A shows userB's name; context B shows userA's name). No testid needs adding.

2. **Gap (2) participantLeft bus event — EXISTS and is correct.** Full chain verified:
   `SignalingClient.ts:456-462` maps the proto `ParticipantLeft` ServerMessage → emits
   `participantLeft {participantId, reason}`; `MeetingSession.ts:463` bridges it; `e2eBus.ts:100-106`
   whitelist-projects `{type:'participantLeft', participantId, reason}` (non-PII, R-23). **No SDK/bus
   change needed** — the task's "one small SDK-side bus addition" premise is already satisfied.

### Gap (2) — chosen MC departure signal (justified)

**Signal: `sum(mc_participant_leaves_total)` — a MONOTONIC counter, summed over ALL reasons.**
- `mc_connections_active` is REJECTED: it is a gauge; a departure is a decrement; the existing
  `pollUntilSumAbove` idiom is monotonic-only; and a shared cluster (Rust env-tests) perturbs a gauge
  in both directions → both false-negative and false-positive risk. (Confirmed by @observability.)
- `mc_participant_leaves_total` (task #64, `crates/mc-service/src/observability/metrics.rs:442`,
  `record_participant_leave`) increments **exactly once per `ParticipantLeft` broadcast** at the single
  roster-removal choke-point (`actors/meeting.rs remove_and_broadcast_left`). Strictly monotonic → drops
  straight into `pollUntilSumAbove`, inherits the series-churn hint. A dead MC cannot increment it (same
  false-pass-killer property as the join-failure helper).
- **Sum over ALL reasons, NOT `reason="voluntary"`:** a Playwright `context.close()` does not *guarantee*
  a clean WebTransport CONNECTION_CLOSE. Clean close → `ClientClosed` → `voluntary` (immediate); an
  ambiguous close → `ConnectionLost` → 30s grace + 5s check → `timeout`. Summing all reasons is immune to
  that classification variance while staying monotonic — the bus `participantLeft(participantId)` event
  provides the per-participant attribution; the counter proves MC recorded a leave server-side. This
  mirrors mc-token-rejection's split (DOM attribution + cluster-wide counter delta).
- **Budget/latency (config-derived, @observability zero-margin fix):** common path (clean close)
  returns as soon as the signal appears (~1 scrape interval, ≤15s) — the ceiling is only hit on the
  grace path. Worst case is config-SSoT-derived (`docs/observability/metrics/mc-service.md:316-320`):
  `MC_QUIC_MAX_IDLE_TIMEOUT`(10) + `MC_DISCONNECT_GRACE_PERIOD`(30) + 5s grace-check ≈ 45s to broadcast,
  + 15s Prometheus scrape SLA ≈ 60s for the counter. A flat 60s has zero margin, so:
  - `waitForParticipantLeft` (bus, no scrape hop) default **60_000** (~45s broadcast + margin).
  - `waitForMcParticipantLeavesAbove` (counter, +scrape) default **90_000** (~45s + 15s scrape + margin);
    < the 120s/test ceiling. Comment DERIVES the budget from the grace/idle/scrape config SSoT (does not
    hardcode a rationale that drifts from `MC_DISCONNECT_GRACE_PERIOD`). Both are LARGER than the
    join-side 60s precisely because the departure signal's worst case includes the disconnect grace path.
  I keep pure `context.close()` (literal task wording) rather than forcing a graceful disconnect — the
  generous ceilings + all-reasons sum make it flake-free regardless of close classification, and the
  common path stays fast.
- New PromQL lives ONLY in `mcMetrics.ts`, LABEL-ANCHOR comment citing `record_participant_leave`
  (metrics.rs:442) + the bounded `LeaveReason` enum (voluntary/timeout/removed/meeting_ended; `removed`
  is reserved / no emitter → NOT asserted).

### FINAL registration-budget tally (the single most important review item)

Registrations = AC `POST /register` calls. **The suite registers exactly TWO users per run → 2 ≤ 4.**

| User | Registered where (once) | Reused via |
|------|-------------------------|------------|
| **V** (shared valid user) | `authAsSharedUser()` first call, in an isolated throwaway context (route-mock-safe) | `signInViaUi(V)` everywhere else (0 each) |
| **userB** (distinct 2nd party) | happy-path two-party test, `signUpViaUi` (genuinely different account, per gap 1) | rejoin via `signInViaUi(userB)` (gap 2, 0) |

Per-spec: happy-path = V(shared) + userB(1) ; meeting-not-found = V(0) ; mc-token-rejection = V(0) ;
auth-rejection = V(0) for its recovery tail; negatives are route-fulfill/never-registered (0).
**Total distinct registrations = 2.**

**FINDING that changes the pre-scout budget:** the pre-scout counted only gap (1)'s +1 and assumed
gap (3) recovery adds 0. That holds for meeting-not-found and mc-token-rejection (their session is NOT
dropped — recovery reuses the live token). It does NOT hold for **auth-rejection**: its negatives hold a
garbage/never-registered credential, so a *valid* recovery join needs a real account. With isolated
per-spec users the suite needs 5 valid-user registrations (happy-path 2 + mnf 1 + mctr 1 + auth-rej
recovery 1) → CANNOT hit ≤4. Sharing one valid user is therefore mandatory; sharing V broadly lands at 2
with headroom and gives a single registration SSoT (aligns with CLAUDE.md "single source of truth").
`authAsSharedUser` registers V in an **isolated throwaway context** so no spec's `page.route` (e.g.
auth-rejection's `fulfillAuthRegister`, which intercepts ALL `**/register`) can corrupt V's registration.

### Gap-by-gap implementation

**Gap (1) distinct-user two-party** — new test in `join-happy-path.spec.ts`. Context A: `authAsSharedUser`
(= V), `bootstrapMeeting`, `joinAsUser`, `waitForJoined`→joinedA. Context B: `signUpViaUi(userB)`
(genuinely different account), `joinAsUser`, `waitForJoined`→joinedB. Assert
`joinedA.participantId !== joinedB.participantId` AND `joinedA.userId !== joinedB.userId` (genuinely
distinct — stronger than the same-user precedent). Context A `waitForParticipantJoined(joinedB.id)`.

**Gap (4) roster DOM** — in the same test, `expectRosterShows(pageA, joinedB.id, userB.displayName)` and
`expectRosterShows(pageB, joinedA.id, V.displayName)` (both peers rendered with correct token-carried
names — ties bus truth to rendered UI). New helpers assert the existing `participant-${id}` li testid.

**Gap (2) leave/teardown/rejoin** — SEPARATE test from gaps 1/4 (operations wall-clock fix: each test
gets a fresh 120s budget; re-establishing the two-party setup is 0 registrations, all sign-ins). Flow:
re-establish V (ctxA) + userB (ctxB, signInViaUi — userB already registered) two-party; baseline
`mcParticipantLeavesSum()`; **drive the app's OWN teardown in ctxB** (navigate ctxB away from the join
view → JoinMeeting unmounts → `onDestroy` → `session.disconnect()` → clean CONNECTION_CLOSE → MC
`ClientClosed`→`voluntary`→immediate leave), THEN `contextB.close()` (still closes the context per the
task; the disconnect makes the departure DETERMINISTIC + fast, observability option (b)). Assert ctxA
`waitForParticipantLeft(joinedB.id)`; `waitForMcParticipantLeavesAbove(baseline)` (all-reasons sum;
common path ~15s, 90s ceiling is config-derived grace insurance, virtually never hit given the clean
close); `expectRosterMissing(pageA, joinedB.id)`. Then rejoin from a FRESH context C: `signInViaUi(userB)`
(0 reg, per task — proves the meeting token carries userB's registered name since SignIn has no client
displayName, `SignIn.svelte:29`), `joinAsUser`, `waitForJoined`→joinedC; assert
`joinedC.participantId !== joinedB.participantId` (new id); ctxA `waitForParticipantJoined(joinedC.id)`;
`expectRosterShows(pageA, joinedC.id, userB.displayName)` (token-carried name correct on clean re-entry).
Worst-case wall-clock: setup joins (fast, ~10s) + clean leave (~15s) + rejoin (~10s) ≈ 40s typical; even
if the rare grace path is hit (90s) the total stays ~110s < 120s. `waitForParticipantLeft` default
60_000 (broadcast worst-case, no scrape hop).

**Gap (3) recovery tails** — MeetingSession is single-use (`JoinMeeting.svelte:8`), so recovery must
REMOUNT the join view: `recoverByJoining(page, code)` clicks `nav-create`, **awaits the create view
visible (`meeting-title`) — a GATED remount, not back-to-back clicks (code-reviewer A: a blind
nav-create→nav-join races the single-use teardown; retries=0 makes that a real failure)** — then
composes `joinAsUser` (which owns nav-join + fillJoinForm — not re-encoded, @dry-reviewer) then
`waitForJoined`. All in the SAME page session, no reload. Per spec:
- meeting-not-found: after the 404 (session NOT dropped), `bootstrapMeeting(token)` a valid meeting →
  `recoverByJoining` → assert joined.
- mc-token-rejection: `clearJoinResponseRewrite(page, code)` (page.unroute) FIRST (recovery runs WITHOUT
  the rewrite, per task) → `recoverByJoining(page, sameCode)` → assert joined.
- auth-rejection (Test A, session dropped → nav-signin restored): `authAsSharedUser` (= V, 0 reg) →
  `bootstrapMeeting(Vtoken)` → `recoverByJoining` → assert joined.

### Reuse map (no re-encoding — @dry-reviewer)
`randomCredentials`, `signUpViaUi`/`signInViaUi`, `bootstrapMeeting`, `joinAsUser`, `waitForJoined`,
`waitForParticipantJoined`, `captureResponse`/`meetingPath`, `expectNoJoinedEvent`, `expectLastErrorCode`,
`recordRequests`/`assertTokenOnlyJoinTraffic`. New mcMetrics helper EXTENDS the one `pollUntilSumAbove`.
`recoverByJoining` is hoisted once (used 3×). Do NOT touch the two documented intentional exceptions
(auth-path literals in `assertTokenOnlyJoinTraffic`; REGISTER_PATH/LOGIN_PATH consts).

### `authAsSharedUser` robustness (fail-loud, write-once — code-reviewer B / operations B/C)
- Memoized on a module-level PROMISE that resolves only on a SUCCESSFUL registration; on failure it is
  reset (not poisoned with undefined/partial) and re-throws a single clearly-named diagnostic
  ("shared user V registration failed: …") so V's failure surfaces at the real cause, not as N cryptic
  downstream `signInViaUi(V)` timeouts. V is the suite's single valid-user SPOF; its failure must be legible.
- Registers V in an ISOLATED throwaway context so auth-rejection's `**/register` fulfill can't corrupt it
  (routes are per-context; sign-in hits LOGIN_PATH regardless — @security confirmed).
- **workers=1 ↔ budget coupling made explicit** (operations C): the once-per-run guarantee rests on
  workers=1 (single worker process → module memo registers V once). A one-line comment at
  `authAsSharedUser` AND in README §Budgets records this coupling so a future `workers` bump (which would
  race the memo and re-register V per worker) is not an invisible budget regression. Note: on a GREEN
  gating run there are no failures/worker-restarts, so V registers exactly once → 2 total; the 100/min dev
  rate-limit absorbs any restart-induced re-registration on already-failing runs.

### Doc pass (operations C)
README §Budgets 4→2 (+ shared-V rationale + workers=1 coupling), §What the specs assert (new
two-party/leave/rejoin/recovery coverage), the "no Playwright E2E yet" drift note, AND reconcile the
`playwright.config.ts` "5/hour" comment vs README "100/min dev" so the rate-limit number is consistent.

### Constraints honored
retries=0/workers=1 (config untouched); PromQL only in mcMetrics.ts; per-run throwaway creds, none
logged; no cert-bypass; bounded waits + point-in-time `expectNoJoinedEvent`; per-context recorder windows.

---

## Implementation (complete — ready for validation)

Static gates GREEN: `svelte-check` 0 errors, `eslint e2e/` clean, `prettier --check` clean
(also fixed one pre-existing `consistent-type-imports` debt on `SdkErrorCode` in fixtures.ts).
Zero production source touched (test-tier + docs only), as classified.

Files changed:
- `e2e/mcMetrics.ts` — `mcParticipantLeavesSum` + `waitForMcParticipantLeavesAbove` (all-reasons
  monotonic sum, 90s config-derived ceiling, label-anchored), extending the one `pollUntilSumAbove`.
- `e2e/fixtures.ts` — `SHARED_USER` + `authAsSharedUser` (fail-loud write-once memo, isolated
  throwaway-context registration), `waitForParticipantLeft` (own contract diagnostic),
  `expectRosterShows`/`expectRosterMissing`, `clearJoinResponseRewrite`, `recoverByJoining`
  (gated remount composing `joinAsUser`). Budget docstring 4→2.
- `e2e/join-happy-path.spec.ts` — 4 focused tests: (1) core a/b/d/e; (2) distinct-user two-party +
  roster DOM (gaps 1,4); (3) leave/counter/rejoin via deterministic clean close (gap 2); (4) bootstrap.
- `e2e/{auth-rejection,meeting-not-found,mc-token-rejection}.spec.ts` — recovery tails (gap 3);
  mnf + mctr now reuse V (0 registrations each).
- `e2e/README.md` §Budgets (4→2 + workers=1 coupling) & §asserts; `playwright.config.ts` rate-limit
  comment reconciled (dev 100/min vs prod 5/60min) + workers=1↔register-once coupling.

**Final registration audit (proven by grep):** exactly 2 real AC registrations per run —
`fixtures.ts` `signUpViaUi(SHARED_USER)` (V, once, memoized) + `join-happy-path` `signUpViaUi(userB)`.
auth-rejection's `signUpViaUi` is route-fulfilled (`fulfillAuthRegister`) → 0. ≤ 4. ✓

---

## Resolution (RESUMED 2026-08-06 — escalation cleared)

**The terminal escalation below (FINDING-1) was resolved by a separate meeting-controller devloop, exactly as recommended.** Task #65 (`2026-08-06-mc-display-name-join-plumbing`, commit `fb9e0c9`) plumbs `claims.display_name` from the validated meeting-token through `connection_join → ConnectionJoin → handle_join → Participant.display_name`, keeping the `"Participant N"` label only as an empty-claim fallback (verified: `meeting.rs:630-636`, `connection.rs:403`). FINDING-2's userId over-assertion was already removed from the working tree in attempt 1.

On resume, Gate 2 was re-run in full and is **GREEN**: Layers 1-5 OK, Layer 6 N/A (audit-aggregate; cargo-audit/pnpm-audit/buf-breaking all pass), Layer 7 `env-tests-passed` + `browser-e2e-passed` (8/8 — the previously-red gap-(2) rejoin and gap-(4) roster **display-name** assertions now pass against the fixed MC). Full log: `/tmp/layer-all-resume.log`.

### Gate 3 — Final Verdicts (all CLEAR)

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | Throwaway creds, no cert-bypass, no secret logging; token-only join invariant preserved. |
| Test | n/a | — | — | — | Implementer is the test specialist. |
| Observability | CLEAR | 0 | 0 | 0 | Monotonic all-reasons `mc_participant_leaves_total` sum; 90s budget config-derived. 1 comment-only NIT, no change required. |
| Code Quality | CLEAR | 0 | 0 | 0 | Gated remount + write-once fail-loud shared-user memo both landed; ADR-0028 held. |
| DRY | CLEAR | 0 | 0 | 0 | All 4 watch-items honored; helpers extend primitives; no cross-service duplication. |
| Operations | CLEAR | 0 | 0 | 0 | retries=0/workers=1 untouched; budget 4→2 documented w/ coupling comment; wall-clock safe. 1 note-only observation. |
| Semantic Guard | CLEAR | 0 | 0 | 0 | Test-tier; no production-code semantic anti-patterns; roster assertions genuinely assert names (not fake-green). |

**No deferrals, no escalations.** Proceeding to commit.

---

## Escalation (HEADLESS — terminal) — SUPERSEDED, see §Resolution above

**Gate 2 could not pass and the blocker is out of this devloop's scope + domain, with no human available to authorize a scope expansion or make the story-readiness call. Terminal escalation per SKILL §Headless Mode.**

- **Layers 1–6: PASS.** Layer 7: Rust env-tests PASS against the live Kind cluster; browser suite ran (8 tests, 6 pass). The 2 NEW tests carrying the roster-**name** assertions are RED because of a real MC product bug (below). Full pipeline log: `/tmp/layer-all-e2e.log` (LAYER=7 RESULT=FAIL; `browser-e2e-failed`).
- **What the devloop DID deliver (all validated, static gates green, but uncommitted because Gate 2 as a whole is red):** gap (1) distinct-user two-party (participant-id distinctness + participantJoined); gap (2) leave→`mc_participant_leaves_total` counter delta→clean rejoin (participant-id + counter parts pass); gap (3) recovery tails on all 3 negative specs; the `mcMetrics.ts` leaves helper; the 4→2 registration rebalance; README/doc updates. The ONLY red assertions are the roster **display-name** checks in gap (2) rejoin and gap (4) — and they are red because the product is wrong, not the test.
- **Blocker = FINDING-1 (below): meeting-controller drops the token/registered `display_name`; `handle_join` hardcodes `"Participant N"` (`meeting.rs:612`, MINOR-003 stopgap).** Independently verified by Lead against source. AC mints the token WITH the name (`internal_tokens.rs:162-207`); MC never consumes it (`connection.rs:401` passes `claims.sub` not `claims.display_name`; client `participant_name` length-checked at `:301` then dropped; `ConnectionJoin`/`handle_join` have no name param).
- **Why terminal, not attempt 2:** re-running Layer 7 without an MC fix fails identically (not flaky). The fix is Rust actor/join-flow plumbing in **meeting-controller's** domain — Domain-judgment cross-boundary → owner-implements (ADR-0024 §6.3) — and the task explicitly scoped this devloop to `packages/web-app/e2e/` + a small bus addition. Weakening/deleting the roster-name assertions to force green is forbidden (ADR-0028 "fix it or delete — never mask"); those assertions correctly prove the story is NOT ready to close. The implementer correctly refused to fake-green.
- **Recommended human/runner action:** schedule a **meeting-controller** devloop to plumb `claims.display_name` (authoritative) — and the client-sent `participant_name` for guests per precedence — through `connection_join` → `ConnectionJoin` → `handle_join` → `Participant.display_name`, keeping `"Participant N"` only as a genuine-absence fallback. Then re-run THIS test devloop (`/devloop --continue=2026-08-06-browser-e2e-coverage-gaps`); the failure-1 fix (userId over-assertion removed) is already in the working tree and the roster-name assertions will then go green. Secondary: decide whether the uniformly-`0` JoinResponse `userId` (FINDING-2) is a product bug.

---

## Issues / Findings (Gate-2 attempt 1)

### FINDING-1 (PRODUCT, type-b — BLOCKS story close): MC does not propagate the display name to the roster
The new gap-(2) and gap-(4) tests both fail because MC stamps every participant's roster name as the
positional placeholder `"Participant N"` instead of the registered/token-carried display name.

Root-cause trace (airtight, code-level):
1. AC mints the meeting token WITH the registered name: `common/src/meeting_token.rs` (`display_name`
   field) + `ac-service/src/handlers/internal_tokens.rs:160-183` resolves it from the users table,
   explicitly "so the meeting token carries the real name for the roster instead of a generic
   'Participant N' placeholder" (fail-closed if no users row).
2. MC validates the token and HAS the name: `mc-service/src/auth/mod.rs:63` `validate_meeting_token`
   returns `MeetingTokenClaims` (with `display_name`); it is in scope at
   `mc-service/src/webtransport/connection.rs:393`.
3. **MC drops it:** `connection.rs:397-405` calls `connection_join(meeting_id, connection_id,
   claims.sub, participant_id, is_host, outbound_tx)` — passes `claims.sub` (user_id) but NOT
   `claims.display_name`. The client-sent `join_request.participant_name` (SDK sends it —
   `sdk-core/.../MeetingSession.ts:473` → `SignalingClient.ts:299`) is only length-checked
   (`connection.rs:301`) and also dropped.
4. `ConnectionJoin` (`actors/messages.rs:70`) and `handle_join` (`actors/meeting.rs:548`) have no
   name param; `handle_join` hardcodes `let display_name = format!("Participant {}", …)`
   (`meeting.rs:612`, "MINOR-003" stopgap).

So the registered name never reaches `Participant.display_name`, which is what the JoinResponse /
ParticipantJoined roster (`connection.rs:955` `name: p.display_name.clone()`) carries to the client.
Every participant — sign-in (empty client name, relies on token) AND sign-up (client sends name) — is
rendered as `"Participant N"`.

This is EXACTLY the task-#63-second-variant condition the gap-(2) test was designed to detect: AC built
the token-carries-name half; MC never consumes it. It is OUT of the test-tier's domain (meeting-controller:
plumb `claims.display_name` through `connection_join` → `ConnectionJoin` → `handle_join` →
`Participant.display_name`, keeping the positional value only as a fallback for a genuinely absent name).
Per ADR-0028 the gap-(2)/gap-(4) roster-name assertions are NOT weakened to fake-green — they correctly
prove the story is not ready. Escalated to the coordinator as a product finding.

### FINDING-2 (minor, note-only): bus `userId` is `0` for every user
`e2eBus.ts` projects `event.userId.toString()`; observed `"0"` for all users, so it is not a per-user
distinctness signal. Removed the (over-reaching, not task-required) userId-distinctness assertion from the
two-party test; distinct accounts are proven by the roster DOM's distinct display names (gap 4). Whether a
uniformly-0 `userId` in the JoinResponse is itself a product bug is left for the coordinator/owning
specialist — recorded here, not blocked on.

---

**Runbook §7 finding (operations doc item):** the "no Playwright E2E yet" note is ALREADY reconciled
(task #61 changelog, 2026-08-06) — §7 correctly states the suite exists and §6.5 defers coverage
specifics to `e2e/README.md` (updated here). No edit made to the operations-owned runbook (it is
current; a gratuitous cross-boundary edit would be noise). Flagged to @operations.

---

## Gate 1 — Plan Confirmation

Classification-sanity guard: **PASS** (`STATUS=OK cross-boundary-classification-clean-1-files`; no GSA paths, all test-tier/docs).

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Observability | confirmed (Gate-2 item: bump leaves-helper budget 60s→~90s for grace-path margin) |
| Code Quality | confirmed (Gate-2 items: gated remount; write-once fail-loud shared-user memo) |
| DRY | confirmed (2 non-blocking diff-time watch-items) |
| Operations | confirmed (Gate-2 items: split gaps 1/4 vs 2; clean-close teardown; workers=1↔budget comment) |
| Semantic Guard | confirmed |

DRY diff-time watch-items (non-blocking): (1) `waitForParticipantLeft` should carry its own
contract-named diagnostic (not a bare event-string swap); (2) `recoverByJoining` should compose
`joinAsUser`/`fillJoinForm`, not re-encode nav/fill.
