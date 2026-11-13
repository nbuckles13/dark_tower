# Devloop Output: MC ends meetings with no participants (join window)

**Date**: 2026-11-12
**Task**: MC must not keep a meeting that has no participants — end never-connected meetings after a config-driven join window via the existing Empty end path; amend ADR-0010 §3; GC cleanup job role restated.
**Specialist**: meeting-controller (paired: global-controller)
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~6h40m (2026-11-12 19:22 → 2026-11-13 01:25), incl. two Layer-7 escalation cycles

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `ca97bbe635fe22308c74797ddecb97e6a0eeb4b4` |
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
| Iteration | `4 (L7: 2 attempts FAIL → operator escalation → iteration 4 → PASS)` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` (spawned at review: diff touches client credential lifetime, MC key custody, actor loop, metrics) |
| Paired global-controller | `paired-global-controller` |
| GSA owner (auth-controller) | `auth-controller` |
| Client owner (Domain-judgment) | `client` |

---

## Task Overview

### Objective
RULING (user, 2026-11-12): an MC meeting exists only while it has participants. Never-connected meetings (actor created at GC assignment) must end through the same Empty end path within a config-driven join window.

### Scope
- **Service(s)**: mc-service (primary), gc-service (assignment_cleanup role), ADR-0010, env-tests, observability docs
- **Schema**: No (expected)
- **Cross-cutting**: Yes (MC + GC)

### Debate Decision
NOT NEEDED - user ruling 2026-11-12 decides the policy.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/devloop-outputs/2026-11-12-mc-empty-meeting-join-window/task.md` (devloop input, written by the Lead; untouched by the implementer) | Mine | |
| `crates/mc-service/src/actors/meeting.rs` | Mine | |
| `crates/mc-service/src/actors/messages.rs` | Mine | |
| `crates/mc-service/src/actors/controller.rs` | Mine | |
| `crates/mc-service/src/actors/join_park.rs` | Mine | |
| `crates/mc-service/src/actors/mod.rs` (register `meeting_timers`, re-export `MeetingTimers`) | Mine | |
| `crates/mc-service/src/actors/meeting_timers.rs` (new: `MeetingTimers`, `LIFETIME_TICK`, `MIN_/MAX_JOIN_WINDOW`) | Mine | |
| `crates/mc-service/src/config.rs` | Mine | |
| `crates/mc-service/src/main.rs` | Mine | |
| `crates/mc-service/src/observability/metrics.rs` | Mine | |
| `crates/mc-service/src/grpc/gc_client.rs` (test `Config` literals) | Mine | |
| `crates/mc-service/tests/handler_set_integration.rs` (T7: an ending meeting answers parked joins `MeetingTeardownInProgress`) | Mine | |
| `crates/mc-service/tests/gc_integration.rs` (`Config` literal) | Mine | |
| `crates/mc-service/tests/otel_grpc_outbound_integration.rs` (`Config` literal) | Mine | |
| `crates/mc-service/tests/common/mod.rs` | Mine | |
| `crates/mc-service/tests/meeting_teardown_integration.rs` (+ `MCLowConnectionCount` `for:` pin test) | Mine | |
| `infra/docker/prometheus/rules/mc-alerts.yaml` (`MCLowConnectionCount` `for:` + text; new `MCJoinsNotReachingMC`) | Mine (MC rule, ADR-0031) | |
| `docs/observability/metrics/mc-service.md` | Mine (observability reviews) | |
| `docs/observability/alerts.md` | Mine (observability reviews) | |
| `infra/grafana/dashboards/mc-overview.json` | Mine (observability reviews) | |
| `docs/runbooks/mc-incident-response.md` (new join-window scenario; new `MCJoinsNotReachingMC` triage section = the rule's runbook_url anchor; root-cause-9 + grace wording) | Mine (operations reviews) | |
| `docs/runbooks/mc-deployment.md` (config reference rows) | Mine (operations reviews) | |
| `docs/TODO.md` (close dead-grace entry if present) | Mine (operations reviews) | |
| `infra/services/mc-service/config.env` (comment-only: key documented, NOT set, no number) | Mine (MC key block) | |
| `crates/common/src/meeting_token.rs` (`MEETING_TOKEN_MAX_TTL_SECONDS: u32`; `default_meeting_ttl()` returns it; "max 900" docs → const; pin test) | **GSA** (security also reviews per §6.4) | auth-controller |
| `crates/common/src/jwt.rs` (`JWT_EXP_LEEWAY_SECONDS: u64 = 60`, assigned at :1140 only; scope doc; pin + accept-inside/reject-leeway+1 tests) | **GSA** (security also reviews per §6.4) | auth-controller |
| `crates/ac-service/src/handlers/internal_tokens.rs` (local `MAX_TOKEN_TTL_SECONDS` deleted, both handlers use common const; AC pin test removed/pointed at common) | Not mine, Domain-judgment | auth-controller |
| `crates/gc-service/src/handlers/meetings.rs` (requests `MEETING_TOKEN_MAX_TTL_SECONDS`; `DEFAULT_TOKEN_TTL_SECONDS` deleted; paired GC authors) | Not mine, Domain-judgment | global-controller (paired) |
| `crates/gc-service/src/tasks/assignment_cleanup.rs` (fail-loud `from_vars`/`from_env` → `Result`; docs: backstop; rename the two backstop tests; paired GC authored) | Not mine, Domain-judgment | global-controller (paired) |
| `crates/gc-service/src/config.rs` (`ConfigError::InvalidAssignmentCleanupConfig`; paired GC authored) | Not mine, Domain-judgment | global-controller (paired) |
| `crates/gc-service/src/main.rs` (cleanup config loaded early with `?`; paired GC authored) | Not mine, Domain-judgment | global-controller (paired) |
| `crates/gc-service/src/repositories/meeting_assignments.rs` (doc + demote the 3 repo-layer INFO logs duplicating caller lines to DEBUG, per Lead ruling; paired GC authors) | Not mine, Domain-judgment | global-controller (paired) |
| `crates/gc-service/src/grpc/mc_service.rs` (`notify_meeting_ended` doc only; paired GC authored) | Not mine, Domain-judgment | global-controller (paired) |
| `docs/decisions/adr-0010-global-controller-architecture.md` (§3; paired GC authors) | Not mine, Domain-judgment | global-controller (paired) |
| `packages/sdk-core/src/session/MeetingSession.ts` (bounded GC re-join loop in `join()`; `retried` label on `dt_client_join_attempts_total`; cancellable backoff) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/session/joinRetry.ts` (new; `joinRetryDelayMs`, `JOIN_RETRY_MIN_SPAN_MS` derived from `JOIN_RETRY`) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/config/clientConfig.ts` (`JOIN_RETRY` const) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/session/events.ts` (state-machine doc: retry re-enters FetchingToken) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/session/__tests__/meeting-session.test.ts` (retry behaviour; the close_reason helper now disconnects out of a retryable backoff) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/session/__tests__/joinRetry.test.ts` (new: delay, min span, allow-list table) | Not mine, Domain-judgment | client |
| `packages/web-app/e2e/mc-token-rejection.spec.ts` (reorder recovery before the counter wait; capture no-joined state first) | Not mine, Minor-judgment | client |
| `docs/observability/metrics/client.md` (`retried` label, emission notes, latency inflation) | Not mine, Domain-judgment | client (observability reviews) |
| `infra/services/mc-service/mc-0-deployment.yaml` (`MC_MEETING_JOIN_WINDOW_SECONDS` `configMapKeyRef`, `optional: true`) | Not mine, Domain-judgment | infrastructure |
| `infra/services/mc-service/mc-1-deployment.yaml` (same, lockstep — guarded per workload by env-config check 5) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/src/env_config.rs` (check 5: overlay ConfigMap patch keys consumed — per workload for overlay-only keys, shared-generator overlays across bases, unparsed forms fail loud; check 2 optional+overlay acceptance; OK line counts overlay patches; tests) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/src/common/services.rs` (`INFRA_OVERLAYS_DIR`, DRY G1) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/src/kustomize.rs` (uses `INFRA_OVERLAYS_DIR`) | Not mine, Domain-judgment | infrastructure |
| `docs/runbooks/devloop-validation.md` (§6.3 env-config failure-map rows for the new tokens) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/src/common/pod_spec.rs` (`ConfigMapRef.optional`) | Not mine, Domain-judgment | infrastructure |
| `infra/kubernetes/overlays/kind/services/mc-service/kustomization.yaml` | Not mine, Minor-judgment | infrastructure |
| `infra/kubernetes/overlays/kind/services/mc-service/configmap-join-window-patch.yaml` (new) | Not mine, Minor-judgment | infrastructure |
| `crates/mc-service/src/actors/session.rs` (`BINDING_TOKEN_TTL` `pub(crate)`: the grace ceiling, security S-1) | Mine | |
| `crates/env-tests/src/fixtures/loki.rs` (new: THE `query_range` client, Result-typed, classifier unit tests; DRY F3) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/mod.rs` (register `loki`) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/30_observability.rs` (moved onto `fixtures::loki::query_range`) | Not mine, Minor-judgment | test |
| `packages/sdk-core/src/media/redialBackoff.ts` (shared `backoffWindowMs`, DRY F1) | Not mine, Domain-judgment | client |
| `crates/env-tests/tests/47_mc_join_window.rs` (new) | Not mine, Minor-judgment | test |

---

## Planning

### Mechanism restatement
Instance: "a never-connected meeting is never ended". Mechanism: **every state an MC meeting actor can be in has a reachable, config-bounded end, every MC meeting-lifetime timer is the configured value, and every "this meeting is ending/ended" outcome reaches the client as one recoverable code.**

States:
- Has participants: ends by leave, close or grace (exists).
- In grace: ends at grace expiry (exists).
- Parked first join: already on the roster, because `handle_join` inserts at meeting.rs:~1906 *before* `join_park.park` at :1435, so it is covered.
- **Zero participants, never joined: no end today (this task).**

Siblings ruled in:
1. `MC_DISCONNECT_GRACE_PERIOD_SECONDS` is dead config (meeting.rs:69 hard-codes 30 s). It moves onto the same config→actor path and the const is deleted.
2. A join that races the end itself gets `Draining`, which goes out as CAPACITY_EXCEEDED (7), or `Internal`, which goes out as INTERNAL_ERROR (6). Neither is the recoverable code (see "MC-side end-race codes").
3. The SDK hard-fails on MC NOT_FOUND/CONFLICT. The client adds a bounded GC re-join.

### MC design
1. **`MeetingTimers { join_window, disconnect_grace }`** is built once in `main.rs` by `Config::meeting_timers()`. It is passed to `MeetingControllerActorHandle::with_meeting_ended_notifier` and to every `spawn_managed`/`spawn_with_seams_wired` through the existing `configure` hook. `MeetingTimers::default()` and Config's defaulting both call the same `default_join_window()` fn and `DEFAULT_DISCONNECT_GRACE_PERIOD_SECONDS`, with no fresh literals.
2. **Timer shape.** The join window is a deadline arm in the style of `kek_wake`: `(participants.is_empty() && end_cause.is_none()).then_some(created + join_window)`. `created` is a `tokio::time::Instant` taken at spawn. It is anchored once and never re-armed.
   - **Bound: exactly the window** (no tick term).
   - Grace stays on the periodic check, whose interval becomes a named const, **`LIFETIME_TICK` (5 s)**. Docs and test bounds reference that const, never a bare 5.
3. **What disarms the window (S-P1):** only a non-empty roster, meaning a successful `participants.insert` (parked joins count). A WebTransport connect, a JWT failure, a refusal or a mint failure never touches the roster. After anyone joins, only the existing last-leave choke point can end the meeting.
4. **Atomicity (S-P2).** The window fires in the actor's own select loop, through **one `end_meeting(cause)` helper**. The last-leave block at meeting.rs:~2150 calls the same helper. One turn sets both `is_shutting_down` and `end_cause`. There is no controller-side timer.
5. **Same end path (S-P3):**
   1. loop break
   2. `answer_parked_joins_draining` (now answers TeardownInProgress for a non-shutdown end, see below)
   3. `end_replacement`
   4. `graceful_shutdown`
   5. `hand_off_teardown` (EndMeeting to every handler)
   6. `kek_lifecycle.remove_meeting`
   7. actor drop (KEK `SecretBox` and dial signing key dropped)
   8. `reap_ended` (`mc_meetings_active` decremented, fence raised)
   9. `teardown_complete`
   10. `NotifyMeetingEnded` sent **after** release

   Nothing new goes on the wire.
6. **Distinguishability: `MeetingEndCause::Empty(EmptyReason)`**, where `EmptyReason` is `LastParticipantLeft` or `JoinWindowExpired`. Grace expiry is a leave, so it counts as `LastParticipantLeft`.
   - `label()` stays `"empty"`. A new `reason_label()` returns a `&'static str`.
   - `notifies_gc`, `label`, `reason_label`, the `TeardownReason` mapping and the zero-init list are all exhaustive matches with no `_ =>` arm.
   - A test asserts that `Empty(JoinWindowExpired)` behaves exactly like `Empty(LastParticipantLeft)` for `notifies_gc` and the teardown reason (A2).
   - `cause` and `reason` both go on the controller's "Meeting ended; releasing it on its media handlers" line and on the backstop ERROR.
   - The actor logs one INFO line on expiry, "No participant joined within the join window; ending the meeting", with `meeting_id`, `reason` and `window_seconds`. It carries no participant or user ids.
7. **MC-side end-race codes (client's gap, closes requirement 2 end to end):**
   - **Ending meeting:** while `end_cause` is `Some(Empty(_)|Closed)`, `handle_join` and the parked-join release answer **`MeetingTeardownInProgress`** (CONFLICT, 5). `Draining` stays reserved for MC-wide shutdown.
   - **Dead mailbox:** a closed mailbox or dropped reply in `MeetingActorHandle::connection_join` maps to **`MeetingTeardownInProgress`**. The actor exits only by ending or by shutdown, and in both cases a GC re-join is the right client action. This also corrects the comment at meeting.rs:1178.
   - **Result:** "meeting ending/ended" reaches the client only as NOT_FOUND (4) or CONFLICT (5).
   - **Panics:** an actor panic also closes the mailbox. A join that hits a panicked actor therefore shows up as `teardown_in_progress`, not `internal`. The `connection_join` mapping comment says so explicitly: "panic also lands here; it is reported by `check_meeting_health` (controller ERROR + `record_panic`), not by this code."
   - `mc-incident-response.md` root-cause-9 (~:1120, `error_type="internal"`) is reworded so it no longer claims that end-race refusals land there.
   - **Metrics:** the `mc_session_join_failures_total{error_type}` values move from `draining`/`internal` to `teardown_in_progress` for those cases. This is documented in the catalog.
8. **Metrics.**
   - **`mc_meetings_ended_total{cause, reason}`:** recorded once per meeting at `reap_ended`, from the same value as the INFO line. Zero-initialised by iterating an exhaustive `MeetingEndCause::COUNTED`, the three pairs `empty/last_participant_left`, `empty/join_window_expired` and `closed/closed`. `Shutdown` is not counted (the meeting is not over and GC is not told), and the doc says so. The values come only from `label()`/`reason_label()`. The keys are MC-local label consts, since `common/observability/labels.rs` has no cause label. No `key_custody`, no alert. Panel: "Meetings Ended by Cause", `sum by (cause, reason) (increase(...[$__rate_interval]))`.
   - **Config echoes:** `mc_meeting_join_window_seconds` and `mc_meeting_disconnect_grace_seconds`. No labels, cardinality 1, set once at boot from the same `MeetingTimers` the actors receive. Each gets a stat panel.
9. **Alert (observability e).** `MCLowConnectionCount` (`mc-alerts.yaml:401`) is `sum(mc_connections_active) < 1 and sum(mc_meetings_active) > 0`.
   - `for:` goes from 5m to **17m**, which is at least the maximum legal window (975 s) plus `LIFETIME_TICK` plus margin.
   - A unit test parses the rule and asserts `for >= MAX_JOIN_WINDOW + LIFETIME_TICK`, reading the **same max const `config.rs` uses for the upper bound** (no re-derived 975). That way the cap and the rule cannot drift.
   - The `description` annotation's "for 5 minutes" is updated.
   - **The 5-minute "nobody reaches MC" detection this alert gave is replaced in this loop (observability ruling)** by a new MC rule, **`MCJoinsNotReachingMC`**:
     - Expression: `sum(increase(gc_meeting_join_total{status="success"}[5m])) >= 3 and on() sum(increase(mc_session_joins_total[5m])) == 0` (Gate 3, observability F1: tightened from 10m/5m so detection is ~7 min from outage start, not ~15).
     - `for: 2m`, severity warning, component webtransport.
     - Why it works: `mc_session_joins_total` counts every join that reaches MC, success or failure. Several successful GC joins with zero MC joins therefore means the client→MC path is down (ingress, NodePort, netpol), independent of the join window.
     - The `>= 3` floor is justified in the rule comment: it keeps a lone GC-only join or probe from firing it.
     - Both series are pre-registered at zero (gc `metrics.rs:1456`, mc `metrics.rs:2733`), so `== 0` evaluates rather than going absent. I'll verify that at implementation.
     - Coverage: an `alerts.md` entry, an ingress/NodePort/netpol runbook step (wording from operations), and the rule test pattern / `33_alert_rules_loaded` guard.
     - Env-tests connect to MC, so a Layer 7 run cannot fire it.
     - `alerts.md` and the runbook describe `MCLowConnectionCount` (17m) as the leak detector and point to `MCJoinsNotReachingMC` for an ingress outage.
   - The summary/description, the `alerts.md` entry and the linked runbook step change meaning to: "a meeting outlived every legal join window with zero connections = leak/defect". The expression is unchanged, so `alert-conventions.md:274` needs no edit.
10. **Post-end connect (S-P5).** MC never creates a meeting on connect.
    - A connect after the end gets `MeetingTeardownInProgress` while the fence is up, or `MeetingNotFound` after it lifts.
    - The next incarnation is a fresh actor with a new KEK, a new sender-id namespace and a new per-actor `DialSigningKey` (meeting.rs:994). Its key reaches MH in `RegisterMeetingRequest.media_dial_public_key` (MH `grpc/mh_service.rs:210`, verified at `mh-service/src/auth/mod.rs:48`). MH drops the old key at release, so dial-token verification is **per incarnation** (ADR-0039 §5).

### Late join: closed by a bounded SDK re-join (Lead ruling; client co-designs)
**Exposure** (corrected after Lead and security feedback): this is not only a ~1 s edge.
- **Any user who GC-joins** an existing never-connected assignment at any point in the window and reaches MC after it ends is refused. GC's reuse path (R-6) never tells MC.
- **The creator** can hit the same thing, because GC assigns *before* it mints the token (meetings.rs:439 → :473).
- **Any join after the end that reaches GC before GC processes `NotifyMeetingEnded`** reuses the row and is refused.

**Existing client behaviour, cited:** on MC NOT_FOUND, `join()` rejects with `SignalingError(NotFound)` at stage `mc_join_response` (`MeetingSession.ts:199-215`; `errorCodeMap.ts:29`). There is **no** auto-retry. That violates requirement 2, so this loop fixes it.

**Fix (owner: client; no proto or wire change):**
- **Retry scope:** only during the initial `join()`, and only when `isRetryableJoinRejection(err)` holds, i.e. `SignalingError` with code `NotFound` or `Conflict`. The only other join-reachable Conflict is "participant already in meeting", which a fresh UUID participant_id can't hit.
- **Never retried:** auth-class codes (Unauthorized, Forbidden), InvalidRequest, Internal, CapacityExceeded, contract faults, connect-stage errors, GC errors, and anything after the join.
- **Loop:** re-runs GC `joinMeeting` → signaling/media creation → `#joinSignaling`. Authentication, identity key creation and the VP8 probe run once. `joinResp` stays loop-local (R-23).
- **Attempt bound:** `JOIN_RETRY = { maxAttempts: 4, baseMs: 250, capMs: 2000 }` in `config/clientConfig.ts`, with the rationale in the doc comment. It is a const, not a runtime knob.
- **Backoff (client, Lead-accepted):** equal jitter, `delay(n) = w(n)/2 + floor(random·w(n)/2)` where `w(n) = min(capMs, baseMs·2^n)`, and `JOIN_RETRY = {maxAttempts: 4, baseMs: 500, capMs: 2000}`. `JOIN_RETRY_MIN_SPAN_MS` is derived as the sum of `w(n)/2` (1750 ms) in the new `session/joinRetry.ts`. The floor exists to exceed MC's `END_MEETING_BACKOFF` (1 s, `teardown.rs:120`): one release retry is the first step off the healthy path. **A Rust drift pin, next to the `SDK_JOIN_DEADLINE` test**, extracts all three `JOIN_RETRY` fields from `clientConfig.ts`, failing with a distinct reason if extraction fails. It computes the span with the same integer formula and asserts it is `> END_MEETING_BACKOFF` (imported const, one-sided). It carries a positive-control fixture whose span is below the backoff and asserts that fixture trips the pin. Original note: a jitter shape **with a floor** (client owns the exact shape, e.g. equal jitter: delay ∈ [w/2, w]), scheduled on the injected scheduler and random source. The guaranteed **minimum** cumulative backoff is a named, exported quantity (`JOIN_RETRY` min span). A client unit test with the random source pinned to 0 asserts that the cumulative scheduled delay ≥ that minimum. This is asserted, not argued.
- **Between attempts:** the failed attempt is released. `disconnect()` during backoff stops the loop.
- **On exhaustion:** the last rejection is rethrown unchanged.
- **Telemetry:** a `retried` label (`"true"`/`"false"`) on `dt_client_join_attempts_total`, still one emission per `join()`. One `[dt-join-retry]` console line per retry carries the attempt number and server code, with no meeting id or token.
- **Docs:** `client.md` covers the label, the extra per-attempt signaling-connection failures, and the latency histograms including retry time.
- **The refusal window, from constants:** MC notifies GC only after teardown completes, and for that whole time GC's row stays live. A retried GC join therefore reuses the row and gets CONFLICT(teardown_in_progress), then NOT_FOUND until GC processes the notify. **The window = teardown + one notify RPC.** The upper bound is `TEARDOWN_MAX` (≈63 s: 2×`PUSH_QUIESCE_BOUND` + 2×(`MH_CONNECT_TIMEOUT` + `MH_RPC_TIMEOUT`) + backoff, `media_routing/teardown.rs:130`) + `FENCE_BACKSTOP_MARGIN` (5 s) + the notify RPC.
- **What the retry covers, stated honestly:**
  - **Covered: the healthy-MH case.** A never-joined meeting has no push backlog, so quiesce is immediate and teardown is one EndMeeting round trip per handler, sub-second in-cluster. The retry's guaranteed minimum span (≥ 1.75 s with the floor) exceeds that plus one notify RPC.
  - **Not covered: a slow or unreachable MH.** Teardown can run up to `TEARDOWN_MAX`. The retries exhaust and `join()` rethrows CONFLICT, so recovery is a user-level re-join within ≤ `TEARDOWN_MAX` + margin.
  - **Named residual (Lead ruling (a); client, code-reviewer and ops all concur):** if a media handler is slow or unreachable, teardown can run up to `TEARDOWN_MAX`. MC prints that bound on its startup line as `teardown_fence_hold_max_seconds`. The bounded retries run out, `join()` rethrows `SignalingError(Conflict)` ("Meeting is restarting, please try again"), counted as `failure/mc_join_response/retried=true`, and the user has to join again. The retry is **not** stretched to cover this. Notify is **not** moved ahead of release, because security relies on GC being told only after release. This residual sits beside the lost-notify residual (`docs/TODO.md:1756`). The same sentence goes in the `JOIN_RETRY` doc comment, the `client.md` entry for the `retried` label, the ADR-0010 note and runbook scenario (a). Scenario (a) also says: if retries ran out on `teardown_in_progress`, check `teardown_fence_hold_max_seconds` and the health of that meeting's MHs; the remedy is a manual rejoin after that bound.
  - **Why the slow-MH case is not covered:** sizing the client to a worst-case MH outage would make a genuine NOT_FOUND take about a minute to surface, which is out of proportion for a meeting nobody joined. This residual goes in the ADR and in the runbook scenario.
  - A GC re-assign that arrives during the fence parks the create on MC; it does not fail it.
- **What stays unrecovered:** a notify lost from a healthy MC (`docs/TODO.md:1756` residual 1). It is pre-existing, alerted by `MCNotifyMeetingEndedFailing`, and named in the ADR.

**`packages/web-app/e2e/mc-token-rejection.spec.ts` is REORDERED (test ruling; the retry is not relied on there).** `clearJoinResponseRewrite` + `recoverByJoining` move to right after `expectLastErrorCode`. The real meeting's first MC participant then arrives seconds after its GC join, so the window never comes into play. `waitForMcSessionJoinFailureAbove(baseline)` runs after the recovery; the counter is monotonic, so it is still valid. `expectNoJoinedEvent`'s meaning is kept: the joined-event state is captured *before* recovery and asserted afterwards. Its own rejection is `jwt_validation` (auth class), which is never retried.

### Config, derived default and constant names (SSoT)
**GSA constants:**
- `common::meeting_token::MEETING_TOKEN_MAX_TTL_SECONDS: u32 = 900` is the single declaration of the cap on every meeting/guest token.
  - AC clamps to it in both handlers; AC's local const and its pin are removed or pointed at it.
  - GC requests it and `DEFAULT_TOKEN_TTL_SECONDS` is deleted.
  - `default_meeting_ttl()` returns it.
  - It has a pin test.
- `common::jwt::JWT_EXP_LEEWAY_SECONDS: u64 = 60` is assigned explicitly as `validation.leeway` at jwt.rs:1140 and nowhere else.
  - Its doc says it governs **only `common::jwt`'s verifier**. MH media_dial and AC validators keep their own settings.
  - It is kept separate from `DEFAULT_CLOCK_SKEW`/`MAX_CLOCK_SKEW`, which are the iat tolerances.
  - Tests: one pins the assignment; behaviourally, a token expired by less than the leeway is accepted and one expired by leeway + 1 s is rejected.
  - Nothing else in MC hard-codes 60.

**`MC_MEETING_JOIN_WINDOW_SECONDS`** is optional.
- **Derived default** = `MEETING_TOKEN_MAX_TTL_SECONDS + JWT_EXP_LEEWAY_SECONDS + SDK_JOIN_DEADLINE` (900 + 60 + 15 = 975 s, stated once here as derived).
- **Justification:** the assigning joiner's token is accepted by MC until `iat + TTL + leeway`. The joiner then has at most the SDK join deadline. The few ms between assignment and token mint are absorbed by that same term.
- **Stated assumption (auth):** this assumes AC and MC wall clocks are synced. If AC runs ahead, a token outlives the window by the skew. An early end is a refusal that the SDK retry recovers from. **Do not "fix" a skew-induced refusal by adding `DEFAULT_CLOCK_SKEW` to the default.**
- **`SDK_JOIN_DEADLINE`** becomes a `pub(crate) const` in `actors/join_park.rs`. The drift test `the_park_deadline_stays_below_the_sdk_join_deadline` changes in two ways:
  - It **asserts equality**, `assert_eq!(SDK_JOIN_DEADLINE, Duration::from_millis(sdk_ms))`, on the value extracted from the SDK's `DEFAULT_JOIN_TIMEOUT_MS`. The fail-loud extraction is kept, and the SDK literal is read in exactly that one place.
  - The margin check is rewritten as `FIRST_JOIN_PARK_DEADLINE + MARGIN < SDK_JOIN_DEADLINE`.

  This matters because the 975 s default and the 23 s floor both include `SDK_JOIN_DEADLINE`. A margin-only check would stay green if the SDK raised its timeout, and both values would then silently undercount.

**Bounds, fail-loud at load** (S-P4/A3):
- **Floor: `FIRST_JOIN_PARK_DEADLINE + SDK_JOIN_DEADLINE` (23 s).** A shorter window could end a legitimate join mid-flight, a self-DoS. The floor is also above `LIFETIME_TICK`, which satisfies A3's sub-tick concern, and is stated at the config site.
- **Ceiling: the derived default**, so the leak can't be configured back in.
- **Refused:** 0, non-numeric and out-of-range values.

**One place per environment:**
- **Prod:** leaves the key unset and uses the Rust-derived default.
- **`config.env`:** a commented house-style block naming the derivation (no number), plus "Where an operator sees it: `mc_meeting_join_window_seconds`".
- **Kind:** sets it once, to **30 s**, in the overlay patch, with a comment giving the justification and the measured gap.
- **Env-test:** reads the gauge.

**Kind = 30 s, evidence:**
- **@test's audit (in writing):** for every Rust env-test, including 40_resilience's crash, zombie and stranded legs, `owned_placement::place_on_owned`, `MhPool` (which never touches GC), 24, 26, 27, 28, 34 and 45, the first-GC-join → first-MC-connect gap is a few seconds. All slow steps (baselines, settle waits, kills) happen before the GC join or after the connect.
- **Browser SDK:** connects within the same `join()`, under 1 s apart.
- **GC-only suites:** never rely on the meeting surviving.
- **Only exceedance:** `mc-token-rejection.spec.ts:62→89` (8–15 s typical, ~75 s worst case). It is removed by reordering that spec (see §Late join), not by the retry.
- **Ops measurement:** during implementation I will measure the maximum per-meeting gap between GC "User joined meeting" and MC "Meeting actor created"/first join in Loki over one recent Layer 7 run. I'll record it next to the overlay value and raise the value if the gap is not comfortably below 30 s.

**Grace:** keeps its key and its 30 s default. Kind does not set it, so behaviour is unchanged.

### Tests
**MC unit (`meeting.rs`, `start_paused`):**
- **Boundary pair:** at window − 1 ms there is no lifecycle end and no notify. At the window the lifecycle event is `Empty(JoinWindowExpired)`.
- **Grace cause:** a participant joins before the window, drops abruptly, and its grace expires *after* the original deadline. The cause is `Empty(LastParticipantLeft)`.
- **Refused joins don't disarm:** a refused join (Conflict, mint fail) leaves the window armed.
- **A1, same-turn join:** a join delivered in the same turn or instant as expiry has exactly one of two outcomes. Either it is admitted and the meeting is alive, or it gets TeardownInProgress and the meeting ends. Never an admitted participant inside an ended meeting.
- **Clean last close:** still ends immediately with `LastParticipantLeft` (extends `test_clean_close_skips_grace_removes_immediately`).
- **Configured grace:** a non-30 s grace ends the participant at the configured value. This fails if the actor ignores config.
- **Dead mailbox:** a dead-mailbox join maps to TeardownInProgress.
- **A2 parity:** `JoinWindowExpired` behaves like `LastParticipantLeft`.
- **Exhaustive `COUNTED`:** a test that a new variant is forced into the list.

**Config:**
- Default == the const sum, **and** a pin of the literal 975.
- 23 accepted, 22 refused, 975 accepted, 976 refused, 0 and non-numeric refused.
- `Config::meeting_timers()` with join_window ≠ grace, to catch a field swap.

**Integration (`meeting_teardown_integration.rs`, real controller + mock MH + recording notifier):**
- **Expiry effects (CORRECTED at implementation, accepted by @test):** a never-joined meeting is never registered on MH, because MC programs handlers at the first join. So the test asserts no RegisterMeeting and no EndMeeting on the stub, with a positive control on the same stub where a joined meeting IS registered and released. It also asserts GC notified exactly once and the KEK-lifecycle entry removed. Superseded wording: "reaches EndMeeting on every handler, notifies GC exactly once, and its KEK lifecycle entry is removed." That removal is **asserted** (entry gone, and its pending-age/sender-id series no longer contributed), not inferred.
- **Fence-backstop path:** `teardown_complete(backstop=true)` with no MH report gives `mc_meetings_ended_total{empty,join_window_expired}` exactly +1 for the meeting (it is recorded at `reap_ended`, never at teardown completion) and GC notified exactly once.
- **Metrics:** `MetricAssertion` deltas show `mc_meetings_active` −1 and `mc_meetings_ended_total{empty,join_window_expired}` +1 exactly once, including via the fence-backstop path (no double count, no skip).
- **Fence:** a join during the fence gets `teardown_in_progress`. A refusal for a participant not on the roster stays keyless (no KEK, no handler set, no dial tokens). The existing SEC-1 / `release_parked_joins` test is extended to the TeardownInProgress variant (security R1).
- **Fresh incarnation:** after the fence lifts, a re-create is a **distinct new actor** (new `created`, KEK generation and dial key), and the join succeeds.
- **Clean last leave:** ends immediately.
- **Alert pin:** the `MCLowConnectionCount` `for:` test (above).
- **Gauges:** echo tests.

**SDK (client, `session/__tests__/`):**
1. NotFound → success. Asserts **two** GC calls and `retried=true`.
2. Conflict → NotFound → success.
3. NotFound × 4 rejects with NotFound. Exactly 4 GC calls, one failure emission with `retried=true`, and 4 signaling-connection failures.
4. Unauthorized, Forbidden, Internal, CapacityExceeded and Timeout each make exactly one GC call.
5. `disconnect()` during backoff makes no further GC call.
6. First-try success has `retried=false`.
7. Backoff delays come from the scheduler, within the jitter window.
9. **With the random source pinned to 0, the cumulative scheduled delay ≥ the exported minimum span** (test ruling).
10. **N attempts make N GC calls**, and each MC join carries *that* attempt's token. Nothing from a failed attempt survives the loop: no joinResp, dial tokens or KEK. `[dt-join-retry]` and the label carry no token, meeting id or URL. `ts_retained_credentials` stays green (security R2).

Test 4 explicitly includes **Forbidden** and **InvalidRequest** (security R3); the predicate is an allow-list.
8. The predicate is tabled over every `SignalingErrorCode`.

**ONE env-test, `47_mc_join_window.rs`** (Edge-sum 0 header; no global counters):
- **Window value:** read from `mc_meeting_join_window_seconds` on the assigned MC pod (`gauge_by_pods`). The test fails loudly if it is absent and has no literal window.
- **Setup:** creates its own meeting, GC-joins once, and never connects.
- **Precondition:** Loki down is a FAIL, with a precondition token.
- **Log sourcing:** a test-file const block names the target and message of each matched line, with a comment pointing at the emitting `file:line`:
  - MC "Meeting actor created", `mc.actor.controller` (`actors/controller.rs` ~:864).
  - The MC end line "Meeting ended; releasing it on its media handlers", `mc.actor.controller`, carrying `cause`/`reason` (`actors/controller.rs` ~:1251).
  - GC "Ended meeting assignment(s)", `gc.service.assignment` (`services/mc_assignment.rs:124`).

  A reworded or retargeted line then fails as "created line never seen", with a hint, rather than as a mystery.
- **Positive control:** this meeting_id's "Meeting actor created" line is present.
- **No premature end:** no end line for this id before window − skew.
- **End line:** `cause="empty" reason="join_window_expired"`. Bounds on **Loki entry timestamps**, **both sides**: window − 2 s ≤ end ts − created ts ≤ window + 2 s (skew allowance). An end-at-creation bug fails the lower bound. On failure both deltas are printed.
- **Poll ceiling (failure only):** window + `LIFETIME_TICK` + `ConsistencyCategory::LogAggregation`.
- **Failure tokens, one per case:**
  1. created line never seen
  2. created but no end line
  3. end line outside the bound
  4. end line with a different reason
- **Re-join gate** (own reason token): GC's INFO "Ended meeting assignment(s)" (target `gc.service.assignment`, this meeting_id, presence only, ts ≥ the MC end ts).
- **Freshness** (own reason token): after the gate, GC re-join and `mc_join`, then assert a **second** "Meeting actor created" for this meeting_id with ts > the end ts.
- **Runtime:** the measured green-path runtime will be recorded here.

**GC:** paired GC's cleanup tests, renamed to backstop wording. There is no "threshold > window" claim.

### Docs / ops
**`mc-service.md`:**
- **New entries:** the counter, both gauges, and the derivation (named in words, number given once as derived), plus the clock-sync assumption and the late-join/retry note.
- **`mc_meetings_active`:** a never-connected meeting is counted from assignment and leaves within the window. A floor that persists past that is a defect.
- **`mc_gc_notify_meeting_ended_total`:** now covers join-window expiry.
- **`mc_session_join_failures_total` (`meeting_not_found` / `teardown_in_progress`):** the SDK retries via GC re-join (4 attempts), and the `draining`/`internal` values have moved.
- **Crash latency:** idle + configured grace + `LIFETIME_TICK`.

**`mc-deployment.md`:**
- A Configuration Reference row for the window: optional, derived default, bounds [23, derived], refuses boot when invalid, gauge name.
- The grace row at :1048 loses its "not yet consumed" caveat.

**Other docs:**
- Grep and fix the other "grace is hardcoded" statements (`mc-incident-response.md` ~:1925, `docs/TODO.md`).
- **New `mc-incident-response.md` scenario:**
  - (a) A user reports "meeting not found": confirm in Loki that this meeting_id ended with `reason=join_window_expired`. The SDK retry or a GC re-join recovers it.
  - (b) A sustained rise in `join_window_expired` relative to `last_participant_left` means GC joins are succeeding but clients aren't reaching MC. Triage order: MC ingress, the UDP NodePort/netpol, then a client defect.
  - The lever is the window key, but **raising it does not fix (b)**.
- The `alerts.md` entry and runbook step for `MCLowConnectionCount` are updated.

### GC / ADR (paired GC)
**ADR-0010 §3** (the paired GC revises its draft after approval to match this plan):
- **Item 1 becomes the MC-side rule:**
  - The last leave ends the meeting immediately.
  - A meeting nobody joins ends after `MC_MEETING_JOIN_WINDOW_SECONDS`. The default is derived (TTL + leeway + SDK join deadline), and the bound is exactly the window.
  - Both causes go through `NotifyMeetingEnded` after release.
  - The ADR names the late-join exposure and the SDK re-join.
- **Item 2 is superseded.**
- **Item 3, the GC backstop:**
  - `assignment_cleanup` ends **only rows whose MC is unhealthy**, i.e. MC died before it notified. Its safety property is MC liveness, not row age, so it can never end a row on a heartbeating MC (S-P6).
  - It does **not** cover a notify dropped by a healthy MC: `docs/TODO.md:1756` residual (1), now exercised more often, named without new mechanism.
  - Step 2 (hard delete after retention) stays as the only remover of ended rows.
  - The cleanup config is fail-loud.
- **Duplicate end log:** the repo-layer INFO lines that duplicate the caller's line are demoted to DEBUG, so `gc.service.assignment` "Ended meeting assignment(s)" is the single INFO record.

## Gate 1

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (v3) |
| Test | confirmed (v3) |
| Observability | confirmed (v3) |
| Code Quality | confirmed (v3) |
| DRY | confirmed (v3) |
| Operations | confirmed (v3) |
| Paired global-controller | confirmed (v3) |
| Auth-controller (GSA owner) | confirmed (v3) |
| Client (owner) | confirmed (v3) |

---

## Pre-Work

None

---

## Implementation Summary

**MC (implementer)**
- `actors/meeting_timers.rs` (new) holds:
  - `MeetingTimers { join_window, disconnect_grace }`
  - `LIFETIME_TICK`
  - `MAX_JOIN_WINDOW` = `MEETING_TOKEN_MAX_TTL_SECONDS` + `JWT_EXP_LEEWAY_SECONDS` + `SDK_JOIN_DEADLINE` (975 s; pinned by derivation and by literal)
  - `MIN_JOIN_WINDOW` = `FIRST_JOIN_PARK_DEADLINE` + `SDK_JOIN_DEADLINE` (23 s)
  - the `MCLowConnectionCount` `for:` pin test
- `config.rs`:
  - `MC_MEETING_JOIN_WINDOW_SECONDS`: optional, bounded on both sides, fail-loud.
  - `Config::meeting_timers()`.
  - Tests: boundaries, refusals, and a field-swap check.
- `main.rs` builds `MeetingTimers` once and echoes them as gauges. The controller (`with_meeting_ended_notifier(.., timers)`) puts them on `MeetingWiring.timers` for every managed meeting actor.
- `actors/meeting.rs`:
  - The hardcoded `DISCONNECT_GRACE_PERIOD` is deleted; the grace check reads `timers.disconnect_grace` on `LIFETIME_TICK`.
  - Join window: an exact deadline arm (`join_window_wake`) anchored at `created` and armed only while the roster is empty. `on_join_window_deadline` re-checks the predicate in the same turn.
  - ONE `end_meeting(cause)` step, shared with the last-leave choke point.
  - `not_admitting()`: an ending meeting answers `MeetingTeardownInProgress` both for joins and for parked joins at exit; `Draining` is for MC shutdown only.
  - `connection_join` maps a closed mailbox or dropped reply to `MeetingTeardownInProgress`, with a comment that a panic lands there too.
- `actors/messages.rs`: `MeetingEndCause::Empty(EmptyReason::{LastParticipantLeft, JoinWindowExpired})`, `COUNTED`, `counted()`, `reason_label()`. Every match is exhaustive.
- `actors/controller.rs`: `mc_meetings_ended_total{cause,reason}` is recorded at `reap_ended`, only for an actor that was actually reaped. Both lines carry `reason`.
- `actors/join_park.rs`:
  - `SDK_JOIN_DEADLINE` is pinned EQUAL to the SDK's `DEFAULT_JOIN_TIMEOUT_MS`; the margin check is rewritten against it.
  - The `JOIN_RETRY` one-sided pin (min span > `END_MEETING_BACKOFF`) has a positive control and an extraction-failure control.
- `observability/metrics.rs`: `record_meeting_ended`, `set_meeting_timers`, and zero-initialisation over `COUNTED`.

**Common / AC (GSA, auth-controller + security)**
- `common::meeting_token::MEETING_TOKEN_MAX_TTL_SECONDS` (900), pinned. `default_meeting_ttl()` returns it.
- The test JSON fixtures moved from multi-line raw strings to `json!`, because `no-hardcoded-secrets`' line lexer ended the test block early at a raw string's closing `}`. The change is value-neutral.
- `common::jwt::JWT_EXP_LEEWAY_SECONDS` (60) is assigned in one `token_validation()`. Tests pin the value and check that a token inside the leeway is accepted and one expired by leeway + 1 s is refused.
- AC's local cap is deleted and both handlers use the common constant.

**Ops / observability / docs**
- `mc-alerts.yaml`: `MCLowConnectionCount` `for:` goes from 5m to 17m and becomes the leak detector. New rule `MCJoinsNotReachingMC`.
- Panels added to `mc-overview.json`.
- Catalog (`mc-service.md`):
  - `mc_meetings_active`
  - new `mc_meetings_ended_total`
  - both gauges, including the derivation, the clock-sync assumption and the late-join residual
  - notify description
  - join-failure `teardown_in_progress` / `draining` / `internal` notes and the SDK-retry note
  - crash latency
- `alerts.md`: entries for both rules.
- `mc-incident-response.md`:
  - Scenario 3 root cause 1 updated.
  - Root causes 8 and 9 notes added.
  - New Scenarios 24 (operations' wording) and 25.
- `mc-deployment.md`: config rows.
- `docs/TODO.md`: the dead-grace third of the ADR-0023 entry is closed.
- `config.env` documents both keys without values.
- Kind overlay `configmap-join-window-patch.yaml` sets 30 s.

**Deviation from the task text (Lead-accepted): how "handlers released" (requirement 4) is met**
1. **MC side.** MC registers a meeting on its MHs only at the FIRST JOIN, because the join carries the media-routing deps (`MeetingMedia::install`, `actors/meeting_media.rs:600`). A never-joined meeting therefore holds NO MH registration, and its teardown has nothing to release. `a_never_connected_meeting_ends_within_its_join_window_through_the_empty_path` asserts that no RegisterMeeting and no EndMeeting reached the stub. A positive control on the SAME stub and controller (a joined meeting is registered, then released after its last leave) shows the absence means "nothing programmed", not "a blind stub".
2. **The release that matters is GC's.** GC ends the assignment row and its MH assignment on `NotifyMeetingEnded`. The env-test proves this per meeting through GC's "Ended meeting assignment(s)" line (`gc.service.assignment`) for its own meeting_id.

**The task's other evidence claim, "leaked meetings get pulled into MH replacement during the resilience tests": checked, and NOT via MC.**
- A never-joined meeting actor cannot take part in MH detection, removal, replacement or `AcquireMediaHandler`. Its health deadlines never arm: `MeetingMedia::health_config()` (`actors/meeting_media.rs:943`) needs the first join's deps, so `next_health_wake()` is `None`. It has no push worker, so no registration-failure trigger. Heartbeats come only from MHs holding a registration, and it has none.
- Where the "pull" actually happened:
  - **GC side.** The leaked meetings' assignment rows stayed ACTIVE, and GC's AcquireMediaHandler ownership read (`gc-service/src/services/mh_replacement.rs:155`, `get_active_assignment_region`) keeps answering for them.
  - **Resilience legs' global baselines.** `40_resilience.rs:282/669/884` read `settled_meetings_active`, and the leaked floor inflated it.

  Both end with this change: the row is ended on notify, and the gauge drops at the window.

**Tests**
- Unit (paused time):
  - boundary pair at window − 1 ms vs the window
  - disarm, plus a configured 60 s grace ending with `LastParticipantLeft`
  - a refused attempt does not disarm
  - a join exactly at the deadline gets one of two consistent outcomes
  - a clean last close ends with `LastParticipantLeft`
  - a dead-mailbox join gets `TeardownInProgress`
  - parity of the two end causes and the `COUNTED` exhaustiveness check
  - controller backstop path: counted once, notified once, and the KEK-lifecycle entry removed (with a positive control)
- Integration (`meeting_teardown_integration.rs`):
  - a never-connected meeting ends through the Empty path
    - **Test F correction (accepted by @test).** "EndMeeting reached every handler" would be vacuous: a never-joined meeting is never registered on MH. The test asserts instead:
      - no RegisterArrived and no EndMeeting on the stub
      - GC notified exactly once
      - counter `{empty, join_window_expired}` +1 and `last_participant_left` +0
      - not before the window
      - `MeetingNotFound` afterwards
      - a positive control: a joined meeting on the same stub is registered and released
  - a re-assign after the end gets a fresh incarnation (generation 1) that survives past the window
  - a clean last leave still ends immediately as a leave
  - the gauge echo
- `handler_set_integration.rs` T7 now expects `MeetingTeardownInProgress`.
- Env-test `47_mc_join_window.rs` (cluster run by the Lead at `layer-all`): the window is read from the gauge; it checks the Loki creation and end timestamps (both bounds), the GC end-line gate, and a second creation after the end.

**Client (owner authors)**: the SDK bounded GC re-join (see §Late join), `joinRetry.ts`, `JOIN_RETRY`, the `retried` label, `client.md`, and the `mc-token-rejection.spec.ts` reorder.

**Gate 3 fixes (iteration 1 → all fixed; none deferred)**
- **security S-1, code-reviewer 3:** `MC_DISCONNECT_GRACE_PERIOD_SECONDS` is bounded to [0, `BINDING_TOKEN_TTL`] (`actors/session.rs`, now `pub(crate)`), with a ceiling+1 refusal test. `mc-deployment.md` says the key is configurable downward only.
- **client F1, semantic-guard:** a `#torn` check before building the signalling client on EVERY attempt (attempt 0 included). It rethrows the last MC rejection, or `Transport` "join cancelled by disconnect()". Tests: disconnect during a retry's GC join makes no second MC dial; disconnect during the first GC join makes no MC dial at all.
- **observability F3, code-reviewer 4:** `#joinRetried` is reset at the top of `join()`, with a test.
- **DRY F2 (Lead-accepted design):**
  - The span formula exists ONLY in TS.
  - `MC_END_MEETING_BACKOFF_MS` lives in `clientConfig.ts`, and `joinRetry.test.ts` asserts the real span is greater than it.
  - The Rust `the_sdk_end_meeting_backoff_mirror_equals_mcs` pins the mirror EQUAL to `END_MEETING_BACKOFF` via one shared `extract_ts_int_const`, which `SDK_JOIN_DEADLINE` also uses. It has positive and expression/missing controls.
  - The Rust copy of the formula is deleted.
- **DRY F1:** `backoffWindowMs` is shared by the redial and the join retry.
- **DRY F4:** `handle_close_meeting` goes through `end_meeting`, which now does `get_or_insert`, so `end_cause` has one setter.
- **code-reviewer 2:** `is_join_window_armed()` is shared by the arm and the re-check.
- **code-reviewer 5:** a type-predicate `isRetryableJoinRejection`.
- **code-reviewer 1 and 7:** the `#[allow]`s are gone and the AC test doc no longer cites a line number.
- **observability F1:** `MCJoinsNotReachingMC` uses a 5m range with `for: 2m` (~7 min). The rule comment, `alerts.md` and the runbook are updated.
- **observability F2:** the parked-join catalog text now says `teardown_in_progress` for a meeting end.
- **operations:** runbook Scenarios 24/25 are renumbered to **30/31**, because 24–29 are reserved by the story's OPS anchor contract. Every reference is updated: `runbook_url`, `alerts.md`, both catalogs and the dashboard.
- **GC F1/F2:** the Scenario 31 lost-notify text (the row is LIVE to GC; it is reused until MC restarts or is marked unhealthy), and the AC doc line.
- **client F2:** `retried` is added to the bare-key list in `client.md`.
- **test 1, code-reviewer 6, DRY F3:**
  - New `fixtures/loki.rs` `query_range` returns `Result`: transport, non-2xx, unparseable body and missing `data.result` are all `Err`, with hermetic classifier tests.
  - Test 47 fails at once with `loki-query-failed`.
  - `30_observability.rs` is moved onto the fixture.
- **test 2:** the env-test reads the window per pod and requires agreement (`PRECONDITION-window-disagrees`).
- **test 3:** the integration window is 3 s (headroom for the joined control).
- **test 4:** the `mc_meetings_active` gauge is asserted 1 after assignment and 0 after the window end.
- **test 5:** present-at-0 over `COUNTED`, and no `shutdown` series.
- **test 6:** the standalone echo test is removed. The gauge echo is asserted inside the never-connected test from the SAME `MeetingTimers` value the controller enforces. A `tests/` reference is required by `validate-metric-coverage`.
- **test 7:** the freshness claim now rests on `MeetingNotFound` before the re-seed.
- **security note:** "not retried" now asserts no timer in [250, 500).

**Gate 3 re-check fixes (iteration 2)**
- **security S-2, code-reviewer 8:** `MC_BINDING_TOKEN_TTL_SECONDS` and `MC_NONCE_GRACE_WINDOW_SECONDS` are DELETED (the `docs/TODO.md` entry's own named alternative; security's preference over wiring). That removes:
  - the fields, the parsing, `DEFAULT_*_SECONDS`, the `main.rs` startup-log field and the test-literal fields
  - the key-loop entries and the `mc-deployment.md` rows

  Both keys are now covered by `test_retired_keys_are_not_read`. The `docs/TODO.md` entry is closed (`[x]`) with a pointer here. The single 30 s literal is `actors/session.rs::BINDING_TOKEN_TTL`, which is the grace ceiling, so no key can disagree with it. That answers code-reviewer 8: there is no dead binding key left to plumb, and a configurable binding TTL would be new work with its own ceiling decision. CR8's nit (grace 60 s) was already fixed in iteration 1 (window 5 s, grace 12 s).
- **test follow-on:** `30_observability.rs` now fails at once with `loki-query-failed` on a query `Err`, never "Check Promtail pipeline".
- **Lead / paired GC:** the pre-existing stale line references in the `internal_tokens.rs` test module (:496, :588, :632) now name the function.
- Verification: targeted only, per the Lead (no `layer-*.sh` while the Lead runs Gate 2):
  - `cargo test -p mc-service` config (58 passed)
  - `gc_integration` / `otel_grpc_outbound_integration`
  - `cargo clippy -D warnings` on mc-service and env-tests

**Gate 2 attempt 1 (Layer 7) failure and fix**
- **Failure:** L1–6 passed. L7 timed out at 1800 s inside `47_mc_join_window`; tests 01–46 passed.
- **Root cause (Lead-confirmed):** the Kind patch added `MC_MEETING_JOIN_WINDOW_SECONDS` to `mc-service-config`, but the MC Deployments consume that ConfigMap per key (`configMapKeyRef`), and no env entry referenced the new key. MC ran the derived 975 s default (Loki: `window_seconds:975`), and test 47 correctly waited ~975 s plus ingest.
- **Fixes:**
  1. Both `mc-N-deployment.yaml` now carry the ref, `optional: true` (prod leaves the key unset, so the derived default applies). `kubectl kustomize infra/kubernetes/overlays/kind` shows the ref in both pod specs and `"30"` in the ConfigMap.
  2. Test 47 fails at once with `PRECONDITION-window-exceeds-budget` when the gauge window exceeds `LAYER7_WINDOW_BUDGET` (60 s, tied to the overlay rationale) instead of eating the suite timeout.
  3. The bug class is closed for overlay ConfigMap patches: `dt-guard env-config` check 5. It was run against the pre-fix tree as a positive control and flagged exactly this key; it is clean after the fix. It has unit tests:
     - an unconsumed overlay key is flagged
     - an optional ref to an overlay-only key passes
     - an optional ref to a key nobody supplies is still `key_not_in_configmap`
     - an inline patch is `overlay_patch_unreadable`

     The `docs/TODO.md` overlay-coverage entry is marked partially closed; workload overlay patches remain open.

**Check-5 review fixes (infrastructure F1–F5, DRY G1–G3, operations)**
- **F1, G3, ops: per workload.**
  - A key ONLY the overlay supplies must be read by EVERY workload consuming that ConfigMap. Real-tree evidence: a copy with mc-1's ref removed → 1 hit naming `mc-1-deployment.yaml`.
  - A base-declared key the overlay re-values needs SOME reader. This refinement was forced by F2: the real codec-profile overlay re-values keys MC reads and MH legitimately does not.
  - Tests: the split, both present, none, a typo, and an unknown ConfigMap.
- **F2: shared-generator overlays** (`media-codec-profile`) are checked across bases, per workload, using per-workload consumers drawn from every base. Tests: overlay-only key one-of-two, and base-key re-value.
- **F3: unparsed forms fail loud** as `unsupported_configmap_patch_form` / `env-config-unsupported-configmap-patch-form`:
  - an inline patch, a kindless JSON6902 `path:` document targeting a ConfigMap, an unreadable path, and `configMapGenerator`/`patchesStrategicMerge`/`patchesJson6902` stanzas
  - a non-ConfigMap target is ignored
  - one test per shape, with the REASON token asserted
- **F4, G2: no swallowed IO errors.** Only `NotFound` on the overlays root reads as empty; anything else propagates.
  - The OK line now counts `-<P>-overlay-configmap-patches`; the real tree reads 7.
  - Positive control `the_real_tree_overlay_patches_are_read` (`OVERLAY-COVERAGE-ABSENT`).
- **F5:** `devloop-validation.md` §6.3 has the new tokens and the optional+overlay acceptance, the overlays claim is corrected, and the module doc now lists check 5 under `# Checks`.
- **G1:** `INFRA_OVERLAYS_DIR` is in `common/services.rs`, used by `env_config` and `kustomize`.

**Operator direction (Iteration 4) — implementation**
1. **Values.**
   - Production: `MC_MEETING_JOIN_WINDOW_SECONDS=30` is set explicitly in base `config.env`.
   - Kind: 10 s in `configmap-join-window-patch.yaml`.
   - The derived value (975 s with today's constants) is now the unset fallback and the ceiling.
   - The patch rationale, the `config.env` block and both deployment comments are updated.
2. **`MIN_JOIN_WINDOW` = `LIFETIME_TICK` (5 s), re-derived** in `meeting_timers.rs` (security's agreement requested). The old 23 s floor over-counted:
   - **The park was never exposed:** `handle_join` puts the joiner on the roster BEFORE parking, and a non-empty roster disarms the window.
   - **The real exposure is the gap between GC join and MC admission.** That gap is environment latency, not a code constant. A join that loses it is refused fail-closed and keyless, and the SDK's bounded GC re-join recovers it.
   - **What the floor guards:** it keeps "window + one tick" window-dominated and refuses a degenerate 0 or sub-tick window.
   - Config tests cover 5 and 10 accepted, and 4 and 976 refused.
3. **Option 1.**
   - The actor's join-window end line carries `age_ms` (monotonic: `created.elapsed()`, the same `Instant` the deadline is anchored to), `lifetime_tick_ms` and `window_seconds`.
   - The controller's "Meeting ended" line carries `age_ms`, which travels on `MeetingLifecycleEvent::Ended`.
   - Test 47 asserts `window ≤ age ≤ window + tick` on those fields. The wall-clock Loki delta appears only in the failure message and the summary line.
   - `LAYER7_WINDOW_BUDGET` is now 20 s, below production's 30 s, so a missing Kind override still fails fast.
4. **The 3.3 s (finding).** Reading the code: the deadline is `sleep_until(created + join_window)` on tokio's monotonic clock, re-armed every loop turn. For a never-joined meeting nothing else in the loop can block:
   - no connections, so `check_connection_health` is trivial
   - no handlers programmed, so there is no health, push or replacement work
   - the grace tick has no participants

   The paused-time unit test fires at exactly the window. A new real-time assertion in `meeting_teardown_integration.rs` bounds it in-process at ≤ window + `LIFETIME_TICK`, including teardown and notify. The cluster evidence compared two **wall-clock** Loki timestamps on a WSL2 host, whose realtime clock is stepped by host time sync. **Conclusion (Lead-corrected wording)**: the Lead's 5 solo runs of test 47 (`age_ms − window` 0..+1 ms; wall-clock delta − window +0.8..+1.8 ms) show the deadline fires on time and that MC's monotonic age and wall clock agree when no skew is present. The earlier +3.3 s was NOT reproduced; because that run predates `age_ms`, it cannot be attributed retroactively to a wall-clock step vs a one-off actor stall. Wall-clock skew remains the more likely cause given the code (nothing in a never-joined actor's loop can block), but it is unconfirmed. A recurrence would now be classified by `age_ms`. The `age_ms` field on the next runs settles it directly: `age_ms` ≈ window while the Loki delta ≈ window + skew. Either way, test 47 now asserts the task's bound on the monotonic field.
5. **Lockstep.** With the key now declared in the base, check 5's per-workload rule no longer applies to it, so replica drift was unguarded again. Fixed with `dt-guard env-config` **check 6**: replica lockstep, meaning every workload of one service that reads a ConfigMap reads the same keys of it.
   - Real-tree copy with mc-1's ref removed → `configmap-refs-not-in-lockstep` naming `mc-1-deployment.yaml`.
   - Unit test for split and both present.
   - Failure-map row in `devloop-validation.md`.
6. **Docs.** `mc-service.md` (configured values, fallback, floor, bound, `age_ms`), `mc-deployment.md` row (prod 30 / Kind 10), and runbook Scenario 31. ADR-0010 §3 wording has been handed to the paired GC.

**Iteration-4 review fixes**
- **infrastructure G6-1/G6-2/G6-3, DRY H2: check 6 reworked.**
  - `lockstep_hits` requires every reader of a ConfigMap within one service to read the same keys and agree on `optional:`. On a tie the hard ref is flagged.
  - It now also runs over SHARED maps that a service's workloads read, but never across bases.
  - Check 5 within a service reports only "read by nothing", so one defect produces exactly one finding (the split test asserts total == 1). The shared cross-base path keeps its per-workload arm.
  - Tests: optional disagreement, shared same-service drift, and a cross-base control.
  - Real-tree copy with mc-1's ref flipped to hard → `configmap-refs-not-in-lockstep`.
  - Doc and runbook row now say per-instance maps are out of scope.
- **DRY H1:** the window numbers now appear only in their homes: `config.env`, the Kind patch value, and the env-table Default/Kind columns. Prose points to the files and the gauge.
  - Kept: the alerts.md and rule-comment quantification of the 17-minute trade-off at the production value, which observability F5 asked for explicitly.
  - Removed: the test that restated 10 and 30.
- **operations F1:** the near-floor hazard (a client consistently slower than the window fails every retry), with its symptom and remedy, is in the `mc-deployment.md` row, the `config.env` block and the `mc-service.md` catalog. Scenario 31(b) has the one case where raising the window IS the lever.
- **operations F2:** the Kind patch now says Rust env-test fixtures have no GC re-join, so the value is a hard bound for them.
- **test 1:** test 47 now has an independent lower bound on the test's own monotonic clock (`ended-before-window-test-clock`): the end is observed at least one window after the test's GC join.
- **test 2 and 3, recorded by the Lead's Layer-7 runs (pending):**
  - Per run of test 47: `age_ms − window` next to the Loki wall-clock `delta − window`.
    - age ≈ window confirms skew.
    - A real age lateness of seconds is an actor-stall finding, NOT absorbed by the tick.
  - The measured maximum GC-join → first-MC-join gap: if it is above ~5 s, raise the Kind value.

## Devloop Verification Steps

- `./scripts/layer-fast.sh` (layers 1–6): **green** (implementer, 2026-11-12).
- Targeted runs:
  - `cargo test -p mc-service --features test-seams`: lib 722 passed; every integration binary passed.
  - `cargo test -p common -p ac-service --lib`: passed.
  - `cargo clippy` on mc-service, env-tests (flows), common, ac-service and gc-service with `-D warnings`: clean.
  - sdk-core `vitest`: `meeting-session.test.ts` 136 passed, `joinRetry.test.ts` 20 passed.
- **NOT run by the implementer: Layer 7** (no cluster reachable from the implementer container: Loki and Prometheus did not answer).
  - See Gate 2 (final) below for the Lead's run.

### Gate 2 (final, Lead, 2026-11-13 00:35–01:10) — `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`
**TOTAL_RESULT=OK** — L1 18s, L2 6s, L3 125s, L4 139s, L5 3s, L6 5s, L7 1819s (env-tests incl. 47 + browser E2E incl. reordered `mc-token-rejection.spec.ts`; web-app:test:e2e 9m04s). Total 2115s.
- **`47_mc_join_window.rs` runtime in the suite: 14.51 s** (Kind window 10 s) — within the operator's "window + ~10 s" budget. Solo runs: 14.49–14.87 s, 5/5 PASS (table in §Human Review).
- **Measured GC "User joined meeting" → MC first "Participant joined" gap** (Loki, every meeting in the window covering the 5 solo runs + this suite): 69 normally-joined meetings, **max 0.132 s, p50 0.025 s** — ≪ the 10 s Kind window; Kind value kept. 6 window-ended-then-rejoined meetings (test 47 ×6): post-end GC join → MC join 0.026–0.029 s (fresh incarnation). 5 meetings GC-joined and never MC-joined (GC-only env-tests): all ended by the join window — the original leak, now closed on-cluster.

## Code Review Results

See §Gate 3 Verdicts (Lead tracking) below: 11 reviewers, all CLEAR / RESOLVED-FIXED, 0 deferred. Per-finding detail is in each reviewer's verdict and the implementer's "Gate 3 fixes" / "Iteration-4 review fixes" sections.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `ca97bbe635fe22308c74797ddecb97e6a0eeb4b4`
2. `git diff ca97bbe6..HEAD`
3. Safe-revert unit: the whole devloop commit. Partial reverts that are safe on their own: (a) MC binary + Kind overlay patch (an older MC ignores `MC_MEETING_JOIN_WINDOW_SECONDS`: config.rs reads known keys from the env map and has no unknown-key rejection, which stays true after this change); (b) the SDK retry (client-only); (c) the GSA constants (value-neutral: 900 and the explicit 60 equal today's values).
4. Reverting MC re-opens the leak (never-connected meetings are held until the pod restarts) but corrupts no state. GC needs no change to roll MC forward or back. Old leaked meetings disappear with their old pods, so a new pod has no catch-up wave of window expiries.
5. Deploy order: none required. No proto or wire change. The SDK retry is harmless against an old MC (it only retries NOT_FOUND/CONFLICT).

---

## Issues Encountered & Resolutions

See §Issues Encountered & Resolutions (Lead log) below.

---

## Lead Notes

- INDEX/review-protocol injection: teammates were instructed to Read `docs/specialist-knowledge/{name}/INDEX.md` and `.claude/skills/devloop/review-protocol.md` as their first action rather than having ~160KB inlined into spawn prompts (same content, read at spawn).
- Semantic Guard not spawned: diff reuses the existing end path (KEK cleanup already there); no new credential-bearing type or Debug/Display surface expected. Lead will re-evaluate at review if the diff touches a checks.md surface.
- Lead completion ruling (planning): DRY + code-reviewer converged on (a) wiring the dead `MC_DISCONNECT_GRACE_PERIOD_SECONDS` config through to the actor, (b) consolidating the 900s join-token lifetime into one constant in `crates/common/src/meeting_token.rs` (GSA → auth-controller owner reviewer spawned; security already on panel). Both folded in as most-complete option fitting one session.

## Issues Encountered & Resolutions (Lead log)

### Issue: paired GC edits landed before Gate 1
**Problem**: paired-global-controller edited 5 GC files (config.rs, grpc/mc_service.rs, main.rs, repositories/meeting_assignments.rs, tasks/assignment_cleanup.rs) during planning, before "Plan approved".
**Resolution**: Lead halted further edits; edits kept in tree (they match Lead + security rulings) but treated as unapproved draft — subject to the approved plan and full Gate-3 review; must appear in the classification table.
- Repeat (19:29): paired-global-controller also wrote the ADR-0010 §3 amendment pre-approval after the halt instruction. Same handling: draft, subject to Gate 3.
- Lead ruling (19:30): late-GC-join gap is real — sdk-core `MeetingSession.join()` fails hard on MC `SignalingErrorCode.NotFound` (verified by Lead + security); no client re-join path exists. Requirement 2 ⇒ minimal bounded client retry (re-run GC joinMeeting on MC NotFound/teardown-in-progress during initial join) folded into this loop; `client` spawned as Domain-judgment owner (planning + review). No proto change permitted without Lead.
- Constant placement settled (security + auth-controller): `MEETING_TOKEN_MAX_TTL_SECONDS` in `crates/common/src/meeting_token.rs`; explicit 60s leeway const in `crates/common/src/jwt.rs`.
- Lead ruling (19:38): slow/unreachable-MH teardown (≤ TEARDOWN_MAX ~63s) can outlast the bounded SDK retry → accepted residual, named in ADR + runbook (option a). Not stretching retry; notify stays after release (security invariant).
- Gate 1 (19:39): all 9 confirmed on v3; classification guard STATUS=OK (Lead-run). Plan approved with two carry-in obligations: (1) add `packages/sdk-core/src/session/joinRetry.ts` row + the Rust one-sided drift test (derived JOIN_RETRY min span > END_MEETING_BACKOFF) to plan/table; (2) MCJoinsNotReachingMC runbook row.

### Issue: Layer 3 flake in fast check (Lead)
**Problem**: First `layer-fast.sh` run (20:10): `run-story-selftest` emitted `STATUS=FAIL REASON=run-story-selftest-failed` although its own summary read `556 passed, 0 failed` and its stderr was empty; the pipeline stopped early with an exit of 1, yet printed `LAYER=3 RESULT=OK` / `TOTAL_RESULT=OK` (inconsistent summary).
**Resolution**: Standalone re-run exit 0; full `layer-fast.sh` re-run green (L1–6 OK, 320s). Diff does not touch `scripts/workflow/`. Non-reproducing → operator lane, no attempt consumed. The summary inconsistency (RESULT=OK beside a FAIL child + early stop) is a pipeline defect, recorded for the operator.

## Gate 3 Verdicts (Lead tracking)

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Auth-controller (GSA owner) | CLEAR | 0 | 0 | 0 | value-neutral consolidation; common tests 26 pass |
| Security | RESOLVED-FIXED | 2 | 2 | 0 | S-1 grace capped at BINDING_TOKEN_TTL; S-2 dead binding/nonce keys deleted, TODO:1505 closed |
| Test | RESOLVED-FIXED (iter 4 re-ack) | 9 | 9 | 0 | Loki Result fixture; per-pod window agreement; 3s window; gauge+zero-init asserts |
| Observability | RESOLVED-FIXED (iter 4 re-ack) | 5 | 5 | 0 | alert 5m/2m; catalog draining→teardown_in_progress; retried reset |
| Code Quality | RESOLVED-FIXED | 8 | 8 | 0 | #[allow]→removed/#[expect]; is_join_window_armed(); dead keys deleted |
| DRY | RESOLVED-FIXED (iter 4 re-ack) | 10 | 10 | 0 | +H1 prose numerals, H2 single-fire, H3 alert-doc literals; F1–F4 + G1 INFRA_OVERLAYS_DIR, G2 no vacuous pass, G3 per-workload |
| Operations | RESOLVED-FIXED (iter 4 re-ack) | 5 | 5 | 0 | runbook renumber; check-5 per-workload; check-6 lockstep; near-floor lockout doc; Kind no-retry note |
| Paired global-controller | RESOLVED-FIXED | 2 | 2 | 0 | runbook recovery bound; stale line ref |
| Client (owner) | RESOLVED-FIXED | 3 | 3 | 0 | #torn before MC dial; client.md five keys; drift pin redesign |
| Infrastructure (owner, dt-guard + K8s) | RESOLVED-FIXED (iter 4 re-ack) | 8 | 8 | 0 | +G6-1 optional-ness, G6-2 shared-within-service, G6-3 doc; dt-guard env_config 69/69; per-workload check 5; shared-cm audit; fail-loud unsupported forms; IO errors; docs. dt-guard env_config 66/66 |
| Semantic Guard | RESOLVED-FIXED (SAFE) | 1 | 1 | 0 | #torn check before every attempt's signalling; 160/160 vitest |
- Lead correction (20:45): the "concurrent `layer-all --max-layer 6`" was the Lead's own Step-6 `layer-fast.sh` (which execs `layer-all.sh --max-layer 6`) overlapping the implementer's follow-up `layer-fast.sh` in the container-local `/tmp/devloop`. No external process; no teammate ran a wrapper. Most likely cause of the 20:10 Layer-3 `run-story-selftest` FAIL-with-0-failures. Rule going forward: one pipeline run at a time — implementer stops running wrappers once it sends "Ready for review"/"fixes done"; Lead runs only after that signal.

### Issue: Layer 4 DB-test collision (Lead, 21:05)
**Problem**: iteration-2 `layer-fast.sh` failed L4: 11 `ac-service handlers::admin_handler` sqlx tests panicked `failed to connect test pool: database "_sqlx_test_…" does not exist … just been dropped`. Untouched by this diff. Cause: reviewers were running targeted `cargo test` (instructed by Lead) concurrently against the shared test Postgres; sqlx's test harness drops other runs' `_sqlx_test_*` databases.
**Correction (21:07)**: security, test and code-reviewer each confirm they ran no sqlx/DB tests concurrently (only mc-service lib tests, no sqlx). Cause unconfirmed; leading hypothesis is a sqlx test-DB cleanup race between nextest processes inside the pipeline itself. Discriminator: re-run with nothing concurrent.
**Re-run (21:12)**: nothing concurrent → L1–6 all OK (293s). Non-reproducing; operator lane, no attempt consumed.
**Root cause confirmed (21:13)**: the implementer ran its own final `layer-fast.sh` (my "run once at the end" instruction crossed its "Fixes done"), overlapping my iteration-2 fast check: `/tmp/devloop/layer-2.log` shows two interleaved runs (START=1794517164 and 1794517167), and the two nextest runs shared the sqlx test-DB cleanup. Same class as the 20:10 collision. Not reviewer test runs, not a pipeline-internal race.
**Resolution**: environment/precondition, not code — no attempt consumed. Reviewers told to stop all cargo/DB test runs; Lead re-runs with nothing concurrent. Rule tightened: during a Lead pipeline run, reviewers run nothing that touches the test DB.

### Issue: Gate 2 attempt 1 — Layer 7 FAIL (Lead, 21:55)
**Result**: L1–6 OK; L7 FAIL `env-tests-failed` — suite hit `DEVLOOP_ENV_TEST_TIMEOUT=1800s` (rc 124) inside `47_mc_join_window` (01–46 passed, Σ finished-in 1163s).
**Root cause**: the Kind override never reached the pods. `configmap-join-window-patch.yaml` adds `MC_MEETING_JOIN_WINDOW_SECONDS: "30"` to `mc-service-config`, but MC's StatefulSet consumes that ConfigMap **per key** (`env[].valueFrom.configMapKeyRef`), and no env entry references the new key. Rendered overlay: key present in the ConfigMap, absent from the pod spec. MC therefore ran the derived default (Loki: `window_seconds:975`), and test 47 — correctly reading the gauge — waited ≈975s + ingest, past the suite budget.
**Positive evidence from the same run**: Loki shows several never-joined env-test meetings ending at 21:40/21:44 with `reason=join_window_expired, window_seconds=975` — the MC mechanism works on the cluster.
**Routed to implementer**: wire the env entry; make 47 fail fast when the window exceeds its Layer-7 budget; close the unconsumed-ConfigMap-key class if cheap. L7 attempt 1/2 consumed (test FAIL).

### Issue: Gate 2 attempt 2 — Layer 7 FAIL, escalated (Lead, 22:50)
**Result**: L1–6 OK. L7 FAIL: env-tests 01–46 pass; `47_mc_join_window` fails `ended-outside-bound` after 32.47s (the override now works: window gauge = 30s). Browser E2E not run (env-tests failed first).
**Evidence (Loki, meeting 94ba415a-3b92-4ff1-818f-56c503780794)**: `Meeting actor created` 22:43:03.798; actor's own `No participant joined within the join window` 22:43:37.119 → **lived 33.32s** vs test bound 30s ± 2s (`SKEW`). End path otherwise correct within ~2ms: graceful shutdown → controller `Meeting ended` (cause empty / reason join_window_expired) → GC `Ended meeting assignment(s)` → `GC notified`.
**Analysis**: the lateness is in MC's deadline firing, not teardown. Task bound is "window + one tick" (30 + LIFETIME_TICK 5s = 35s) → **33.3s satisfies the task's requirement**; it violates only the test's tighter ±2s, which assumed the "exact deadline" claim. 3.3s of lateness is unexplained: the deadline is `sleep_until(created + window)` on the monotonic clock, while the evidence compares wall-clock log timestamps. A wall-clock adjustment (host is WSL2) or actor-loop stall are candidates; not yet distinguished.
**Status**: both L7 attempts consumed → escalated to the operator per the devloop's Layer 7 limit. Re-run gate-2 measurements still unrecorded (47 aborted before the re-join step).

## Human Review (Iteration 4) — operator direction after L7 escalation

**Feedback** (2026-11-13): "the MC period should be configurable and we should set it to something short but safe for other tests. 30s is too long to wait for an env-test, something like 5 or 10s would be better. production can use something like 30s. so do option 1, then run just that test itself a few times to verify it is reliably passing. then run the full suite and continue the devloop"

**Lead interpretation**: "MC period" = the join window (`MC_MEETING_JOIN_WINDOW_SECONDS`), already config-driven. Kind → 10s (upper end of the operator's range; safest for other env-tests). Production → 30s, set explicitly in base `infra/services/mc-service/config.env`; the derived 975s stays as the unset fallback and the ceiling. The 23s floor (`MIN_JOIN_WINDOW`) must be re-derived to admit 10s. Option 1: MC's end line carries its own monotonic age; test 47 bounds on that against window + LIFETIME_TICK. Additional L7 runs authorised by the operator: 47 alone ×N, then the full suite.

### Test 47 solo runs (Lead, 2026-11-13, operator-directed; build = iteration 4 deployed via `dev-cluster deploy`)
Env exported from `/tmp/devloop/ports.json` as `layer7.sh` does (first attempt without it failed at cluster connect — harness setup, not the test).

| Run | Result | age − window (MC monotonic) | Loki wall-clock delta − window | Test runtime |
|-----|--------|-----------------------------|--------------------------------|--------------|
| 1 | PASS | +1 ms | +1.0 ms | 14.81 s |
| 2 | PASS | 0 ms | +0.8 ms | 14.68 s |
| 3 | PASS | +1 ms | +1.2 ms | 14.87 s |
| 4 | PASS | +1 ms | +1.8 ms | 14.49 s |
| 5 | PASS | +1 ms | +1.3 ms | 14.49 s |

**Finding on the earlier 33.3 s**: with Kind at 10 s, MC's monotonic age and the wall-clock Loki delta agree to ≤ 2 ms on every run, and both sit at the window. MC fires on time. The Gate-2 attempt-2 overrun (+3.3 s, wall-clock only — `age_ms` did not exist yet) is therefore not reproduced; it cannot be attributed retroactively between a host wall-clock step and a one-off actor stall. The test now bounds on `age_ms`, so a recurrence will say which.
**Env-test runtime**: ≈ 14.5–14.9 s per run (window 10 s + Loki ingest + re-join) — within the operator's "window + ~10 s" budget.
