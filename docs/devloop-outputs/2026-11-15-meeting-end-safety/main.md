# Devloop Output: Meeting ending correct and prompt (story 4 follow-up, before T-2)

**Date**: 2026-11-15
**Task**: Story 4 follow-up — make meeting ending correct and prompt: (1) GC cleanup must not end rows of live (degraded/draining) MCs; (2) no optional/fallback join-window (and sibling) config; (3) NotifyMeetingEnded incarnation-safe; (4) browser clean leave removes the participant at once. Close/narrow the two TODO entries, correct ADR-0010 §3.
**Specialist**: meeting-controller (paired: global-controller)
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~2h55m (06:30 → 09:25 UTC)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c8fb634c875e6ee03f1346bb867165b5edd56019` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `meeting-controller` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Paired global-controller | `paired-global-controller` |
| Database (conditional: GC SQL) | `database` |
| Protocol (conditional: likely internal.proto change) | `protocol` |
| Client (conditional: SDK leave path / browser E2E budgets) | `client` |
| Auth-controller (internal.proto GSA co-owner) | `auth-controller` |

---

## Task Overview

### Objective
Operator task text (verbatim): `~/.cache/devloop/story-runs/story-runner/2026-10-31-survive-handler-loss/meeting-end-safety.prompt.md`. Since 195b7059 meeting ends are routine, so the end path must be correct (GC must not end live MCs' rows; a lost notify must not strand a meeting id), config must fail loudly (no optional/fallback), and a browser's deliberate leave must not be held through the reconnect grace.

### Scope
- **Service(s)**: MC, GC (+ proto if the incarnation fix needs it; SDK/web-app only if the leave signal needs it; MC deployment manifests)
- **Schema**: Yes — `migrations/20261115000001_meeting_assignment_id.sql` (`meeting_assignments.assignment_id UUID NOT NULL`, no default; roll-forward only, rollback order in its header and gc-deployment.md)
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED — operator task fixes the problems and constraints; ADR-0010 §3 is corrected, no new ADR.

---

## Runtime Requirement (operator AVOID: no Layer 7 runtime growth)

Baseline = the last full Gate 2 on this branch (`docs/devloop-outputs/2026-11-14-injection-orphan-meeting-fix/gate2-attempt3.log`, tree a1c475ba; c8fb634c changes only run-story scripts): Layer 7 2179 s; Rust env-tests Σ `finished in` 1372.41 s (35 binaries); browser E2E 776 s (33 passed).

| Measure | Baseline | After | Delta |
|---------|----------|-------|-------|
| Layer 7 total | 2179 s | 2417 s | +238 s, all cluster rebuild: provision 81 s + deploy 310 s (provision.sh changed → rebuild) vs 3 s + 110 s in the baseline run. Excluding setup: 2066 s → 2026 s (−40 s) |
| Rust env-tests Σ | 1372.41 s | 1364.88 s (35 binaries, all green) | −7.5 s (no growth) |
| Browser E2E | 776 s | 581 s (33 passed) | **−195 s** (clean leave: KEK-rotation budgets drop the grace, MH-leg end proofs drop ~one grace each) |

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mc-service/**` | mine | — |
| `crates/common/**` (`meeting_lifecycle.rs`: shared join-window floor + `AssignmentId`; not a GSA file) | Minor-judgment | mine + global-controller |
| `crates/gc-service/**` (P1, P2-GC, P3-GC) | Domain-judgment | global-controller (paired implements) |
| `crates/gc-test-utils/**` | Domain-judgment | global-controller (paired implements) |
| `migrations/20261115000001_meeting_assignment_id.sql` | Domain-judgment — GSA | database |
| `proto/dark_tower/internal/v1/internal.proto` (`assignment_id`, `controller_id`, Notify outcome) | Domain-judgment — GSA | protocol |
| `proto/dark_tower/signaling/v1/signaling.proto` (`LeaveRequest`) | Domain-judgment — GSA | protocol |
| `crates/proto-gen/**` (roundtrip tests) | Minor-judgment — GSA | protocol |
| `packages/proto-gen/**` (verify-codegen presence entries) | Minor-judgment — GSA | protocol |
| `packages/sdk-core/**` (leave window, `LEAVE_ACK_TIMEOUT_MS`, `disconnected` state, tests) | Domain-judgment | client |
| `packages/web-app/**` (e2e grace budgets, mcLoki proof + Node tests) | Minor-judgment | client + test |
| `crates/env-tests/**` (loki.rs leave line; 01/02 required-key rows) | Minor-judgment | test + observability |
| `crates/dt-guard/**` (env-config check 7) | Domain-judgment | infrastructure (Lead: operations reviews) |
| `crates/mh-service/**` (P2 required keys) | Minor-judgment | media-handler (via review; Lead ruling) |
| `crates/ac-service/**` (P2 required keys) | Minor-judgment | auth-controller (on panel) |
| `crates/ac-test-utils/**` | Minor-judgment | auth-controller (on panel) |
| `infra/services/mc-service/**` | Minor-judgment | infrastructure (operations reviews) |
| `infra/services/gc-service/**` | Minor-judgment | infrastructure (operations reviews) |
| `infra/services/mh-service/**` | Minor-judgment | infrastructure (operations reviews) |
| `infra/services/ac-service/**` | Minor-judgment | infrastructure (operations reviews) |
| `infra/kind/scripts/**` (provision AC_HASH_SECRET; iterate.sh base config) | Minor-judgment | infrastructure (operations reviews) |
| `infra/kubernetes/overlays/kind/services/mc-service/**` (comment) | Minor-judgment | infrastructure (operations reviews) |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | Minor-judgment | observability |
| `infra/docker/prometheus/rules/gc-alerts.yaml` | Minor-judgment | observability |
| `infra/docker/ac-service/README.md` (required AC keys) | Minor-judgment | auth-controller (on panel) |
| `docs/API_CONTRACTS.md` (LeaveRequest; Assign/Notify contract) | Minor-judgment | protocol |
| `infra/grafana/dashboards/**` | Minor-judgment | observability |
| `docs/observability/**` | Minor-judgment | observability |
| `docs/runbooks/**` | Minor-judgment | operations |
| `docs/decisions/adr-0010-global-controller-architecture.md` (§3) | Minor-judgment | global-controller (paired drafts) |
| `docs/DATABASE_SCHEMA.md` | Minor-judgment | database |

GSA co-reviewers (outside the Owner column): migration — security also reviews; `internal.proto` — security and auth-controller also review (GSA mirror owners); `signaling.proto` — security also reviews (LeaveRequest authority).

---

## Planning

### P1 — mechanism: GC reads `health_status` as liveness
`health_status` is placement eligibility; liveness is heartbeat freshness. Status is wrong both ways: a degraded/draining/pending (re-registered) MC still heartbeats, and a draining MC that dies is never re-marked (`mark_stale_controllers_unhealthy` skips draining). **Sites (one class, all fixed):** `get_healthy_assignment` :105 (reuse) → `get_live_assignment`, any status, fresh heartbeat; `atomic_assign` failover arm :339 → re-point only ended rows or STALE MCs; `end_stale_assignments` :561 + :578 → STALE MCs only. **Not in class:** `get_candidate_mcs` :218 (NEW-meeting placement keeps healthy+fresh); MH supersede `decide` (ADR-0039 §7 identity rule). **Predicate:** written ONCE as a SQL fragment (positive form: `mc.last_heartbeat_at < NOW() - <threshold>`), used as-is by cleanup (subquery AND outer re-assert, identical text) and failover, negated by reuse. **Why not `= 'unhealthy'` (observability) or `'unhealthy' OR stale` (database):** a dead draining MC never becomes `unhealthy` (leak); and `unhealthy` is set by GC only on staleness, so the OR adds only a self-reported-unhealthy MC that still heartbeats — alive, so ending/failing over its rows splits the meeting. The positive heartbeat predicate enumerates no status, so no future status can silently qualify. **Threshold SSoT:** the configured `mc_staleness_threshold_seconds` threaded into the repo fns; `DEFAULT_HEARTBEAT_STALENESS_SECONDS` deleted. Operator view: a draining/degraded MC's rows persist while it heartbeats; once it stops, the next join fails the row over and the backstop ends it after `GC_INACTIVITY_HOURS` (runbooks say so). **Tests (fail on c8fb634c):** `meeting_assignment_tests.rs` matrix — fresh healthy/degraded/draining/pending/unhealthy-status rows are NOT ended (asserted per meeting_id), stale healthy-status and stale draining rows ARE; the same matrix for the failover arm and reuse.

### P2 — mechanism: a setting whose absence silently substitutes a value
**Invariant (Lead rulings 1 + 4):** in ALL FOUR services (AC, GC, MC, MH), every key whose absence makes code substitute a value — topology (region, peer URLs, bind addresses), behaviour (timers, bounds, costs), `OTEL_SAMPLE_RATE`, `DEPLOYMENT_ENVIRONMENT` — is REQUIRED via the existing `MissingEnvVar` + `bounded` idiom (key name and parse error in the error), has a value in the base `config.env`, and is read by a HARD ref in every workload of that service. Default-value constants and `optional_seconds()` are deleted.
**OUT, uniformly:** `OTEL_ENABLED` / `OTLP_ENDPOINT` (unset = export off: a mode), `MC_ID` / `GC_ID` / `MH_HANDLER_ID` (identity derivation; MUST stay per-process — a stable id would re-strand a restarted MC's rows, database note), retired optional-ref keys (Lead ruling 2: dead config; TODO "Delete the story-3 retired config keys" annotated as ready to execute).
**Enumeration (non-test code of each `config.rs`; re-verified by grep at implementation and recorded in §Implementation Summary):**
- MC (mine): `MC_MEETING_JOIN_WINDOW_SECONDS` (derived value = ceiling only), `MC_DISCONNECT_GRACE_PERIOD_SECONDS` (today no value, no ref), `MC_CLOCK_SKEW_SECONDS`, `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS`, `MC_REGION`, `GC_GRPC_URL`, `MC_{WEBTRANSPORT,GRPC,HEALTH}_BIND_ADDRESS`, `OTEL_SAMPLE_RATE`, `DEPLOYMENT_ENVIRONMENT`.
- GC (paired): `MC_STALENESS_THRESHOLD_SECONDS`, `GC_CLEANUP_INTERVAL_SECONDS`, `GC_INACTIVITY_HOURS`, `GC_RETENTION_DAYS`, `BIND_ADDRESS`, `GC_GRPC_BIND_ADDRESS`, `GC_REGION`, `AC_JWKS_URL`, `AC_INTERNAL_URL`, `JWT_CLOCK_SKEW_SECONDS`, rate-limit RPM, `OTEL_COLLECTOR_ENDPOINT`, `TELEMETRY_PROXY_MAX_BYTES`, `OTEL_SAMPLE_RATE`, `DEPLOYMENT_ENVIRONMENT` (CORS_ALLOWED_ORIGINS: OUT — unset = empty allowlist, fail-closed, a mode; paired additionally makes an unparseable entry refuse startup instead of being dropped with a warn).
- MH (me, Minor-judgment, media-handler via review): `MH_{GRPC,HEALTH,WEBTRANSPORT}_BIND_ADDRESS`, `MH_REGION`, `GC_GRPC_URL`, `OTEL_SAMPLE_RATE`, `DEPLOYMENT_ENVIRONMENT`; `DEFAULT_*` consts at :22-28 deleted.
- AC (me, Minor-judgment, auth-controller reviews): `BIND_ADDRESS`, `JWT_CLOCK_SKEW_SECONDS`, `BCRYPT_COST`, `OTEL_SAMPLE_RATE`, `DEPLOYMENT_ENVIRONMENT`, plus the remaining `else { DEFAULT_* }` arms (~:264/285/512/572) classified by the same rule at implementation. AC conditions (auth-controller): every bound stays fail-closed (BCRYPT_COST [10,14] + warn < 12; JWT_CLOCK_SKEW_SECONDS > 0, ≤ MAX_CLOCK_SKEW, warn < 60); `DEFAULT_BCRYPT_COST` survives only as the renamed warn threshold `RECOMMENDED_MIN_BCRYPT_COST`, unused `DEFAULT_*` deleted; `common::jwt::DEFAULT_CLOCK_SKEW` (GSA) is NOT edited, AC just stops falling back to it; `config.env` values equal today's effective values (no bcrypt/skew change) with hard refs in the statefulset; every AC Config harness (`ac-test-utils`, `tests/common/test_state.rs`, unit tests) sets the keys explicitly, default-path tests become per-key missing-key tests; any default arm left in place (incl. secrets) is listed with its reason in §Implementation Summary.
**Every Config-from-vars harness** supplies the new keys: unit-test base maps, `mc-test-utils`/`gc-test-utils`/MH/AC test utils, `crates/mc-service/tests/*`, `scripts/dev-web.sh`, Kind overlays, `deploy.sh render_env_overlay`. Each new `config.env` comment uses the existing format (enforced by / where an operator sees it / what a bad value does).
**Config echo:** MC's "Configuration loaded successfully" line gains `meeting_join_window_seconds`, `disconnect_grace_period_seconds`, `clock_skew_seconds`, `gc_grpc_url`, `otel_sample_rate`, `environment`; GC/MH/AC startup lines checked for the same.
**Mechanical guard:** new `dt-guard env-config` check — a key in a service's `config.rs` required set (the `MissingEnvVar` set rule 1 builds) must not be read through `optional: true`; positive-control fixture + real tree clean. Retired keys are not in any required set, so untouched by construction. The ~:2404 test is an overlay-coverage control (kept); any fixture encoding `optional: true` for the join window is changed. Check 2's "optional ref to an overlay-only key" arm: flagged to infrastructure, not deleted (machinery).
**Rollout / rollback:** content-addressed ConfigMaps land with the Deployments; older images treat these keys as optional, so extra keys are harmless on rollback.
**Retired vs live (for test):** retired = `MC_MAX_RECEIVE_SLOTS`, `MC_AUDIO_CODEC`, `MC_AUDIO_MAX_BITRATE_BPS`, `MC_AUDIO_FRAME_RATE_HZ` (+ MH's four); everything else is live.
**Tests:** `join_window_unset_is_the_derived_default` flips to "unset → Err naming the key"; one missing-key test per newly required key per service; 01/02/03 deployment-config env-tests assert hard refs (no waits).
**Docs:** config.env texts, deployment comment blocks, `mc-service.md:47`, `mc-deployment.md` rows (Required: Yes), `mc-incident-response.md:4468`, `mc-alerts.yaml:482-488`, `mc-overview.json:2718`, AC/GC/MH deployment runbook config tables, ADR-0010 §3.

### P3 — mechanism: an end that does not name what it ends
`NotifyMeetingEnded(meeting_id, region)` ends whatever row is active. **Reachable today, beyond the TODO's two residuals:** GC calls `AssignMeetingWithMh` BEFORE `atomic_assign`; when two first joins race, the loser's MC holds an ORPHAN meeting whose join-window end notify ends the WINNER's live row (split at meeting start). **Fix:**
- GC mints `assignment_id` (`Uuid::new_v4`, OS CSPRNG) PER MC ATTEMPT inside `assign_meeting_with_mh`'s retry loop, sends it in `AssignMeetingWithMhRequest.assignment_id`, and `atomic_assign` binds that value on insert, revive and failover re-point (GC-minted, not DB-minted, because MC must hold it before the row is written). The lost-race fallback still returns the winner's row.
- Migration `20261115000001_meeting_assignment_id.sql`: `assignment_id UUID NOT NULL DEFAULT gen_random_uuid()` (backfill), then `DROP DEFAULT` (every write must supply GC's value); COMMENT ON COLUMN; rollback in header comment; PK-keyed, no new index. Not `assigned_at` (not unique; it is the inactivity clock). Not "generation" (internal.proto's DO-NOT-COLLAPSE block).
- MC: `assignment_id` nil/unparsable → `INVALID_REQUEST`; stored on `ManagedMeeting` and on a create parked behind a teardown fence; handed to the notifier with the meeting id.
- `NotifyMeetingEndedRequest` gains `controller_id` (3) + `assignment_id` (4), both required (GC `INVALID_ARGUMENT` on empty / nil / unparsable). `controller_id` is self-asserted under the shared MC token — documented as an anti-misroute narrowing match like `AcquireMediaHandlerRequest`, not authz; `assignment_id` is a non-secret discriminator, not a credential. Kept because a per-attempt id plus controller match means an ambiguous-timeout MC-A orphan can never end MC-B's row either way.
- GC: ONE conditional UPDATE `WHERE meeting_id, region, meeting_controller_id, assignment_id, ended_at IS NULL` (mirrors `get_active_assignment_region`'s (meeting, controller) keying). The `region: None` arm is deleted (no production caller). Response: `acknowledged` replaced by `NotifyMeetingEndedOutcome outcome` {UNSPECIFIED=0, ENDED, NOT_CURRENT}; NOT_CURRENT is a SUCCESS, because it also covers "my own earlier ambiguous attempt already committed" — so it can never be an error status. Outcome discloses no holder. MC treats UNSPECIFIED as a defect: ERROR, counted `rejected`, NOT retried (exactly one attempt, tested).
- MC retry: every failure is retried with the existing capped backoff and no attempt cap, until a response or shutdown (flush budget unchanged), EXCEPT `INVALID_ARGUMENT` and an `UNSPECIFIED` outcome (defects: ERROR, counted `rejected`, not retried). Safe because a duplicate can end only its own incarnation.
- **No join-path re-send** (so no rate bound / join_miss trigger needed): residual (1) shrinks to "while MC→GC fails", and then MC's heartbeats fail too, so P1's staleness failover re-points the row on the next join. Restarted MC: `MC_ID` is unset in the deployments, so each process registers a fresh id and the old id goes stale (database's restart concern does not arise; recorded in ADR). Queue overflow stays counted + ERROR.
- **Same race, MC side (paired GC finding, confirmed):** `mc_service.rs` writes the Redis MH record (`store_mh_assignments`) before `create_meeting` returns `Conflict`, and the Err arm then `delete_mh_assignment`s the LIVE meeting's record. Fix: validate (pure) → `create_meeting` (Conflict → reject, Redis untouched) → write Redis; a Redis write failure ends the just-created meeting through the controller's end path (its notify is fenced to its own `assignment_id`) and rejects `Unhealthy` as today.
- **Rollout:** GC + MC + migration deployed together; no mixed-version acceptance, no skew window (no production).
- **Fence invariant (stated in the Assign proto header and ADR, tested):** a live row (X, i) implies MC X ACCEPTED i in that same attempt. GC writes a row only after MC accepted that `assignment_id`; MC holds at most one `assignment_id` per live meeting because a second Assign for a held meeting is refused (`Conflict` → rejected) — there is NO adopt-the-new-id path. Reuse (`get_live_assignment`, `mc_assignment.rs:191`) makes no MC call and sends nothing; the ADR and a code comment there state that adding an MC call on reuse would break the fence. Test: a duplicate Assign with a different id is rejected and leaves the held meeting's id AND its Redis MH record untouched.
- **Accept→commit bound (GC, paired implements; database conditions):** GC records when MC accepted; `atomic_assign` runs in a txn: pool acquire (wait capped at remaining budget) → BEGIN → elapsed check → `SELECT set_config('statement_timeout', <remaining>, true)` → upsert → COMMIT. On SQLSTATE 57014 or a failed elapsed check: no row, retryable error, next MC with a fresh `assignment_id` (the orphan is fenced). Budget held well under MC's join-window floor: `LIFETIME_TICK` (5 s) moves to a const in `crates/common` read by both MC (`meeting_timers.rs` re-uses it — mine) and GC, with the margin rationale commented there. Residual (ADR-0010 §3): only a commit landing after the budget; detected by `gc_meeting_end_notifications_total{outcome="fenced"}`. Test: tiny injected budget → no row written.
- **Why MC-first, not DB-claim-first (paired ruling, database accepted):** a GC killed between a DB claim and the MC call would leave a live row its MC never received, on a healthy MC — permanently stranded; MC-first's crash window leaves only a fenced orphan.
- **Validation:** one boundary parse per side into a newtype `AssignmentId(Uuid)` (GC and MC); nil/empty/malformed rejected (MC `INVALID_REQUEST`, GC `INVALID_ARGUMENT`); the parse is the single source of both; the gRPC Status / rejection message is a fixed string ("assignment_id is invalid", ADR-0011), the parse detail is logged server-side as the error variant only, never the raw input. `AssignmentId` is threaded through `atomic_assign`, `ManagedMeeting`, the parked create and the notifier (no raw String/Uuid); GC compares as a UUID value. GC's `selection_is_malformed` verify-before-trust arm is extended to the id (GC minted it, but the verifier stays the full producer-side definition).
- **Wording (AC/security/protocol):** `controller_id` is self-asserted under the shared MC credential; any MC-credential holder who knows (meeting_id, controller_id, assignment_id) can end that row — a correctness fence, not authz (points at the per-instance-identity residual). `assignment_id` is a non-secret discriminator, never called a token/credential. No new gRPC service or method path. Response: `acknowledged` (tag 1) reserved; `NotifyMeetingEndedOutcome outcome = 2` {UNSPECIFIED=0, ENDED=1, NOT_CURRENT=2}; UNSPECIFIED (GC defect / version skew, same class as INVALID_ARGUMENT) → MC ERROR, counted `rejected`, NOT retried (CQ-6; mock-GC test asserts exactly one attempt); MC's ACTION is identical for ENDED and NOT_CURRENT (stop retrying). One line: the enum is no useful oracle (probing needs a 122-bit random guess; a triple holder can end the row anyway). Old at-most-once text in internal.proto (header, ~:951 cross-ref), `atomic_assign` comment (~:314) and `docs/API_CONTRACTS.md` ~:817 rewritten.
- **MC notify worker (semantic-guard):** the meeting actor and controller still only ENQUEUE (`MeetingEndedNotifier`, `try_send`) and never await GC. The worker keeps a bounded pending set (channel + pending ≤ `MEETING_ENDED_QUEUE_CAPACITY`, pulls only when below it) with a per-entry next-attempt time; a failed entry is re-scheduled with its own capped backoff, so one stuck notify never blocks later ones. Shutdown flush unchanged.
- **Observability:** MC `mc_gc_notify_meeting_ended_total{outcome=acked|retried|rejected}` (old `{status}` series removed), zero-initialised; drops stay on `mc_gc_meeting_ended_notifications_dropped_total` (unchanged alert); gauges `mc_gc_notify_meeting_ended_pending` (+1 enqueue, −1 ack/reject/drop) and `mc_gc_notify_meeting_ended_oldest_pending_age_seconds` (0 when empty; set from the earliest enqueue time each loop iteration and on each ack/reject/drop, precedent `set_kek_rotation_pending_age()`), paired on one panel; unit test ages one entry while others ack and asserts the age tracks the oldest. Alerts: `MCNotifyMeetingEndedRejected` (`increase(...{outcome="rejected"}[15m]) > 0`, for 0m) and `MCNotifyMeetingEndedStuck` (`max(oldest_pending_age_seconds) > 120`, for 0m — the age already integrates time; 120 s spans several capped-backoff attempts; a count would false-fire under churn) replace `MCNotifyMeetingEndedFailing`; nothing keyed on `retried`. GC: wire stays ENDED/NOT_CURRENT; GC-internal `gc_meeting_end_notifications_total{outcome=ended|already_ended|fenced}` from ONE statement (data-modifying CTE: the conditional UPDATE + an EXISTS probe of a live row for (meeting_id, region) with a different controller or id, read from the same snapshot) — still a single atomic statement (S3-2). `fenced` logged WARN (meeting_id, region, notifying controller_id, both ids, validated `caller_sub` from the Claims extension); `already_ended` DEBUG. Labels bounded (outcome only). Optional correlation: `assignment_id` on MC's "Meeting actor created"/"Meeting ended" lines and GC's "Ended meeting assignment(s)" (messages unchanged, loki pins unaffected). Catalogs, gc-overview + mc-overview panels, `alerts.md`, Scenario 20, split-meeting paragraphs (mc ~:3698, gc-incident-response :379) updated; incident line on a growing pending gauge.
- **Rollout (ops wording):** deployed together; transient join/notify failures only during the roll; no compat path; old-MC rows are reaped by staleness. Migration `20261115000001_meeting_assignment_id.sql` is roll-forward only — reverting GC needs a follow-up migration (drop the column or restore a default); written in `gc-deployment.md` §rollback and `mc-deployment.md`. New file only (no checksum edits); one transaction.
- **Tests:** GC repo/handler (a) matching notify ends; mismatch on controller only and on id only end nothing; insert without `assignment_id` fails (no default); duplicate notify for an ended incarnation → NOT_CURRENT, no row change; (b) old MC's notify after failover leaves MC-B's row live; (c) previous incarnation after revive leaves revived row live; (d) orphan from a lost race leaves the winner live; (e) empty/nil/malformed fields → INVALID_ARGUMENT; revive/re-point write the new id. Proto roundtrips. MC: unit (id threaded to the notifier; parked create keeps its own); mock-GC integration (ambiguous failure retried K times, K above the old cap, then success, paused time; one stuck notify does not delay a later one; shutdown ends retries within the flush budget; INVALID_ARGUMENT not retried; request carries mc_id + assignment_id; nil id refused at assign; duplicate assign leaves the live meeting's Redis record intact).

### P4 — mechanism: a deliberate leave is inferred, never stated
- **Wire:** `ClientMessage.leave_request = 13`, `message LeaveRequest {}` (no reason, no target). Proto comment: sent on the authenticated signalling stream; MC removes the sender at once (`LEAVE_REASON_VOLUNTARY`), ends the meeting if last; no reply — MC closes the session; a stream end WITHOUT a prior LeaveRequest keeps today's classification and grace. Header comment states protocol conditions 6a-f: accepted only after JoinResponse on the sender's own stream (pre-join = protocol violation); immediate removal, meeting ends if last; no reply, MC closes; client sends then closes within a bound; no ordering inference; any message after it ignored. Codegen presence entries + `signaling_roundtrip.rs` asserts `Some(Message::LeaveRequest(_))`; SDK codec/exhaustive case lists updated.
- **MC:** the bridge loop treats `LeaveRequest` as TERMINAL (no later buffered message processed): it forwards a leave carrying its `connection_id` into the EXISTING `MeetingMessage::ParticipantLeave` path (no second removal path), awaits it, closes the connection locally, and returns. The meeting actor removes via `remove_and_broadcast_left(Voluntary)` ONLY if the participant's live connection == that connection_id (security C1); otherwise no-op. The same connection check is applied to `handle_disconnect` (all arms: a stale connection neither removes nor starts a grace for a participant held by another connection). The disconnect that follows the leave no-ops (participant gone): no second disconnect or leave counted. Removal drops the session binding/correlation (verified in `remove_and_broadcast_left`, added there if missing). Pre-join `LeaveRequest` is a non-Join first message → refused as today.
- **Why no `client_left` cause (CQ-5 / semantic-guard / observability's first ask):** the leave is routed through the existing `ParticipantLeave` path (DRY/Lead), which removes the participant BEFORE the transport ends; the close that follows no-ops in the actor and is never counted, so no path ever produces a `ClientLeft` disconnect. Observability accepted this shape.
- **No new DisconnectCause:** `cause` keeps exactly three values. Catalog: `leaves{voluntary}` also counts explicit leaves, which emit no disconnect; `server_initiated` re-described (local close incl. the close after a handled leave, which is not counted). ONE INFO line, target `mc.actor.meeting`, `meeting_id` + `participant_id` as event fields, new pinned message (const in `loki.rs` + EMITTERS row, twin in `mcLoki.ts`, pin in `tests/mcLoki.test.ts`).
- **Not inferred from order:** outbound-closed stays `ServerInitiated`, idle timeout `ConnectionLost`, unknown variants `ConnectionLost` — all keep the grace. No client bytes logged.
- **SDK leave window (client C1-C3, security C3):** `close()` synchronously sets a `#leaving` state. From then on ONE check at the `#dispatch` entry drops every inbound server message before any sink (KEK intake, roster keys, handler set + dial tokens, identity, events); no further sends; no `'error'` emitted from the window. No `reader.cancel()` (its STOP_SENDING races MC's read of the leave). The window ends on `transport.closed` (MC closes after a leave — the acknowledgement) OR `signaling.leaveAckTimeoutMs` (named in `clientConfig.ts` with a range entry, default 1000 ms), then `transport.close()`. Credentials (dial tokens, keys, identity) are cleared synchronously in `disconnect()`, never after the wait; the deferred closure captures only the transport/stream, never a token. TSDoc on `close()`/`disconnect()`: the signalling transport closes within the bound after `disconnect()` returns. A second close/disconnect in the window sends nothing and does not alter the pending wait. If the leave is lost, MC applies grace and the tightened mcLoki proof fails loudly.
- **SDK:** `disconnect()` stays synchronous: state → Disconnecting, keys/KEK cleared, then (only if `#joinSettled && #stream` and via `close()`, never `#terminate` error paths) the LeaveRequest is queued on `#sendTail` BEFORE media teardown; a detached tail closes the transport when the write resolves or after `LEAVE_FLUSH_BOUND_MS` (named in `clientConfig.ts`), whichever first; write error/timeout → close anyway (MC then applies grace: that IS loss). Idempotent: one leave per session. No token change.
- **Rust env-tests:** `mc_session` leaves stay transport closes (ClientClosed; `meeting_end.rs` keeps `MC_CLEAN_CLOSE`); the LeaveRequest path is exercised in MC integration tests and the browser lane only. `tests/mcLoki.test.ts` gains an ADVERSE fixture (departure proven only by grace-expiry → proof FAILS). kek-rotation(+video) budgets read W from MC's base `config.env` through the existing `mcRotationBudget()` (repo, not the cluster's config echo; no ingest term — the poll margin `ROTATION_POLL_MARGIN_MS` covers the push and bus tick), no new literal.
- **Browser E2E (every grace site, together):** `kek-rotation.spec.ts` (+ `-video`) budget → W + margin; `mcLoki.ts` end proof requires the new leave line and NO LONGER accepts grace-expiry (a P4 regression fails, not slows); `mhLeg.ts`, `cohortContexts.ts` RESIDUAL, `join-happy-path.spec.ts` grace insurance, `videoScenario.ts`/`fixtures.ts` budgets, `e2e/README.md` timings. Expected delta: about one grace (30 s) per MH-replacement leg end proof and per S6/leave-rejoin spec — Lead measures.
- **Tests:** MC integration, paused time: leave followed by the transport close → removed once, ParticipantLeft once, no grace timer for that id; leave → ParticipantLeft without advancing to the grace; last leaver ends the meeting at once; KEK leave-debounce starts at the leave; negative controls — transport drop with no leave, and a server-initiated close, both still hold the grace; C1 — join on A, reconnect on B, leave/ClientClosed reported for A → still rostered on B; leave cannot remove anyone else; second disconnect after leave not counted. SDK unit: KEK/roster/handler-set frame inside the window installs nothing (every holder empty, no event); MC closes first → resolves without waiting out the bound; no MC close → `transport.close()` at the bound; leave written before `transport.close()`; close on write reject and on hang past the bound (FakeScheduler); no leave pre-join or on error-path terminate; double disconnect sends one leave.

### Docs
ADR-0010 §3 (paired GC drafts; I supply item-1 MC wording): item 1 — join window REQUIRED (derived value is the ceiling only); notify carries controller_id + assignment_id, GC ends only the matching row, MC retries until answered; item 3 — liveness = heartbeat freshness; reuse/failover/backstop share it; residuals rewritten (orphan race closed; remaining: notify stranded only while MC→GC fails, covered by failover). SQL snippet updated. TODO: close "`NotifyMeetingEnded` is not incarnation-safe" and the clean-disconnect entry; annotate retired-keys entry.

---

## Problems → Resolution

| # | Problem | How addressed |
|---|---------|---------------|
| 1 | GC cleanup ends live (degraded/draining) MCs' rows | GC (paired): ONE liveness predicate = MC heartbeat freshness against the configured `MC_STALENESS_THRESHOLD_SECONDS`, written once and used by the cleanup (subquery + outer re-assert), `atomic_assign`'s failover arm and the sticky reuse lookup; status never means dead. Repo constant deleted. See paired GC's summary for the GC tests. |
| 2 | Optional/fallback join-window (and sibling) config | Every behaviour/topology key with a silent code default is REQUIRED in AC, GC, MC and MH (MissingEnvVar, base `config.env` value, hard ref); `optional_seconds`, the `DEFAULT_*` value constants, MH's latency-ratio carve-out and AC's 32-zero-byte `AC_HASH_SECRET` fallback are gone; new `dt-guard env-config` check 7 refuses an `optional: true` ref to a required key (positive control). OUT by ruling: `OTEL_ENABLED`, `OTLP_ENDPOINT`, per-process ids, retired keys (TODO annotated ready). |
| 3 | NotifyMeetingEnded not incarnation-safe | GC-minted per-attempt `assignment_id` (UUIDv4; shared newtype `common::meeting_lifecycle::AssignmentId`), sent on Assign, written only for the accepted attempt; Notify carries `controller_id` + `assignment_id`; GC ends only the matching row and answers ENDED/NOT_CURRENT. MC never adopts a second assignment (fence invariant), creates the meeting before any Redis write (duplicate-assign Redis bug fixed), and retries each notify on its own capped backoff until answered (INVALID_ARGUMENT / UNSPECIFIED final). GC bounds accept→commit under the shared 5 s floor. TODO entry closed. |
| 4 | Browser clean leave held through grace | Explicit `ClientMessage.leave_request` (empty). MC: terminal in the bridge loop, removes via the existing `ParticipantLeave` path only for the live connection (every disconnect is bound to the live connection too), closes the session. SDK: leave window (one dispatch gate, no sends/errors), waits for MC's close or `LEAVE_ACK_TIMEOUT_MS`, then terminal `disconnected` state. E2E: S6 budget W, mcLoki accepts only the leave line, leaves wait for `disconnected`. TODO entry closed. |

---

## Pre-Work

None.

---

## Implementation Summary

**MC (meeting-controller).**
- `grpc/gc_client.rs`: `notify_meeting_ended` is ONE attempt (fenced request with `controller_id` + `assignment_id`, bounded by `NOTIFY_ATTEMPT_TIMEOUT`, verdict Acked/Retry/Rejected, outcome-labelled metric); `drain_meeting_ended` keeps a bounded pending set with per-entry capped backoff (no head-of-line blocking), publishes `mc_gc_notify_meeting_ended_pending` + `..._oldest_pending_age_seconds`, flushes within `SHUTDOWN_NOTIFY_FLUSH_BUDGET` and counts the remainder dropped. `MAX_NOTIFY_ATTEMPTS` / `is_provably_unsent` deleted (no longer meaningful).
- `actors/controller.rs`: `AssignmentId` on `create_meeting`, `ManagedMeeting`, parked creates and `TeardownFence`; the notifier receives it; never adopts a second assignment.
- `grpc/mc_service.rs`: pure `validate_assign_request` (assignment id + hand-off) → `create_meeting` → Redis write (failure removes the meeting); refusals through one `reject()`.
- `webtransport/connection.rs` + `actors/meeting.rs`: `LeaveRequest` terminal in the bridge loop; `handle_leave`/`handle_disconnect` act only on the live connection (`is_live_connection`); new INFO "Participant left (LeaveRequest) — removing immediately" (pinned `MC_LEAVE_REQUEST`).
- `config.rs` / `meeting_timers.rs` / `main.rs`: required keys, `MIN_JOIN_WINDOW` from the shared floor, timers threaded explicitly (`MeetingTimers::for_tests` test-only), config echo extended.
- Alerts `MCNotifyMeetingEndedRejected` / `MCNotifyMeetingEndedStuck` replace `MCNotifyMeetingEndedFailing`; dashboard panel "GC Meeting-Ended Notify Backlog"; catalog, alerts.md, Scenario 20, deployment runbook rows + rollout note.

**Gate 3 additions (P2, AC).** Two substitute-default reads lived in AC's `main.rs`, outside `config.rs` (code-reviewer R2): `AC_DRAIN_SECONDS` (an unparseable value was masked as 30) and the cluster name (`CLUSTER_NAME` defaulting to "us" at startup vs `AC_CLUSTER_NAME` defaulting to "default" at rotation — one concept, two keys). Both are now REQUIRED in AC `Config` (drain `0..=30`; one `AC_CLUSTER_NAME` validated `^[a-z0-9][a-z0-9-]{0,62}$`, read once and passed to `initialize_signing_key` and the rotate handler; security-cleared). Existing signing keys are unaffected (init no-ops when an active key exists; kids are resolved by exact JWKS lookup) — only a FRESH database's first kid prefix changes (from `us` to the configured name, `dark-tower-prod` in the base). GC's `GC_DRAIN_SECONDS` got the same treatment (paired GC).

**MH / AC (P2).** MH: binds, region, GC URL, `OTEL_SAMPLE_RATE`, `DEPLOYMENT_ENVIRONMENT`, `MH_MEDIA_LATENCY_SAMPLE_RATIO` required (ValueSource provenance removed). AC: `BIND_ADDRESS`, `AC_HASH_SECRET` (secretKeyRef + `provision.sh`), `JWT_CLOCK_SKEW_SECONDS`, `BCRYPT_COST`, the four rate-limit keys, `OTEL_SAMPLE_RATE`, `DEPLOYMENT_ENVIRONMENT` required; bounds unchanged; `DEFAULT_BCRYPT_COST` → `RECOMMENDED_MIN_BCRYPT_COST`, rate-limit defaults → `RECOMMENDED_*` (warn thresholds only); config.env values equal the previous effective values; `common::jwt` untouched. Surviving default arms: none in AC/MC/MH behaviour or topology keys.

**SDK / web-app (P4).** `SignalingClient.close()` leave window + `terminated` promise; `MeetingSession.disconnect()` leave-first and terminal `disconnected` state; `LEAVE_ACK_TIMEOUT_MS = 1000`. E2E: `configEnv.mcRotationBudget` = W; `VIDEO_SLOT_STATE_LIVENESS_MS` = poll margin; `mhLeg` end-proof ceiling = log-ingest ceiling; `cohortContexts.waitForLeaveAcknowledged`.

**Proto.** `internal.proto` (`assignment_id` on Assign; `controller_id`/`assignment_id` on Notify; `NotifyMeetingEndedOutcome`, tag 1 `acknowledged` reserved), `signaling.proto` (`LeaveRequest`, tag 13); roundtrip tests; verify-codegen presence entries.

**Tests added (MC side).** `controller.rs` unit (duplicate create refused + incarnation kept; parked create keeps its own id; backstop notify names the incarnation); `mc_service.rs` unit (empty/malformed/nil assignment id → INVALID_REQUEST); `gc_integration.rs` (request carries mc_id + id; NOT_CURRENT acked; INVALID_ARGUMENT and UNSPECIFIED exactly one attempt; 12 failures then ack; stuck notify does not delay another + oldest-age gauge; shutdown within flush budget); `disconnect_latency_integration.rs` (leave removes at once + following close uncounted; server-initiated close keeps grace; stale connection's leave/close no-op; leave rotation within W); `meeting_teardown_integration.rs` (real-session LeaveRequest: removal, MC closes the session, last leaver ends the meeting). Rust env-test leaves stay transport closes (`ClientClosed`); the LeaveRequest path is exercised in MC integration and the browser lane.

**Not testable without a Redis fixture (recorded).** "A refused duplicate assign leaves the live meeting's Redis record intact" is guaranteed by ordering (create before any write; Conflict returns before the store) and the controller-level refusal test; `McAssignmentService` cannot be built without live Redis (`meeting_assignment_metrics_integration.rs` header).

**Host-side actions for the Lead.** Re-run `infra/kind/scripts/provision.sh` (or the setup that calls it) on an existing cluster so `ac-service-secrets` gains `AC_HASH_SECRET` before deploying AC; then the coordinated GC + MC + migration deploy.

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
| Semantic Guard | confirmed |
| Paired global-controller | confirmed |
| Database | confirmed |
| Protocol | confirmed |
| Client | confirmed |
| Auth-controller | confirmed |

---

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 3 | 3 | 0 | F1 SignalingClient clears binding token at close; F2 leave-window test positive control; F3 distinct dev AC_HASH_SECRET. Cleared R2 AC_CLUSTER_NAME |
| Test | RESOLVED-FIXED | 4 | 4 | 0 | T1 threshold boundaries; T2 schema NOT NULL test; T3 env-test 31 reads live deployment; T4 main.md wording |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | F1 age gauge 5 s tick; F2 assignment_id on MC lines; F3 commit_budget_exceeded; F4 config echo GC/MH; F5 cleanup threshold |
| Code Quality | RESOLVED-FIXED | 9 | 9 | 0 | R1 expect; R2 drain + AC_CLUSTER_NAME required; R3 WARN; R4 cancel-aware notify; R5-R9 docs, migration note |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | RECOMMENDED_JWT_CLOCK_SKEW rename; leave-ack doc names MC_DISCONNECT_GRACE_PERIOD_SECONDS. Extraction: OTel scaffold TODO updated |
| Operations | RESOLVED-FIXED | 6 | 6 | 0 | +GC_DRAIN_SECONDS runbook row; GC rollback note; AC secret rotate snippet + keys; MH sample-ratio docs; MH required-keys table; MC rollback wording |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | AssignmentId::parse keeps uuid::Error source (len-only Display) |
| Paired global-controller | RESOLVED-FIXED | 2 | 2 | 0 | F1 MC fence-id tests; F2 gc_client drain doc claims |
| Database | RESOLVED-FIXED | 3 | 3 | 0 | F1 exact already_ended/fenced; F2 pre-COMMIT budget re-check; F3 migration rollback order |
| Protocol | RESOLVED-FIXED | 5 | 5 | 0 | F1 leave rule by stream position; F2 fence residual comment; F3 old-shape decode test; F4 test message; F5 API_CONTRACTS |
| Client | RESOLVED-FIXED | 7 | 7 | 0 | F2 LeaveOutcome + e2e asserts acknowledged; F1,F3-F7 |
| Auth-controller | RESOLVED-FIXED | 2 | 2 | 0 | F1 caller_sub on AlreadyEnded log; F2 distinct dev AC_HASH_SECRET |

---

## Accepted Deferrals

None.

---

## Validation

Fast checks: implementer and Lead `layer-fast.sh` TOTAL_RESULT=OK on every iteration. Gate 2: attempt 2 TOTAL_RESULT=OK, `gate2-verdict` GATE2=PASS (table below). Runtimes: §Runtime Requirement.

### Gate 2 attempts

| Attempt | L1-6 | L7 | Notes |
|---------|------|----|-------|
| 1 | L1-5 OK; **L6 FAIL** (`cargo-audit-failed`) | NOT-RUN | **Tool failure, not an advisory** (the wrapper says so: "the tool failed, not an advisory"): `cargo audit` waited on a locked `/tmp/cargo-home/advisory-db` then could not load `crates/{ac-service,media-protocol}/fuzz/Cargo.lock` mid-run. Non-reproducing: a standalone `scripts/layer6.sh` immediately after → `STATUS=OK REASON=layer6-summary` (preexisting advisories only; no dependency manifest changed in this diff). Operator lane, not counted against the L1-6 attempt budget. Log: `gate2-attempt1.log`. |
| 2 | OK | **OK** (L7 2417 s, L7 attempt 1 of 2) | **TOTAL_RESULT=OK, GATE2=PASS.** Rust env-tests 35/35 green, Σ 1364.88 s; browser E2E 33/33 passed in 581 s (incl. the clean-leave end proofs and the KEK-rotation specs with the grace removed from their budgets). Cluster rebuilt by Layer 7's provision (provision.sh changed), so `AC_HASH_SECRET` and the new migration were applied fresh. Log: `gate2-attempt2.log`. |

---

## Final Summary

All four meeting-end problems are fixed and both TODO entries are closed. (1) GC treats an MC as dead only when its heartbeat is stale against the configured `MC_STALENESS_THRESHOLD_SECONDS` (one `mc_is_live!` predicate for reuse, failover and the cleanup backstop), so degraded/draining MCs keep their live meetings; ADR-0010 §1/§3 corrected. (2) No silent config defaults remain for behaviour/topology keys in AC, GC, MC or MH: each is required, has a base value and a hard ref, and dt-guard env-config check 7 fails a required key behind `optional: true`. `AC_HASH_SECRET` no longer falls back to a public all-zero HMAC key; AC reads one validated `AC_CLUSTER_NAME`. (3) `NotifyMeetingEnded` is fenced by a GC-minted per-attempt `assignment_id` (new NOT NULL column) echoed by MC; GC ends only the matching row, MC retries every ambiguous failure per entry, duplicate assigns no longer wipe a live meeting's Redis record, and the accept→commit gap is bounded under the shared join-window floor. (4) The SDK's deliberate leave sends an explicit `LeaveRequest`; MC removes the participant from their live connection at once (meeting ends at once if they were last); network loss and server closes keep the grace; the SDK drops inbound messages during the bounded leave window and reports a leave outcome the browser tests assert. Browser E2E got 195 s faster; Rust env-tests unchanged. 12/12 reviewers RESOLVED-FIXED, nothing deferred.

**Host action for existing clusters:** re-run `infra/kind/scripts/provision.sh` before deploying (AC now requires `AC_HASH_SECRET`), and deploy GC + MC + the migration together.

