# Devloop Output: MC-B1 — Heartbeat intake, verdicts, remove-only reconcile, binding gate, connection_id required

**Date**: 2026-11-10
**Task**: run-story task #13 (story `docs/user-stories/2026-10-31-survive-handler-loss.md`) — MC serves MeetingHeartbeat, verdicts from meeting-actor state, remove-only reconcile, binding gate, connection_id REQUIRED, MC_MH_HEARTBEAT_TIMEOUT_MS config, telemetry retirements.
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~3h (planning ~7m, implementation ~47m, review ~25m, Gate 2 ~32m incl. Layer 7)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `f32d6443c52b25b9a5f5374b7b7e05f672998e69` |
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
| Test | `paired-test` (paired, replaces test slot) |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | not spawned (no check surface: no credential/secret types, no Debug over payloads/keys) |
| Protocol | `paired-protocol` (paired; GSA owner for `proto/**`) |

---

## Task Overview

### Objective
See task-13 prompt (run-story). Governing design: ADR-0039 §1, §2, §6.

### Scope
- **Service(s)**: mc-service (+ proto internal contract, infra config, dashboards, alerts, runbooks)
- **Schema**: No
- **Cross-cutting**: Yes (proto GSA)

### Debate Decision
NOT NEEDED — ADR-0039 governs.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `proto/dark_tower/internal/v1/internal.proto` (rpc line; banner present tense; `connection_id` REQUIRED on both Notify requests; outcome enumeration FOUR; `handler_id` compare text; SIZE AND VALIDATION; P-2 residual; `drain_remaining_ms` receiver rule; interval formula) | Domain-judgment (GSA) | protocol |
| `crates/proto-gen/tests/internal_roundtrip.rs` (doc/message rewording only) | Domain-judgment (GSA) | protocol |
| `packages/proto-gen/scripts/verify-codegen.sh` (positive `meetingHeartbeat: {` assertion) | Domain-judgment (GSA) | protocol |
| `docs/decisions/adr-0039-mh-replacement-and-migration.md` (§1 dated amendment note only; rule home stays the proto) | Domain-judgment | protocol (paired); security reviews (records P-1) |
| `crates/mc-service/src/**` | Mine |  |
| `crates/mc-service/tests/**` (new `heartbeat_intake_integration.rs`; updated fixtures and suites) | Mine (paired-test reviews) |  |
| `crates/mc-test-utils/src/media.rs` | Mine |  |
| `infra/services/mc-service/config.env` (new REQUIRED key) | Minor-judgment | infrastructure |
| `infra/services/mc-service/mc-0-deployment.yaml` (configMapKeyRef) | Minor-judgment | infrastructure |
| `infra/services/mc-service/mc-1-deployment.yaml` (configMapKeyRef) | Minor-judgment | infrastructure |
| `infra/grafana/dashboards/mc-media.json` (new Handler Heartbeats row; retired panels; selector edit) | Mine (MC-owned board, ADR-0031) | observability reviews |
| `infra/docker/prometheus/rules/mc-alerts.yaml` (MCMediaGenerationDivergence expr `outcome!="match"`, re-assert text removed) | Mine (ADR-0031 per-service) | observability + operations review |
| `docs/observability/metrics/mc-service.md` | Minor-judgment | observability |
| `docs/observability/alerts.md` | Minor-judgment | observability |
| `docs/observability/metrics/mh-service.md` (one retired-name cross-reference) | Minor-judgment | observability |
| `docs/runbooks/mc-deployment.md` | Minor-judgment | operations |
| `docs/runbooks/mc-incident-response.md` (Sc 13, 15, 18) | Minor-judgment | operations |
| `docs/runbooks/client-dev-local.md` | Minor-judgment | operations |
| `docs/runbooks/mh-deployment.md` (retired-name / connection_id coupling text) | Minor-judgment | operations (media-handler informed) |
| `docs/runbooks/mh-incident-response.md` (retired gauge reference) | Minor-judgment | operations (media-handler informed) |
| `crates/mh-service/tests/common/mock_mc.rs` (`meeting_heartbeat` -> `Status::unimplemented` stub; tonic trait compile coupling) | Minor-judgment | media-handler |
| `crates/mh-service/tests/media_session_binding_integration.rs` (assertion message text only) | Mechanical | media-handler |
| `crates/mh-service/tests/webtransport_integration.rs` (assertion message text only) | Mechanical | media-handler |
| `crates/env-tests/tests/26_mh_quic.rs` (retired-name references) | Minor-judgment | test |
| `crates/env-tests/src/fixtures/injection.rs` (retired-name reference) | Minor-judgment | test |
| `crates/env-tests/tests/02_mc_deployment_config.rs` (new REQUIRED key in `WIRED_KEYS`) | Minor-judgment | test |
| `docs/user-stories/2026-10-31-survive-handler-loss.md` (MC-B1 "Add:" gains the anomalies counter) | Mechanical |  |
| `docs/TODO.md` (tick MC-B1 pins; close bindings-leak and handler_id entries; narrow connectivity item 2) | Mechanical |  |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine |  |

---

## Planning

### Mechanism restatement

MC becomes the per-(meeting, handler) verdict authority: an MH asks "am I still in your set?" and MC answers from the one meeting actor's state, and uses the answer's connection list to *shrink* (never grow) its own connected/binding state. Same-class set beyond the task text: (i) the binding gate is the add-side of the same invariant "only an in-set, non-removed handler may create MC state" — it also covers Disconnected (removed handler = no-op) and empty ids; (ii) the per-key bind instant is needed in TWO stores (connectivity keys and the `HandedOutBindings` ledger), both covered; (iii) `RegisterMeetingResponse.handler_id` comparison — its outcome is retired, so where the comparison lands is decided below (OPEN-1).

### Design

1. **Proto (GSA, protocol)**: `rpc MeetingHeartbeat` after `NotifyParticipantDisconnected`, short pointer comment to the message header. Banner rewritten to present tense; ORDERING paragraph and "Enforcement owner" lines kept. `NotifyParticipantConnectedRequest.connection_id`: EMPTY MEANS LEGACY paragraph (incl. RETIREMENT CONDITION) replaced with REQUIRED text (protocol's wording); sibling pointer on `NotifyParticipantDisconnectedRequest.connection_id` updated. Field stays bare `string`. Roundtrip test doc rewording; verify-codegen positive `meetingHeartbeat: {` assertion (casing read from regenerated output).
2. **gRPC handler** (`media_coordination.rs`, `#[instrument(skip_all)]`), order:
   1. validate: `validate_id_field` on meeting_id/mc_id/handler_id; `serving_state` must decode to SERVING|DRAINING (0 and unknown numbers -> INVALID_ARGUMENT); `apply_refusal` unknown number -> INVALID_ARGUMENT; SERVING with non-zero `drain_remaining_ms` -> INVALID_ARGUMENT (DRAINING with 0 accepted = deadline reached); `connection_ids.len() <= max_heartbeat_connection_ids` (ONE named fn: `config.max_participants` (MC-wide total; no per-meeting cap exists) `* MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER`, computed once at service construction), `validate_id_field` on each entry. Nothing logged per heartbeat; ids never logged as a list.
   2. stamp `receipt = tokio::time::Instant::now()`.
   3. **owner check FIRST** (S-1): `mc_id != this MC's id` -> count `owner_mismatch`, `FAILED_PRECONDITION("Invalid request")` static message, before any controller lookup.
   4. route through the controller (`get_meeting_handle`, same path as Notify*). `MeetingNotFound` (the only definite "no actor") -> count `released_ended`, answer RELEASED(MEETING_ENDED), no actor mailbox touched. Every other failure (lookup timeout, actor mailbox full past the budget, actor closed, oneshot dropped) -> `UNAVAILABLE`, counted on `mc_messages_dropped_total{actor_type="meeting"}` (existing series, Q4), NEVER RELEASED (S-2).
   5. Budget: the lookup + `send_timeout` into the meeting mailbox share `interval / 2`; the reply wait is bounded by the remaining interval. `interval` is the same derived value pushed to MH (below).
3. **Meeting actor** `MeetingMessage::MediaHandlerHeartbeat { heartbeat: HandlerHeartbeat, receipt, respond_to }` -> `handle_handler_heartbeat`:
   - verdict from `MeetingHandlers`: member (Registering or Active) -> OWNED; in removed record -> RELEASED(its trigger); else RELEASED(UNSPECIFIED) (`released_unassigned`, WARN once per arrival — MH ends the meeting on it, so it is self-limiting). Owner-mismatch never reaches the actor. No UNKNOWN.
   - removed record carries the trigger: `removed: BTreeMap<HandlerId, RemovalTrigger>`, `RemovalTrigger {HeartbeatTimeout, ApplyLag, PlannedTransfer}` with `fn release_cause()` (the ONE projection to `ReleaseCause`); `MeetingHandlers::remove(id, trigger)`; `removal_trigger(id)` query. `remove` has no production caller until MC-B2. Test seam `MeetingMessage::RemoveHandlerForTest` behind `test-seams` (set-only removal; MC-B2 owns the full removal sequence) — mirrors `ForceKekRotation`.
   - only on OWNED: liveness update + reconcile (S-4). RELEASED mutates nothing.
4. **Liveness record** — new pure module `media_routing/liveness.rs`: `HandlerLiveness { last_accepted: Instant, previous_accepted: Option<Instant>, applied_generation: u64, apply_refusal, serving_state, drain_remaining_ms }`, and `fn observe(&mut self, hb, receipt) -> Acceptance {Accepted{previous: Option<Instant>}, Stale}`. Accepted iff `applied_generation >= last accepted` (equal accepted). A Stale heartbeat updates NOTHING (no instant shift, no reconcile, no interval observation) — paired-test/S-3; it still gets OWNED and still observes lag. Stored in `MeetingMedia` as `BTreeMap<HandlerId, HandlerLiveness>`; an entry is created only for a handler `resolve()` finds, and dropped when the handler leaves the set, so a heartbeat can never add a handler. Separate from `MeetingHandlers` deliberately: liveness changes must not touch set versioning. `pub(crate)` read accessor `liveness(&HandlerId)` for MC-B2's `assess()` plus a test-seams query on the actor handle. First heartbeat for a pair: `previous = None` -> retires nothing, no interval observation. Transition INFO log only on: first accepted heartbeat for a pair, serving_state change, apply_refusal change.
5. **Remove-only reconcile** (on Accepted with `previous = Some(p)`): connectivity keys become `BTreeMap<ConnectionKey, Instant /*recorded_at*/>`; `ParticipantConnectivity::retire_absent(handler, present, before: p) -> Vec<ConnectionKey>` retires keys with `recorded_at < p` (strict; equal is KEPT) absent from `connection_ids`, through the same `disconnect` path (tombstoned), then `sync_routing`; actor runs `reconcile_media(All)` + `flush_views` iff any routing changed. Ledger: `by_connection: HashMap<(handler, conn), (SenderId, Instant)>`, `release_absent(handler, present, before)`; release only shrinks the exclusion snapshot used at the NEXT epoch reset — the current allocator never reissues within the generation (cursor-only, `SenderIdAllocator`), pinned by a test (S-4). Bind instant stamped in the actor at record time, heartbeat at gRPC arrival: skew errs toward KEEPING a key — doc comment states this so nobody "fixes" it (S-5). Never looks up an incoming id: walks MC's own keys for that handler and tests membership in a `HashSet<&str>` of the request.
   - `mc_media_handler_connected_set_corrections_total` unit (Q1): one per distinct `(handler, connection_id)` retired from MC state by a heartbeat — union of connectivity keys and ledger entries (a departed participant's leaked binding is the same lost Disconnected). Catalog states this. One INFO line per correction event (meeting_id, handler_id, count; not the id list).
6. **Binding gate** (`handle_media_connected`): order becomes (a) empty `connection_id` -> sender 0, `Unapplied::ConnectionIdMissing` (new reason `connection_id_missing`, zero-init, catalog), nothing recorded; (b) handler `resolve()` fails (out-of-set OR removed) -> sender 0, `HandlerNotInSet`, nothing recorded; (c) roster resolve; (d) only then `mh_bindings.record` + connectivity. Wire answer for (a)/(b) needs a binding outcome: new `SenderLookup::Refused` -> `SenderBindingOutcome::Refused` ("refused") on `mc_media_sender_bindings…{outcome}` (OPEN-2). Disconnected: empty id -> counted no-op (`connection_id_missing`); removed/out-of-set handler -> `HandlerNotInSet` no-op that does NOT release a ledger entry and touches no connectivity (a removed handler's ledger entries are released by MC-B2's bulk release after the set push). `HandlerNotInSet` rustdoc loses the "story-4 re-registration" framing.
7. **Legacy deletion**: `ConnectionKey::is_legacy`, legacy branches in connectivity.rs, `unreleasable` in epoch.rs, `validate_optional_id_field` (connection_id now goes through the gate, not INVALID_ARGUMENT, so MH sees sender 0 as the task states), `record_notification_without_connection_id` + zero-init. Replacement tests for every deleted test (below).
8. **Config**: `MC_MH_HEARTBEAT_TIMEOUT_MS` via `bounded()` with `MIN_MH_HEARTBEAT_TIMEOUT_MS = 1`, `MAX_MH_HEARTBEAT_TIMEOUT_MS = 30_000` beside the settle-window consts; REQUIRED, `MissingEnvVar` on one line, no Rust default (6000 lives in config.env only). `Config::mh_heartbeat_interval()` — the ONE derivation: `max(1, N / 3)` ms (floor, so three beats fit inside N — `3*I <= N` for N >= 3; below that the interval is 1 ms — and `I >= 1` for every N in the Lead's (0, 30000], range kept verbatim; revised from ceil at paired-test D). Fed to `RegisterMeetingRequest.heartbeat_interval_ms` (via `MediaRoutingDeps`/push target, `u32::try_from`, no `as`), to the actor-send budget, and to the histogram buckets. config.env comment: enforced by, range, rolls mc-0/mc-1, echo gauge. configMapKeyRef (non-optional) in both deployments; env-test 02 / dt-guard env_config checked.
9. **Re-assert framing deletion**: media_routing/mod.rs:28, pusher.rs:52 + "story-4 periodic re-assert" paragraphs, teardown.rs STORY-4 NOTE, generation.rs registry doc, meeting_media.rs + media_coordination.rs hazard paragraphs (now: lost Disconnected on a live MH is healed by the reconcile; crash is MC-B2). No dependency on `process_start_epoch_ms` added.

### Telemetry

- `mc_media_handler_heartbeats_total{outcome}` — `HeartbeatOutcome` enum (`ALL` + `label()`), zero-init 5, key_custody=operator. Counted where answered (router for owner_mismatch/released_ended, actor for the rest).
- `mc_media_handler_heartbeat_interval_seconds` — histogram, buckets derived from N in `configured_prometheus_builder` (`[N/6, N/3, N/2, 2N/3, N, 1.5N, 2N]`, `Matcher::Prefix`). Q2: observed on ACCEPTED heartbeats with a previous accepted receipt only — it is the liveness clock MC-B2 reads; a stale arrival is not liveness, and the stale count is visible through lag.
- `mc_media_handler_applied_sequence_lag` — histogram, integer buckets `[0,1,2,3,5,10,25,100]`, `current(meeting, handler).saturating_sub(applied)`; Q3: no observation while MC has never pushed to the pair (`current()` is None).
- `mc_media_handler_connected_set_corrections_total` — counter, unit above.
- `mc_media_handler_heartbeat_deadline_seconds` — config gauge beside `set_connect_settle_window`.
- Q4 drop: existing `mc_messages_dropped_total{actor_type="meeting"}` (no new series).
- Empty id refusal: `mc_mh_notifications_unapplied_total{reason="connection_id_missing"}` (no renamed counter).
- Retired, every site in this commit (lists from @dry-reviewer/@observability/@paired-protocol adopted verbatim): `mc_media_generation_divergence` (+ `divergence_magnitude` if it has no other consumer), `handler_id_mismatch`, `mc_mh_notifications_without_connection_id_total`. Catalog keeps a one-line "RETIRED (story 4 MC-B1), version-skew only" note each. Alert expr -> `outcome!="match"`, byte-identical in alerts.md and the dashboard "exact selector" panel. `git grep` residue pasted here after implementation.
- Panels on mc-media.json for each new series.

### OPEN questions (need a ruling)

- **OPEN-1 (protocol, observability, Lead)**: retiring `handler_id_mismatch` leaves `internal.proto`'s "MC MUST compare `handler_id` and FAIL LOUD" without a home. Proposal: keep the comparison, move it out of `PolicyPushOutcome` (canonical enumeration becomes four values), and treat a mismatch as a TERMINAL registration failure — under ADR-0039 a different id at the dialed endpoint means the dialed incarnation is gone — counted on the existing `mc_register_meeting_total{status="error"}` + `error!` line. Proto text at the outcome enumeration and at `handler_id` updated accordingly.
- **OPEN-2 (observability, security)**: the gate's sender-0 answer needs a `SenderBindingOutcome`; proposal `handler_refused` (covers out-of-set/removed/empty-id; the unapplied reason gives the split).
- **N floor**: Lead range (0, 30000] kept; zero interval made unrepresentable by `max(1, N/3)`, pinned at N=1 -> 1, N=2 -> 1, N=3 -> 1, N=7000 -> 2333, N=30000 -> 10000.

### Tests (paired-test items 1-9; all `start_paused`, current_thread)

New `crates/mc-service/tests/heartbeat_intake_integration.rs`:
- `verdicts_all_arms_in_one_test` — OWNED Active, OWNED Registering, RELEASED removed x2 with DIFFERENT triggers, RELEASED(MEETING_ENDED) no actor, RELEASED(UNSPECIFIED) never-in-set, FAILED_PRECONDITION with in-set handler AND with no-actor meeting; per-arm outcome delta + adjacency; OWNED cause UNSPECIFIED.
- `first_heartbeat_before_register_ack_is_owned` (held ack, then released, still OWNED).
- `reconcile_retires_only_keys_bound_before_the_previous_heartbeat` (K1/K2 timeline, equal-instant kept, hb4 retires K2, first heartbeat retires nothing, ledger released, routing resync asserted on the pushed snapshot, corrections delta exact).
- `a_heartbeat_never_adds_a_key` (unknown id listed: ledger/connectivity/routing unchanged).
- `stale_heartbeat_is_not_liveness` (gen 5, 4, 5, 6; liveness record read via seam; nothing retired by the stale one; interval not observed).
- `binding_gate_refuses_out_of_set_and_removed_handlers` (sender 0, ledger unchanged via seam, connectivity unchanged, unapplied +1; positive in-set arm).
- `zombie_disconnect_never_touches_the_replacement`.
- `empty_connection_id_is_refused_on_both_events` (replaces `an_empty_connection_id_is_counted_as_legacy`).
- `serving_state_unspecified_and_unknown_are_rejected`, `oversized_connection_ids_list_is_rejected` (cap and cap+1), `drain_remaining_on_serving_is_rejected`.
- `a_full_meeting_mailbox_answers_unavailable_and_counts_the_drop` (never RELEASED).
- `heartbeat_interval_and_lag_are_observed` (lag unobserved before any push; exact value after; buckets rendered for N=7000 differ from the 6000 set).
- paired-test A/B/C: cross-handler isolation in reconcile (H2 key/ledger untouched by H1's heartbeat); RELEASED mutates nothing (removed handler's empty heartbeat leaves its keys/ledger, corrections unobserved; zombie Disconnected releases no ledger entry); mailbox-full via test-seams `ParkForTest` + fill to capacity, dropped exactly +1, UNAVAILABLE, positive arm OWNED after release.
Unit: config.rs (`N=0`, `N=30001` rejected; 1, 30000 accepted; missing key; interval derivation table), liveness.rs, connectivity.rs (`retire_absent` boundaries; replaces `legacy_empty_id_has_set_semantics_and_no_tombstone` with `retired_keys_are_tombstoned_by_reconcile`), epoch.rs (`release_absent`; replaces `a_binding_without_a_connection_id_is_never_released` with `released_binding_never_reissued_within_the_generation`), placement.rs (removal trigger recorded per id), metrics.rs zero-init render of the new series. mh_client/register path: `register_meeting_carries_the_derived_heartbeat_interval` (N=7000 -> 2333 at the fake MH). Retirement: confirm.rs tests repurposed for OPEN-1's outcome.

### Gate-1 addenda (adopted)

- **security P-1**: `connection_ids` over the cap, or any entry failing `validate_id_field`, does NOT reject the heartbeat: the verdict and liveness update still happen, the reconcile is SKIPPED for that heartbeat (no keys retired, no HashSet built), and the skip is counted (series per observability ruling; proposal `mc_media_handler_reconcile_skipped_total{reason=over_cap|malformed_id}`). The liveness/verdict path never depends on the list being well-formed. Rationale: MC's caps do not bound MH's "begun, not Disconnected" superset (answered-but-over-key-cap binds, departed participants with live MH connections), and a rejected heartbeat would be a false Dead after MC-B2. Proto SIZE AND VALIDATION paragraph updated (protocol). tonic's decode limit bounds the allocation.
- **security P-2**: internal.proto NOT AN AUTHORIZATION CONTROL gains one residual sentence: a forged heartbeat with an empty `connection_ids` retires every key/binding that handler bound before its previous heartbeat, without knowing any id (DoS only; no key exposure; R-35 holds, allocator cursor-only).
- **security**: send budget = `max(1 ms, interval / 2)`.
- **security**: MC never adopts or rewrites the handler id seen in a `RegisterMeetingResponse`; nothing is recorded on mismatch (proto text cites it).
- **protocol OPEN-1 (a)/(b)/(c)**: a mismatched response gives NO credit — handler stays Registering, echo/applied_generation not consumed (test both arms); outcome enumeration FOUR, landing site named (`mc_register_meeting_total{status="error"}` + `error!` with ids); `applied_generation` field: divergence-gauge clause repointed to the heartbeat's `applied_generation` / applied-sequence lag, ZERO paragraph's "re-register" -> "reads as applied-generation lag, an ADR-0039 §6 APPLY_LAG input". `drain_remaining_ms` receiver rule (SERVING + non-zero INVALID_ARGUMENT; DRAINING + 0 = deadline reached) stated at the field.
- **dry 1**: ONE removal helper on the actor (`remove_handler(id, trigger)`) is the only production caller of `MeetingHandlers::remove` and drops the liveness entry in the same call; `RemoveHandlerForTest` goes through it; asserted `liveness(id) == None` after removal and a later heartbeat answers RELEASED without recreating an entry; rule recorded at liveness.rs module doc.
- **dry 2**: ONE retire predicate `liveness::retirable(recorded_at, before, present) -> bool` (strict `<`, equal kept), called by both `retire_absent` and `release_absent`; boundary pinned once in its unit test.
- **dry 3**: ONE bind instant: `handle_media_connected` captures `Instant::now()` once and passes it to both the ledger and connectivity.
- **paired-test D**: interval `max(1, N/3)` (floor) — see item 8.
- **code-reviewer 1**: verdict decision is a pure `MeetingHandlers::heartbeat_verdict(&HandlerId)` unit-tested in placement.rs; liveness record, observe/reconcile glue and lag computation in liveness.rs; `handle_handler_heartbeat` orchestrates only (< ~50 lines).
- **code-reviewer 2/3**: exhaustive matches, no `_` arms; `ALL` coverage tests; `remove` dead-code via `cfg_attr(..., expect(dead_code, reason))` if needed, never `allow`.
- **code-reviewer 4**: the budget split is one named fn beside `Config::mh_heartbeat_interval()` (`heartbeat_send_budget() = max(1 ms, interval/2)`); `max_heartbeat_connection_ids` via `checked_mul`, failing at construction.
- **code-reviewer 5**: `applied > current` is a contract fault (MH cannot apply a generation MC never sent; `current` is recorded before the push is sent) -> `warn!` + counted, never a silent 0. Counter shared with the P-1 skip (observability ruling pending): proposal ONE `mc_media_handler_heartbeat_anomalies_total{reason=connection_ids_over_cap|connection_id_malformed|applied_ahead_of_sent}`.
- **code-reviewer 6**: legacy + retired-name `git grep` residue pasted here after implementation.
- **code-reviewer 8**: `SenderLookup::Refused` rustdoc lists the three causes (out-of-set, removed, empty connection_id); the unapplied reason carries the split.
- **observability OPEN-1 (amended, adopted)**: a `handler_id` mismatch lands on `mc_register_meeting_total{status="handler_replaced"}` (bounded constant, all three statuses zero-initialised, catalog: "the dialed incarnation is gone; expected on MH restart; not a coordination failure; MCRegisterMeetingFailureRate reads status=\"error\" only"), `warn!` with expected/got ids, worker stops; register_meeting_integration repurposed to assert it. Proto wording agreed with protocol.
- **observability OPEN-2**: token `refused` (not `handler_refused`) on `mc_media_sender_binding_responses_total`; catalog points the cause split at `mc_mh_notifications_unapplied_total{reason=handler_not_in_set|connection_id_missing}`; tests assert both.
- **observability Q2 condition**: `applied_sequence_lag` observed on EVERY OWNED arrival (stale included); interval only on accepted heartbeats with a previous accepted receipt; test: a stale heartbeat yields zero interval observations and one lag observation.
- **protocol (post-P-1)**: the interval property is stated QUALIFIED everywhere ("`3*I <= N` for N >= 3; below that the interval is 1 ms"); `heartbeat_interval_ms` field text becomes "floor(N/3), minimum 1" (one exact formula home, `Config::mh_heartbeat_interval()` cites it). SIZE AND VALIDATION paragraph names the receiver's list bound by its source (`MC_MAX_PARTICIPANTS` x `MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER`, the MC-side constant MC enforces in `crates/mc-service/src/media_routing/connectivity.rs`, named in the proto text), states that an over-cap or malformed list skips the WHOLE reconcile (never drops just the bad entry), counted, while the verdict and liveness still apply under the same applied_generation-not-lower rule. ADR-0039 §1 gets a dated amendment note pointing at the proto rule home. P-2 residual sits beside the existing forged-heartbeat residual in NOT AN AUTHORIZATION CONTROL.
- **observability (anomalies counter APPROVED)**: `mc_media_handler_heartbeat_anomalies_total{reason=connection_ids_over_cap|connection_id_malformed|applied_ahead_of_sent}` — enum ALL + label(), zero-init 3, key_custody=operator, catalog expected-zero ("non-zero = MH contract or version-skew fault"), panel sum by reason, one MetricAssertion test per reason (over-cap/malformed also assert verdict counted + no correction); `applied_ahead_of_sent` records NO lag observation (asserted unobserved, no clamp); warn! transition-only per (meeting, handler), malformed id contents never logged (meeting_id, handler_id, count/len only). Added to the story's MC-B1 "Add:" list. **WARNING candidate for OBS-ALERTS.**
- **protocol (OPEN-1 text)**: `RegisterMeetingResponse.handler_id` rewritten per protocol's text (expected cause = later incarnation at a reused address, ADR-0039 §7; registration FAILS for the dialed handler; MC records nothing, does not advance state, NEVER adopts the answering id — a new incarnation enters only via GC's acquire path; counted `status="handler_replaced"`, ids in a WARN log only). Outcome enumeration FOUR; "every path that can fail on this response has a bounded label". `mc_register_meeting_total` catalog: 3 statuses, zero-init, panel legend checked.
- **observability interval**: `Config::mh_heartbeat_interval()` is the single derivation feeding RegisterMeetingRequest, the send budget and the histogram buckets; config test table uses the floor values.
- **protocol (bound wording)**: `MC_MAX_PARTICIPANTS` is MC-WIDE, not per-meeting (config.rs; MC has no per-meeting participant cap). Proto SIZE AND VALIDATION and the ADR amendment say: "bounded by `MC_MAX_PARTICIPANTS` (MC-wide total, crates/mc-service/src/config.rs) x `MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER` (crates/mc-service/src/media_routing/connectivity.rs); MC has no per-meeting participant cap, so this is the tightest bound MC holds." The phrase "per-meeting caps MC already enforces" is removed wherever it remains (proto header, ADR amendment, MC code docs).

### Ops

mc-deployment.md: skew bullet rewritten — HARD ordering: MH at/above the task-20 (connection_id) image before this MC rolls, never rolled back below while this MC is live; pre-deploy check `sum(increase(mc_mh_notifications_without_connection_id_total[24h])) == 0` on the OLD MC; new REQUIRED key in the "rides this release" bullet; §Configuration Reference row (default 6000, range (0, 30000] ms, interval max(1, N/3), relation to `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` [300, 3600], trade-offs, mailbox load; small N is legal but expensive — do not set below ~1500 ms without load testing); mh-deployment.md :322-325, :380, :1421 (retired counter + connection_id coupling note: an old MH's Connecteds are now REFUSED, not degraded); rollback: MC-B1 forward-safe before MH-2, MC-B1 is the floor MH-2 requires, alert-rule rollback hazard (roll rule with image or expect the `handler_id_mismatch` page). Safe-revert unit: the whole commit; ConfigMap key harmlessly unused on revert.


---

## Gate 1 — Plan Confirmations

Lead rulings: OPEN-1 (handler_id mismatch → terminal registration failure, `mc_register_meeting_total{status="handler_replaced"}`, WARN) accepted — protocol (GSA owner) + observability concur. OPEN-2 (`refused` binding outcome) accepted. Interval `max(1, floor(N/3))` accepted; range (0, 30000] unchanged. Classification-sanity guard: STATUS=OK.

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Protocol (paired) | confirmed |

---

## Pre-Work

None

---

## Implementation Summary

- **Heartbeat intake** (`grpc/media_coordination.rs`): `rpc MeetingHeartbeat` served. Receipt stamped at arrival; ids and enums validated (INVALID_ARGUMENT); owner check FIRST (FAILED_PRECONDITION, static message, counted `owner_mismatch`); `connection_ids` screened (over the MC-wide bound or malformed → reconcile skipped, counted; never rejected); routed through the controller with lookup+send bounded by `send_budget` (= max(1 ms, interval/2)) and the reply by one interval. Only `MeetingNotFound` → RELEASED(MEETING_ENDED); every other routing failure → UNAVAILABLE + `mc_messages_dropped_total{actor_type}`.
- **Verdict + liveness** (`placement.rs::heartbeat_verdict`, `liveness.rs`, `actors/meeting.rs::handle_handler_heartbeat`): OWNED (Registering included) / RELEASED(removal trigger) / RELEASED(UNSPECIFIED). Removed record now carries `RemovalTrigger` (one projection `release_cause()`). Per-handler `HandlerLiveness` beside the set; accepted iff applied_generation not lower; stale changes nothing but still reads lag. ONE removal path `MeetingMedia::remove_handler` drops liveness too.
- **Remove-only reconcile**: bind instants on connectivity keys and on `HandedOutBindings` (one instant for both); ONE predicate `liveness::retirable` (strict, equal kept); runs only on an accepted OWNED heartbeat with a previous receipt; retirements tombstone like a Disconnected and resync routing through the normal path; counted once per distinct (handler, connection_id).
- **Binding gate**: membership checked before any record (sender 0, `refused` + `handler_not_in_set`); removed-handler Disconnected is a no-op (ledger kept for MC-B2's bulk release). Empty `connection_id` refused at the gRPC boundary (`connection_id_missing`). Legacy mode, `unreleasable`, `validate_optional_id_field`, `is_legacy` deleted.
- **handler_id (OPEN-1)**: `confirm::handler_replaced` is the one comparison, checked before classification; mismatch → `McError::HandlerReplaced` (terminal, no credit, handler stays Registering), `mc_register_meeting_total{status="handler_replaced"}`, WARN. `PolicyPushOutcome::HandlerIdMismatch` and `divergence_magnitude` deleted.
- **Config**: `MC_MH_HEARTBEAT_TIMEOUT_MS` (0, 30000], REQUIRED; `HeartbeatTiming` is the single derivation (interval, send budget, histogram buckets, deadline gauge); interval sent in every RegisterMeeting.
- **Telemetry**: 5 new series (+ anomalies counter, observability-approved) with panels, catalog, zero-init and tests; 3 retirements with every reference; alert expr `outcome!="match"`.
- **Docs**: re-assert framing removed from MC docs (mod.rs, pusher.rs, teardown.rs, generation.rs, assignment.rs, meeting_media.rs, media_coordination.rs); runbooks (ordering, config row, rollback, Sc 13/15/18), proto text, ADR §1 amendment, TODO closures.

---

## Files Modified

See §Cross-Boundary Classification (every touched path is a row; `validate-cross-boundary-scope` clean).

---

## Devloop Verification Steps

- `cargo test -p mc-service --features test-seams`: all suites green (new `heartbeat_intake_integration.rs`: 10 tests).
- `./scripts/layer-fast.sh`: TOTAL_RESULT=OK (layers 1-6).
- Retired-name residue (`git grep -n 'handler_id_mismatch\|mc_media_generation_divergence\|without_connection_id\|is_legacy\|unreleasable\|validate_optional_id_field'` outside debates/decisions/devloop-outputs/user-stories/TODO/knowledge): only deliberate RETIRED notes — catalog entries (version-skew), the alert-rule rollback-hazard comment, mc-deployment.md pre-deploy check on the OLD MC + rollback note, Sc 15 retired row, `metrics.rs` doc + the zero-init test asserting the names never render, and one test-file doc line. No code path emits any of them.

- **Lead fast check (Step 6)**: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-fast.sh` TOTAL_RESULT=OK.
- **Gate 2 (Step 7.5)**: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` — L1 OK, L2 OK, L3 OK (112s), L4 OK (166s), L5 OK, L6 OK (cargo-audit preexisting advisories; buf-breaking passed), L7 OK (env-tests-passed + browser-e2e-passed, 1601s). TOTAL_RESULT=OK on first attempt (1 attempt of 3 / L7 1 of 2).

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | S-1..S-9, P-1/P-2 verified in code and tests |
| Test (paired) | RESOLVED-FIXED | 4 | 4 | 0 | stale-reconcile arm, empty-id Disconnected distinguishing input, precedence pins restored, bind-site asserts |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | RELEASED WARN dedupe (+ test), every anomaly counted (+ test), ActorType labels, fifth-value wording, constant named in catalog |
| Code Quality | RESOLVED-FIXED | 9 | 9 | 0 | CQ-1..CQ-9 |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | intake fixture used everywhere; `make_controller` hoisted (8 sites), TODO closed |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | env-test 02 sentence; handler_replaced interim documented; mh-incident-response restart remedy rewritten |
| Protocol (paired) | CLEAR | 0 | 0 | 0 | GSA owner sign-off; trailer recorded |

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `f32d6443c52b25b9a5f5374b7b7e05f672998e69`
2. `git diff f32d6443..HEAD`
3. **Safe-revert unit**: the whole commit (proto rpc + server impl + config key + alert rule + runbooks together). Reverting leaves the `MC_MH_HEARTBEAT_TIMEOUT_MS` ConfigMap key harmlessly unused. Not revertible once MH-2 is live (roll MH back first).

---

## Issues Encountered & Resolutions

### Interim behaviour on an in-place MH restart — weighed at Gate 3 (ops F2, Lead relay)

**Is it a reachable interim regression at this commit? Yes, for meetings that already held the restarted pod.** An in-place MH restart is a new incarnation (new per-incarnation `handler_id`, same address). At this commit:
- every push to that address for an EXISTING meeting is answered by the new incarnation → `handler_replaced` (terminal, not credited); a handler whose INITIAL registration hit this stays Registering (client-invisible);
- the new incarnation's Connecteds name a `handler_id` outside the set → binding gate → `sender_id` 0.
So that handler is unusable for those meetings until MC-B2 (detection/removal) and MC-C (replacement) land. Each such meeting runs on its other handlers; a meeting whose only handler was that pod has no media until re-created. New meetings GC places on the pod are unaffected.

**Today's (pre-MC-B1) behaviour, for comparison:** the mismatch was credited as `handler_id_mismatch` (Programmed), the new incarnation was re-programmed on the next structural change, and its Connecteds were answered with a real `sender_id` (connectivity not recorded — `handler_not_in_set` — but MH bound on the answer). Media could therefore resume on that address *by accident*, while MC's connectivity and routing still named the dead incarnation's id — the unsound state ADR-0039 §6 closes (a binding answered for a handler outside the set; the self-asserted-identity S1b class). The previous text of `mh-incident-response.md` already called the restart an outage for those meetings.

**Why accepted rather than held:** the binding gate is a task-mandated ADR-0039 §6 change (item 6), and the no-credit rule on a different incarnation is the Gate-1 OPEN-1 ruling (protocol + observability + security + Lead). The recovery path is the story's own MC-B2/MC-C sequence; ADR-0039's context records no production and backward compatibility as not a concern. Documented at `docs/observability/metrics/mc-service.md` (`mc_register_meeting_total`), `docs/runbooks/mc-deployment.md` (post-deploy checklist) and `docs/runbooks/mh-incident-response.md` (restart remedy, rewritten with operations' wording).
