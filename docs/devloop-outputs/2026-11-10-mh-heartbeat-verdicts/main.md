# Devloop Output: MH-2 per-meeting heartbeat, verdicts, release causes, all-or-none pinned

**Date**: 2026-11-10
**Task**: Story 4 task #15 (MH-2): MH per-meeting heartbeat to MC, verdict handling, EndMeeting cause, R-8 all-or-none tests, process-start epoch retirement
**Specialist**: media-handler
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~3h (approximate total time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c9222b24c8e3331e88afcd9235885b2a6bcdf220` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (spawned) |
| Implementing Specialist | `media-handler` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` (spawned) |
| Test | `paired-test` (spawned; paired) |
| Observability | `observability` (spawned) |
| Code Quality | `code-reviewer` (spawned) |
| DRY | `dry-reviewer` (spawned) |
| Operations | `operations` (spawned) |
| Semantic Guard | `semantic-guard` (spawned) |
| Paired Protocol | `paired-protocol` (spawned; GSA owner) |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
     - Tier (ADR-0037 §D2): set the Tier row at setup from the manifest tier; under `light`, Step 5 SKIPS the Gate-1 plan round and writes a `### Gate 1 — SKIPPED (tier=light; reason: …)` marker instead of pending rows. A resumed devloop reads the tier from THIS Tier row (not the manifest, not the confirmations table); no Tier row ⇒ full (fail-safe).
-->

---

## Task Overview

### Objective
MH half of ADR-0039 liveness/fencing: per-meeting heartbeat to the owning MC at the MC-declared interval, obey verdicts, honour EndMeeting cause, pin R-8 all-or-none apply, retire process_start_epoch_ms (R-36).

### Scope
- **Service(s)**: mh-service (primary); mc-service, gc-service, common (behaviour-neutral trigger vocabulary + epoch-reader removal); proto (GSA); env-tests
- **Schema**: {Database schema changes? Yes/No}
- **Cross-cutting**: {Does this affect multiple services? Yes/No}

### Debate Decision
NOT NEEDED - design governed by ADR-0039 and the 2026-10-28 MH replacement debate


---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mh-service/src/session/mod.rs` | Mine | — |
| `crates/mh-service/src/session/heartbeat.rs` (new) | Mine | — |
| `crates/mh-service/src/session/release.rs` (new) | Mine | — |
| `crates/mh-service/src/grpc/heartbeat_sender.rs` (new) | Mine | — |
| `crates/mh-service/src/grpc/mc_client.rs` | Mine | — |
| `crates/mh-service/src/grpc/gc_client.rs` | Mine | — |
| `crates/mh-service/src/grpc/mod.rs` | Mine | — |
| `crates/mh-service/src/grpc/mh_service.rs` | Mine | — |
| `crates/mh-service/src/routing/mod.rs` | Mine | — |
| `crates/mh-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mh-service/src/process.rs` (delete) | Mine | — |
| `crates/mh-service/src/lib.rs` | Mine | — |
| `crates/mh-service/src/main.rs` | Mine | — |
| `crates/mh-service/src/observability/metrics.rs` | Mine | — |
| `crates/mh-service/tests/**` | Mine | — |
| `crates/mh-test-utils/src/**` | Mine | — |
| `infra/grafana/dashboards/mh-media.json` | Mine | — |
| `docs/observability/metrics/mh-service.md` | Mine | — |
| `docs/runbooks/mh-deployment.md` | Mine | — |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Mine | — |
| `proto/dark_tower/internal/v1/internal.proto` | Not mine, Domain-judgment | protocol |
| `packages/proto-gen/scripts/verify-codegen.sh` | Not mine, Domain-judgment | protocol |
| `crates/proto-gen/tests/internal_roundtrip.rs` | Not mine, Domain-judgment | protocol |
| `crates/common/src/observability/labels.rs` | Not mine, Minor-judgment | observability |
| `docs/observability/label-taxonomy.md` | Not mine, Minor-judgment | observability |
| `crates/mc-service/src/grpc/mh_client.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/src/media_routing/confirm.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/src/media_routing/placement.rs` | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-service/src/media_routing/liveness.rs` (import) | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/src/media_routing/mod.rs` (re-export) | Not mine, Mechanical | meeting-controller |
| `crates/mc-test-utils/src/mock_mh.rs` | Not mine, Mechanical | meeting-controller |
| `crates/gc-service/src/observability/metrics.rs` | Not mine, Minor-judgment | global-controller |
| `crates/env-tests/**` (45 new; probe/injection interval const, set-only injection, 26/29/37/38/39 call sites) | Not mine, Minor-judgment | test |
| `docs/runbooks/mc-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/API_CONTRACTS.md` | Not mine, Minor-judgment | protocol |
| `docs/TODO.md` | Not mine, Mechanical | — |

---

## Planning

### Problem restated (mechanism, not instance)

The mechanism is: **MH reports, per held meeting, the state it has installed and obeys the owning MC's verdict about whether to keep holding it.** That mechanism widens three ways, and all three are in scope:
- The applied state must be reported from the moment the registration RECORD exists. Today a policy-invalid registration upserts nothing, because the whole request is validated before the upsert. The `RegisterMeetingRequest` header INTERIM paragraph gives this reorder to MH-2. So: record validation, then upsert, then policy validation and apply. A policy refusal becomes an `apply_refusal` and does not roll the record back.
- A release can come from three carriers: `EndMeeting(cause)`, `RELEASED(cause)`, and MH-3's owner-silence. They take ONE release path (`handle_end_meeting`) and ONE cause projection. The `trigger` vocabulary is shared fleet-wide, so it moves to `common`, and MC and GC point at it.
- `process_start_epoch_ms` goes in one unit with every reader: MH, MC, mc-test-utils, the proto-gen roundtrip, TS regen, docs, and the verify-codegen absence check.

### A. Registration reorder (`grpc/mh_service.rs`)
1. Record validation. Any failure rejects the whole request before the upsert, and the prior record stays in force. The checks are:
   - `validate_caller_ids`;
   - endpoint presence, length and scheme;
   - **`heartbeat_interval_ms == 0` → INVALID_ARGUMENT** (new).

   Counted on `mh_grpc_requests_total{method="register_meeting",status="error"}`, which is pre-boundary as today. There is no policy outcome. WARN with `reason="zero_heartbeat_interval"`. The WARN fires on EVERY 0-interval refusal (never once per streak), with a stable `reason` field. The runbook post-deploy check and the §Rollback signal name both pieces of evidence, plus one line on why there is no dedicated series (contract scope; the generic series carries every register error).
   *Scope note (Lead ruling):* operations asked for a dedicated `mh_register_meeting_record_rejections_total{reason}`. It was not added, because it is not in telemetry-contract.md and the task says "invent nothing beyond them". This is not a deferral.
2. `MeetingPolicy::from_request` runs as a pure function and its result is held.
3. Upsert the record. `MeetingRegistration` gains `heartbeat_interval: Duration`, and the lifecycle message carries `validation_refusal: Option<ApplyRefusal>`. The meeting cap stays an upsert refusal, as today.
4. On a policy reject: record the outcome and return INVALID_ARGUMENT as today, but the record IS upserted (`mc_id` rebound) and `apply_refusal` is set. Otherwise apply as today.
5. Replace the INTERIM paragraph in the proto (with protocol's wording): a policy reject is answered INVALID_ARGUMENT, but the record HAS been upserted, so an error status does not mean nothing was recorded; MC tells the two apart through the heartbeat. Delete the EndMeeting INTERIM clause, and grep for any remaining INTERIM. The catalog notes that a policy-refused registration holds a slot against `MH_MAX_REGISTERED_MEETINGS` until EndMeeting (operations P3). Split the existing "rejected registration must not upsert" test into a record arm (no upsert) and a policy arm (upserts, then beats with 0 plus INVALID).

### B. Schedule and beat build (`session/heartbeat.rs`, new; actor owns it)
- `HeartbeatSchedule`:
  - `BTreeMap<(tokio::time::Instant, u64 seq), MeetingId>` plus `HashMap<MeetingId, Entry{interval, due_key, fence, refusal}>`. The earliest due is O(log n), which matters at the cap.
  - The upsert inserts with first due = `now + U[0, interval)`. The RNG is a `SmallRng` injected by constructor: from entropy in prod (`rand::make_rng`), and `seed_from_u64` in tests.
  - Re-register: the latest interval applies, and the next due = `min(existing due, now + new interval)`.
  - Release removes the entry.
  - All arithmetic is `checked_add` with a saturating far-future fallback, so there is no panic at `u32::MAX` ms (ADR-0002).
- `run()` gains exactly ONE select arm: `sleep_until(earliest)`, guarded `if schedule non-empty && beat sink present`, so it is disabled when empty and never busy-loops. It calls `self.emit_due_beats(now)`, which builds at most `MAX_BEATS_PER_WAKE` (64) beats per wake. Any remaining beats stay due, so the arm is ready again and `select!`'s random fairness keeps the lifecycle, config and EndMeeting arms served.
- The beat is built in the actor:
  - `meeting_id`, the REGISTERED `mc_id`, `handler_id` (passed into the actor);
  - `SERVING`, with `drain_remaining_ms` 0;
  - `applied_generation = routing.load().generation_for(meeting)`;
  - `apply_refusal` from the entry;
  - `connection_ids` = this meeting's `active_connections` entry, flattened (per-meeting key, so nothing crosses meetings). It is added BEFORE `NotifyParticipantConnected` begins and removed BEFORE `NotifyParticipantDisconnected` is sent (`connection.rs`), so it is the begun-not-Disconnected superset. Pending (pre-registration) connections live in a different map and are excluded.
- **Fence**: each entry has `fence: u64` from a process-global monotonic counter. The counter bumps when an upsert CHANGES `(mc_id, endpoint)` and on every fresh insert after a release. The fence is never reused. Each beat carries `(meeting_id, fence, endpoint)`. A RELEASED verdict whose fence does not equal the current entry's fence is dropped: it came from a re-registration or a release, so it is counted `verdicts{outcome="released_stale"}` and nothing is done. The fence counter is a plain `u64` field owned by the schedule, not a static.
- Hand-off: `beat_tx.try_send`. When the queue is full, the beat is dropped and `dropped_queue_full` is counted. That logs WARN at the start of a streak and INFO when it ends. The actor never awaits.
- `refusal` updates:
  - an Applied outcome clears it;
  - `Failed(StreamCeilingExceeded)` sets STREAM_CEILING;
  - `Failed(EdgeCapExceeded)` sets EDGE_CAP;
  - the validation reject sets INVALID or TRANSPORT_INCONSISTENT;
  - transient outcomes, stale and gen-0 leave it unchanged, so they show as lag.

  **One** exhaustive projection fn, `apply_refusal_of(ApplyAttempt) -> RefusalEffect`, sits next to `PolicyRejection::outcome` in `routing/mod.rs` (D3). `ApplyAttempt` is either `Validation(PolicyRejection)` or `Actor(ApplyOutcome)`. There are no wildcards.
- `mh_media_meetings_without_clients`: recomputed and set on the existing 10 s rotation tick (registered meetings with no `active_connections` entry), and published as 0 at construction.

### C. Sender task (`grpc/heartbeat_sender.rs`, new)
- **Seam**: the `HeartbeatTransport` trait (a generic parameter `S: HeartbeatTransport` on the sender, not `dyn`) has `async fn send(&self, endpoint, MeetingHeartbeatRequest) -> Result<MeetingHeartbeatResponse, HeartbeatError>`.
  - Prod `TonicHeartbeatTransport`: a cache of `HashMap<String, Channel>`, endpoint-keyed and clone-out, shaped like GC's `mc_client.rs:get_channel` (D4). Channels are built via `Endpoint::from_shared(..).connect_timeout(..).connect_lazy()`, so the build never awaits.
  - Auth comes from McClient's `add_auth`, made a crate-local free fn `authorized_request(&TokenReceiver, T)` that both use (no 5th copy). The token is read per call and inserted only into the metadata. It is never cached in the channel or the endpoint, and is never logged. Errors are logged as a bounded kind plus the `tonic::Code` only, never `{:?}` of a Request, metadata or Status.
- **Cache bound**: evicted best-effort on an `Evict{endpoint}` command. The actor `try_send`s it when a release or an endpoint change leaves no registration referencing the endpoint. Hard backstop: cap = `MH_MAX_REGISTERED_MEETINGS` (live endpoints ≤ registered meetings), evicting the oldest insert when at the cap (no new crate). A missing or empty token fails closed: nothing is sent, and the beat counts as `error`. Evicting a live endpoint only costs a rebuild. Same scheme for the per-meeting failure-streak map (log transitions): `Forget{meeting}` plus a cap.
- **One outstanding beat per meeting**: `in_flight: HashSet<MeetingId>`. A beat for an in-flight meeting is skipped and counted `skipped_in_flight`. Otherwise the beat is spawned into a `JoinSet`, so one meeting's hang never delays another's. The sender loop `select!`s over the queue, `join_next` and the shutdown token.
- **Per-attempt deadline** = `interval * 3 / 4`, derived from the meeting's interval with no constant (D5). It is always below the interval (1 ms → 750 µs) and cannot overflow. There is NO retry.
- **`skipped_in_flight` is reachable by construction**: (i) a re-register that shortens the interval pulls the next due in while a beat is in flight; (ii) MH-3's DRAINING reset-to-now. It is tested via (i).
- **Outcomes** (exactly one per beat, every exit, counted before any early return):
  - `success`: a response was received, whatever the verdict;
  - `error`: any gRPC Status (incl. FAILED_PRECONDITION and UNIMPLEMENTED), transport error, invalid endpoint, or auth-header build failure;
  - `timeout`: the per-attempt deadline expired;
  - `skipped_in_flight`, `dropped_queue_full`.

  The latency histogram is observed for `success` and `error` only (completed RPCs). Buckets go up to 10 s (the max interval is N/3 = 10 s).
- **Verdicts** go to `mh_mc_heartbeat_verdicts_total{outcome=owned|released|released_stale|misroute|owner_mismatch|unspecified_verdict}` (no `unknown`):
  - FAILED_PRECONDITION → `owner_mismatch`, no action;
  - an UNSPECIFIED or unknown verdict number → `unspecified_verdict`, no action;
  - OWNED → nothing;
  - RELEASED → the per-beat task sends `SessionMessage::HeartbeatReleased{meeting_id, fence, cause}` back on the **lifecycle mailbox** via an `mpsc::WeakSender`. Weak, so the sender task cannot keep the actor alive and there is no shutdown cycle. The in-flight slot is held until the send completes.

  The ids MH acts on come from the beat, never the response (S6).
- **Shutdown**: the task ends when the beat channel closes (actor gone) or the shutdown token fires. Dropping the `JoinSet` aborts in-flight calls, so SIGTERM never waits on beats.
- Queue capacity is the constant `HEARTBEAT_QUEUE_CAPACITY = 256` (≥ 4× beats per wake). It is not a config key.

### D. Release path (one path, one projection)
- `handle_end_meeting(meeting, mc_id, release: ReleaseKind)`. `ReleaseKind` is projected by ONE fn, `release_kind(ReleaseCause) -> ReleaseKind`, an exhaustive match, no wildcard (D2). The kinds are:
  - `Removal(RemovalTrigger)` for HEARTBEAT_TIMEOUT, APPLY_LAG and PLANNED_TRANSFER;
  - `MeetingEnded`;
  - `Misroute` for UNSPECIFIED, which is legal on a verdict only. On a RELEASED verdict, an unknown cause number (where `try_from` fails) is also `Misroute`: it ends the meeting and is counted as a misroute, never dropped. On EndMeeting, an unknown number is INVALID_ARGUMENT, tested with cause 99.

  Both the EndMeeting path and the verdict path call it. The verdict path passes the REGISTERED `mc_id`, so the Ownership check is kept.
- **One home per event**:
  - `Removal` → `mh_media_handler_releases_total{trigger,clients,key_custody}`;
  - `MeetingEnded` and `Misroute` → `mh_media_meeting_teardowns_total{outcome="released"}`, and misroute is additionally counted on the verdict counter;
  - `clients=present` iff the meeting's `active_connections` is non-empty at release. Both read ONE actor predicate, `has_clients(meeting)`, which is the same set as `meetings_without_clients`, so a session that has begun its bind counts and a pending pre-registration session does not.
- Active connections are cancelled as today. In `connection.rs` that leads to `ConnectionEnd::MeetingReleased` → HANDLER_RELEASED via MH-CLOSE's `close.rs`, with no Disconnected.
- **Pending (awaiting-bind) connections**:
  - on `Removal`/`Misroute`: their close token is cancelled and they are removed from the pending map. `await_meeting_registration` gains a `close` arm → `RegistrationOutcome::Released` → `session.closing(ValidatedClose::MeetingReleased)` (HANDLER_RELEASED, already validated);
  - on `MeetingEnded`: left alone, as today.
- One INFO per release with `meeting_id`, `mc_id`, cause and clients.
- **EndMeeting**: id validation, then the `cause` check. `ReleaseCause::try_from` must succeed and must not be UNSPECIFIED; otherwise the request is refused INVALID_ARGUMENT with a generic message before any state change. That refusal is counted on the new `mh_media_meeting_teardowns_total{outcome="rejected_invalid_cause"}`, recorded at the gRPC boundary and documented as the one non-actor outcome, plus the grpc error status. Then the ownership check, then the release.
- Delete the STORY-4 NOTE in `handle_end_meeting`, and any "re-assert cadence" premise in comments I touch (`CONFIG_APPLY_CHANNEL_BUFFER` doc, the stale-path comments).

### E. Shared `trigger` vocabulary (D1, option (a), agreed by DRY and observability)
- Move MC's `placement::RemovalTrigger` (the same 3 variants) to `common::observability::labels::RemovalTrigger`, with `ALL`, `label()` and a `TRIGGER_LABEL` key.
- MC: `pub use` re-export at `crate::media_routing::RemovalTrigger`; `release_cause()` becomes an MC-side extension trait `ReleaseCauseExt` (common has no proto-gen dependency). Behaviour-neutral, about 10 LoC.
- GC: `ReplacementTrigger::label()` delegates its three shared values, and `invalid` stays local. Output is byte-identical.
- One `label-taxonomy.md` row names the common enum as the SSoT. MH-3's `owner_silence` wraps it and is not added to it.
- The inverse projection (`ReleaseCause` → trigger) exists in both GC and MH (common cannot host it). DRY files this as an extraction opportunity.

### F. process_start_epoch_ms retirement (one unit)
- Proto: delete field 4 and its comment block; add `reserved 4; reserved "process_start_epoch_ms";` with a "Was:" note (protocol's text).
- verify-codegen: `assert_message_field_absent ... RegisterMeetingResponse processStartEpochMs appliedGeneration`.
- Regenerate the TS.
- Delete `process.rs`, `lib.rs` mod, `main.rs` plumbing, the `MhMediaService::new` param, and test `process_start_epoch_is_identical_across_two_calls_in_one_process`.
- Delete the MC `mh_client.rs` log reads, `confirm.rs:345`, the `mock_mh.rs` field and builder, and the proto-gen roundtrip lines.
- Docs: `API_CONTRACTS.md:752` (check `:817`), `mh-service.md:318`, and `TODO.md` §Media Path Obligations (c) marked retired by MH-2 / ADR-0039 §1. Protocol edits its own INDEX.

### G. Telemetry (each with a panel in `mh-media.json` (description and runbook link), a catalog entry, zero-init from `ALL`, and an ADR-0032 test)
- `mh_mc_heartbeats_total{outcome}`: 5 values, no `key_custody` (matches `mh_gc_heartbeats_total`).
- `mh_mc_heartbeat_latency_seconds`: histogram, explicit bucket Matcher (low end copied from `mh_gc_heartbeat`, top 10 s, `ANCHOR (DRY)` naming MC's deadline range and `mc_media_handler_heartbeat_deadline_seconds`). Panels link only to the top-level `mh-incident-response` (no dangling anchors).
- `mh_mc_heartbeat_verdicts_total{outcome}`: 6 values.
- `mh_media_handler_releases_total{trigger,clients,key_custody="operator"}`: 3×2 series. `owner_silence` is not emitted (MH-3).
- `mh_media_meetings_without_clients{key_custody}`: gauge.
- `mh_media_meeting_teardowns_total{outcome="rejected_invalid_cause"}`: a new value. Its catalog entry and panel description say that a non-zero value means an MC below MC-B1 (no cause set) is in the fleet, and point to the mh-deployment.md §Rollback coupling paragraph.

No id labels; logs on transitions only (WARN when a meeting goes ok → failing, INFO on recovery, with ids and the consecutive count; the first occurrence per streak for owner_mismatch, unspecified and queue-full). No alert rules (that is OBS-ALERTS).

### H. Tests (Tier 1b, `start_paused`, no sleeps, every negative test with a positive arm)
Fake `HeartbeatTransport` with per-meeting scripted replies (Owned, Released(cause), Status, Pending-forever).
- Jitter and cadence (seeded): the first beat is in [0, interval), then exactly +interval. Re-register with a new interval → the new gaps. Release → no further beat, positive arm before.
- Independence: M1 hangs, M2 gets k beats over k intervals. M1 gets 1 call: no timeout at deadline−1 ms, a timeout at the deadline, deadline < interval, no retry.
- `skipped_in_flight` via the shortened-interval re-register.
- `dropped_queue_full`: a tiny queue that is not drained, and the actor still answers a lifecycle request.
- Wedged lifecycle: an in-crate test registers through actor methods on an UNSPAWNED actor and advances several intervals → zero beats. Spawn it → beats.
- Full config mailbox → the beat shows the OLD generation and no refusal; drain → it advances.
- Connection set: pending → absent, active → present, removed after a disconnect → absent. Structural: `remove_connection` precedes the Disconnected notify, so a notify that exhausts its retries is already out of the set. A real-tonic arm without paused time, against `mock_mc` failing disconnects, if wall time allows (~3 s); otherwise actor level only. Will settle with @paired-test.
- Verdicts in one test, one meeting per arm:
  - RELEASED(HEARTBEAT_TIMEOUT) with a client: unregistered, HANDLER_RELEASED close, zero Disconnected, releases{heartbeat_timeout,present}+1;
  - RELEASED(APPLY_LAG) with no clients → `none`;
  - RELEASED(UNSPECIFIED) → ended, misroute;
  - RELEASED(MEETING_ENDED) → teardowns{released};
  - OWNED, FAILED_PRECONDITION, UNSPECIFIED verdict, transport error → still held, proven by a subsequent beat;
  - a stale-fence RELEASED after a re-register to a new `mc_id` → held.
- All-or-none, parameterised INVALID / TRANSPORT_INCONSISTENT / STREAM_CEILING / EDGE_CAP, frames through the transport shim:
  - gen 4 has A→B;
  - gen 5 = add A→C plus a refusing change: C gets nothing, A→B flows, the beat reports 4 plus the kind;
  - gen 6 (adds A→C) → C receives, and the beat reports 6 with no refusal;
  - a first-ever refusal → 0 plus the kind.
- Interval 0 refused (no schedule) vs non-zero accepted. EndMeeting UNSPECIFIED → INVALID_ARGUMENT, counted, held; MEETING_ENDED ends it. A removal cause closes pending connections with HANDLER_RELEASED; MEETING_ENDED leaves them.
- One real-tonic test (not paused) that extends the existing `tests/common/mock_mc.rs` `meeting_heartbeat` stub (no second mock): channel reuse (one TCP accept over ≥2 beats) and request fields.
- Zero-init and ADR-0032 drivability for every value.
- Env-test `45_mh_meeting_heartbeat.rs`:
  - per-pod baseline, join a meeting, each holder's `success` rises;
  - per-entity "still held" after >2 intervals via a wrong-owner EndMeeting → FAILED_PRECONDITION;
  - `releases_total` flat as secondary evidence only;
  - waits derived from MC's `mc_media_handler_heartbeat_deadline_seconds` gauge (via `gauge_by_pods`), never from config files.

### H2. Zero-interval callers outside mh-service (paired-test blocker)
- Add ONE env-test const, `ENV_TEST_HEARTBEAT_INTERVAL_MS = 10_000` (the deployed N/3), in `fixtures/mh_grpc.rs`. `probe_registration` and `injection.rs` `register_full`/`register_refused` use it. mh-test-utils `media_policy` gets its own realistic default, because it is a different crate (no env-tests dependency). The mh-service and mc-service test fixtures are fixed as Layers 4-5 find them.
- Update the `NEVER_DIALLED_MC_ENDPOINT` doc and the comment in 29: MH now heartbeats it once per interval and gets `error` (no action, no release).
- **Injection hazard**: `Injection` registers with the REAL MC's endpoint and `mc_id` on every injectable handler. A handler NOT in MC's set will now get RELEASED(UNSPECIFIED) within an interval → misroute → the injected meeting ends mid-test. The consumers are 26:1309/:1596, 37:385, 38:279 and 39:332 (all via `injectable_handlers` = every live MH pod). The preferred fix is to RESTRICT `injectable_handlers` to the participant's handler-set pods, using the SAME refs/URLs→pod resolver as env-test 45 (one home). The injection.rs doc ("registers a NEW meeting id on EVERY pod") is updated either way. Audit rule:
  - consumers that inject only on pods in MC's set (carrying pods) are fine;
  - for the rest, restrict to set members (resolved from the participant's handler set), or document why a misroute release does not affect the assertion.
- Paired-test additions:
  - every test asserting on sender-side metrics is `current_thread`;
  - the verdict test drives PLANNED_TRANSFER too, and the stale-fence arm has a current-fence positive twin;
  - the env-test's wrong-owner EndMeeting carries MEETING_ENDED and asserts exactly FAILED_PRECONDITION, with a real-owner positive arm;
  - env-test 45 resolves holders from the participant's handler set (refs/URLs → pods), waits for `up==1` before taking baselines, and applies the ceiling only on failure;
  - `clients=present` gets an arm with a session that has been added but is still awaiting its `sender_id` (mock MC holding Connected);
  - 5(c): `McClient` retry backoff injectable via a test constructor (production default unchanged), with event-synchronised mock calls, a positive arm for the second session, and an arm proving the connection is absent while the notify is still retrying.

### I. Ops docs
- `mh-deployment.md` §Rollback: an MH-2 coupling paragraph covering:
  - MC below MC-B1 while MH-2 is live → every registration is refused (0 interval) and EndMeeting is refused (UNSPECIFIED), so the ratchet applies. Roll MH back first;
  - MH alone back is safe until MC-B2 (to be verified in MC-B1 code). This is THE lever for a bad MH-2 deploy (a jump in releases or misroutes), because there is no kill switch;
  - the signals.
- `mh-deployment.md` §Post-Deploy checklist: heartbeat success ratio, releases flat, both refusals at 0, and the absent-series line.
- `mc-deployment.md` :211-216 and :829-832: correct the "owner-silence" consequence (that is MH-3).
- Safe-revert unit: the whole commit.


### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Protocol (paired, GSA owner) | confirmed |

Classification-sanity guard: `STATUS=OK`. Lead rulings: MC/GC trigger edits Minor-judgment review-only; no `mh_register_meeting_record_rejections_total` (not in telemetry contract); `rejected_invalid_cause` value on existing `mh_media_meeting_teardowns_total` allowed (task explicitly requires the refusal to be counted). Lead note: teammate prompts referenced INDEX.md / review-protocol.md by path (read on spawn) instead of inlining contents.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

Implemented per §Planning A–I, with all Gate-1 amendments.

### Heartbeat (MH half of liveness)
| Item | Before | After |
|------|--------|-------|
| Schedule | none | `session/heartbeat.rs::HeartbeatSchedule`, which tracks per meeting: due times (BTreeMap), seeded jitter in [0, I), the latest interval, the fence and `apply_refusal`. It starts at the record upsert and is removed on release. |
| Actor | no heartbeat | One `sleep_until` arm, disabled when the schedule is empty, calls `emit_due_beats`: at most 64 beats per wake, handed off by `try_send`. A full queue counts `dropped_queue_full`, logged once per streak. |
| Sender | — | `grpc/heartbeat_sender.rs::HeartbeatSender<S: HeartbeatTransport>`:<br>• one in-flight beat per meeting, each on its own JoinSet task;<br>• per-attempt deadline of 3/4 of the interval, with no retry;<br>• RELEASED goes back to the actor over a weak lifecycle-mailbox sender;<br>• logs transitions only. |
| Transport | per-call channel | `TonicHeartbeatTransport` caches one lazy channel per endpoint (capped, oldest evicted, plus `Evict` commands). The token is added per call via the shared `grpc::authorized_request`, which fails closed on an empty token. |
| Verdicts | — | OWNED: nothing. RELEASED: fenced, then `handle_end_meeting` with the registered `mc_id`. FAILED_PRECONDITION, UNSPECIFIED, unknown values and transport errors: counted only. A RELEASED with no cause is a misroute and still ends the meeting. |

### Registration / EndMeeting
- Validation now runs record first, then upsert, then policy. A 0 interval is a record reject. A policy reject now upserts the record and sets `apply_refusal`. The proto INTERIM paragraphs are replaced.
- `EndMeeting.cause` is projected once by `ReleaseKind::of`. UNSPECIFIED and unknown causes return INVALID_ARGUMENT and count as `rejected_invalid_cause`. A removal cause closes pending connections with HANDLER_RELEASED; `MEETING_ENDED` leaves them.
- `apply_refusal` is projected once, by `routing::apply_refusal_of`.

### Retirement / shared vocabulary
- `process_start_epoch_ms` is deleted (tag and name reserved). It is removed from `process.rs`, its plumbing, MC log reads, `mock_mh`, `confirm.rs`, the roundtrip tests and docs, and a verify-codegen absence line is added.
- `RemovalTrigger` moved to `common::observability::labels`. MC re-exports it and gets the `ReleaseCauseExt` trait; GC's labels delegate to it.

### Telemetry
`mh_mc_heartbeats_total`, `mh_mc_heartbeat_latency_seconds`, `mh_mc_heartbeat_verdicts_total`, `mh_media_handler_releases_total` and `mh_media_meetings_without_clients` are new. `mh_media_meeting_teardowns_total` gains the `rejected_invalid_cause` value. Each comes with a catalog entry, a panel in `mh-media.json`, zero-init and tests.

### Env-tests
- One `ENV_TEST_HEARTBEAT_INTERVAL_MS` constant.
- `injectable_handlers` is restricted to the owner's handler-set pods, via the new `mh_address::pods_serving`.
- New `45_mh_meeting_heartbeat.rs`.

## Devloop Verification Steps

Fast check (Lead, post-implementation): `DEVLOOP_FMT_APPLY=1 ./scripts/layer-fast.sh` → `TOTAL_RESULT=OK` (L1–L6).

Gate 2 (Lead, post-Gate-3, final tree): `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → `TOTAL_RESULT=OK`, no FMT_APPLIED rewrites.

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 Compile | OK | 7 |
| 2 Format | OK | 4 |
| 3 Guards | OK | 118 |
| 4 Test | OK | 136 |
| 5 Lint | OK | 1 |
| 6 Audit | OK (cargo audit pre-existing advisories; pnpm SKIPPED-NO-DIFF no-dep-changes; buf breaking passed) | 6 |
| 7 Env-tests | OK (`env-tests-passed`, `browser-e2e-passed`; incl. new 45_mh_meeting_heartbeat and re-scoped 26/37/38/39) | 1694 |

Attempts: layers 1–6 1/3, Layer 7 1/2.

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

Per-reviewer verdicts, counts and finding summaries are in the Gate 3 table below. Semantic Guard native verdict: SAFE after fix (mapped RESOLVED-FIXED). DRY true-duplication findings: F1 (env-test heartbeat interval hand-copy) — fixed. DRY extraction opportunities appended to `docs/TODO.md` §Cross-Service Duplication (DRY): per-endpoint channel cache; ReleaseCause→trigger projection; note on existing `add_auth` entry.

### Gate 3 — Reviewer Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | S-1 authorized_request empty-token fail-closed tests |
| Test (paired) | RESOLVED-FIXED | 10 | 10 | 0 | T1–T10 (zero-init, cache evict, bounded loops, env-test interval SSoT) |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | panel unit, relay-failure masking, latency only on-wire, catalog wording, key_custody |
| Code Quality | RESOLVED-FIXED | 6 | 6 | 0 | F1 apply_refusal_of fail-closed; F2–F6 cleanups |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | env-test interval derived once; 2 extraction opportunities filed in docs/TODO.md |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | deploy order, silent sender death, panel runbook links |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | error-context: grpc_code kept in transition logs (native SAFE after fix) |
| Protocol (paired, GSA owner) | CLEAR | 0 | 0 | 0 | GSA ownership satisfied (protocol + security in plan and review) |

---

## Accepted Deferrals

- (none surfaced in this devloop)

DRY extraction opportunities (not deferrals) are filed in `docs/TODO.md` §Cross-Service Duplication (DRY): per-endpoint tonic Channel cache; ReleaseCause→RemovalTrigger inverse projection (GC + MH).

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `c9222b24c8e3331e88afcd9235885b2a6bcdf220`
2. Review all changes: `git diff c9222b24..HEAD`
3. Soft reset (preserves changes): `git reset --soft c9222b24`
4. Hard reset (clean revert): `git reset --hard c9222b24`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit. The proto field retirement must move with its MH/MC readers and verify-codegen check; the MH-2 MH binary requires MC-B1 live (roll MC-B1 first, roll MH back first — see `docs/runbooks/mh-deployment.md` §Rollback MH-2 coupling). (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Handoff to MH-3: the heartbeat sender stops at SIGTERM
**Problem**: `main.rs` cancels `shutdown_token` at SIGTERM, and the heartbeat sender runs on a child of it, so beats stop immediately. That is correct for MH-2, where MH still sheds at shutdown. MH-3's DRAINING beats, however, need the sender to outlive SIGTERM until the drain ends.
**Resolution**: MH-3 must give the sender task its own token, cancelled at the drain end (operations, Gate 2).

---

## Lessons Learned

1. Tightening a wire precondition (interval 0 refused) breaks test fixtures that only Layer 7 exercises — the paired test reviewer caught it at Gate 1, before review.
2. Env-test fixtures that talk to the real MC must derive timing from MC's published config, not a hand copy (DRY/T8).
3. A sender task whose handle is discarded can die silently; the actor must log loudly on a closed hand-off queue (ops F2).


---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```
