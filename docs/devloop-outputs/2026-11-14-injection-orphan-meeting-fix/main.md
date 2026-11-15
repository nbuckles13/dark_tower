# Devloop Output: Remove MC-F's global-counter env-test gate; fix the injection-suite orphan meeting at its source

**Date**: 2026-11-14
**Task**: Story 4 follow-up — remove `metrics::await_no_mc_meetings` / `foreign-mc-meetings` from every `MhPool` leg and make `Injection::release_and_prove` close the suite's sessions and wait (meeting-scoped Loki evidence, grace-derived bound) until THAT meeting has ended on MC.
**Specialist**: test
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~3h30m (20:41 → 00:12 UTC)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `702d681a3172c03fe875d807a1b4c68dd908f43d` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (test)` |
| Implementing Specialist | `test` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Client (owner, Domain-judgment TS rows) | `client` |
| Semantic Guard | `not spawned — env-test-only diff, no credential/Debug/credential-lifetime surface` |

---

## Task Overview

### Objective
Operator rulings (story 4, docs/user-stories/2026-10-31-survive-handler-loss.md): an MC meeting ends when its last participant leaves (195b7059); env-tests must not get slower; env-tests must not couple through global metric counters. MC-F (7cbe8e8e) added a global `mc_meetings_active == 0` precheck on every pool leg instead of fixing the injection suites' orphaned meeting. This devloop removes that gate and fixes the orphan at its source.

### Scope
- **Service(s)**: env-tests only (`crates/env-tests/`), docs. No MC/SDK product code (out of scope; a meeting that does not end after sessions close is a product bug → escalate with Loki evidence).
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED — operator rulings settle the design; no new ADR.

---

## Runtime Requirement (operator, REQUIRED 5)

Measured by the Lead against `devloop-mh-failure-transfer` (Kind), `ENV_TEST_ORG_SUBDOMAIN=devtest`, binaries prebuilt (`--no-run`), each binary timed individually, wall clock of `cargo test -p env-tests --features all --test <bin>`.

| Binary | Baseline (702d681a) | After | Delta | Notes |
|--------|---------------------|-------|-------|-------|
| 40_resilience | 557.7 s wall (529.48 s test; 10 passed) | 492.3 s (454.37 s); 40-after-39: 492.1 s (450.59 s) | **−65.4 s wall / −75 s test (−12–14%)** | FASTER ✔ (REQUIRED 6: 40 alone and 39→40 both PASS) |
| 26_mh_quic | 87.7 s (82.74 s; 11 passed, 1 ignored) | 113.1 s (102.78 s) in the sequence; see A/B below | A/B same-hour: +5.1 s test, of which meeting-end proofs = 3.0 s | meeting-end wait: CHURN 2.02 s, S4 1.01 s ✔ |
| 37_mh_stream_attribution | 79.1 s (75.73 s) | 77.0 s (67.72 s) | −2.1 s wall / −8.0 s test | ✔ |
| 38_mh_video_forwarding | 83.5 s (80.06 s) | 90.5 s (84.06 s) | +7.0 s wall / +4.0 s test | within one meeting-end wait + cluster noise (see A/B note) |
| 39_mh_video_keyframes | 85.8 s (79.83 s) | 93.5 s (84.13 s) | +7.7 s wall / +4.3 s test | within one meeting-end wait + cluster noise (see A/B note) |
| Rust env-test total (Layer 7, Σ per-binary "finished in") | 1437.75 s, 35 binaries — last full Layer-7 run (`/tmp/devloop/layer-7-env-test.log`, 2026-11-13 22:51, tree at 4188141c; env-test diff to 702d681a is mh_pool.rs only) | Gate 2 attempt 2: **1324.17 s**; attempt 3 (final tree): **1372.41 s** | **−113.6 s / −65.3 s** | did not grow ✔ |

Baseline run: 2026-11-14 20:42–20:59 UTC, sequential 40 → 26 → 37 → 38 → 39, all green.

**Interim proof run (pre-review tree, 2026-11-14 21:31–21:58 UTC)**, sequential 40 → 26 → 37 → 38 → 39 → 40, all green (REQUIRED 6: 40 alone, then 39 immediately followed by 40 — the orphan pair — both PASS). Wall: 40 = 495.7 s (−62.0; one implementer `layer-fast.sh` overlapped it), 26 = 92.0 s (+4.3), 37 = 70.1 s (−9.0), 38 = 87.5 s (+4.0), 39 = 86.0 s (+0.2), 40-after-39 = 503.1 s (−54.6). Loki over the window: 27 MC "Clean transport close", 21 "Meeting ended … last_participant_left"; the 11 grace-expiry lines are 26's out-of-scope drop-at-teardown tests. The Gate-3 Rust fixes change the code under test, so the final tree is re-timed and re-proven below before Gate 2.

**Final tree run (post-Gate-3, 2026-11-14 22:01–22:28 UTC)**, sequential 40 → 26 → 37 → 38 → 39 → 40, all green; figures in the table above. Code-reviewer F8/F9 (behaviour-neutral: an import and a panic string) were edited mid-run, so a rebuild may sit inside a wall figure; the test-time (`finished in`) column excludes it.

**A/B for 26 (cluster-noise control).** 26 read +20 s test in the sequence, so the Lead re-ran 26 back-to-back on the final tree and on a 702d681a worktree, `--test-threads=1 --nocapture`, timestamped per test: final 100.07 s vs base 95.02 s (+5.05 s). Per test, only the two injection tests moved materially (CHURN +1.6 s, S4 +1.2 s); the `[meeting_end]` lines report proofs of 2.02 s (CHURN, 3 participants, 2 handlers) and 1.01 s (S4, 2 participants, 2 handlers). The base binary itself ran 95.0 s vs 82.7 s at 20:5x: the cluster's per-run noise on these suites is ±10 s, so 37/38/39's ±4–8 s deltas are within one meeting-end wait (1–2 s) plus noise. The proof cost is the logged `proved in`, ~1–2 s per meeting. Lead fast check on the final tree: `layer-fast.sh` TOTAL_RESULT=OK.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/env-tests/src/fixtures/loki.rs` (`MeetingScope` + MC log-line consts hoisted from 47; pure matcher/classifier) | Mine |  |
| `crates/env-tests/src/fixtures/meeting_end.rs` (NEW: graceful leave + per-meeting proof) | Mine |  |
| `crates/env-tests/src/fixtures/mod.rs` (register `meeting_end`) | Mine |  |
| `crates/env-tests/src/fixtures/mc_session.rs` (`leave` graceful-close primitive) | Mine |  |
| `crates/env-tests/src/fixtures/participant.rs` (`Participant::leave -> Departure`) | Mine |  |
| `crates/env-tests/src/fixtures/metrics.rs` (remove `FOREIGN_MC_MEETINGS`, `await_no_mc_meetings`, `mc_meetings_active_total`; add `mc_disconnect_grace`) | Mine |  |
| `crates/env-tests/src/fixtures/mh_pool.rs` (drop the gate call + doc) | Mine |  |
| `crates/env-tests/src/fixtures/injection.rs` (`release_and_end_meeting`, the one public end path) | Mine |  |
| `crates/env-tests/src/fixtures/owned_placement.rs` (misses leave gracefully; `OwnedMeeting.missed`) | Mine |  |
| `crates/env-tests/tests/26_mh_quic.rs` (S4 + CHURN call sites) | Mine |  |
| `crates/env-tests/tests/37_mh_stream_attribution.rs` (call site) | Mine |  |
| `crates/env-tests/tests/38_mh_video_forwarding.rs` (call site) | Mine |  |
| `crates/env-tests/tests/39_mh_video_keyframes.rs` (call site) | Mine |  |
| `crates/env-tests/tests/40_resilience.rs` (module doc; 5 legs: fleet-gauge drain -> meeting-scoped proof) | Mine |  |
| `crates/env-tests/tests/47_mc_join_window.rs` (consume the hoisted scope) | Mine |  |
| `crates/env-tests/tests/30_observability.rs` (Loki precondition -> `loki::require_loki`, third consumer) | Mechanical |  |
| `crates/env-tests/tests/27_mc_slot_placement.rs` (inline `close(0,"leave")` -> `mc_session::leave`) | Mechanical |  |
| `crates/env-tests/tests/28_mh_egress_admission.rs` (same) | Mechanical |  |
| `crates/env-tests/tests/34_mc_kek_rotation.rs` (same) | Mechanical |  |
| `crates/env-tests/tests/46_mh_media_dial_admission.rs` (same) | Mechanical |  |
| `crates/env-tests/README.md` (meeting-end rule + tokens) | Mine |  |
| `packages/web-app/e2e/mhLeg.ts` (before-leg gate deleted; afterLeg drain replaced by the per-meeting proof; `legBounds`) | Domain-judgment | `client` |
| `packages/web-app/e2e/mcLoki.ts` (NEW: Loki client + pure verdicts) | Domain-judgment | `client` |
| `packages/web-app/e2e/env.ts` (`E2E_LOKI_URL`) | Domain-judgment | `client` |
| `packages/web-app/e2e/mcMetrics.ts` (`mcConfigGaugeMaxMs`, `mcDisconnectGraceMs`; `mcMeetingsActiveTotal` deleted) | Domain-judgment | `client` |
| `packages/web-app/e2e/mhLegRules.ts` (`FOREIGN_MC_MEETINGS`, `MEETINGS_NOT_DRAINED`, `SERVICE_JOB_SCRAPE_SETTLE_MS` deleted; `LOG_AGGREGATION_CEILING_MS`) | Domain-judgment | `client` |
| `packages/web-app/e2e/README.md` (leg BEFORE/AFTER lists; CRASH positive-control wording) | Domain-judgment | `client` |
| `packages/web-app/e2e/mh-replacement.spec.ts` (CRASH: B[any] stays the hard overlay control; A/C[slot of B] recorded, equal-timer race; header) | Domain-judgment | `client` (test co-reviews, ADR-0039 test-gate owner) |
| `packages/web-app/tests/mcLoki.test.ts` (NEW: drift pins + verdict tests) | Domain-judgment | `client` |
| `packages/web-app/tests/_rustSource.ts` (NEW: hoisted `strConst` + `logLineConst`) | Domain-judgment | `client` |
| `packages/web-app/tests/mhLeg.test.ts` (pins removed/updated; uses the shared helper) | Domain-judgment | `client` |
| `packages/web-app/tests/e2e-env.test.ts` (owns `E2E_LOKI_URL`) | Domain-judgment | `client` |
| `scripts/layer7.sh` (export `E2E_LOKI_URL`; Loki-not-ready WARN names its new dependents) | Minor-judgment | `infrastructure` |
| `docs/runbooks/devloop-validation.md` (§8: Loki WARN row; meeting-end token triage row) | Minor-judgment | `operations` |
| `docs/TODO.md` (delete the injection-orphan entry; one sentence on the `server_initiated` browser-leave entry) | Mechanical |  |
| `docs/specialist-knowledge/test/INDEX.md` (meeting-end pointer) | Mine |  |

---

## Planning

### Mechanism and invariant (narrowed, Lead ruling)
**A suite that releases MH state behind MC's back (the injection suites), or that 40's legs depend on (40's own legs and their placement misses), proves its OWN meeting ended and was released on its handlers before it returns, with evidence scoped to that meeting_id, never a fleet counter.** It does NOT claim every meeting-creating suite does: 24, 26's non-injection fns (incl. S5 and the local `mc_join` wrapper ~:922), 27, 28, 34, 35, 36, 45, 46 and 47 never release a meeting on MH behind MC, so MC's handlers keep heartbeating it and MC ends it after its disconnect grace. That is the operator's rule working, not a leak, and with the global gate gone nothing couples to them (Lead ruling; no TODO). This wording and reason go in `meeting_end.rs`'s module doc and the env-tests README evidence rule.

Root cause: the suites end by `drop` at test-fn return, so the runtime dies before quinn's implicit CONNECTION_CLOSE is sent. MC sees transport loss and holds the meeting for idle + grace. In the injection suites MH already released the meeting (our `EndMeeting`), so MC declares those MHs Dead and asks GC for a replacement (39 -> 40).

### Design
1. **One graceful-close primitive** `mc_session::leave(conn: &wtransport::Connection)` = `close(VarInt 0, b"leave")`. MC maps `ApplicationClosed` -> `ClientClosed` (`mc-service/src/webtransport/connection.rs:110`) -> immediate removal (`actors/meeting.rs` ~:2100), and the last removal ends the meeting (~:2224). The 7 existing copies (27:1270/1305, 28:616, 34:216/280/443, 46:303) call it. `Participant::leave(self) -> Departure { meeting_id: Uuid, participant_id, label }` closes and returns what the proof needs (ids never logged; failure text names labels).
2. **`fixtures/loki.rs`** (hoist from 47, on its 2nd consumer): `MeetingScope { cluster, loki, meeting_id: Uuid, since_ns }`, `LogLine`, `Entry`, `entries`, `await_entry(line, ceiling, poll, token, pick)`; PURE structural matcher (target + `fields.message` + `fields.meeting_id`). One home per line const, each citing its emitter: `MC_CREATED`, `MC_MEETING_ENDED` (controller reap, `cause`/`reason`), `MC_MEETING_RELEASED` (`mc.teardown` "Meeting released on its media handlers", `released`/`failed`/`quiesce_timed_out`, `teardown.rs:803`), `MC_CLEAN_CLOSE` (`mc.actor.meeting` "Clean transport close — removing participant immediately (grace skipped)", `participant_id`), `MC_GRACE_EXPIRED`. One `SCOPE_SKEW_SLACK` const (narrows the scan only, never orders). Query failure panics at once as `loki-query-failed` (as 47), never read as "not yet". The hint names `fixtures/loki.rs`. 47 migrates (drops its `MC_ENDED`/`MC_CREATED` copies; keeps 2 s poll as an argument).
3. **`fixtures/meeting_end.rs`**: `loki_precondition(cluster)` (token `PRECONDITION-loki-unavailable`, fail never skip) called at the START of each injection fn and each 40 leg; `await_ended(phase, cluster, prom, departures)` and `leave_and_await_ended(phase, cluster, prom, ps, also: Vec<Departure>)` (`also` = 40's placement misses, already left). Per meeting, in this order:
   a. **Positive control first** (obs F1/F3, test F2): one `MC_CLEAN_CLOSE` line per departed participant_id, under `LogAggregation` ceiling, token `clean-close-never-seen` (names labels missing). Participant ids are fresh per join, so no stale-incarnation risk. This is the primary proof of the graceful path and catches a forgotten participant; an ingest outage surfaces here, not as an MC defect.
   b. **End line** anchored on MC's own Loki clock (obs F2, test F3, security S2): accept only `MC_MEETING_ENDED` entries with ts >= anchor = max(since_ns, newest clean-close ts); EXACTLY ONE such entry; `cause=empty`, `reason=last_participant_left`. No anchor lines -> error, never "accept any". Token `meeting-not-ended` under ceiling = `mc_disconnect_grace` + `LogAggregation`; message: meeting_id, ceiling, grace read, close time, the exact LogQL, and "MC did not end an emptied meeting: product defect, not env (see 195b7059)".
   c. **Released** (test F1): exactly one `MC_MEETING_RELEASED` at/after the end line, `failed == 0`, `quiesce_timed_out == false` (token `meeting-not-released`). MH acknowledges `EndMeeting` for an unknown/already-released meeting as a no-op (`mh-service/src/grpc/mh_service.rs:864` test), which MC counts as `Released`, so `failed == 0` holds in the injection suites too. In 40, every leg ends with only live handlers in the set (dead/frozen ones were removed earlier in the leg), so `failed == 0` is asserted there as well. Ceiling: same budget from the end line; MC's `TEARDOWN_MAX` (2 x PUSH_QUIESCE_BOUND 16 s + 2 release attempts of 15 s + backoff, ~63 s) fits inside grace 30 s + ingest 45 s; stated in the doc.
   d. **Backup absence** (after the positive end line in the same scope = its positive control): no `MC_GRACE_EXPIRED` for this meeting since the anchor (token `meeting-ended-by-grace`); residual noted (fails open on a reword; (a) is the closed check). NOT asserting absence of "Participant disconnected, grace period started": MC documents a legitimate race where a ConnectionLost is processed before the queued ClientClosed (`meeting.rs` ~:2088) — that would flake.
   Poll 1 s. Pure verdict fns (pick/anchor/clean-close set/released fields) are separated from I/O.
4. **`mc_disconnect_grace(prom)`**: thin wrapper over `mc_config_gauge_max(prom, "mc_meeting_disconnect_grace_seconds")`, beside `mc_replacement_retry_budget`.
5. **`injection.rs`**: `release_and_prove` keeps its order and goes private; the ONE public end path `release_and_end_meeting(injections, &cleanup, cluster, prom, ps)` asserts `ps` non-empty and every `p.gc.meeting_id` equals the injections' (Uuid; Injection parses its id once, failing loudly naming it), releases+proves every pod, then `leave_and_await_ended`. "If cleanup is skipped" doc: on a panic `ReleaseOnDrop` still releases on MH and sessions drop abruptly (MC holds the meeting for its grace); acceptable only because 40 keeps `assert_baseline` `foreign-mh-activity` + the zombie pre-freeze re-assert.
6. **Call sites** (26 S4, 26 CHURN, 37, 38, 39): the suite's only MC sessions are the `ps` from `meeting_of` (verified: no other `mc_join` in those fns, nothing moved out of `ps`); passed by value. `loki_precondition` at fn start. MH-side sessions/subscribers dropped after.
7. **40**: `run_leg*` gate removed; `settled_meetings_active`/`await_meetings_drained` + import deleted; each of CRASH/ZOMBIE/STRANDED/PLANNED/NO-TARGET ends with `leave_and_await_ended(ps, meeting.missed)`. Module doc: `foreign-mc-meetings` paragraph replaced by the backstop statement and the ordering note (pool-leg binaries 40/44 are preceded by suites that either prove their meeting ended or open none).
8. **owned_placement.rs**: a miss calls `first.leave()` and pushes the `Departure` into `OwnedMeeting.missed`; proven in the leg's single end wait.
9. **metrics.rs**: delete `FOREIGN_MC_MEETINGS`, `await_no_mc_meetings`, `mc_meetings_active_total` (only 40 imports it; 30_observability/metric_hygiene checked at implementation).
10. **TS (Lead rulings): before-leg gate DELETED; after-leg drain REPLACED by a per-meeting Loki proof that survives TODO:1527.** Why replace (dry F1): the drain ran BEFORE `owner teardown` and guarded "MC re-placing a live meeting" (mhLeg.ts:361). TODO:1527 (docs/TODO.md, "MC classifies the browser SDK's CLEAN disconnect as `server_initiated`", owner meeting-controller, OUT of scope; Lead confirmed on cluster Loki) means a browser leave currently takes the grace path, so the proof must accept either departure path and must NOT pin the defect. Design:
    - **Env**: `E2E_LOKI_URL` in `e2e/env.ts` beside `E2E_PROMETHEUS_URL`; `scripts/layer7.sh` exports it from `ENV_TEST_LOKI_URL` beside the Prometheus export (:1512); unset = loud failure.
    - **Client** `e2e/mcLoki.ts`: plain-fetch `query_range`; classifier with Rust's semantics (non-2xx / unparseable / bad shape -> `loki-query-failed`, never "no lines"); structural matcher (app + JSON `target` + `fields.message` + `fields.meeting_id` / `fields.participant_id`).
    - **Meeting id (independent source, client P2/C1)**: per page, `page.waitForResponse` registered BEFORE the action that triggers the join collects EVERY 2xx GC join response through the bus `joined` event (the SDK join-retry, `sdk-core/src/session/joinRetry.ts`, can make several); only `meetingId` is read from the parsed body; body and token are never kept, logged or annotated (credentialScan / `assertTokenOnlyJoinTraffic` stay green). All of a meeting's members' responses must name ONE id, else `meeting-id-unresolved` (own token). Every meeting the leg opened is proven (placement misses included).
    - **Participant ids**: the bus `joined` event's `participantId` (`e2eBus.ts:256`), filled from the JoinResponse (`SignalingClient.ts:740`).
    - **Anchor (client C2, security S2 twin)**: `anchor = max(sinceNs, newest MC_CREATED ts for this meeting_id)`; `sinceNs` captured per leg before the first join (no fixed lookback). No created line after `sinceNs` -> `created-never-seen` (own token), never "accept any" (MC can end the meeting by `join_window_expired` and recreate it under the same id during a join retry).
    - **Verdict order** (pure functions):
      1. **Departure positive control (Lead ruling)**: ONE departure line per participant id at/after the anchor, where `MC_CLEAN_CLOSE` OR `MC_GRACE_EXPIRED` counts, each naming the GC meeting id; awaited first under the grace+ingest ceiling, token `departure-never-seen` (names labels missing). Fails closed on an ingest gap and survives the TODO:1527 fix (the wait then shrinks by itself).
      2. **End**: exactly one `MC_MEETING_ENDED` at/after the anchor, ts >= the newest departure ts (MC's Loki clock), cause=empty, reason=last_participant_left (the grace path ends the same way, `controller.rs:1290-1299`); token `meeting-not-ended`.
      3. **Released**: exactly one `MC_MEETING_RELEASED` at/after the end, `handlers >= 1 && released == handlers && failed == 0 && !quiesce_timed_out`; token `meeting-not-released`.
      No grace-absence assertion, no clean-close-only assertion (test (b): no dead TS verdicts or tokens).
    - **Ceiling**: `mcConfigGaugeMaxMs(promql)` generalized from `mcReplacementRetryBudgetMs` (twin of Rust `mc_config_gauge_max`); `mcReplacementRetryBudgetMs` and new `mcDisconnectGraceMs` are thin wrappers; ceiling = grace + the e2e ingest ceiling (no scrape settle: Loki). NOT `mcRotationBudget().graceSeconds` (config-file read).
    - **`legBounds`**: `testInfo.setTimeout` drops `2 * meetingsBoundMs` and adds 1x (grace + ingest) for the single after-leg proof.
    - **Cost**: about one grace per leg (today's drain paid grace + retry + scrape); the deleted before-leg gate is a net saving.
    - **`afterLeg`**: `leave members` -> `meeting ended (per-meeting proof)` -> `owner teardown`; site comment cites docs/TODO.md:1527 and explains the ordering. TODO:1527 gains one sentence: when fixed, the departure lines become clean closes and the browser wait drops to ~ingest (the proof needs no change; optionally tighten to clean-close only then).
    - **Drift pin**: `strConst` hoisted from `tests/mhLeg.test.ts:282` into a shared test helper; `tests/mcLoki.test.ts` pins ONLY the consts TS uses (created, clean-close, grace-expired, ended, released: app/target/message) and its tokens to the Rust consts in `crates/env-tests/src/fixtures/loki.rs` / `meeting_end.rs`. The extractor fails with its own token (`rust-const-not-found`) when a const is missing or not a single plain string literal; one adverse case on a fixture lacking the const goes red.
    - **Node tests**: every token: `meeting-id-unresolved` (two ids across retries/members), `created-never-seen`, `departure-never-seen` (one id missing; a departure naming another meeting), end before newest departure, two end lines, end-recreate-end sequence (only the post-anchor end counts), wrong cause/reason, release missing, handlers==0, released<handlers, failed>0, quiesce timeout, Loki error as its own token.
    - **Deleted**: `awaitMcMeetingsZero`, `mcMeetingsActiveTotal` + PromQL const, `MEETINGS_NOT_DRAINED`, `FOREIGN_MC_MEETINGS` + pin, `meetingsBoundMs`.
11. **Docs**: delete TODO entry; add one sentence to the TODO "MC classifies the browser SDK's CLEAN disconnect as `server_initiated`" entry (when fixed, add the clean-close check to `mcLoki` and the browser wait drops to ~ingest); MC-F main.md note; README evidence rule (narrowed rule + `meeting_end` + tokens `PRECONDITION-loki-unavailable`, `clean-close-never-seen`, `meeting-not-ended`, `meeting-not-released`, `meeting-ended-by-grace`, `loki-query-failed`); e2e README before/after lists; `foreign-mc-meetings` removed everywhere.

### Step 3 (released, failed==0) in 40's legs (Lead check)
`failed` counts only the teardown's `end_meeting_plan` calls, i.e. the handlers in the meeting's set AT the end (`teardown.rs` `run`); a handler REMOVED earlier is released by its own `release_removed` task (`removals`, uncounted, awaited only for the fence). MC pushes every set change to participants, so a participant's view excluding a handler means MC's set already excludes it. Per leg, at the close:
- **CRASH**: the killed incarnation is removed and a replacement added (`await_replacement` per participant), and the replacement's dial token admits a dial (`reach`); the owned MH restarted in place. Set = survivor + replacement, both live.
- **ZOMBIE**: the frozen MH is removed while STILL frozen (mandatory first assertion), then resumed; the leg then observes its release verdict (heartbeat or buffered EndMeeting), so the removal release has completed. Set = whatever replacement MC holds (pins nothing about size) or empty; any member is live (a not-yet-registered replacement answers EndMeeting as an acknowledged no-op, `mh_service.rs:864`). 
- **STRANDED**: owned MH killed; removal + per-participant replacement asserted, pair heard on the new refs. Set all live.
- **PLANNED**: drained X removed after Y added (make-before-break asserted); final set = survivor + Y at the pre-drain size, pair heard off X. All live.
- **NO-TARGET**: each set becomes exactly the survivors (asserted), pair heard off the drained MH. All live.
So `failed == 0` and `quiesce_timed_out == false` are the true expectation in every leg (and every injection suite); no per-site relaxation.
**Non-vacuity (observability):** `failed == 0` is trivially true for an empty plan, so the verdict also requires `released == handlers` and `handlers >= 1` whenever any departing participant's last-seen handler set was non-empty (derived per call from the participants' own views, not a per-site literal; ZOMBIE is the one leg whose set may legitimately be empty at close if MC-C's replacement has not landed, and then the participant's view is empty too). Hermetic test for the empty-plan case.
**Timing wording (observability):** the released line is emitted only after the meeting's outstanding handler-removal release tasks finish (`teardown.rs` ~:790), so a removal still in flight against a crashed/frozen pod delays it. In every leg the removal happened well before the close (it is asserted earlier in the leg, ZOMBIE even observes its release verdict), so this is normally already done; the grace+ingest ceiling covers `TEARDOWN_MAX` (~63 s) either way.

### Hermetic tests (test F4, security S2)
In `loki.rs`/`meeting_end.rs` `#[cfg(test)]`: matcher negatives (right message wrong meeting_id; wrong target; non-JSON); pick over (a) no anchor line -> error, (b) end line older than the newest clean-close/created anchor -> rejected, (c) valid; exactly-one (two end lines after anchor -> error); wrong cause/reason; one participant without clean-close; grace line present; teardown missing; `failed > 0`; `quiesce_timed_out`; Loki error -> `loki-query-failed`, never "not ended" (a pure classifier over `Result<Vec<Entry>, LokiError>`).

### Freshness of meeting ids (test F3)
26 S4 and CHURN each `meeting_of` a fresh GC meeting; each 40 leg and every placement attempt `create_meeting`s a new one; no fn re-joins an ended id (47's re-join is outside the rule). The anchor + exactly-one guard covers a reused id anyway.

---

## Pre-Work

None.

---

## Implementation Summary

**Rust (`crates/env-tests`)**
- `fixtures/loki.rs`: 47's scope hoisted as `MeetingScope` (`entries`, `await_entry(line, ceiling, poll, token, pick)`), `LogLine` + one const per matched MC line (`MC_CREATED`, `MC_MEETING_ENDED`, `MC_MEETING_RELEASED`, `MC_CLEAN_CLOSE`, `MC_GRACE_EXPIRED`), `SCOPE_SKEW_SLACK`, `Entry` field accessors, pure `logql_filter` / `meeting_logql` (Uuid-typed id) / `matching_entries` / `classify_scoped` (`LOKI_QUERY_FAILED` is its own token). Hermetic tests: matcher negatives, LogQL shape, Loki error never "no lines".
- `fixtures/meeting_end.rs` (NEW): `Departure`, `leave_and_await_ended`, `await_ended`. Per meeting: clean-close per departed participant id (`clean-close-never-seen`, ingest ceiling) -> anchor = newest clean-close (MC's Loki clock) -> exactly one end, `empty`/`last_participant_left` (`meeting-not-ended`, grace + ingest) -> exactly one release after it, `failed 0`, no quiesce timeout, `released == handlers`, `handlers >= 1` when a departing participant saw a non-empty set (`meeting-not-released`) -> backup: no grace-expiry for a departed id (`meeting-ended-by-grace`). Pure verdicts with hermetic tests for every token (anchor absent / older end rejected / exactly one / wrong cause / empty plan / failed / quiesce / two releases / missing fields).
- `mc_session::leave` (one graceful-close primitive; the 7 inline copies in 27/28/34/46 now call it) and `Participant::leave -> Departure`.
- `metrics.rs`: `FOREIGN_MC_MEETINGS`, `await_no_mc_meetings`, `mc_meetings_active_total` deleted; `mc_disconnect_grace` (wrapper over `mc_config_gauge_max`).
- `mh_pool.rs`: gate call removed from `run_leg_with` (comment names the backstop).
- `injection.rs`: `release_and_prove` private; `release_and_end_meeting(phase, injections, cleanup, cluster, prom, ps)` = assert ps non-empty and in the injected meeting (Uuid), release+prove every pod, then `leave_and_await_ended`. Docs: "Ending the meeting" + panic path in "If cleanup is skipped".
- Call sites 26 S4, 26 CHURN, 37, 38, 39: `loki::require_loki` at fn start; cleanup -> `release_and_end_meeting`.
- `40_resilience.rs`: `settled_meetings_active` / `await_meetings_drained` + import deleted; `loki::require_loki` at each of the 5 legs' start; each leg ends with `leave_and_await_ended(..., ps|vec![z], meeting.missed)`. Module doc: no fleet counter, ordering assumption, panic backstop.
- `owned_placement.rs`: a miss `leave()`s and is carried in `OwnedMeeting.missed`.
- `47_mc_join_window.rs`: consumes the hoisted scope and consts (behaviour unchanged, 2 s poll).
- README: meeting-end rule + token table.

**TS (`packages/web-app`)**
- `e2e/mcLoki.ts` (NEW): plain-fetch `query_range` + `classifyQueryRange` (`loki-query-failed`), `logqlFilter` / `meetingLogql` (UUID-only) / `matchingEntries`, pure verdicts `resolveMeetingId` (`meeting-id-unresolved`), `missingDepartures` (`departure-never-seen`; clean close OR grace expiry), `createdAnchor` (`created-never-seen`), `pickEnd`, `releasedVerdict`; `proveMeetingsEnded`. `env.ts` imported lazily (pure parts node-testable).
- `e2e/mhLeg.ts`: before-leg gate and `awaitMcMeetingsZero` deleted; `watchGcJoins` (listener installed at member open; reads only `meetingId` of every 2xx `GET /api/v1/meetings/{code}`), `departureOf` (bus `joined` participant ids), `recordAndClose` (records then always closes; used by `release` and AFTER); AFTER: leave each member -> `meetings ended (per-meeting proof)` -> owner teardown, with the site comment (ordering, TODO "server_initiated"). `legBounds`: `meetingEndCeilingMs = mcDisconnectGraceMs() + LOG_AGGREGATION_CEILING_MS`, replacing `2 * meetingsBoundMs`; scope start per leg before any join.
- `e2e/mcMetrics.ts`: `mcConfigGaugeMaxMs` (twin of `mc_config_gauge_max`), `mcReplacementRetryBudgetMs` / new `mcDisconnectGraceMs` wrappers; `mcMeetingsActiveTotal` deleted.
- `e2e/mhLegRules.ts`: `FOREIGN_MC_MEETINGS`, `MEETINGS_NOT_DRAINED`, `SERVICE_JOB_SCRAPE_SETTLE_MS` (now unused) deleted; `LOG_AGGREGATION_CEILING_MS` (pinned to eventual.rs).
- `e2e/env.ts`: optional `E2E_LOKI_URL` (no default; `mcLoki` fails loudly when unset); `scripts/layer7.sh` exports it from `ENV_TEST_LOKI_URL`.
- `tests/_rustSource.ts` (NEW): hoisted `strConst` + `logLineConst`, both failing with `rust-const-not-found` on a missing/reshaped const. `tests/mcLoki.test.ts` (NEW): drift pins (5 log lines, shared tokens, scope slack), adverse extractor cases, every verdict token incl. end-recreate-end. `tests/mhLeg.test.ts`: uses the shared helper; FOREIGN/scrape-settle pins removed; LogAggregation ceiling pinned.

**Docs**: TODO entry deleted; one sentence added to the "MC classifies the browser SDK's CLEAN disconnect as `server_initiated`" entry; INDEX pointer; e2e README leg lists.

**Gate-3 fixes (iteration 1)**
- Security F1: `env.ts` `validatedUrl` refuses URLs carrying credentials (never echoing them), for every URL var; test in `tests/e2e-env.test.ts`.
- Client F1/F2/F3 + observability O1 + ops F3 (TS): `pickEnd(ends, anchor, departedAt)` fails an end before the last departure; ONE deadline for departure/created/end of every meeting + ONE release deadline after it (`meetingEndBudgetMs` = 2 x ceiling, reserved by `legBounds`); `readOptionalUrl` has no fallback literal.
- Same twin in Rust (Lead): ONE deadline (grace + ingest) for clean-close + end of every meeting, ONE release deadline after it; `pick_end(entries, since, newest_close)` fails two ends, an end before the newest clean close, or another cause.
- Code-reviewer F1-F7 / DRY D1-D5: one poll kernel `MeetingScope::poll_until` (`await_entry` wraps it); `loki::require_loki` + `PRECONDITION_LOKI_UNAVAILABLE` (47, meeting_end, all call sites); 47 uses `since_now_ns` and the hoisted `MC_WINDOW_FIRED` / `GC_ENDED`; `named_ids` helper; `Injection.meeting_id: Uuid`; `prove_meeting_ended` extracted; TS `poll` flat; `classifyMember` -> `participant-id-unresolved` (Lead ruling: loud); TS precondition token exported + pinned.
- Observability O2: `loki::EMITTERS` + hermetic `every_log_line_const_matches_its_emitter` (exact message literal with its nearest `target:`), plus an adverse test.
- Test R1/R3/R4: step 4 fails on ANY grace expiry for the meeting (passed participants by label, others by count); `meeting-not-ended` text names "a participant not passed to the end path"; isolated `released < handlers` test; scope starts at the earliest participant's JOIN (`Participant.joined_ns`) minus the slack.
- Code-reviewer F8/F9 + DRY D7: `30_observability` uses `require_loki` (generic message); `meeting_end`'s unused re-export dropped.
- Gate 2 L7 (Lead ruling, option a): CRASH leg's A/C[slot of B] overlay gates were a pre-existing coin flip (`MH_MAX_IDLE_TIMEOUT_MS` == `MC_MH_HEARTBEAT_TIMEOUT_MS`); B[any] stays the hard positive control, A/C recorded as `observed-overlay-on-slot-of-B`. Timers untouched.
- Ops F1/F2/F4: layer7.sh Loki WARN and runbook rows name every Loki-dependent test + the tokens; §8 triage row by lane; `meeting_end.rs` §Bounds cites `teardown_fence_hold_max_seconds` instead of numbers.

**Not done in-tree (host action for @team-lead)**: the superseded note on MC-F's `docs/devloop-outputs/2026-11-12-mc-planned-transfer/main.md`. `validate-cross-boundary-scope` fails any edit touching two devloop `main.md`s (`cross-boundary-multi-devloop-collision`), so it must land in its own commit. Text, for its Accepted Deferrals pointer: "SUPERSEDED 2026-11-14 by `docs/devloop-outputs/2026-11-14-injection-orphan-meeting-fix/`: the compensating `MhPool::run_leg*` gate (`metrics::await_no_mc_meetings`, token `foreign-mc-meetings`) and its browser twin were removed; the injection suites end their own meeting via `injection::release_and_end_meeting`, proved from per-meeting MC log lines (`fixtures/meeting_end.rs`); the TODO entry is deleted."

**Validation**: `./scripts/layer-fast.sh` TOTAL_RESULT=OK (L1-L6). Cluster runs (40; 39 then 40) done by the Lead, all green (§Runtime Requirement).

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Client | confirmed |

---

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | F1 TS readUrl accepted credentialed URLs |
| Test | RESOLVED-FIXED | 4 | 4 | 0 | R1 unlisted-participant grace, R2 TS silent skip, R3 released!=handlers test, R4 scan window |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 | O1 TS shared deadline, O2 LogLine emitter pins |
| Code Quality | RESOLVED-FIXED | 9 | 9 | 0 | F1-F9 (F8/F9 behaviour-neutral, built in Lead layer-fast) |
| DRY | RESOLVED-FIXED | 7 | 6 | 0 | D1-D5, D7 fixed; D6 (MC-F main.md note) = Lead separate doc-only commit (guard blocks two devloop main.md in one diff) |
| Operations | RESOLVED-FIXED | 4 | 4 | 0 | F1 Loki WARN, F2 runbook triage row, F3 TS budget, F4 doc numbers |
| Client | RESOLVED-FIXED | 3 | 3 | 0 | F1 early end, F2 budget, F3 dead default (+ unjoined-member ruling) |

---

## Accepted Deferrals

None.

---

## Validation

Fast checks: implementer and Lead `layer-fast.sh` TOTAL_RESULT=OK on every iteration. Cluster proof (REQUIRED 6) and per-binary timings: §Runtime Requirement. Gate 2: attempt 3 TOTAL_RESULT=OK (table below).

### Gate 2 attempts

| Attempt | L1-6 | L7 | Notes |
|---------|------|----|-------|
| 1 | OK | FAIL `env-tests-failed` (compile, 118 s; no test ran) | **Lead-induced stale build, not a code defect.** For the 26 A/B the Lead built a 702d681a worktree with `CARGO_TARGET_DIR=/work/target`; cargo's path-independent metadata hash plus workspace-relative dep-info let that older `env-tests --features all` lib artifact be judged fresh against /work's (older-mtime) sources, so Layer 7 compiled the current tests against the base lib (`unresolved import fixtures::meeting_end`, `no field missed`). Layers 1-6 (incl. L4 nextest without `--features all`) were green. Fix: `cargo clean -p env-tests`; `cargo test -p env-tests --features all --no-run` then built clean. Not counted against the L7 test budget (no env-test executed); recorded here rather than masked. Log: `gate2-attempt1.log`. |
| 2 | OK | FAIL `browser-e2e-failed` (L7 2317 s; **L7 attempt 1 of 2 consumed**) | **Rust env-tests ALL GREEN**: 35 binaries, Σ `finished in` = **1324.17 s vs 1437.75 s reference (−113.6 s, −7.9%)** — REQUIRED 5 total ✔. Browser E2E 32 passed / 1 failed: `mh-replacement.spec.ts:225` CRASH leg — "A never showed the reconnecting overlay on slot 1 … Overlay transitions after the event: []". PLANNED and STRANDED legs passed (incl. the new per-meeting afterLeg proof). Loki: CRASH meeting 4d4fcb4c on owned s0 + s5; B (85e80250) on s0 only from 23:23:03.5; MC declared s0 Dead (silence) 23:23:09.045, replacement s4 granted, B on s4 at 23:23:10.557. Four earlier specs' meetings (members gone, MC grace, on s4/s5) were still live when the leg started — the removed before-leg gate would have waited for them. Diagnosis (causal vs pre-existing flake) routed to client + implementer. Log: `gate2-attempt2.log`; evidence copied to the Lead's scratchpad. |

**Attempt-2 failure → in-loop fix (Lead ruling).** Client diagnosed (implementer concurring): a race pre-existing in the C3 spec (3b3ae16e), not caused by this diff. `MH_MAX_IDLE_TIMEOUT_MS` == `MC_MH_HEARTBEAT_TIMEOUT_MS` (6 s each, by guarded design); SIGKILL sends no CONNECTION_CLOSE, so browser loss detection and MC's dead-declaration race with arbitrary phase. MC won (s0 dead 23:23:09.045, detection_ms=6001); A still held s5, so `slotContinuity.ts handlerRemoved` took the planned-transfer hold and correctly showed no overlay (trace: A overlays=[] and transportFaults=[] for ~35 s). The four in-grace meetings were on s4/s5, never s0, and cannot move either timer. Fix (option a): `mh-replacement.spec.ts` CRASH hard-gates only B[any] (blocked to s0, so no other connected handler: overlay in both detection orders); A/C slot-of-B recorded as the `observed-overlay-on-slot-of-B` annotation; header + e2e README updated; timers untouched. Re-review: client RESOLVED-FIXED, test RESOLVED-FIXED (lower tier `sdk-core slotContinuity.test.ts` pins every shape). Lead `layer-fast.sh` TOTAL_RESULT=OK. Browser suite runtime at attempt 2: 845 s vs 834 s at C3's Gate 2 (the per-meeting end proof replaces the old drain at ~1 grace per leg; before-leg gate removed).
| 3 | OK | **OK** (L7 2179 s; L7 attempt 2 of 2) | **TOTAL_RESULT=OK, `gate2-verdict` GATE2=PASS.** Rust env-tests 35/35 green, Σ 1372.41 s (vs 1437.75 s reference). In-gate per-binary test time: 40_resilience 463.27 s (baseline 529.48 s), 26 108.24 s, 37 73.84 s, 38 86.14 s, 39 74.29 s (26's in-gate figure is cluster noise per the A/B above; its two meeting-end proofs log 1.0 s and 2.0 s). Browser E2E 33/33 passed in 776 s (C3's Gate 2: 834 s; attempt 2: 845 s), including all three mh-replacement legs with the per-meeting end proof. Note: the implementer's `layer-fast.sh` overlapped the first ~6 min of this run (layers 1-4 / L7 deploy; check-only, no tree change). Log: `gate2-attempt3.log`. |

---

## Final Summary

The MC-F compensating control is gone: `metrics::await_no_mc_meetings`, `FOREIGN_MC_MEETINGS` and `mc_meetings_active_total` are deleted with every Rust call site, and the browser twin's before-leg gate and after-leg global drain are deleted too. No env-test waits on or asserts a fleet meeting counter any more. The orphan is fixed at its source: `injection::release_and_end_meeting` (the only public end path, used at all 5 injection call sites) keeps EndMeeting-on-pod first, then closes every session gracefully (`mc_session::leave`) and proves THAT meeting ended from MC's own Loki lines (`fixtures/meeting_end.rs`: per-participant clean close → exactly one `last_participant_left` end → exactly one release with zero failures), bounded by the grace config echo plus Loki ingest; 40_resilience's five legs use the same proof. The browser legs carry a TS twin (`e2e/mcLoki.ts`) that accepts a grace-expiry departure while MC's browser-leave classification defect (docs/TODO.md, owner meeting-controller) stands. Runtime: 40_resilience −62 to −75 s, injection suites +1–2 s meeting-end proof each (within noise), Rust env-test total down. Gate 2 also surfaced a pre-existing race in the C3 CRASH spec (equal 6 s timers), fixed in-loop with owner review.

