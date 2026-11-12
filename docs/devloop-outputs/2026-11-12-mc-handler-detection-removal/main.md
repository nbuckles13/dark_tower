# Devloop Output: MC-B2 — Detection (assess), Dead/Impaired declaration, removal; zombie env-test leg

**Date**: 2026-11-12
**Task**: Story 4 task MC-B2 (run-story task #21): per-(meeting, handler) health assess, Dead/Impaired declaration, ordered removal, zombie/crash env-test legs
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~3h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `0bf0e44f1e83da56052642280637d01a9eaadd2f` |
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
| Iteration | `3` |
| Security | `security` |
| Test | `paired-test` (paired, replaces test slot) |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Media Handler (conditional, zombie leg) | `media-handler` |

---

## Task Overview

### Objective
Task prompt: `/home/dev/.cache/devloop/story-runs/story-runner/2026-10-31-survive-handler-loss/task-21.prompt` (MC-B2). Story: `docs/user-stories/2026-10-31-survive-handler-loss.md`. Governing design: ADR-0039 §1, §2, §6.

### Scope
- **Service(s)**: mc-service (+ env-tests, dashboards/catalog, runbook)
- **Schema**: No
- **Cross-cutting**: Yes (MC↔MH behaviour; env-test zombie leg reviewed by media-handler)

### Debate Decision
NOT NEEDED - ADR-0039 governs.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mc-service/src/media_routing/handler_health.rs` (new) | Mine | — |
| `crates/mc-service/src/media_routing/liveness.rs` | Mine | — |
| `crates/mc-service/src/media_routing/mod.rs` | Mine | — |
| `crates/mc-service/src/media_routing/placement.rs` | Mine | — |
| `crates/mc-service/src/media_routing/pusher.rs` | Mine | — |
| `crates/mc-service/src/media_routing/teardown.rs` | Mine | — |
| `crates/mc-service/src/media_routing/generation.rs` | Mine | — |
| `crates/mc-service/src/media_routing/connectivity.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting/health.rs` (new) | Mine | — |
| `crates/mc-service/src/actors/messages.rs` (test-seams probe field) | Mine | — |
| `crates/mc-service/src/webtransport/connection.rs` (test call-site of `mark_active`) | Mine | — |
| `crates/mc-service/src/actors/meeting_media.rs` | Mine | — |
| `crates/mc-service/src/actors/controller.rs` | Mine | — |
| `crates/mc-service/src/media_admission/epoch.rs` | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` | Mine | — |
| `crates/mc-service/tests/heartbeat_intake_integration.rs` | Mine | — |
| `crates/mc-service/tests/handler_set_integration.rs` | Mine | — |
| `crates/mc-service/tests/handler_detection_integration.rs (new)` | Mine | — |
| `crates/mc-service/tests/media_dial_integration.rs` | Mine | — |
| `crates/mc-service/tests/common/mod.rs` | Mine | — |
| `crates/mc-service/tests/common/media_dial.rs` | Mine | — |
| `crates/env-tests/tests/40_resilience.rs` | Not mine, Minor-judgment (paired) | test |
| `crates/env-tests/src/fixtures/*` (hoisted helpers only: placement-of-owned-MH, deadline wait) | Not mine, Minor-judgment (paired) | test |
| `crates/env-tests/tests/44_gc_mh_registration.rs` (calls the hoisted `sum_over_pods`) | Not mine, Mechanical (paired) | test |
| `crates/env-tests/README.md` (test listing rows) | Not mine, Mechanical (paired) | test |
| `infra/grafana/dashboards/mc-media.json` | Mine (MC service-owned dashboard, ADR-0031) | — (observability reviews) |
| `docs/observability/metrics/mc-service.md` | Mine (MC catalog) | — (observability reviews) |
| `docs/runbooks/mc-deployment.md` | Mine (MC runbook) | — (operations reviews) |
| `docs/TODO.md` (§Media Path Obligations S1b residual text extended) | Not mine, Minor-judgment | security |
| `crates/mc-service/tests/common/coordination.rs` (new, hoisted MH->MC call builders) | Mine | — |
| `crates/gc-service/src/observability/metrics.rs` (`"trigger"` literal -> shared `TRIGGER_LABEL`, value-identical; DRY review) | Not mine, Mechanical | global-controller |
| `crates/env-tests/src/fixtures/gc_fleet.rs` (`INITIAL_HANDLERS_KEY` named in the drift-tested key mirror; DRY review) | Not mine, Mechanical | test |
| `infra/docker/prometheus/rules/mc-alerts.yaml` (`MCPushQuiesceTimeouts` wording only: removals also drain; expr unchanged) | Mine | — (observability reviews) |
| `docs/observability/alerts.md` (same wording, mirrored) | Not mine, Mechanical | observability |
| `docs/runbooks/mc-incident-response.md` (one PromQL comment: counting unit) | Mine | — (operations reviews) |
| `docs/user-stories/2026-10-31-survive-handler-loss.md` (two WARNING bullets added to the OBS-ALERTS task block, edited by observability at review) | Not mine, Minor-judgment | observability |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |
| `docs/devloop-outputs/2026-11-12-mc-handler-detection-removal/main.md` | Mine | — |

No proto change (`EndMeetingRequest.cause`, `ReleaseCause`, `ApplyRefusal`, `ServingState` all exist). No new config key. No GSA path is touched.

---

## Planning

### Problem, in mechanism language (and the wider class)

MC-B1 records per-(meeting, handler) evidence (receipt instants, applied generation, refusal, serving state) but nothing turns evidence into a decision. MC-B2 adds (a) ONE pure decision function over that evidence with an explicit next-deadline output, (b) a timer arm so silence is a decision input without a message, and (c) ONE ordered removal transaction whose first mutation (set drop + removed record) is what every later fence (verdict RELEASED, binding gate, mint) keys on.

Wider class surfaced and completed here:
1. **Every acceptance rule now feeds a one-way decision.** MC-B1's "applied ahead of sent" heartbeat raised the accepted watermark, so after detection one forged/buggy heartbeat makes every honest heartbeat Stale and kills a healthy handler at N (security S-2). The acceptance rule is tightened: an ahead heartbeat is counted (anomaly) and its watermark is clamped to what MC sent (originally planned as NOT liveness; revised at Layer 7, see Issues). Same reasoning applied to lag: a generation MH has not yet had one interval to apply is not "lag" (observability 4c) — lag uses the generation that was already sent at the PREVIOUS accepted heartbeat (the same previous-heartbeat rule as the reconcile, so the codebase has one "MH had a full interval" notion).
2. **Every MC→MH `EndMeeting` sender sets a cause and is covered by the meeting's teardown fence.** Removal adds a second sender; its in-flight task is handed to the meeting's teardown (awaited before the teardown completes) so a same-id meeting re-created after teardown can never receive a late removal release or a late per-pair generation eviction.
3. **Every path that names a handler to a client reads the post-removal snapshot** (immediate set push, TTL/3 refreshes, FAILED re-pushes, JoinResponse): all mint from `client_snapshot()` at mint time, and removal mutates the set before anything is minted. Test asserts no post-removal set carries a token with aud = X.

### Design

**1. `media_routing/handler_health.rs` (new, pure).**
- `HealthConfig` from `HeartbeatTiming` (no literals): `timeout()` = N, new `HeartbeatTiming::apply_lag_limit()` = L = `APPLY_LAG_FACTOR` (= 2, one named const in liveness.rs) × N, and `detection_histogram_buckets()` derived from N (N/2, N, 1.25N, 1.5N, 2N, 2.5N, 3N, 4N — covers > L).
- `Observations { confirmed_at: Option<Instant>, last_accepted: Option<Instant>, applied: Option<u64>, owed: Option<u64> /* sent as of the previous accepted heartbeat */, apply_refusal, serving_state, refusal_seen_at, client_loss_reports: u32 /* weight 0 */ }`.
- `HandlerHealth = Live(LiveHealth { lag: Lag::{None | Since(Instant) | ContractFault(ApplyRefusal)}, draining_since: Option<Instant> }) | Declared(Declaration { trigger: RemovalTrigger, cause: DeclareCause::{Silence | LagHeld | CapacityRefusal(ApplyRefusal)}, evidence_at: Instant })`. Named projection `state_label()` = healthy|lagging|contract_fault|draining|impaired|dead for logs only.
- `assess(now, &obs, &prev, &cfg) -> HandlerHealth`, precedence: Declared is absorbing (one-way) → Dead (confirmed_at is Some AND now − max(confirmed_at, last_accepted) > N) → Impaired on STREAM_CEILING/EDGE_CAP → contract fault (INVALID/TRANSPORT_INCONSISTENT: lag clock suppressed, never Impaired; reset when the refusal clears) → Impaired when Lag::Since(t) and now − t > L → Lagging iff applied < owed → Healthy. Draining is an overlay recorded from serving_state (sticky; MC-C/MF read it); it never removes. Unconfirmed (Registering) handlers are never Dead. Client reports are read and ignored (weight 0; tested).
- `next_deadline(&health, &obs, &cfg) -> Option<Instant>`: min(age origin + N + 1 ms, lag onset + L + 1 ms) — strictly past the boundary, so a fire never re-arms the same instant (no busy loop under paused time).
- `Transition` helper: classifies prev→next into {none, lag_entered, lag_cleared (suspicion cleared), declared (suspicion declared iff prev was Lagging), draining_entered, contract_fault_entered/cleared} so the actor acts and counts in one place.
- `detection_start`: heartbeat_timeout → last accepted receipt, else confirmed_at; apply_lag (lag held) → lag onset; apply_lag (capacity refusal) → receipt of the refusing heartbeat. (Deviation from the contract's single "last accepted heartbeat" start, per observability: last-heartbeat→declaration is ~0 for apply_lag and says nothing. Documented in the recorder doc and catalog.)

**2. Evidence storage (one home each).**
- `confirmed_at` lives on the `MeetingHandlers` member (set in `mark_active`, the Registering→Active transition; never in liveness). Read via `confirmed_at(id)`.
- `HandlerLiveness` gains `owed_generation` (sent as of the last accepted heartbeat) and `lag`/health state lives in a `health: BTreeMap<HandlerId, HandlerHealth>` beside liveness in `MeetingMedia`, dropped by the ONE `remove_handler`. `observe()` takes `sent` and clamps the accepted watermark to it when applied > sent (anomaly still counted; revised at Layer 7 from "not liveness", see Issues).

**3. Actor (`actors/meeting.rs`).**
- On an OWNED heartbeat (any acceptance): observe → assess that pair → act on transition. Lagging (entered or still lagging on an accepted heartbeat) → `HandlerPusher::republish()` (re-sends the CURRENT job through the same watch; one per heartbeat, bounded by L). Removed handlers have no pusher, so republish can never target them.
- New `health_wake` `sleep_until` arm (same shape as settle/kek/fanout) at the min `next_deadline` over the meeting's handlers; on fire, assess every confirmed member at `Instant::now()`.
- **Stall guard (operations 1):** receipts are already stamped at the gRPC edge (MC-B1). Additionally, when the health arm fires it first PROCESSES THE ALREADY-QUEUED BACKLOG: a snapshot of `receiver.len()` messages taken at fire time, through the same `handle_message` path and mailbox accounting as the mailbox arm. The drain stops at once if a processed message ends the meeting (`end_cause` set) or the channel closes, and then no assess runs. Otherwise it assesses unconditionally. After the assess every remaining deadline is strictly in the future (expiry = boundary + 1 ms), so the arm cannot spin. So every heartbeat that was enqueued before the judgment is applied first (operations), and the work is bounded by the mailbox capacity, so continuous traffic cannot postpone the judgment past one backlog (security S-9, paired-test A). Tier-1b twins: (i) park the actor, queue a heartbeat stamped before the deadline, advance past it, unpark → not Dead; (ii) keep the mailbox continuously non-empty with unrelated traffic while the handler is silent → declared Dead within one backlog of the deadline; (iii) a close queued in the backlog when the arm fires → no declaration and no removal EndMeeting, and the exit goes through the loop's single clean path. In twin (i) the heartbeat must be inside the backlog the drain processes (actor parked), so the drain is what saves the handler.
- INVALID/TRANSPORT_INCONSISTENT: counted on EVERY heartbeat carrying it (new `HeartbeatAnomaly` reasons `apply_refused_invalid` / `apply_refused_transport_inconsistent` on the existing `mc_media_handler_heartbeat_anomalies_total` — expected-zero, same panel), ERROR logged once per transition into the fault.
- One INFO per health transition (target `mc.actor.meeting`): meeting_id, handler_id, from, to, trigger (declarations only), and bounded scalars only: age_ms, applied, owed, refusal label. No Debug of any struct, no url, no connection ids, no tokens.

**4. Removal transaction (`MeetingMedia::remove_handler` stays THE one entry; the actor drives the ordered sequence around it), on a Declared transition:**
1. `remove_handler(id, trigger)`: set drop + removed record + version bump + liveness + health dropped, AND purge X's keys from every view's connectivity (new `ParticipantConnectivity::forget_handler`, keyed by (handler, connection) — a colliding connection_id under H2 is untouched), then `reroute_all`. Returns the removed endpoint, the `SetChange`, and X's pusher moved OUT of the map.
2. `reconcile_media(Affected::All)` — normal render; X is not in `iter_with_ref`, so nothing is published to it and edges re-place on surviving shared handlers.
3. If `active_changed`: `broadcast_handler_set()` (typed `send_handler_set` path; mints from the post-removal snapshot).
4. `flush_views()`.
5. Only now: `mh_bindings.release_handler(X)` (new bulk method on `HandedOutBindings`, per-entry removal semantics; ids are NOT returned to the allocator — reclaimed only by `AdmissionEpoch` reset; test: a joiner after removal never gets one of X's ids).
6. Spawn (never awaited in the actor) `teardown::release_removed(...)`: `quiesce(X's own child token, [X's task])` (reuses the one drain kernel, bound and `record_push_quiesce`) → `end_meeting_removal(meeting, mc_id, endpoint, trigger, &Quiesced)` → `release_one(.., attempts = 1)` (no retry; bounded by MH_CONNECT_TIMEOUT + MH_RPC_TIMEOUT; failure logged WARN with kind + detail and counted on `mc_media_end_meeting_total`) → `policy_generations.remove_pair(meeting, X)` AFTER the drain (a draining worker could otherwise re-create the entry). The JoinHandle is kept in `MeetingMedia.removals` and moved into `MediaTeardown`, whose `run` awaits it before evicting the meeting — the controller fence covers it.
7. Record `declarations_total{trigger}` + `detection_seconds{trigger}` exactly once (the Declared state is absorbing and the handler leaves the health map in step 1, so a second assess cannot double-count); INFO line for removal with version and survivors count.
- Per-handler cancel: `HandlerPusher` stores its own child of `pushers_cancel`; meeting teardown still cancels the parent (covers all). No KEK rotation on removal (MH never holds the KEK; ADR §6 asks none). `dial_signing_key` untouched.
- From then on: verdict RELEASED(trigger) (MC-B1 branch), binding gate refuses X, zombie Disconnecteds → `handler_not_in_set`; no `RegisterMeeting` to X is ever issued again (no pusher, `add` refuses PreviouslyRemoved).

**5. Teardown.** `end_meeting_plan` already iterates current members only with cause MEETING_ENDED (P1); doc updated ("removal releases go through `end_meeting_removal`"), and a test asserts a removed handler gets no second EndMeeting. `EndMeetingCall.cause` doc fixed.

**6. `mc_media_meetings_without_handlers`.** Recompute-and-set census. DRY: `MutedSourceCensus`'s body is generalised to `MeetingCensus<V>` (set/retain/poison-recovering lock/fold); `MutedSourceCensus` becomes that type with its per-kind publish, and `HandlerlessCensus` = `MeetingCensus<bool>` publishing the count. Written by the actor on every set change (freshness), cleared in `hand_off_teardown`, pruned + published by the controller `check_meeting_health` walk (load-bearing), published 0 at boot. Semantics: a meeting counts iff its set has NO member (Registering counts as a handler "on its way").

**7. Metrics** (all `key_custody="operator"`, no id labels): `mc_media_handler_declarations_total{trigger}` (zero-init heartbeat_timeout + apply_lag only; planned_transfer is MC-F's), `mc_media_handler_suspicions_total{outcome=declared|cleared}` (Lagging→Impaired / Lagging→Healthy only; Lagging→Dead and refusal-direct Healthy→Impaired are NOT suspicion outcomes — documented), `mc_media_handler_detection_seconds{trigger}` (N-derived buckets registered by full-name matcher), `mc_media_meetings_without_handlers`. Trigger labels only via `RemovalTrigger::label()`; suspicion outcome a new `SuspicionOutcome` enum with `ALL` + `label()`. Each: catalog row (start point per trigger, census semantics), mc-media.json panel (rate/increase; detection as histogram_quantile by trigger beside the N gauge), zero-init + render test, ADR-0032 `MetricAssertion` tests through real transitions incl. unobserved assertions (cleared suspicion / Draining record no declaration or detection).

**8. Docs.** `mc-deployment.md`: L = 2N (`APPLY_LAG_FACTOR`) beside N (12 s at default), "raises false declarations" in present tense; hard ordering — every MH on MH-2+ before this MC rolls, no MH rollback below MH-2 while it is live, pre-deploy check `mh_mc_heartbeats_total{outcome="success"}` rising on every MH pod; rollback to MC-B1 is safe (cause=MEETING_ENDED already set at teardown; removal state is in-memory only), and meetings left at zero handlers stay dark until they end (MC-B1 never re-adds). No alert on the gauge here (OBS-ALERTS owns `MCMediaMeetingsAwaitingHandler`; Sc 24 is OPS-INCIDENT's). `docs/TODO.md` S1b residual: "forge-to-declare as well as forge-to-suppress" (a self-asserted heartbeat carrying STREAM_CEILING/EDGE_CAP now forces removal of a peer handler inside the meeting's own set; never cross-meeting).

### Code-review shape (adopted at Gate 1)
- Narrow enums projected once at the edge: `ContractFault {Invalid, TransportInconsistent}` and `CapacityRefusal {StreamCeiling, EdgeCap}`, each with `label()`. `LiveHealth { lag: Option<Instant>, contract_fault: Option<ContractFault>, draining_since }`, so a contract fault is not a lag variant.
- One boundary helper pair, `expired(origin, span, now)` / `expiry(origin, span)`, shared by `assess` and `next_deadline`.
- `remove_handler` returns a named `RemovedHandler { endpoint, set_change, pusher }`.
- One actor method, `remove_declared_handler`, reads as steps 1-7 with the ordering invariant in its doc. Helpers go to `meeting_media.rs` or `media_routing/removal.rs` if it grows past about 80 lines.
- Removal JoinHandles are awaited in `MediaTeardown::run` without unwrap. A JoinError is WARNed and the teardown continues. The wait is bounded by 2x the quiesce bound plus one attempt. `removals` is bounded by the removed set.
- `MeetingCensus<V: Default + PartialEq>` stays minimal: storage, set/retain/lock and a fold. No trait.
- The dt-guard histogram-buckets guard is checked against the runtime-derived detection buckets before Layer 3.

### Tests
- **Tier 1a** (`handler_health.rs`): one named table row per transition, L/N derived from `HeartbeatTiming`, both sides of each boundary (N vs N+1ms, L vs L+1ms), unconfirmed never Dead + positive twin after confirm, one-way Dead/Impaired, stale-lower not liveness, client reports alone never Dead, DRAINING → Draining no declaration, STREAM_CEILING/EDGE_CAP → Impaired, INVALID/TRANSPORT_INCONSISTENT → contract fault never declared even past L, transient lag (applied < sent but ≥ owed) not Lagging; `next_deadline` asserted in each row. Liveness: ahead-of-sent accepted with the watermark clamped to sent, honest heartbeat afterwards still liveness (revised at Layer 7).
- **Tier 1b** (new `tests/handler_detection_integration.rs` on the existing `heartbeat_intake_integration` / `handler_set_integration` rig in `tests/common`, paused time, mock MH): deadline arm fires on its own (N−1ms none, +2ms declared); stall guard; ONE ordered event log over mock-MH calls + client frames (set-without-X before bulk binding release before drain before EndMeeting(cause)); (X, conn) keys survive Lagging/Impaired/Draining; failed EndMeeting → called exactly once across several intervals, removal complete, next heartbeat RELEASED(HEARTBEAT_TIMEOUT); APPLY_LAG cause on the impaired path; apply-failure gate (applied stuck → Lagging → republish observed on mock MH → Impaired at L+; catch-up before L → Healthy, suspicion cleared, no declaration); two-handler: H1 silent → edges on H2's snapshot, nobody sharing H2 unreachable; zombie Disconnected with a COLLIDING connection id → handler_not_in_set, H2 untouched (positive control: H2's own Disconnected changes it); no RegisterMeeting to X after removal; no post-removal set carries aud = X; sender_id non-reuse after removal; teardown releases only the current set with MEETING_ENDED; metrics (both suspicion outcomes, detection observed once per declaration, gauge 0→1 single-handler and 0 two-handler, unobserved arms).
- **Env (40_resilience.rs, `#[serial]`, `MhPool::run_leg`, paired with test):** placement per paired-test's proposal — `up_scraped()` one owned MH, then create+join meetings until one's handler set (via `mh_address`) contains the owned slot, attempt bound derived from pool size with a loud `owned-mh-not-placed` failure listing observed sets, every miss left cleanly. Crash leg: precondition set size == 2 (`owned-mh-not-placed` also covers "placed, but a 1-member set"); pre-kill per-entity evidence that the pair's media flowed through the owned MH (that pod's forwarded-frames counter, pinned by `instance_for_pod_ip`, rose while the pair exchanged media); owned heartbeats rose before kill; kill via injector; declarations{heartbeat_timeout} and detection _count rise; media resumes on the survivor, with the survivor pod's forwarded-frames counter rising (derived failure-only ceiling); after in-place restart: GC superseded{match=both} +1 summed across settled replicas, unknown_handler unchanged; MH ready again before return. Zombie leg: freeze (injector proves it, scope-guard resume on every path), preconditions N + interval + 2×stop_call < budget < owner-silence; declaration rises while frozen; resume (restart_count/pod_uid unchanged); carrier-agnostic primary assertion releases_total{trigger=heartbeat_timeout} on that pod +1 and owner_silence unchanged; carrier identified and logged (verdict vs buffered EndMeeting); NOT asserting `clients` label or HANDLER_RELEASED closes (idle race; pinned by MH-2/MH-3 tests, cited). "No Disconnected" stays Tier 1b. Injector-unavailable → loud failure with its reason (the fallback is a recorded decision, not a runtime skip). No credentials in output.

### Open questions (resolved in plan unless a reviewer objects)
- detection_seconds start point per trigger diverges from the contract's single wording (observability recommendation adopted).
- `mc_media_push_quiesce_total` now also counts single-handler removal drains (same Race-B indicator); catalog row reworded "per teardown or removal".

### Gate 1 Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Media Handler | confirmed |

---

## Pre-Work

None

---

## Implementation Summary

**Detection (ADR-0039 §2).**
- `media_routing/handler_health.rs` holds the pure `assess` and `next_deadline`, with one boundary helper pair `expired`/`expiry` (1 ms resolution, so a deadline wake never re-arms the instant it fired at).
- Refusals are projected once at the edge into narrow `ContractFault`/`CapacityRefusal` enums (`RefusalClass::from_wire`).
- `HandlerHealth = Live { lag_since, contract_fault, draining_since } | Declared { cause, evidence_at }`. Declared is absorbing.
- `SuspicionOutcome::of` is the one place lag-suspicion endings are classified.
- Client loss reports are an input with weight 0.
- L = `APPLY_LAG_FACTOR` (2) × N, together with the N-derived detection buckets, lives on `HeartbeatTiming`. There is no new config key.

**Evidence.**
- `confirmed_at` lives on the `MeetingHandlers` member and is set at `mark_active`.
- `HandlerLiveness` tracks `owed_generation`: what MC had sent by the PREVIOUS accepted heartbeat. Lag is judged against it, so a push racing one heartbeat is never lag.
- The acceptance rule is tightened (security S-2, **revised at Layer 7**, see Issues): a heartbeat ahead of MC's sent generation is liveness, but the accepted watermark is CLAMPED to sent (`clamp_to_sent`), and the ahead heartbeat is still counted. A refusal counts only while MC's policy is pending (applied < owed).
- New anomaly reasons `apply_refused_invalid` / `apply_refused_transport_inconsistent` are counted on every heartbeat that carries them and logged at ERROR on transition.

**Actor (`actors/meeting/health.rs`, new child module).**
- `assess` runs on every OWNED heartbeat (stale and ahead included).
- A new `sleep_until` health arm fires at the earliest `next_deadline`. It stays armed because it is recomputed every iteration.
- **Stall guard:** on fire, the arm first processes a SNAPSHOT of the queued mailbox backlog through the one `dispatch` path, stopping if a message ends the meeting, and then judges unconditionally. This is bounded and cannot be starved.
- A Lagging handler's current snapshot is re-published to its worker (`HandlerPusher::republish`) on each accepted heartbeat.
- One INFO is logged per health transition, with bounded scalars only.

**Removal transaction (`remove_handler_completely`), in order:**
1. `MeetingMedia::remove_handler`: set drop, removed record and version bump, liveness, health, the push worker moved OUT, connectivity purge (`forget_handler`, keyed by (handler, connection), tombstones dropped too), and re-route.
2. A normal re-render.
3. The client set push (typed path, tokens minted post-removal), then the view flush.
4. The bulk binding release (`HandedOutBindings::release_handler`, sharing one kernel with `release_absent`, ids never reissued).
5. A spawned `teardown::release_removed`:
   - single-handler `quiesce` on the worker's own child token;
   - one `EndMeeting(trigger cause)` via `end_meeting_removal` + `release_one(attempts = 1)`, never retried;
   - `PolicyGenerations::remove_pair`.

The JoinHandle is handed to the meeting's `MediaTeardown` and awaited before the meeting is evicted, so the controller fence covers it. Declarations and detection are recorded once, at the transition.

**Teardown.** The plan names the CURRENT set only, with MEETING_ENDED (already true since P1; now tested against a removed handler).

**Telemetry.**
- New: `mc_media_handler_declarations_total{trigger}`, `_suspicions_total{outcome}`, `_detection_seconds{trigger}` (per-trigger start points per observability), `mc_media_meetings_without_handlers` (recompute-and-set `HandlerlessCensus`; `MutedSourceCensus` generalised to one `MeetingCensus<V>`, pruned on the controller walk, cleared at teardown, 0 at boot).
- One contract ADDITION, approved by observability: `mc_media_handler_removal_releases_total{trigger, outcome}`. Removal releases stay off `mc_media_end_meeting_total`, whose failure alert would otherwise page on every MH crash.
- Each new series has a catalog entry, a panel (new "Handler Detection" row in `mc-media.json`), zero-init + a render test, and ADR-0032 tests.

**Tests.**
- Tier 1a covers the table in `handler_health.rs`, plus unit tests for `liveness`, `placement`, `pusher` (republish, per-handler drain), `epoch`, `connectivity`, `generation`, `teardown` and the census.
- Tier 1b: new `tests/handler_detection_integration.rs`, 12 tests:
  - the deadline arm alone;
  - both stall-guard twins plus an end queued in the backlog (mutation-checked: disabling the drain fails the queued-heartbeat test);
  - removal order observed AT a held `EndMeeting`, edges on the survivor, no aud = X, no re-push, teardown releases the current set only;
  - a failed EndMeeting is called once, followed by a RELEASED heartbeat;
  - a zombie Disconnected with a colliding id;
  - sender-id non-reuse;
  - lag held to Impaired with republish observed, and catch-up cleared;
  - contract fault never declared vs capacity refusal declared;
  - draining removes nothing.
- **Fixture change (wider class):**
  - The suite's fake MH now behaves like a deployed MH-2+ handler. `heartbeat_via` heartbeats every meeting it acked, mirroring MC's held connection ids so the remove-only reconcile never retires live keys.
  - The full test stack enables it.
  - `advance_live` steps paused time below one interval.
  - Without this, 35 existing tests across 7 suites, which advance time past N with a never-heartbeating fake, would read as real Dead declarations. Two MC-B1 assertions changed meaning by design and were updated: an ahead-of-sent heartbeat's watermark is clamped to sent, and removal bulk-releases.
- Env (paired-test, `40_resilience.rs`): the crash-to-survivor leg (owned-MH placement, non-vacuous pre-kill transport evidence, per-participant set → {survivor}, declarations + detection rise, GC superseded{both} +1, unknown_handler unchanged) and the zombie leg (removed-while-frozen as the first assertion, carrier-agnostic release on THAT pod exactly once, owner_silence unchanged).

**Docs.**
- `mc-deployment.md`: L = 2N beside N, the MC-B2 hard ordering (MH-2+ first, no MH rollback below MH-2, pre-deploy heartbeat check), and the MC-B2 → MC-B1 rollback note.
- `docs/TODO.md`: S1b residual extended to forge-to-declare.
- MC `INDEX.md` updated.

---

## Files Modified

New:
- `crates/mc-service/src/media_routing/handler_health.rs`
- `crates/mc-service/src/actors/meeting/health.rs`
- `crates/mc-service/tests/handler_detection_integration.rs`
- `crates/env-tests/src/fixtures/owned_placement.rs` (paired-test)

Modified (MC):
- `crates/mc-service/src/media_routing/{liveness,placement,pusher,teardown,generation,connectivity,mod}.rs`
- `crates/mc-service/src/media_admission/epoch.rs`
- `crates/mc-service/src/actors/{meeting,meeting_media,controller,messages}.rs`
- `crates/mc-service/src/observability/metrics.rs`
- `crates/mc-service/src/webtransport/connection.rs` (test call-site)
- `crates/mc-service/tests/{common/mod.rs,common/media_dial.rs,handler_set_integration.rs,heartbeat_intake_integration.rs,media_dial_integration.rs}`

Modified (env-tests, paired-test):
- `crates/env-tests/tests/40_resilience.rs`
- `crates/env-tests/src/fixtures/{metrics,mod}.rs`
- `crates/env-tests/tests/44_gc_mh_registration.rs`
- `crates/env-tests/README.md`

Docs/infra:
- `docs/observability/metrics/mc-service.md`
- `infra/grafana/dashboards/mc-media.json`
- `docs/runbooks/mc-deployment.md`
- `docs/TODO.md`
- `docs/specialist-knowledge/meeting-controller/INDEX.md`

---

## Devloop Verification Steps

Gate 2 attempt 1: L1-6 OK, L7 FAIL (26/37/38/39 — see Issue 1). Gate 2 attempt 2 (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`): L1-L7 all OK, `TOTAL_RESULT=OK` (L7 1738s; 40_resilience crash + zombie legs green).

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | SEC-R1: sender_id non-reuse test made discriminating (epoch-reset unobserved, release proven before next admission) |
| Test (paired) | RESOLVED-FIXED | 6 | 6 | 0 | dup assess row, dead assertion, busy-mailbox proof, ordering doc, exact stream count |
| Observability | RESOLVED-FIXED | 7 | 7 | 0 | declaration cause/detection_ms in log, one INFO per transition, $__range detection panel, histogram zero-init note, quiesce alert wording; approved removal_releases_total contract addition; OBS-ALERTS block extended in story |
| Code Quality | RESOLVED-FIXED | 10 | 10 | 0 | #[expect], metrics only on successful removal, attempts derived from TeardownReason, named types, module hygiene |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | TRIGGER_LABEL in MC+GC, INITIAL_HANDLERS_KEY drift-tested, shared tests/common/coordination.rs |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | post-deploy rollback-trigger checklist in mc-deployment.md; carried check (deferred health arm stays armed) passed |
| Semantic Guard | CLEAR | 0 | 0 | 0 | credential-leak, actor-blocking, error-context, metrics-path-completeness all clean |
| Media Handler (conditional) | RESOLVED-FIXED | 3 | 3 | 0 | zombie leg registered_meetings check, exact test citations, removal contract-defect outcomes at ERROR |

Post-Layer-7 delta round (S-2 revision): security re-verified (C1-C3); code-quality #10 shared refusal gate `Observations::effective_refusal`; test D1 real capacity-refusal Tier-1b test; ops F2 / obs F6-F7 catalog wording; semantic-guard, DRY, media-handler re-confirmed.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `0bf0e44f1e83da56052642280637d01a9eaadd2f`
2. `git diff 0bf0e44f..HEAD`
3. `git reset --soft 0bf0e44f` / `git reset --hard 0bf0e44f`
7. **Safe-revert unit**: the whole commit (detection, removal, fake-MH heartbeat test rig and docs move together; MC-B2 → MC-B1 rollback is safe per mc-deployment.md since removal state is in-memory only).

---

## Issues Encountered & Resolutions

### Issue 1: Gate 2 attempt 1 — Layer 7 regressions in 26/37/38/39
**Problem**: 4 pre-existing media env-tests lost their MH client session mid-test ("not connected" / stream reset 18241); 40_resilience passed.
**Root cause**: the env-test Injection fixture writes policy for the real MC's meetings directly on MH at a generation MC never sent, so MH heartbeats applied > sent. The planned S-2 rule (ahead = not liveness) declared those healthy handlers Dead at N, and a STREAM_CEILING refusal of an injected policy was read as Impaired; close 18241 was the removal's EndMeeting downstream.
**Resolution**: ahead heartbeat = liveness with the watermark clamped to sent (`liveness::clamp_to_sent`); a refusal is evidence only while MC's policy is pending (`Observations::effective_refusal`, shared by assess and the contract-fault anomaly). Regression tests at Tier 1a/1b, S1b residual text narrowed (security C1-C3).

---

## Lessons Learned

1. A one-way decision built on an acceptance rule turns that rule's edge cases into removals; out-of-band writers (env-test Injection) exist and must be modelled — clamp, don't reject.
2. Refusal evidence must be attributed to the writer whose policy is pending (applied < owed) before it can drive a declaration or a contract-fault ERROR.
