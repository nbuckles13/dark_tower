# Meeting Controller Metrics Catalog

**Service**: Meeting Controller (mc-service)
**Implementation**: `crates/mc-service/src/observability/metrics.rs`
**Job Label**: `mc-service-local` (local development), `mc-service` (production)

All MC service metrics follow ADR-0011 naming conventions with the `mc_` prefix.

---

## Connection & Meeting Metrics

### `mc_connections_active`
- **Type**: Gauge
- **Description**: Number of active WebTransport connections
- **Labels**: None
- **Usage**: Monitor connection load and capacity utilization
- **Dashboard**: MC Overview - Active Connections gauge

### `mc_meetings_active`
- **Type**: Gauge
- **Description**: Number of active meetings hosted by this MC instance
- **Labels**: None
- **Usage**: Monitor meeting load and capacity utilization
- **Dashboard**: MC Overview - Active Meetings gauge

---

## Actor System Metrics

### `mc_actor_mailbox_depth`
- **Type**: Gauge
- **Description**: Mailbox depth (pending messages) for each actor type
- **Labels**:
  - `actor_type`: Actor type (`controller`, `meeting`, `connection`)
- **Cardinality**: Low (3 actor types)
- **Usage**: Monitor backpressure. High values indicate slow processing. Warning at 100, critical at 500.
- **Dashboard**: MC Overview - Actor Mailbox Depth by Type

### `mc_actor_panics_total`
- **Expected-empty**: yes — every actor panic is a bug, so a healthy service reads zero here
- **Type**: Counter
- **Description**: Total actor panic events
- **Labels**:
  - `actor_type`: Actor type (`controller`, `meeting`, `connection`)
- **Cardinality**: Low (3 actor types)
- **Alert**: ANY non-zero value indicates a bug and should trigger investigation
- **Usage**: Detect actor crashes, identify problematic actor types
- **Dashboard**: MC Overview - Actor Panics (Total), Actor Panics by Type

---

## Message Processing Metrics

### `mc_messages_dropped_total`
- **Expected-empty**: yes — a healthy actor system drops no messages, so this reads zero when healthy
- **Type**: Counter
- **Description**: Messages dropped due to backpressure
- **Labels**:
  - `actor_type`: Actor type (`controller`, `meeting`, `connection`)
- **Cardinality**: Low (3 actor types)
- **Usage**: Detect overload conditions. Non-zero values indicate the system is overloaded.
- **Dashboard**: MC Overview - Messages Dropped by Actor Type, Message Drop Rate (%)

---

## GC Heartbeat Metrics

### `mc_gc_heartbeats_total`
- **Type**: Counter
- **Description**: Total GC heartbeat attempts
- **Labels**:
  - `status`: Heartbeat outcome (`success`, `error`)
  - `heartbeat_type`: Heartbeat type (`fast`, `comprehensive`)
- **Cardinality**: Low (2 statuses x 2 types = 4 series)
- **Usage**: Monitor GC registration health, detect connectivity issues

### `mc_gc_notify_meeting_ended_total`
- **Type**: Counter
- **Description**: `NotifyMeetingEnded` calls to GC — MC telling GC a meeting ENDED (emptied or closed) after its handlers were released (ADR-0010 §3)
- **Labels**:
  - `status`: `success`, `error`
- **Cardinality**: 2; zero-initialised
- **A GC-coordination metric, like its `mc_gc_heartbeats_total` sibling — no `key_custody`.** Coarse `status`, because a fire-and-forget notification's remedies do not differ by failure mode.
- **Send policy: AT MOST ONCE for anything GC may have processed.** Retried (with backoff, up to `MAX_NOTIFY_ATTEMPTS`, ~31 s) only when the request provably never left MC (the call failed in its connect phase, e.g. GC restarting); never after it may have been sent, because a duplicate landing after the meeting's NEXT incarnation was assigned would end that live assignment. So `error` is final for that meeting.
- **What a sustained `error` costs, user-visibly**: GC keeps reusing the ended meeting's assignment, and every join to that meeting id gets `meeting_not_found` from MC until MC restarts. There is no other detector.
- **Alert**: `MCNotifyMeetingEndedFailing`
- **Recorded in**: `grpc/gc_client.rs::notify_meeting_ended` via `observability/metrics.rs::record_gc_notify_meeting_ended`
- **Dashboard**: MC Overview - GC Meeting-Ended Notifications (Fencing & Heartbeat Latency row)

### `mc_gc_meeting_ended_notifications_dropped_total`
- **Expected-empty**: yes
- **Type**: Counter
- **Description**: A meeting-ended notification DROPPED before it was attempted — the notify queue was full, or it was produced after the queue closed at shutdown, or it was still queued when the shutdown flush budget expired
- **Labels**: none
- **Cardinality**: 1; zero-initialised
- **Its own series, not a `status` value on `mc_gc_notify_meeting_ended_total`**: a pre-attempt loss ("MC is losing events") and an RPC failure ("GC is down") are opposite investigations.
- **Any non-zero is a stale GC assignment with no other detector** — joins to that meeting id fail until MC restarts.
- **Alert**: `MCMeetingEndedNotificationsDropped` (warning, `> 0`)
- **Two arms, and only one of them is reliably scraped.** The queue-full arm is ordinary runtime and is a dependable detector. The shutdown arms increment within seconds of process exit, so by this diff's own argument (a counter incremented that close to exit is never scraped) the counter there is BEST-EFFORT and the ERROR log line is the load-bearing evidence. Do not read a flat counter after a restart as proof nothing was dropped at shutdown; read the pod's last log lines.
- **A mid-send cut-off at the shutdown deadline is deliberately NOT counted here**: whether GC committed it is unknown, and `NotifyMeetingEnded` is at-most-once, so it is never retried and is reported on its own ERROR line instead. This counter is only the provably-never-sent remainder.
- **Recorded in**: `grpc/gc_client.rs::MeetingEndedQueue::meeting_ended` (queue full or closed) and `grpc/gc_client.rs::drain_meeting_ended` (the remainder past the shutdown flush budget), both via `observability/metrics.rs::record_gc_meeting_ended_notification_dropped`
- **Dashboard**: MC Overview - GC Meeting-Ended Notifications Dropped (Fencing & Heartbeat Latency row)

### `mc_gc_heartbeat_latency_seconds`
- **Type**: Histogram
- **Description**: GC heartbeat round-trip latency
- **Labels**:
  - `heartbeat_type`: Heartbeat type (`fast`, `comprehensive`)
- **Buckets**: [0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000]
- **SLO Target**: p99 < 100ms
- **Cardinality**: Low (2 types)
- **Usage**: Monitor heartbeat latency, detect GC connectivity degradation

---

## Redis Metrics

### `mc_redis_latency_seconds`
- **Type**: Histogram
- **Description**: Redis operation latency
- **Labels**:
  - `operation`: Redis command (`get`, `set`, `del`, `incr`, `hset`, `hget`, `eval`, `zadd`, `zrange`)
- **Buckets**: [0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000]
- **SLO Target**: p99 < 10ms
- **Cardinality**: Low (~10 operations)
- **Usage**: Monitor Redis dependency health, identify slow operations

---

## Fencing Metrics

### `mc_fenced_out_total`
- **Expected-empty**: yes — fencing is split-brain recovery; a healthy single-writer reads zero
- **Type**: Counter
- **Description**: Fenced-out events (split-brain recovery)
- **Labels**:
  - `reason`: Fencing reason (`stale_generation`, `concurrent_write`)
- **Cardinality**: Low (2-3 reasons)
- **Usage**: Detect split-brain scenarios. Should be rare in normal operation. Investigate if rate > 0.1/min.

---

## Join Flow Metrics (R-13)

### `mc_webtransport_connections_total`
- **Type**: Counter
- **Description**: Total WebTransport connection attempts by outcome
- **Labels**:
  - `status`: Connection outcome (`accepted`, `rejected`, `error`)
- **Cardinality**: Low (3 status values)
- **Usage**: Monitor connection acceptance rate, capacity rejections, and connection errors
- **Recorded in**: `server.rs` accept loop
- **Scope of `error`**: Connection ESTABLISHMENT errors ONLY — session/stream accept,
  first-message decode, JWT/join failures (i.e. the connection handler returns an
  `McError` before or during join). A participant departing MID-SESSION (clean tab-close
  or abrupt loss) is NOT an `error`; those are observed on
  `mc_participant_disconnects_total` / `mc_participant_leaves_total` below. (Prior to
  task #64, post-join transport read/write failures also bumped `status="error"`; that was
  reclassified as a normal disconnect.)
- **Alert**: `MCHighWebTransportRejections` (warning, rejection rate >10% for 5m — keys off
  `status="rejected"`, unaffected by the `error`-scope change)
- **Dashboard**: MC Overview - WebTransport Connections by Status (Join Flow row)

### `mc_jwt_validations_total`
- **Type**: Counter
- **Description**: Total JWT validation attempts by result, token type, and failure reason
- **Labels**:
  - `result`: Validation outcome (`success`, `failure`)
  - `token_type`: Token type (`meeting`, `guest`, `service`)
  - `failure_reason`: Reason for failure (`none`, `signature_invalid`, `expired`, `missing_token`, `scope_mismatch`, `malformed`)
- **Cardinality**: Low (bounded, 2 x 3 x 6 = 36 max, but most combos are sparse in practice)
- **Usage**: Monitor authentication health, detect token validation failures, diagnose failure causes
- **Recorded in**: `connection.rs` after JWT validation, `grpc/auth_interceptor.rs` for service tokens
- **Alert**: `MCHighJwtValidationFailures` (warning, failure rate >10% for 5m)
- **Dashboard**: MC Overview - JWT Validations by Result & Type (Join Flow row)

### `mc_session_joins_total`
- **Type**: Counter
- **Description**: Total session join attempts by outcome
- **Labels**:
  - `status`: Join outcome (`success`, `failure`)
- **Cardinality**: Low (2 status values)
- **Usage**: Monitor join success rate, overall join volume
- **Alert**: `MCHighJoinFailureRate` (warning, failure rate >5% for 5m)
- **Dashboard**: MC Overview - Session Joins by Status (Join Flow row)

### `mc_session_join_duration_seconds`
- **Type**: Histogram
- **Description**: Duration from WebTransport session accept to JoinResponse sent (or error)
- **Labels**:
  - `status`: Join outcome (`success`, `failure`)
- **Buckets**: [0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.000, 5.000]
- **Cardinality**: Low (2 status values)
- **Usage**: Monitor join latency, identify slow joins. Extended to 5s because join includes actor processing.
- **Mixes two populations since story 2 task 12.** A join whose meeting id is still being torn down on its media handlers can be PARKED behind the controller's teardown fence and served when the teardown completes: up to `teardown_fence_hold_max_seconds` later (MC's startup line), far past the top bucket, so those observations land in `+Inf`. A p99 excursion here is therefore either "MC is slow" or "one meeting's teardown wedged and its rejoins were parked". Separate them with `mc_media_push_quiesce_total{outcome="timed_out"}`, `mc_media_teardown_fence_backstop_total` and `mc_session_join_failures_total{error_type="teardown_in_progress"}` (the parked-queue overflow), all of which move only in the second case. The buckets are deliberately unchanged: this histogram backs the MC session-join SLO (`docs/observability/slos.md`), and re-bucketing would break burn-rate comparability. The parked population is bounded and rare (a rejoin during teardown normally reaches GC's live row and fails `meeting_not_found` instead), so it cannot move a 30-day p99 objective.
- **Recorded in**: `connection.rs` measuring full join flow
- **Alert**: `MCHighJoinLatency` (info, p95 >2s for 5m, success only)
- **Dashboard**: MC Overview - Session Join Latency (P50/P95/P99) (Join Flow row)

### `mc_session_join_failures_total`
- **Expected-empty**: yes — every series is a join failure, so a healthy join path reads zero
- **Type**: Counter
- **Description**: Total session join failures by error type
- **Labels**:
  - `error_type`: Bounded by `McError` enum variants (e.g., `jwt_validation`, `internal`, `meeting_not_found`, `mc_capacity_exceeded`, `meeting_capacity_exceeded`, `identity_key_invalid`)
- **Cardinality**: Low (~20 error variants, bounded by `McError` enum)
- **Label-name note**: the label is `error_type`, **not** `reason`. `reason` is reserved by
  `docs/observability/label-taxonomy.md` for per-frame media-path drop reasons; this classifies a
  *service operation* failure. Do not "normalise" one into the other.
- **ADR-0036 media-path values** (story task 10):
  - `identity_key_invalid` — the joiner presented an `identity_public_key` whose length was
    **neither 0 nor exactly 32 bytes**. Covers short, long and oversized as ONE value deliberately:
    the client-facing message is byte-identical across all of them, and a per-cause label would be a
    server-side oracle for a distinction the wire deliberately hides. **Absent is NOT here** — length
    0 is admitted (the contract's NO KEY PUBLISHED state) and lands on
    `mc_join_identity_key_presence_total{presence="absent"}` below. So this value means a client sent
    a wrongly-encoded key, never that a client has not implemented the field.
  - `teardown_in_progress` (story 2 task 12) — the join named a meeting whose PREVIOUS incarnation
    MC is still releasing on its media handlers (the last participant left moments ago and MC's
    teardown has not finished). **The OPPOSITE investigation from `conflict`**: `conflict` rising
    means clients double-joining (a client defect, no server action); this rising means teardowns
    are slow or wedging and rejoins are being turned away (an MC or MH fault). Normally a window of
    milliseconds; the worst case is MC's `teardown_fence_hold_max_seconds` startup-line field.
    Rising together with `mc_media_push_quiesce_total{outcome="timed_out"}` is the signature of a
    wedged teardown (see the EndMeeting scenario in `docs/runbooks/mc-incident-response.md`). The
    WIRE code is the same `CONFLICT` as `conflict` — only this operator-facing label splits them.
  - **`sender_id_space_exhausted` is RETIRED (story 2 task 9, R-16) and is never emitted.** It meant
    MC refused a joiner because the meeting's 65535 `sender_id`s were used up. MC now performs an
    immediate KEK-epoch reset instead — a new KEK plus a fresh namespace that excludes every id
    bound at that moment — and **the join succeeds**. The event is counted on
    `mc_meeting_kek_generated_total{trigger="sender_space_exhausted"}`, not here, and the value is no
    longer zero-initialised (a permanently-zero series would read as "checked, clean" for a
    condition that cannot occur).
- **Usage**: Diagnose join failure root causes, alert on specific failure patterns
- **Recorded in**: `connection.rs` on join failure only
- **Alert**: Used indirectly via `MCHighJoinFailureRate` (this metric provides error type breakdown for diagnosis)
- **Dashboard**: MC Overview - Session Join Failures by Type (Join Flow row)

---

### `mc_meeting_assignments_total`
- **Type**: Counter
- **Description**: Outcome of every `AssignMeetingWithMh` RPC — MC's only meeting-admission entry
  point from GC.
- **Labels**:
  - `status`: `success` | `rejected` — **`success`, not `accepted`**, matching GC (see below)
  - `rejection_reason`: bounded by the `RejectionReason` proto enum — `none` (on `success`),
    `at_capacity`, `draining`, `unhealthy`, `invalid_request`, `unspecified`
- **Cardinality**: Low (2 × 6, and most combinations unreachable)
- **Mirrors GC deliberately**: `gc_mc_assignments_total{status, rejection_reason}` is the other end
  of this same RPC and uses the same label names and value spellings, per `label-taxonomy.md`
  §Shared Label Names. Read them side by side to turn "GC says MC rejected 5% of assignments" into
  "which reason, on which MC instance" without a log dig. **The success arm is `success`, GC's
  spelling — deliberately NOT `accepted`**, which is what `mc_webtransport_connections_total` uses:
  that metric measures connection admission, a different concept with no cross-service counterpart,
  whereas this one measures the same RPC GC already measures, and GC is the incumbent emitter. A
  responder querying `status="success"` against both series must not get a silently empty one on the
  MC side, and no guard could catch that because both spellings are canonical in the taxonomy.
  One deliberate difference remains: MC labels `invalid_request` as `status="rejected"`, GC as
  `status="error"` (from GC's side it is a fault in GC's own request). The `rejection_reason` values
  match, which is what makes the join work.
- **Usage**: Attribute `GCMCAssignmentFailures` pages to an MC-side cause. Before this metric the
  RPC had no instrumentation at all, so a responder paged by that alert opened the MC dashboard and
  found nothing to look at.
- **Known limitation**: `unhealthy` conflates a Redis MH-assignment store failure with a
  `create_meeting` failure (which includes meeting-KEK generation). Only the MC log record
  distinguishes them — see `mc-incident-response.md` Scenario 6 root cause 6. Separating them means a
  third `RejectionReason` enum value, i.e. a proto change, not a second counter.
- **Recorded in**: `grpc/mc_service.rs`, at all four exit points of `assign_meeting_with_mh`
- **Alert**: None MC-side; the fleet condition is covered by GC's `GCMCAssignmentFailures`
- **Dashboard**: MC Overview - Meeting Assignment Outcomes (Join Flow row)

---

### `mc_join_identity_key_presence_total`
- **Type**: Counter
- **Description**: Whether each joiner reaching the join call published an Ed25519 identity signing key.
- **Labels**:
  - `presence`: `present` (exactly 32 bytes, published on the roster) | `absent` (empty — the
    contract's NO KEY PUBLISHED state, which MC admits)
- **Cardinality**: 2
- **Usage**: The **absent ratio**. MC admits participants that publish no identity key (ADR-0036 §4,
  `signaling.proto`), so "every client omits the key and nobody notices" must not be the silent
  steady state. `absent / (absent + present)` answers "are clients publishing keys?" directly.
- **Why both arms rather than an absent-only counter**: an absence-only series has no denominator, so
  a raw count of absent joins is unreadable without a separate join count to divide by — the same
  defect that disqualified `mc_meeting_kek_generated_total` as a missing-key signal. Follows the
  `mc_join_display_name_resolved_total{outcome}` precedent.
- **Does NOT count malformed keys.** A length other than 0 or 32 is refused at the trust boundary and
  lands on `mc_session_join_failures_total{error_type="identity_key_invalid"}`.
- **Denominator is join ATTEMPTS, not admissions** — recorded at the parse, before the join call, so
  a join that then fails on capacity, `sender_id` exhaustion, Conflict or Draining is still counted.
  **Chosen, not inherited**: the question is about the client population, so conditioning on
  server-side admission would answer a different question and would blank the series during exactly
  the incident an operator would consult it in. **Therefore `sum()` of this counter does NOT equal
  `mc_session_joins_total{status="success"}`** — do not treat the gap as a bug.
- **What it does not tell you**: whether anything downstream fails closed on a keyless participant.
  That obligation is the consumer's (drop frames, never "skip verification"), is owned by client, and
  is tracked in `docs/TODO.md`. A healthy-looking `absent` series is not evidence the consumer
  handles it.
- **Carries no participant and no meeting dimension** (ADR-0036 §11), and no key material.
- **Recorded in**: `webtransport/connection.rs`, after authentication and identity-key validation, before the join call — see the denominator note above
- **Dashboard**: MC Overview - Identity Key Presence (Join Flow row)

---

## Media-path label conventions (ADR-0036 §11)

**This catalog is the only home for which labels a media-path metric carries.** Any
change routes through here — never peer-to-peer between an emitter and a dashboard,
and never by editing a panel description to match a label somebody added. Nothing in
the tree compares a label key or a label value to anything: `dt-guard`'s
`application-metrics` and `dashboard-panels` validate metric **names** against code,
catalogs, dashboards and alerts, and `alert_rules.rs` pins an alert `expr`
byte-for-byte against its `alerts.md` entry, but **no guard reads a label roster**. A
roster written anywhere else decays silently and CI stays green.

Note the division of labour with `docs/observability/label-taxonomy.md`, because the
two are easy to conflate: a **shared label's** name, bounded value set, and the reason
it is bounded live in the taxonomy — it is the *first* home for a label that more than
one service emits. **Which labels a given metric carries** lives here. Neither is a
copy of the other.

### Declared, not yet carried: `media_kind` and `content_kind`

Media-path counters gain two dimensions when video and content share land (**story 3**,
not this story):

| Label | State |
|---|---|
| `media_kind` | **Declared, not carried.** No media-path metric emits it today. Values in `label-taxonomy.md`. |
| `content_kind` | **Declared, not carried.** No media-path metric emits it today. Values in `label-taxonomy.md`. |

Both are registered in `docs/observability/label-taxonomy.md` as shared labels, which
is where their value sets are authoritative. They are recorded here as *declared* so
that story 3 adds a dimension to an existing scheme rather than inventing one, and so
that a reader who greps for them today finds "not carried yet" instead of silence.
**Do not write a dashboard query or an alert that selects on either** — the selector
would match nothing and yield an empty series rather than an error.

### The transmit/receive convention

`direction` (`ingress` | `egress`) is carried **only where both directions exist on one
counter**. The convention itself — that it is pipeline-relative and never
participant-relative, why the `uplink`/`downlink` reading is barred, why it is admitted
on relay metrics at all, and the explicit statement that the acceptance **does not
generalise to the client's drop counter** — is stated once in
`docs/observability/label-taxonomy.md` §Permitted partner: `direction`. It is not
restated here, because a partial copy that dropped either of those last two clauses
would read as complete.

**Receive-only counters carry no `direction` label at all.** A direction label with one
possible value is a label that can only ever be wrong: it invites a selector that will
one day match nothing, and it implies a second direction that does not exist.

### What that means for MC

**MC carries no `direction` label on any media metric, and is not expected to grow
one.** This is not an omission. MC's transmit and receive paths are *separate metrics*,
each inherently single-direction — `mc_media_send_directives_total` versus
`mc_media_receive_capability_declarations_total` — so there is no MC counter on which
both directions exist. Per the convention above, the label is therefore absent by
construction.

Every MC media-path metric carries `key_custody=operator` (ADR-0036 §4) and **no meeting,
participant or stream identifier of any kind**, hashed or otherwise.


---

## Media Key Custody Metrics (ADR-0036 §4, §11)

Every series here carries `key_custody=operator` and NO meeting, participant, sender or generation
label (ADR-0036 §11). Nothing here is, or may be described as, a forward-secrecy, end-to-end or
zero-trust measure: MC generates and holds every KEK it issues.

### `mc_meeting_kek_generated_total`
- **Type**: Counter
- **Description**: Meeting key-encryption keys generated, by what caused them
- **Labels**:
  - `trigger`: `meeting_created` | `participant_left` | `sender_space_exhausted` (`media_admission::RotationTrigger::ALL`)
  - `key_custody`: single permitted value `operator`. Adding a second requires an ADR-0036 §4
    amendment — this is a *constraint*, not a snapshot of today's deployment.
- **Cardinality**: 3
- **The three values are NEVER summed into one rate.** They have different bounds and different alerts:
  - `meeting_created` — one per meeting actor. Alerted by nothing.
  - `participant_left` — the leave-debounced rotation, at most one per meeting per W. → `MCKekRotationStorm`.
  - `sender_space_exhausted` — the immediate KEK-epoch reset when a meeting's `sender_id` namespace runs out. Exempt from the debounce, bounded by 65,536 admissions. → `MCKekEpochResetOnSenderIdExhaustion` (info).
- **Alert partition — a new `trigger` value MUST be classified into exactly one of the above.** The storm rule uses a POSITIVE selector (`trigger="participant_left"`), so an unclassified new value is **silently unalerted**. This is the one place in the KEK telemetry that trades fail-closed for semantic honesty: the storm threshold `2/W` is derived from the debounce and can never be approached by an arm the debounce does not govern, so a per-trigger storm series would read as coverage while being unable to fire.
- **No longer identically the meeting-creation count.** It was until story 2 task 9, when rotation landed; the `meeting_created` series still is.
- **Still NOT a "missing key material" signal.** That condition is per-join and client-observed (`MCMediaMissingKeyMaterial`, on `dt_client_media_frames_dropped_total`).
- **Emits no key material and no generation.** A per-meeting generation series would be a membership-change trace. *Which* meeting is answered by the meeting actor's span.
- **Recorded in**: `actors/meeting.rs` at meeting-actor creation, `rotate_due` and the epoch reset in `handle_join`, via `observability/metrics.rs::record_meeting_kek_generated`
- **Alert**: `MCKekRotationStorm` (warning, `participant_left` only), `MCKekEpochResetOnSenderIdExhaustion` (info)
- **Dashboard**: MC Overview - Meeting KEK Issuance by Trigger (Join Flow row)

### `mc_meeting_kek_pushes_total`
- **Type**: Counter
- **Description**: One outcome per rostered recipient per KEK rotation (`MeetingKekUpdate`)
- **Labels**:
  - `outcome`: `delivered` | `dropped_outbound` | `participant_gone` | `actor_unavailable` | `timed_out` (`media_admission::KekPushOutcome::ALL`)
  - `key_custody`: `operator`
- **Cardinality**: 5
- **The five values are disjoint and each names a distinct remedy.** Only `participant_gone` is benign.
  - `delivered` — handed to the connection's outbound stream. **Not a client acknowledgement.**
  - `dropped_outbound` — the outbound stream channel was full or closed. Also on `mc_participant_outbound_messages_dropped_total{payload_kind="meeting_kek_update"}`.
  - `participant_gone` — rostered but no live connection (inside the reconnect grace period). **Benign**: a returning participant receives the current KEK, on reconnect re-issue or a fresh join.
  - `actor_unavailable` — the participant actor exited or its mailbox closed. `MCActorPanic` territory.
  - `timed_out` — handed to the mailbox and no answer within 2 s. **Names the observable, not an inferred cause**: MC cannot tell a wedged actor from a lost reply or a slow turn. Read `mc_actor_mailbox_depth`; `MCHighMailboxDepthCritical` territory. Distinct from `actor_unavailable` because the remedy is.
- **Exactly one increment per recipient, including a silent one.** A recipient that never answers is recorded as `timed_out` rather than dropped from the tally — otherwise it would leave BOTH the numerator and the denominator of the failure ratio, and `MCKekPushFailureRate` would get *quieter* as more actors wedged.
- **The security residual this is the only control for:** a member whose push is not delivered never rotates its own transmit keys, so a departed participant keeps opening THAT member's media past W.
- **Recorded in**: `media_admission/rotation.rs::collect_push_outcomes` via `observability/metrics.rs::record_kek_push`
- **Alert**: `MCKekPushFailureRate` (warning, `outcome!~"delivered|participant_gone"` > 1% for 10m — negated so a value added later fails CLOSED into the alert)
- **Dashboard**: MC Media - KEK Push Outcomes (KEK Lifecycle row)

### `mc_meeting_kek_rotation_failures_total`
- **Expected-empty**: yes — a healthy rotation path fails nothing, so this reads zero when healthy
- **Type**: Counter
- **Description**: KEK rotations that could not be performed. The KEK was left unchanged.
- **Labels**:
  - `reason`: `rng` | `generation_exhausted` (`media_admission::KekRotationFailed::ALL`)
  - `key_custody`: `operator`
- **Cardinality**: 2
- **The two reasons have OPPOSITE remedies** — the point of the label. `rng` (CSPRNG failure) is overwhelmingly the live arm and is retried after W. `generation_exhausted` (the `u16` KEK generation at its ceiling) is permanent for that meeting and `MCKekRotationOverdue` will not clear until it ends; at the W floor it takes ~22.7 days of uninterrupted rotation in one meeting, so it is effectively unreachable.
- **While either persists, the W bound is SUSPENDED, not delayed**: every departed participant keeps a KEK that opens all current media.
- **Recorded in**: `actors/meeting.rs::rotation_failed` and the epoch reset in `handle_join` via `observability/metrics.rs::record_kek_rotation_failure`
- **Dashboard**: MC Media - KEK Rotation Failures (KEK Lifecycle row)

### `mc_meeting_kek_rotation_coalesced_leaves`
- **Type**: Histogram
- **Description**: Roster removals covered by one leave-triggered rotation
- **Labels**:
  - `key_custody`: `operator`
- **Buckets**: 1, 2, 3, 5, 10, 25, 50, 100 (its own matcher on its full name — never a shared `mc_meeting_kek` prefix, which would also catch the seconds histogram below)
- **Unit-last spelling (`_leaves`)** deliberately: the tree's first unitless histogram, and `_count` would collide with the histogram's own `_count` series.
- The debounce is measured from the OLDEST un-rotated departure and never reset by a later one, so a busy meeting folds several departures into one rotation per W.
- **Recorded in**: `actors/meeting.rs::rotate_due` (`participant_left` only)
- **Dashboard**: MC Media - KEK Departures Coalesced per Rotation (KEK Lifecycle row)

### `mc_meeting_kek_rotation_duration_seconds`
- **Type**: Histogram
- **Description**: From the rotation decision to the LAST per-recipient push outcome
- **Labels**:
  - `key_custody`: `operator`
- **Buckets**: 0.0005 … 2.5 s (its own full-name matcher). The top bucket covers the 2 s outcome timeout, so a fully timed-out rotation lands in a real bucket rather than `+Inf`.
- Measures the window in which some members already hold the new key and others do not — the operationally meaningful quantity, not CSPRNG cost. Emitted even when recipients time out.
- **Recorded in**: `media_admission/rotation.rs::collect_push_outcomes`
- **Dashboard**: MC Media - KEK Rotation Duration (KEK Lifecycle row)

### `mc_meeting_kek_rotation_pending_age_seconds`
- **Type**: Gauge
- **Description**: Age of the oldest un-rotated departure, as a MAXIMUM across live meetings
- **Labels**:
  - `key_custody`: `operator`
- **Cardinality**: 1
- **A maximum, sampled — not written by the actors.** A 5 s timer samples the times the meeting actors recorded. Last-writer-wins would let a healthy meeting's 0 erase an overdue meeting's age; event-driven writes would go stale; and sampling what the actors wrote means a WEDGED actor still ages. Zero at boot.
- **Cleared at rotation**, not when every push succeeds — one wedged recipient must not pin it (that is the push counter's job).
- While above zero, a departed participant still holds a working KEK: a **confidentiality** window, not a media-quality one.
- **Label set MUST stay identical to the overdue threshold's**, or `MCKekRotationOverdue`'s bare `a > b` matches nothing and the page silently never fires.
- **Recorded in**: `media_admission/rotation.rs::KekLifecycle::publish_fleet_gauges` (sampler task in `main.rs`)
- **Alert**: `MCKekRotationOverdue` (page)
- **Dashboard**: MC Media - KEK Rotation Pending Age vs Overdue Threshold (KEK Lifecycle row)

### `mc_meeting_kek_rotation_window_seconds`
- **Type**: Gauge
- **Description**: W, the rotation debounce window this pod enforces (`MC_KEK_ROTATION_DEBOUNCE_SECONDS`)
- **Labels**:
  - `key_custody`: `operator`
- **Cardinality**: 1
- **A CONFIG ECHO**, set once at boot from the SAME value the debounce timer and the `kek_rotation_debounce_seconds` wire field use (`Config::kek_lifecycle`), so the three cannot drift. It should equal the ConfigMap value on every MC pod; a mismatch is a stale ConfigMap or a pod that did not roll — for W a **security** fact, since W is the exposure bound on a departed participant.
- **No retention gauge exists, deliberately**: clients derive retention as `min(W/2, ceiling)`, so it would always be a function of this one.
- **Recorded in**: `main.rs` via `KekLifecycle::publish_config_gauges`
- **Dashboard**: MC Media - KEK Rotation Pending Age vs Overdue Threshold (KEK Lifecycle row)

### `mc_meeting_kek_rotation_overdue_threshold_seconds`
- **Type**: Gauge
- **Description**: W × 2 — the pending age above which `MCKekRotationOverdue` pages
- **Labels**:
  - `key_custody`: `operator`
- **Cardinality**: 1
- **Exists so the page compares two gauges with no arithmetic**, keeping W out of PromQL. The multiplier is a named Rust constant (`KEK_ROTATION_OVERDUE_MULTIPLIER`, compile-time asserted ≥ 2): at 1, healthy pending age reaches the threshold just before every rotation.
- **Recorded in**: `main.rs` via `KekLifecycle::publish_config_gauges`
- **Dashboard**: MC Media - KEK Rotation Pending Age vs Overdue Threshold (KEK Lifecycle row)

### `mc_meeting_sender_ids_issued_max`
- **Type**: Gauge
- **Description**: The highest `sender_id` namespace consumption across live meetings, in the current epoch
- **Labels**:
  - `key_custody`: `operator`
- **Cardinality**: 1
- **Cursor consumption, NOT admissions.** After an epoch reset the allocator's cursor runs past ids excluded by the reset's snapshot without issuing them, and counts them. That is the right quantity for namespace pressure — an excluded id genuinely is not available — but it is **not comparable to a join count**, and it resets at each epoch.
- **A maximum, not a sum**, so one meeting near a reset is not hidden behind fleet noise.
- **Flapper visibility, deliberately without an alert.** Flapper eviction does not ship; nothing bounds a flapping client's join-path cost, and this is how it is seen. Exhaustion self-repairs into an epoch reset, so a threshold alert here would fire on a healthy condition.
- **Recorded in**: `media_admission/rotation.rs::KekLifecycle::publish_fleet_gauges`
- **Dashboard**: MC Media - Sender ID Namespace Consumption (max) (KEK Lifecycle row)

---

## MH Communication Metrics

### `mc_register_meeting_total`
- **Type**: Counter
- **Description**: Total RegisterMeeting RPC attempts by outcome
- **Labels**:
  - `status`: RPC outcome (`success`, `error`)
- **Cardinality**: Low (2 status values)
- **Usage**: Monitor MC→MH meeting registration reliability, detect MH connectivity issues
- **Recorded in**: `mh_client.rs` after RegisterMeeting RPC completes
- **Dashboard**: MC Overview - RegisterMeeting RPC Rate by Status (MH Coordination row)

### `mc_register_meeting_duration_seconds`
- **Type**: Histogram
- **Description**: RegisterMeeting RPC round-trip latency
- **Labels**: None
- **Buckets**: [0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000]
- **Cardinality**: 1 (no labels)
- **Usage**: Monitor MC→MH RPC latency, detect MH performance degradation
- **Recorded in**: `mh_client.rs` measuring full RPC round-trip
- **Dashboard**: MC Overview - RegisterMeeting RPC Latency (P50/P95/P99) (MH Coordination row)

## Media-Routing Control Plane Metrics (ADR-0036 §8, §11)

Both metrics are emitted together, on **every** push outcome including `match`,
by `MhClient::register_meeting`'s confirm step. Neither carries a meeting
identifier, a `sender_id`, an `egress_stream_id` or a generation number as a
label (ADR-0036 §11). Neither carries `handler_id`: it is a per-incarnation
token (`mh-service` derives it from `HOSTNAME` plus a fresh UUID at process
start, and `MH_HANDLER_ID` is unset in all three MH manifests), so as a label it
would be one permanently-retained series per MH incarnation MC has ever pushed
to — worst under a crashloop, which is exactly when the metric must work. The
one-handler-versus-all-handlers triage query is answered from the `error!` line,
which does carry `handler_id`. The label may return once
`2026-09-02-mh-stable-handler-id` makes the id stable.

### `mc_media_policy_pushes_total`
- **Type**: Counter
- **Description**: Forwarding-policy pushes to an MH, classified by what the handler's reply proved (ADR-0036 §8)
- **Labels**:
  - `outcome`: `match`, `no_applied_generation`, `generation_mismatch`, `transport_mode_mismatch`, `handler_id_mismatch`
  - `key_custody`: single value `operator` (ADR-0036 §4 — MC can read media keys; that is accepted operator custody, never described as end-to-end)
- **Cardinality**: 5 (bounded at the type level by `PolicyPushOutcome::ALL`; a sixth value is a compile error)
- **Counts response evaluations, NOT meetings — with ONE exception.** A retried push contributes one sample per attempt; a terminal outcome exactly one. A divergence *ratio* over this denominator is therefore per-attempt, not per-meeting. The exception: an MC-restart floor on the FIRST confirm of a (meeting, handler) in an MC process is recorded on `mc_media_policy_generation_adoptions_total` INSTEAD (or here as `generation_mismatch` if adoption fails). **Total evaluated replies = `sum(mc_media_policy_pushes_total) + sum(mc_media_policy_generation_adoptions_total)`.**
- **This is the detection signal.** `mc_media_generation_divergence` is the magnitude a responder reads next — see that entry for why the gauge cannot carry detection.
- **`handler_id_mismatch` is a DIAGNOSTIC, not an alerting signal**, until `MH_HANDLER_ID` is stable per deployment: the id is per-incarnation, so an ordinary MH pod restart produces that outcome by construction and an `outcome!="match"` page would fire on every MH rollout. Alert expressions must read `outcome!~"match|handler_id_mismatch"`. **Three sites hold that one expression** — the alert rule (story task 21), this entry, and `docs/runbooks/mc-deployment.md`'s post-deploy checklist — and they are **one decision with one revert trigger**, `2026-09-02-mh-stable-handler-id`.
- **Label key is `outcome`, not `status`**, matching MH's `mh_media_policy_applies_total{outcome,key_custody}` so both ends of one handshake sit side by side in a query.
- **Usage**: Detect a meeting whose forwarding policy MC could not confirm as live
- **Recorded in**: `grpc/mh_client.rs::confirm` via `observability/metrics.rs::record_media_policy_push` (and `media_routing/pusher.rs` for a FAILED restart-floor adoption, as `generation_mismatch`)
- **Dashboard**: MC Media - Media Policy Pushes by Outcome (Media Routing row)

### `mc_media_policy_generation_adoptions_total`
- **Type**: Counter
- **Description**: MC-restart generation floors: a restarted MC's first push to a live handler was answered with a HIGHER applied generation (the pre-restart MC's number), and MC adopted it as a floor and re-pushed strictly above it (ADR-0036 §8; story 2, OPS-16)
- **Labels**:
  - `outcome`: `adopted`, `superseded`
  - `key_custody`: single value `operator`
- **Cardinality**: bounded at the type level by `FloorAdoption::ALL`
- **Increment boundary**: once per restart-floor reply NOT recorded on `mc_media_policy_pushes_total`, under one of two values. A reply is a restart floor only when its `outcome` is `generation_mismatch` with `applied > sent` AND this (meeting, handler) has had no confirmed push and no adopted floor in this MC process — the first-confirm discriminator (`MeetingProgramming::restart_floor_adoptable`). **Fail-closed**: a higher echo after any confirm (including the `u64::MAX` ratchet wedge) is not a restart floor and is recorded on `mc_media_policy_pushes_total{outcome="generation_mismatch"}`; so is a restart floor whose adoption FAILED. Both page on `MCMediaGenerationDivergence`.
- **THE TWO VALUES ARE NOT INTERCHANGEABLE, and only one has an upper bound.** `adopted` is the RECOVERY event: the floor was adopted and the content re-pushed above it, at most **one per (meeting, handler) per MC process** (the discriminator latches). `superseded` is ROUTINE COALESCING: the reply was discarded because a newer render was already published and is pushed in its place, so **no adoption happened**, the pair stays adoptable, and it has NO per-pair bound — under churn one pair can contribute several. A rising `superseded` rate means renders are arriving faster than pushes settle during a restart, which is ordinary. They are separate label values rather than one count because merging them would hide a churn rate inside a number a responder reads as "how many meetings recovered".
- **WHY THIS EXISTS: without it, every MC rollout with live meetings pages.** The restart floor is the expected shape of an MC restart under a live meeting, not a divergence. Recorded as `generation_mismatch` it fired `MCMediaGenerationDivergence` (`for: 0m`, `> 0`) once per rollout with live meetings, training oncall to wave through the page whose real subject is a handler that never installed a policy. Recording it HERE instead leaves the alert expression, `alerts.md` and the `mc-deployment.md` post-deploy gate unchanged — deliberately no subtraction, which would be fail-open at window boundaries and would make the page's existence conditional on this series existing.
- **WHY A SIBLING COUNTER, NOT A SIXTH `outcome` VALUE — this is the design, not an unfinished GSA edit.** The `outcome` vocabulary is canonical in `proto/dark_tower/internal/v1/internal.proto`, and each value exists "because some field on this response can fail in a way MC must act on". A restart floor is not a response-field failure: MH answered truthfully, and "MC adopted a floor" is MC's own decision about what to do next. Folding it into the proto enum would make the MC↔MH contract carry MC-internal control flow and would put the false page back.
- **NOT VISIBLE UNDER ANY `outcome` LABEL.** A responder splitting `mc_media_policy_pushes_total` by `outcome` after an MC restart sees a clean breakdown for this arm by design; read this counter alongside it.
- **Denominator**: total evaluated replies = `sum(mc_media_policy_pushes_total) + sum(mc_media_policy_generation_adoptions_total)` — summing **both** label values, which is what keeps the reconstruction exact.
- **Triage surface**: the WARN at `mc.register_meeting.trigger` ("Handler holds a newer policy generation than this MC issued ... adopting it as a floor"), carrying `sent_generation`, `applied_generation` and `adopted_generation`. A rise here during an MC rollout is expected, one per live (meeting, handler); a rise with NO MC restart is not — investigate as a second MC programming the same meeting.
- **Recorded in**: `media_routing/pusher.rs::push_until_settled` via `observability/metrics.rs::record_policy_generation_adoption`; zero-initialised at boot
- **Dashboard**: MC Media - Policy Generation Floor Adoptions (Media Routing row)

### `mc_media_sender_binding_responses_total`
- **Type**: Counter
- **Description**: Whether MC could resolve a participant's `sender_id` for the Media Handler that asked, and if not, why (ADR-0036 §2, §4; R-15)
- **Labels**:
  - `outcome`: `resolved`, `meeting_unknown`, `participant_unknown`, `user_ambiguous` (`registry_full` RETIRED, story 2 task 20 — see below)
  - `key_custody`: single value `operator` (ADR-0036 §4 — MC can read media keys; that is accepted operator custody, never described as end-to-end)
- **Cardinality**: bounded at the type level by `SenderBindingOutcome::ALL`. **Deliberately no restated integer** — the list above is the operator-facing artifact and a second encoding of its length only rots. (This number drifted four times during the devloop that added the metric; the compile-checked length in `ALL: [Self; N]` is the guard, a prose count is an unchecked copy.)
- **Increment boundary**: once per `NotifyParticipantConnected` **response MC emits**, on every path that produces a response. If the RPC fails before a response is formed, nothing increments here — that population is MH's `mh_media_session_starts_total{outcome="declined_mc_unavailable"}` plus MC's transport-level metrics.
- **THE `sender_id` VALUE IS NEVER A LABEL.** ADR-0036 §11 bars it as a per-participant metric dimension, span attribute or structured log field. The bounded `outcome` token carries every bit of triage signal the value would, at the `SenderBindingOutcome::ALL` cardinality instead of 65535.
- **Why this is NOT redundant with MH's counter — read this before deleting either.** MH observes only `sender_id == 0` and **structurally cannot** tell which unresolved outcome produced it; its `mh_media_session_starts_total{outcome="declined_no_sender_binding"}` is the **union of MC's `0`-answering outcomes**: `meeting_unknown` (routing/registration fault, remedy in the MC↔MH registration path), `participant_unknown` (join race — **self-clearing**), `user_ambiguous` (one user, two roster entries — **never self-clears, and reconnecting CAUSES it**). Only MC can split them. **This series therefore carries strictly more information than any function of the MH series.** The values are named rather than counted deliberately: a restated count is an unchecked copy that goes stale on the next addition, while naming them cannot.
- **The discriminator for this whole vocabulary is REMEDY, not cause.** Two of the four values exist as separate series because acting on a neighbour's remedy would be wrong or harmful, not because their causes differ. When adding a value, the question is "does a responder do something different?", not "is this a distinct cause?".
- **`user_ambiguous`: the folded label would have been actively harmful, which is why it is separate.** MC mints a fresh `participant_id` per join and does not bar a second join, so **a user who reconnects produces a second roster entry for the same `sub`**. If this were folded into `participant_unknown`, the union's documented remedy is "benign race — reconnect", and following that instruction on this cause **converts a transient failure into a permanent one**. This is not merely two remedies in one series; it is a series whose documented remedy is *harmful* for one of its members.
- **`user_ambiguous` is NOT resolved by `connection_id`** (story 2 task 20). `connection_id` distinguishes MH CONNECTIONS; this ambiguity is between two MC roster ENTRIES (joins) behind one token `sub`, and MC cannot map an MH connection to one of its own joins. The remedy has one home: `docs/TODO.md` §Media Path Obligations, "`user_ambiguous` has no operator remedy". Since task 20 the same fail-closed rule also governs connectivity: an ambiguous `sub` records connectivity for NO entry (counted `mc_mh_notifications_unapplied_total{reason="user_ambiguous"}`).
- **`registry_full` — RETIRED (story 2 task 20). Version-skew only.** It counted the per-meeting `MhConnectionRegistry` cap declining a binding. The registry and its cap are gone: participant connectivity now lives in the meeting actor, bounded structurally (roster × the meeting's frozen handler set × a small per-handler key bound), and MC never declines a binding for capacity. A non-zero value can only come from an MC image predating the retirement during a rolling deploy — **not a capacity signal; there is no cap to raise.** The actor's own bound refuses a surplus *connection key* without touching the binding, on `mc_mh_notifications_unapplied_total{reason="connection_bound_refused"}`.
- **Shared denominator, recorded so it is not alerted on twice.** This increments on the same path as `record_mh_notification("connected")`, after the same validation gate, so `sum(mc_media_sender_binding_responses_total)` equals `mc_mh_notifications_received_total{event_type="connected"}` **by construction**. Not duplication — this one carries the resolution dimension the other lacks — but two series encoding one count invite either a double alert or the deletion of the wrong one.
- **Label key is `outcome`, not `status`**, matching the sibling `mc_media_policy_pushes_total` and MH's counterpart so both ends of one handshake sit side by side in a query.
- **Usage**: Detect a participant MC cannot bind for MH — and distinguish a routing fault from a join race, which MH alone cannot
- **Recorded in**: `grpc/media_coordination.rs::notify_participant_connected` via `observability/metrics.rs::record_sender_binding_response`
- **Dashboard**: MC Media - Sender Binding Responses by Outcome (Media Routing row)

### `mc_media_generation_divergence`
- **Type**: Gauge
- **Description**: Last-observed magnitude by which a handler's live forwarding policy differed from the one MC pushed
- **Labels**:
  - `key_custody`: single value `operator`
- **Cardinality**: 1
- **Value is an unsigned MAGNITUDE, not a difference**: `|sent - applied|`, computed from the **applied** value in the response. Feeding it from the sent value yields a constant 0 — the silent regression ADR-0036 §8 names. `saturating_sub` is equally wrong: `applied > sent` is reachable (an MC restart re-deriving from 1 against a handler holding a higher generation, or the `policy_generation: u64::MAX` ratchet wedge) and would render as a healthy 0 on the one response shape that proves the two ends disagree. Direction is not lost — the `error!` line carries both numbers.
- **Last-write-wins, pod-level.** Its clearing path is the next push of any (meeting, handler) on this pod, so it is overwritten rather than latched and cannot wedge above zero for a pod's lifetime. The price: a healthy push can erase a diverged reading. MC also inherits the global `scrape_interval`, which is coarse relative to a one-shot-per-registration write, so a divergence can be overwritten before it is ever scraped. **Both are independent reasons this gauge is not the detection signal** — that is `mc_media_policy_pushes_total`.
- **Written on every registration push, which since story 2 means every structural change.** Joins, leaves, capability declarations and mutes each re-render the meeting and re-push any handler whose snapshot changed, so this gauge is written far more often than the story-1 once-per-meeting — and the last-write-wins caveat above gets WORSE, not better: a diverged reading is overwritten by the next healthy push of ANY (meeting, handler) on the pod. With the ADR-0036 §8 re-assert cadence still deferred (story 4), **this gauge does not observe a handler restart**: a handler can lose all forwarding policy while this value holds its last healthy reading.
- **An MC restart against a live handler is now self-correcting.** A restarted MC re-derives generations from 1 while MH still holds K; the push worker adopts MH's truthfully echoed K as a floor and re-pushes at K+1 (`PolicyGenerations::adopt_floor`), and it is recorded on `mc_media_policy_generation_adoptions_total`, NOT here: a successful adoption never writes this gauge. Only a FAILED adoption (recorded as `generation_mismatch`) writes a magnitude.
- **Usage**: Read the magnitude after `mc_media_policy_pushes_total{outcome!~"match|handler_id_mismatch"}` has fired; never page on it
- **Recorded in**: `grpc/mh_client.rs::confirm` via `observability/metrics.rs::record_media_policy_push` (and `media_routing/pusher.rs` for a FAILED restart-floor adoption, as `generation_mismatch`)
- **Dashboard**: MC Media - Media Generation Divergence (magnitude) (Media Routing row)

## Client-Facing Media Signalling Metrics (ADR-0036 §5, §6, §11)

MC's client-facing half of the media path: the client declares what it can
decode, MC composes what it must produce and what will fill its slots.

**Identity rules, stated at the width MC actually keeps them.** An invariant
claimed more strongly than it is held is worse than none: the next person
auditing ADR-0036 §11 against this catalog would conclude something false and
then have to relitigate which of the two is the defect.

- **Meeting identifier** (raw or hashed) — barred from metric **labels** and
  **span attributes**. **Not barred from logs.** §11's flat prohibition is scoped
  to metrics, and MC's connection-lifecycle logs carry `meeting_id` by
  convention throughout; the media-signalling WARNs and ERRORs in
  `actors/meeting_media.rs` (where composition and emission now live) and the
  remaining media-signalling logs in `webtransport/connection.rs` carry it
  deliberately, because it is what makes the context-unavailable WARN
  actionable at all.
- **Participant id and stream identity** — `slot_id`, `sender_id`, stream
  number, `switch_command_id` — barred from labels, span attributes **and**
  logs. These are the per-frame/per-stream dimensions §11's log bullet names.

Every label domain is an exhaustive `match` over a Rust enum with an `ALL`
constant, so a new value is a compile error rather than an unbounded series. There is deliberately
no `#[instrument]` anywhere in `media_signaling`: its functions take
`SlotId`/`SenderId`/`&ReceiveCapability` parameters that an auto-instrumented
span would record as attributes, on a surface no guard covers.

**Scrape periodicity**: MC is scraped at the `mc-service` job's own per-job `scrape_interval` in `infra/kubernetes/observability/prometheus.yml`, the authoritative config (R-36; see `docs/observability/dashboard-conventions.md` §Periodicity). Cite the key, never a number.

### `mc_media_receive_capability_declarations_total`
- **Type**: Counter
- **Description**: Client receive-capability declarations, by disposition (ADR-0036 §6)
- **Labels**:
  - `outcome`: `accepted`, `accepted_unchanged`, `duplicate_slot_id`, `slot_count_over_cap`, `slot_id_out_of_range`, `pinned_sender_id_zero`, `pinned_sender_id_out_of_range`, `media_kind_unspecified`, `declaration_budget_exhausted`
  - `key_custody`: single value `operator` (ADR-0036 §4 — MC can read media keys; that is accepted operator custody, never described as end-to-end)
- **Cardinality**: 9 (bounded at the type level by `CapabilityOutcome::ALL`; a tenth value is a compile error)
- **This metric PARTITIONS declarations.** Every declaration lands in exactly one bucket of exactly one metric. A rejection reported on some other series would silently break that relationship — which is why the identical-re-declaration no-op is *counted* rather than dropped, and why the slot cap (R-1: over the cap is rejected whole, never one silent participant) is counted HERE as `slot_count_over_cap` rather than on a second counter.
- **THE SUCCESS SET IS `{accepted, accepted_unchanged}`.** The failure predicate is `outcome!~"accepted|accepted_unchanged"`. Same shape as `mc_media_send_directives_total`'s `{emitted, emitted_empty_targets}`: one pattern across both metrics in this family — the success set is the prefix-shared pair, the failure predicate is the negated alternation.
- **PERMANENT — no revert trigger.** This alternation is a correct, permanent classification, not a defect workaround. If you have seen a similar-looking alternation elsewhere in this catalog that carries a revert trigger, that one is a temporary carve-out and this is not; do not fold them together, and do not strip this alternation as part of any cleanup.
- **`accepted` ALONE means "declarations MC acted on", and that distinction is a security property.** `accepted_unchanged` records a re-declaration identical to the one already in force: MC does no work and sends nothing. It is therefore **client-inflatable at near-zero server cost**, and **must not appear in the denominator of any ratio a client has an incentive to deflate** — a rejection ratio computed over it is an evadable alert. Use `accepted` alone as that denominator. There is no alert on this metric today, which is exactly why the rule is written down now rather than discovered by whoever builds one.
- **A non-zero `accepted_unchanged` is also real information**: a sustained rate means a client is re-declaring pointlessly.
- **Rejection is always WHOLE-DECLARATION.** MC never accepts a prefix, never applies last-write-wins to a duplicate, and never clamps an out-of-range id.
- **Remedies differ by value:**
  - `duplicate_slot_id`, `slot_count_over_cap`, `slot_id_out_of_range`, `pinned_sender_id_zero`, `pinned_sender_id_out_of_range` — **client defects.** Fix the client. `slot_count_over_cap` may instead mean `MC_MAX_RECEIVE_SLOTS` is set below what legitimate clients need — compare the client's configured N against `mc_media_receive_slot_cap`.
  - `media_kind_unspecified` — **read version skew first, not a forgotten field.** The proto's zero is what a pre-ADR-0036 peer's `AUDIO = 0` decodes to, so the likeliest producer is a client that *meant audio*. A fleet-wide rise correlates with a client rollback or a partial rollout. Treating it as "a kind we happen not to have" would have made that present as "everyone's meetings are empty" with no signal naming skew.
  - (`slot_id_not_planned` — **retired in story 2.** It rejected audio in a slot MC had not planned, because story 1 fixed the egress slot at the join-time push before a client could declare. Since story 2 declared audio slots are the assignment's INPUT, so no well-formed declaration can miss a plan and the value no longer exists. A story-1-era runbook or dashboard naming it is stale.)
  - `declaration_budget_exhausted` — a client re-declaring more than `MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS` times on one connection: a client bug or an attack. Since story 2 an ACCEPTED distinct declaration is a structural change (re-render, re-push, re-emit to every declared participant), which is exactly why this budget exists.
- **Logging**: every rejection increments this counter; the WARN fires **once per connection**, carries the outcome token, and never carries the offending `slot_id`/`pinned_sender_id` value.
- **Usage**: Are clients' capability declarations landing, and if not, whose bug is it?
- **Recorded in**: `webtransport/connection.rs::handle_receive_capability` via `observability/metrics.rs::record_receive_capability`. Only `accepted` declarations reach the meeting actor (`MeetingActorHandle::register_receive_capability`); every rejection is decided connection-side.
- **Dashboard**: MC Media - Receive Capability Declarations by Outcome (Client Media Signalling row)

### `mc_media_send_directives_total`
- **Type**: Counter
- **Description**: Send-directive compositions, by disposition (ADR-0036 §5)
- **Labels**:
  - `outcome`: `emitted`, `emitted_empty_targets`, `unknown_stream_number`, `transport_mode_unspecified`, `handler_url_unresolved`, `meeting_state_unavailable`, `assignment_failed`
  - `key_custody`: single value `operator`
- **Cardinality**: bounded at the type level by `DirectiveOutcome::ALL`. Deliberately no restated integer — the list above is the operator-facing artifact and a second encoding of its length only rots.
- **THE SUCCESS SET IS `{emitted, emitted_empty_targets}`, NOT `{emitted}`.** ADR-0036 §5 makes an empty target set a *specified success* — "A target set may be empty. That means send nothing." A failure predicate must read `outcome!~"emitted|emitted_empty_targets"`; `outcome!="emitted"` would page on a legal state. **`emitted_empty_targets` is ROUTINE**: a solo participant, a participant nobody holds in a slot, and a participant not yet connected to any handler (or still inside its connect settle window) is directed to send nothing — and is re-sent a non-empty directive the moment someone starts holding it. **"Holds" means shares a CONNECTED handler with** (story 2 task 20, ADR-0036 §9) — not "was placed on the same handler", which was task 6's meaning and is retired. **Frequency shift:** under task 6's round-robin placement this was routine in every split two-person meeting; under the edge model an all-connected meeting co-locates, so it collapses back to solo participants, unheld participants and the settle window. It stays a success either way; only the expected magnitude dropped. A target set now names EVERY handler owning one of the sender's edges, so a single directive may carry several targets (`mc_media_send_targets_total`).
- **PERMANENT — no revert trigger.** This alternation is a correct, permanent classification grounded in ADR-0036 §5, not a workaround. `mc_media_policy_pushes_total`'s superficially identical `outcome!~"match|handler_id_mismatch"` is a *temporary* carve-out with a recorded revert trigger; **the two share a shape and nothing else.** Do not fold this entry into that decision, and do not strip this alternation as part of any cleanup: doing so starts paging on a routine success while appearing to complete a documented task.
- **Three values are MC defects — ANY non-zero value indicates a bug**: `unknown_stream_number` (a forwarding plan named a stream number MC has no policy entry for), `transport_mode_unspecified` (a plan carried no transport mode; MC fails closed rather than defaulting to datagram) and — since story 2 — `handler_url_unresolved` (every handler url a client sees comes from one frozen `MeetingHandlers` value — the edge's owning handler id is only ever a member of it, because connectivity can hold only handlers resolved from that value — so a miss cannot happen by construction; it fails CLOSED — nothing is emitted for that participant, never an `ACTIVE` slot or a send target with an empty url). The remaining **two** failure values are environmental: `meeting_state_unavailable` and `assignment_failed`. (`no_planned_egress_slot`, story 1's join-time "MC planned no slot for you", is retired: declared slots are now the assignment's input.)
- **Recorded by the MEETING ACTOR — failures on every composition, successes only on CHANGE.** Compositions are triggered by the participant's own actions and by a PEER's join, leave, declaration or mute (which the participant's own connection never sees); plus once at join if the connection cannot resolve the meeting. A composition FAILURE records its stage unconditionally, so every reason MC did not **compose** a directive lands on this one series. A SUCCESSFUL composition records `emitted`/`emitted_empty_targets` only when the directive CHANGED (see below); an unchanged recomposition records nothing. Under the story-2 model a roster change recomposes everyone but changes only some directives, so the `emitted` rate is well below the composition rate.
- **A failure RATIO over this series overstates the failure rate.** Its denominator excludes successful-but-unchanged compositions (never recorded) while failures are recorded every time. No alert divides by it today; if one is added, it must not treat `sum()` as "compositions".
- **Failure values are DISJOINT BY CONSTRUCTION.** Composition is sequential stages — render the slot table, resolve handler urls, build streams — and the first failing stage returns its own value. An assignment failure can never also be reported as an unresolved handler url, because url resolution is never reached.
- **`emitted` MEANS COMPOSED, NOT DELIVERED.** It fires when the actor sends a CHANGED directive, before the two delivery hops, each covered by a different series:
  1. the meeting-actor → participant-handle send — mailbox **CLOSED** (participant actor gone): `mc_media_slot_view_emissions_total{outcome="delivery_failed"}` (counted since story 2; previously a WARN only).
  2. the participant actor's stream `try_send` — channel **FULL**: `mc_participant_outbound_messages_dropped_total{payload_kind="signaling_raw"}`.
  So a healthy `emitted` alongside a non-zero value on either of those is the shape of "MC composed a directive the client never received".
- **Only CHANGED directives are emitted.** The actor retains each declared participant's last directive and sends a new one only when it differs, so the `emitted` rate tracks target-set changes (someone starts or stops holding this participant), not structural churn.
- **Usage**: Is MC actually directing clients to send, and when it silently is not, why?
- **Recorded in**: `actors/meeting_media.rs::flush_one` (every failed composition; every CHANGED successful one) and `webtransport/connection.rs::handle_receive_capability` (`meeting_state_unavailable` when the actor cannot register a declaration), both via `observability/metrics.rs::record_send_directive`
- **Dashboard**: MC Media - Send Directives by Outcome (Client Media Signalling row)

### `mc_media_slot_states_total`
- **Type**: Counter
- **Description**: Slot states conveyed to subscribers (ADR-0036 §6)
- **Labels**:
  - `slot_state`: `unspecified`, `active`, `source_muted`, `withheld_by_congestion`, `fewer_sources_than_slots`, `zero_requested`, `source_unreachable`, `switch_pending`
  - `key_custody`: single value `operator`
- **Cardinality**: bounded at the type level by the proto `SlotState` enum, mirrored exhaustively by `media_signaling::slot_state_label`
- **The bucketed slot-state signal ADR-0036 §11 mandates**, joined to no identity: it answers *how many and how bad*; **MC's own assignment state answers *who***, at investigation time, in a system that legitimately holds that mapping. There is no per-slot series and must not be one.
- **The label domain MIRRORS THE WIRE ENUM EXHAUSTIVELY** — all eight `SlotState` variants, not the three MC can reach today. A hand-picked subset needs editing the moment §7 makes `switch_pending` live, and a `SLOT_STATE_UNSPECIFIED` reaching the wire is an MC defect that must be *visible* rather than absent. Keeping the vocabulary identical to the wire's is also what makes this distribution comparable with the client's.
- **Reachable since story 2**: `active`, `source_muted`, `fewer_sources_than_slots`. **Every declared-but-unfilled slot is `fewer_sources_than_slots`** — a solo participant (loopback is removed, R-3), a meeting smaller than N, a participant sharing a connected handler with too few peers, and every slot of a participant not yet connected. `source_unreachable` stays **unemitted** under the story-2 edge model: a participant the subscriber shares NO connected handler with consumes no slot and is named in `StreamAssignments.unreachable_sender_ids` instead (see `mc_media_unreachable_senders_total`); the state is reserved for a PINNED source with which the subscriber shares no connected handler, which static fill cannot produce. It stays in the label domain because the domain mirrors the wire. `withheld_by_congestion` is MH-observed (§6) and arrives with the slot-state notification; `switch_pending` arrives with §7 switching; `zero_requested` may be **unemittable in principle** — it means "requested zero of this kind" but rides a message whose `slot_id` echoes a slot the subscriber *declared*, and a subscriber who declared a slot of that kind did not request zero of it. That is an open protocol question recorded in `docs/TODO.md`, not a settled "reachable later".
- **`source_muted` is client mute (§5) OR server mute (§7)** — one wire state covers both causes (who muted whom rides `ParticipantMuteUpdate`). MC does **not** withdraw or re-issue that source's send directive; the slot state is the entire signal. A `source_muted`/`active` oscillation with a flat `mc_media_send_directives_total` is the healthy shape.
- **THIS COUNTER IS PER-EMISSION, AND EMISSIONS ARE BOTH CLIENT- AND SERVER-DRIVEN.** One increment per declared slot per `StreamAssignments` the actor actually SENDS (only changed views are sent). Since story 2 the actor re-emits on a PEER's join, leave, declaration or audio-mute change, not only on the subscriber's own declaration, so the distribution is weighted by meeting churn as well as by any one client. **Any SLO or ratio built on it must be per-connection-normalised.**
  - The client-driven share is bounded: mute-driven re-emits are rate-limited per connection (`mc_media_mute_requests_total{outcome="rate_limited"}`), a video-only toggle re-emits nothing, and each re-emit reaches only the subscribers HOLDING the muted source. The server-driven share is bounded per meeting by the actor's per-turn flush bound (`mc_media_slot_view_emissions_total{outcome="deferred"}`).
- **Usage**: How many slots are filled versus dark, and in what way
- **Recorded in**: `actors/meeting_media.rs::flush_one` via `observability/metrics.rs::record_slot_state`
- **Dashboard**: MC Media - Slot States (Client Media Signalling row)

### `mc_media_mute_requests_total`
- **Type**: Counter
- **Description**: Client `MuteRequest` dispositions (ADR-0036 §5 client mute)
- **Labels**:
  - `outcome`: `applied`, `applied_no_recompose`, `unchanged`, `rate_limited`, `actor_unavailable`
  - `key_custody`: single value `operator`
- **Cardinality**: bounded at the type level by `MuteOutcome::ALL`
- **This metric PARTITIONS mute reports.** Every post-join `MuteRequest` on a connection with a media-signalling context lands in exactly one bucket.
- **THE SUCCESS SET IS `{applied, applied_no_recompose}`.** The failure predicate is `outcome=~"rate_limited|actor_unavailable"`, **not** `outcome!="applied"` — that would count two routine states as failures.
- **THIS PREDICATE IS STATED POSITIVELY WHILE ITS TWO NEIGHBOURS ARE NEGATED, AND THAT IS DELIBERATE — do not harmonise it.** `mc_media_receive_capability_declarations_total` and `mc_media_send_directives_total` both use `outcome!~"..."`, which is correct for *them* because their success sets are closed and small. Here the enumeration is on the failure side, so **a variant added later defaults to NOT being counted as a failure** rather than silently joining the failure set the way a negated predicate would. A new disposition should have to be classified deliberately, not inherit "page-worthy" by omission. If a later pass makes these three consistent, it should move the others to positive form, not this one to negative.
  - `applied` — the audio flag moved on a declared connection and was reported to the meeting actor, which re-emits the changed slot view to the subscribers HOLDING this source. Any composition failure on that re-emit is counted by the actor on `mc_media_send_directives_total` and `mc_media_slot_view_emissions_total{outcome="composition_failed"}` — it is no longer visible only in logs.
  - `applied_no_recompose` — reported to the actor, with nothing to re-convey: either this connection has not declared a receive capability yet, or **only `video_muted` changed**. The slot view reads only audio mute, so no subscriber's view changes. **This is what an ordinary camera button produces** — expect it to be the largest bucket once clients wire video controls.
  - `unchanged` — identical to the report already in force. No actor hop, no recomposition. Not a failure: MC correctly did nothing.
- **`unchanged` IS CLIENT-INFLATABLE AT NEAR-ZERO SERVER COST, and must not appear in a ratio denominator.** Same property as `accepted_unchanged` on the capability counter, and the same rule: **the denominator is `applied` + `applied_no_recompose`**. The short-circuit fires before any actor hop, roster read or recomposition, so a client repeating one state drives this value at line rate for a tuple compare and a counter increment. "What fraction of mute reports are being applied", computed over `unchanged`, is a number one participant can drive to zero.
- **`unchanged` is exempt from the rate limiter ON PURPOSE — do not "fix" the ordering.** The no-op check runs *before* the token is spent, so identical repeats do not drain the bucket. Reversing that would let a client spamming a steady state exhaust its own budget on messages that do no work and thereby suppress its next **genuine** toggle, while the metric reported `rate_limited` for an expensive path that was never approached.
- **`rate_limited` is the visible edge of the mute-work bound, and is NOT by itself an incident.** Client mute is the only repeatable client-driven path that puts work on the **shared meeting actor's mailbox** — one actor hop plus a re-emit to the source's holders. A per-connection token bucket (burst 8, sustained 4/s) bounds it; the resulting fan-out is bounded per meeting by the actor's flush bound.
  - **This is NOT a meeting-wide contention signal.** The roster `broadcast_update` the bound was first sized against was removed on security's S-2 judgment that no consumer needed a SELF-mute fan-out. (`MuteChanged` IS wire-serialized since story 2 task 12 — for SERVER mute only; a self-mute still does not fan out, by that judgment, not because the encoder cannot.) The meeting-wide cost of a mute is its re-emit to the source's holders, which the actor bounds per turn; do not route a meeting-wide latency investigation here.
  - **A human never reaches the limiter.** A sustained non-zero rate means one connection is toggling far above human rates: a client-side repeat loop or a reactive-state bug first, an abusive peer second. It is bounded to that connection either way.
  - **Why a rate limit and not a budget.** A cumulative budget would permanently deny a repeatable steady-state user action: once spent, that participant's `audio_self_muted` freezes and every other client renders a live speaker as muted for the rest of the session. That is a correctness failure strictly worse than the amplification it would prevent. See `media_signaling`'s module doc for the criterion in general form.
  - **Dropping a report is safe, and here is why it is safe even for a merely broken client.** ADR-0036 §5 enforces client mute at **capture on the client**, so a suppressed report means the audio genuinely stopped and only the indicator other participants see is stale. There is no window in which someone believes they have stopped transmitting and has not. A suppressed report deliberately does not update MC's cache of the last reported pair, so the client's next toggle re-attempts rather than being swallowed — the staleness is bounded by that next action, not by the session.
- **`actor_unavailable` is environmental and terminal**: the meeting actor's mailbox is closed, so the connection is already going away.
- **Usage**: Is client mute being applied, and when it is not, why? Is any connection driving mute recomposition hard enough to be clamped? (Per-connection cost, not meeting-wide — see the `rate_limited` bullet.)
- **Recorded in**: `webtransport/connection.rs::handle_mute_request` via `observability/metrics.rs::record_mute_request`
- **Dashboard**: MC Media - Mute Requests by Outcome (Client Media Signalling row)

### `mc_media_server_mute_requests_total`
- **Type**: Counter
- **Description**: `ServerMuteRequest` dispositions — a host's server mute of another participant (ADR-0036 §5, §7; story 2 R-8, R-9)
- **Labels**:
  - `action`: `mute`, `unmute` — the verb the request ASKED for (`mute` if either kind is requested muted, `unmute` if neither), never its result
  - `outcome`: `applied`, `unchanged`, `not_permitted`, `unknown_target`, `rate_limited`, `actor_unavailable`
  - `key_custody`: single value `operator`
- **Cardinality**: 12 (2 x 6), bounded at the type level by `ServerMuteAction::ALL` x `ServerMuteOutcome::ALL`, and **zero-initialised as the FULL cross product, unreachable cells included** — so an absent series means "not up or not scraped", never zero
- **Deliberately SEPARATE from `mc_media_mute_requests_total`, and not merely because the names overlap.** Client mute and server mute differ in decider (the participant about itself versus a host about someone else), enforcement point (client capture, ADR-0036 §5, versus MH ingress, §7) and remedy. A merged series would make every existing client-mute query silently count moderation events.
- **`action` is NOT A JOIN KEY with `dt_client_media_mute_transitions_total{action}`**, whose value set is byte-identical: that series is a local user's capture-side toggle, this one a host's moderation decision. Summing `by(action)` across the two metric names means nothing (`docs/observability/label-taxonomy.md`, the `action` row).
- **It PARTITIONS requests**: one increment per `ServerMuteRequest`, in exactly one `outcome`. `sum()` is requests received.
- **THE FAILURE PREDICATE IS STATED POSITIVELY: `outcome=~"rate_limited|actor_unavailable"`.** `unchanged`, `not_permitted` and `unknown_target` are not MC failures. Positive for the same reason as `mc_media_mute_requests_total`: a variant added later defaults to NOT being a failure. Do not harmonise it to `outcome!="applied"`.
- **The metric distinguishes two refusals the CLIENT never can — deliberately.** `not_permitted` and `unknown_target` reach the client as ONE byte-identical error; the split exists only here and in the `mc.webtransport.connection` WARN, which are operator-visible. Do not "harmonise" that asymmetry in either direction.
- **`unknown_target` is reachable ONLY for a host requester.** Authority is checked BEFORE the target is looked up, so a non-host naming a participant who does not exist is `not_permitted`. That ORDERING is the security control (a non-host learns nothing about who is in the meeting); the single wire error is defence in depth on top of it. The `{action="mute",outcome="unknown_target"}` cell staying at zero while a non-host is refused is what the ordering test reads.
  - `applied` — the requested state differed and was recorded; for an audio change, MC re-rendered (the muted sender rides the next registration snapshot of every handler carrying one of its edges, advancing that handler's generation) and re-emitted `SOURCE_MUTED` to the source's holders.
  - `unchanged` — already in the requested state; nothing broadcast or pushed.
  - `not_permitted` — the requester's token does not carry `MeetingRole::Host` (refused at the connection, before any actor hop) or its roster entry is not a host (refused again in the actor; the two are AND-composed). **CLIENT-INFLATABLE, exactly like `unchanged` on the client-mute counter and `accepted_unchanged` on the declarations counter**: the check is cheap and precedes any actor hop, so one non-host drives it at line rate. **Never put it in a ratio denominator, and never alert on its raw rate without per-connection normalisation.**
  - `unknown_target` — an authorized host named a participant not in this meeting.
  - `rate_limited` — a host's server-mute work exceeded its per-connection bucket (no looser than client mute's: burst 8, 4/s sustained; a compile-time assertion keeps it so). Host requests only: a non-host is refused before the bucket.
  - `actor_unavailable` — the meeting actor's mailbox is closed; environmental. Not reachable through the public test path; a stays-at-zero cell in the unit suite.
- **No alert, by decision.** A non-zero mute rate is healthy moderation, and `not_permitted` is client-inflatable; MH's `server_muted` drop reason records the same no-alert decision for the enforcement side.
- **Usage**: Is moderation working, and when a mute seems not to take, did MC even apply it? (Then read the state gauge below and MH's `server_muted` drops.)
- **Recorded in**: `webtransport/connection.rs::handle_server_mute_request` / `refuse_server_mute` via `observability/metrics.rs::record_server_mute_request`
- **Dashboard**: MC Media - Server Mute Requests (Client Media Signalling row)

### `mc_media_unmute_requests_total`
- **Type**: Counter
- **Description**: Participant `UnmuteRequest` dispositions — a server-muted participant asking the host(s) to lift its mute (story 2 R-10). NOTIFIES only; never clears a mute.
- **Labels**:
  - `outcome`: `relayed`, `not_server_muted`, `no_host_connected`, `rate_limited`, `actor_unavailable`
  - `key_custody`: single value `operator`
- **Cardinality**: 5, bounded by `UnmuteRequestOutcome::ALL`; zero-initialised
- **A separate family from `mc_media_server_mute_requests_total`, with the sharper reason recorded:** merged, `action="unmute"` would name a host LIFTING a mute and a participant ASKING to be lifted in one series — two operations by two principals behind one selector. No `action` label here: the request has one direction.
- **Counting unit: ONE INCREMENT PER REQUEST, never per host it is relayed to.** A per-host delivery failure is the LATER hop, counted by the outbound path (`mc_participant_outbound_messages_dropped_total{payload_kind="signaling_raw"}`) — disjoint from this series, so alert on at most one of them.
- **Failure predicate, positive: `outcome=~"rate_limited|actor_unavailable"`.** `relayed` is success; `not_server_muted` and `no_host_connected` are ROUTINE (`no_host_connected` is normal once the host has left a meeting that carries on). `not_server_muted` is client-driven: keep it out of every ratio denominator.
- **Usage**: Are muted participants' requests reaching a host?
- **Recorded in**: `webtransport/connection.rs::handle_unmute_request` via `observability/metrics.rs::record_unmute_request`
- **Dashboard**: MC Media - Unmute Requests (Client Media Signalling row)

### `mc_media_refusal_replies_suppressed_total`
- **Expected-empty**: yes — a legitimate client never reaches a reply burst, so this reads zero when healthy
- **Type**: Counter
- **Description**: A REFUSAL reply withheld by its path's reply-rate limiter
- **Labels**:
  - `surface`: `server_mute`, `capability`
  - `key_custody`: single value `operator`
- **Cardinality**: 2, bounded by `RefusalReplySurface::ALL`; zero-initialised
- **NOT A REFUSAL COUNT.** The refusal itself is counted once, on its own path — `mc_media_server_mute_requests_total{outcome="not_permitted"|"unknown_target"}` or a `mc_media_receive_capability_declarations_total` rejection outcome — whether or not a reply went out. These are deliberately DISJOINT views of one request: the refusal is about the request, the suppression about the reply. Suppression is therefore never an `outcome` value, which would break those counters' partition and under-count refusals exactly when a probe is most active.
- **EVERY SUPPRESSED REPLY IS COUNTED HERE; ONLY THE FIRST IS LOGGED PER CONNECTION. A SINGLE WARN DOES NOT MEAN A SINGLE SUPPRESSION.** The one-shot WARN only became safe *because* this counter exists; before story 2 task 12 the capability path's suppressed replies had a latched WARN and NO counter.
- **No alert, by decision**: client-inflatable by construction.
- **Usage**: Is some connection looping refused requests (an SDK bug, or a probe)?
- **Recorded in**: `webtransport/connection.rs::refuse_server_mute` and `reject_capability` via `observability/metrics.rs::record_refusal_reply_suppressed`
- **Dashboard**: MC Media - Suppressed Refusal Replies (Client Media Signalling row)

### `mc_media_server_muted_sources`
- **Type**: Gauge
- **Description**: How many participants MC believes are SERVER-muted (audio) right now, SUMMED across this pod's meetings
- **Labels**: `key_custody` (single value `operator`)
- **Cardinality**: 1
- **A STATE level, not an event rate.** Counting unit: server-muted participants in force on this MC pod — NOT mute events. A mute applied yesterday is still counted here; `mc_media_server_mute_requests_total` stays flat for it.
- **Present at zero from process start**, so ABSENT means "not up or not scraped", never "nobody is muted".
- **Recomputed and `set` from live state, never delta-maintained.** Each meeting actor writes its ABSOLUTE count into a census; the controller's health walk prunes meetings that no longer exist and re-sets the pod total (the load-bearing publish), and every mute change also publishes (freshness only). So an actor that dies without running its exit path cannot leave a phantom count behind.
- **Read it against MH's enforcement — at FLEET level, and directionally.** `sum(mc_media_server_muted_sources)` across MC instances against `sum(rate(mh_media_frames_dropped_total{reason="server_muted"}[5m]))` across MH instances. Per instance the comparison means nothing: MC and MH meeting sets do not nest (a meeting on mc-0 has edges on mh-0 and/or mh-1, and each handler serves both MCs). A level and a counter are NEVER divided. Gauge > 0 while the fleet drop rate is flat is only a CANDIDATE disagreement — first exclude the benign cause: a muted participant who is silent, disconnected, in grace or not yet on a handler drops nothing. Same class of caveat as `mc_media_generation_divergence`.
- **What it does NOT answer**: whether a mute is in force in ONE meeting. ADR-0036 §11 bars a per-meeting dimension; the meeting-scoped question is MC's state and the `mc.webtransport.connection` "Server mute decision" log.
- **Recorded in**: `actors/meeting_media.rs::MutedSourceCensus::publish` via `observability/metrics.rs::set_server_muted_sources`; boot zero in `webtransport/server.rs`
- **Dashboard**: MC Media - Server-Muted Sources (Client Media Signalling row)

### `mc_media_slot_view_emissions_total`
- **Type**: Counter
- **Description**: Dispositions of dirty participants in the meeting actor's slot-view flush (the server-driven re-emit of `SendDirective` + `StreamAssignments`, ADR-0036 §5/§6; story 2)
- **Labels**:
  - `outcome`: `sent`, `deferred`, `delivery_failed`, `composition_failed`
  - `key_custody`: single value `operator`
- **Cardinality**: bounded at the type level by `SlotViewEmission::ALL`
- **Increment boundary**: once per dirty participant per actor flush turn in which it is carried over (`deferred`), or taken up AND its view composed (`composition_failed`) or composed as CHANGED and handed over (`sent`, `delivery_failed`). Taken-up cases that produce NO disposition: a participant whose recomposed view is UNCHANGED; the two ROUTINE skips — disconnected-in-grace (a reconnect re-dirties it) and not-yet-declared (its declaration dirties it); and the three invariant-violation skips (no roster entry, no routing deps, no view), which are logged as a WARN at `mc.actor.meeting` ("Dirty participant has no state to flush", with a bounded `reason`) rather than counted.
- **THIS DOES NOT PARTITION EMISSIONS.** A participant deferred on three turns and sent on the fourth increments four times for one delivered view, so `sum()` is flush-turn dispositions, not views delivered, and a "deferral ratio" is per-turn, not per-participant. Every neighbouring entry in this family partitions; this one deliberately does not.
- **Success set `{sent, deferred}`; the failure predicate is stated POSITIVELY: `outcome=~"delivery_failed|composition_failed"`.** `deferred` is the ROUTINE shape of the bound under load — the actor takes at most `SLOT_VIEW_FLUSH_BATCH` dirty participants per turn and carries the rest, coalesced, never dropped — so `outcome!="sent"` would page on normal operation. Positive form for the same reason as `mc_media_mute_requests_total`: **a variant added later defaults to NOT being a failure** and has to be classified deliberately. Do not harmonise it to the negated form.
- **Remedies differ by value:**
  - `composition_failed` — **MC defect; any non-zero value is a bug.** Nothing was sent to that participant (fail closed).
  - `delivery_failed` — **environmental**: the participant actor was gone (mailbox closed) when the actor handed it the view. Disjoint from `mc_participant_outbound_messages_dropped_total{payload_kind="signaling_raw"}`, which is the LATER hop (stream channel full).
  - `deferred` — routine. A sustained high rate against a meeting's size means large meetings with heavy churn; the view still converges.
- **Shared denominator, recorded so it is not alerted on twice.** A composition failure increments BOTH this series (`composition_failed`) and `mc_media_send_directives_total{outcome=<stage>}`. `mc_media_send_directives_total` carries the STAGE dimension and is the alerting and triage home; `composition_failed` exists here only so the flush-turn disposition set is complete. Do not alert on both.
- **Usage**: Is the meeting actor's server-driven re-emit keeping every declared participant's view current, and is the per-meeting bound engaging?
- **Recorded in**: `actors/meeting_media.rs::flush` / `flush_one` via `observability/metrics.rs::record_slot_view_emission`
- **Dashboard**: MC Media - Slot View Emissions by Outcome (Client Media Signalling row)

### `mc_media_unreachable_senders_total`
- **Type**: Counter
- **Description**: Roster participants named unreachable across emitted `StreamAssignments` — a routable participant whose connected handler set is DISJOINT from the subscriber's (ADR-0036 §9; story 2 R-33, task 20)
- **Labels**:
  - `key_custody`: single value `operator`
- **Cardinality**: 1
- **Increment boundary**: by the length of `unreachable_sender_ids` on each `StreamAssignments` the actor SENDS — recorded only after the handover succeeded, alongside `mc_media_slot_view_emissions_total{outcome="sent"}`, so a delivery failure moves neither side of the ratio below. No sender, meeting or handler identity, ever.
- **EMISSION-WEIGHTED, NOT A POPULATION.** It rises with churn among partially-connected participants, and a stable meeting emits nothing. It does NOT answer "who cannot reach whom right now" — that is the connectivity INFO lines at `mc.actor.meeting` ("Participant media connectivity changed", "Participant in-edge handlers changed"), whose latest line per participant is its current state. Mean unreachable senders per emitted view: `rate(mc_media_unreachable_senders_total[5m]) / rate(mc_media_slot_view_emissions_total{outcome="sent"}[5m])`.
- **A non-zero rate is now a CONNECTIVITY-DEGRADATION signal, not a design-expected steady state.** Every client is offered every handler and co-location puts an all-connected meeting on one handler, so a healthy meeting emits zero here. Non-zero means some participants reached a strict subset of their handlers — rare, and the affected user hears silence from the peers named (see Scenario 18 in `docs/runbooks/mc-incident-response.md`).
- **Permanently zero on a single-handler deployment** — by construction, not a vacuous detector: with one handler every ROUTABLE participant is connected to it, and a participant not yet connected (or re-establishing after losing every connection) is never named unreachable — it is counted on `mc_media_not_yet_connected_senders_total` instead.
- **This field is a wire contract, not a courtesy.** `signaling.proto` requires the client to mark these roster entries distinctly; client consumption lands with story 2 task 20 (SDK) and the rendering with task 15 (UI). Its rarity is not a reason for a client to ignore it: an unreachable peer is otherwise wholly silent.
- **Usage**: How often participants are told part of the roster is unreachable — read as degraded media connectivity
- **Recorded in**: `actors/meeting_media.rs::flush_one` via `observability/metrics.rs::record_unreachable_senders`
- **Dashboard**: MC Media - Unreachable Senders Named (Client Media Signalling row)

### `mc_media_not_yet_connected_senders_total`
- **Type**: Counter
- **Description**: Roster members NOT routable — not yet connected to any handler, inside their connect settle window, or re-establishing after losing every connection — counted per emitted `StreamAssignments`, the composed subscriber ITSELF INCLUDED
- **Labels**:
  - `key_custody`: single value `operator`
- **Cardinality**: 1
- **The partner of `mc_media_unreachable_senders_total`**: that series is "silent because the connected sets are disjoint"; this one is "silent because someone is not connected yet". Together they partition MC-explained silence on a delivered view.
- **Includes the subscriber itself**, because a subscriber that is not connected receives an all-`fewer_sources_than_slots` view that is byte-identical to a healthy small meeting's on `mc_media_slot_states_total` — counting only its peers would leave that case invisible. **This mixes two populations in one series** (the subscriber's own contribution of at most 1, and its peers' of up to N); no split is kept.
- **NOT expected-empty** — do not tighten it into one. Every join passes through this state until the handlers report the participant's connections, so the series is non-zero on ordinary churn. **Persistence, not rate, is the fault signal**: a participant that passes through the state at join is healthy; one that stays in it is the fault (its client reached no handler, or MH→MC notifications are not arriving). The rate alone cannot tell those apart — the connectivity INFO lines and `mc_mh_notifications_unapplied_total` can. Any future alert needs a duration-over-threshold shape (an operations decision).
- **Emission-weighted**: normalise by `mc_media_slot_view_emissions_total{outcome="sent"}`.
- **Usage**: Is MC silent toward participants because their media connectivity has not been observed?
- **Recorded in**: `actors/meeting_media.rs::flush_one` via `observability/metrics.rs::record_not_yet_connected_senders`
- **Dashboard**: MC Media - Not-Yet-Connected Senders (Media Connectivity row)

### `mc_media_send_targets_total`
- **Type**: Counter
- **Description**: Send targets carried on each emitted `SendDirective`, summed over its streams
- **Labels**:
  - `key_custody`: single value `operator`
- **Cardinality**: 1
- **The co-location objective, made observable.** A sender sends to every handler owning one of its edges (ADR-0036 §9 multi-handler send), and MC's edge policy minimises that. Mean targets per emitted directive: `rate(mc_media_send_targets_total[5m]) / rate(mc_media_send_directives_total{outcome="emitted"}[5m])` — ~1 when participants reach every handler; above 1 only with genuinely partial connectivity. A sustained rise with healthy connectivity means co-location has degraded, which shows up otherwise only as client uplink cost.
- **Only CHANGED directives are recorded** (same boundary as `emitted`), so this is emission-weighted too.
- **Usage**: Are senders being spread across handlers more than connectivity requires?
- **Recorded in**: `actors/meeting_media.rs::flush_one` via `observability/metrics.rs::record_send_targets`
- **Dashboard**: MC Media - Mean Send Targets per Directive (Media Connectivity row)

### `mc_media_edge_moves_total`
- **Type**: Counter
- **Description**: Publisher→subscriber edges that changed handler between two renders, by why
- **Labels**:
  - `reason`: `connectivity_change`, `unexpected`
  - `key_custody`: single value `operator`
- **Cardinality**: bounded by `EdgeMove::ALL`
- **Per-value expectations differ.** `connectivity_change` is routine: an edge's handler left one party's connected set while another shared handler remained, so the edge moved (keeping its slot and its sender). **`unexpected` is expected-empty and alertable at `> 0`**: an edge moved while BOTH parties are still connected to its old handler — an edge-stability invariant violation (ADR-0036 §9 as implemented in task 20; the same principle as R-4 slot stability). The ERROR line at `mc.actor.meeting` names both handler ids.
- **Detected independently of the code it checks**: a diff of consecutive renders in `reconcile`, not a counter inside the slot table's mutation path, so a regression there cannot also hide its own evidence.
- **Usage**: Edge churn from connectivity changes; any `unexpected` is an MC defect to capture and escalate
- **Recorded in**: `actors/meeting_media.rs::record_edge_moves` via `observability/metrics.rs::record_edge_move`
- **Dashboard**: MC Media - Edge Moves by Reason (Media Connectivity row; `unexpected` red)

### `mc_media_connect_settles_total`
- **Type**: Counter
- **Description**: Participants' connectivity episodes settling, by how
- **Labels**:
  - `outcome`: `complete` (connected to every handler of its meeting before the window's end), `window_elapsed` (the settle window ended with a strict subset)
  - `key_custody`: single value `operator`
- **Cardinality**: bounded by `SettleOutcome::ALL`
- **Per EPISODE, so it IS a population**: `window_elapsed` counts participants that did not reach every handler within `MC_MEDIA_CONNECT_SETTLE_MS` — the operator's countable answer to "how many participants failed to connect everywhere". Read against `mc_media_connect_settle_window_seconds`.
- **The window is a masking mechanism, so it is counted.** While a participant is establishing, MC withholds routing (no edges, not unreachable) rather than act on a partial set; without the window, staggered connects would pin every edge involving the participant to its first handler permanently (see `media_routing/connectivity.rs`). `window_elapsed` is the visible edge of that mask.
- **Usage**: Scenario 18 Step 0's population signal; a rising `window_elapsed` share is participants that cannot reach every handler
- **Recorded in**: `actors/meeting_media.rs::sync_routing` via `observability/metrics.rs::record_connect_settle`
- **Dashboard**: MC Media - Connect Settles by Outcome (Media Connectivity row)

### `mc_media_connect_settle_window_seconds`
- **Type**: Gauge
- **Description**: The connect settle window this pod enforces (`MC_MEDIA_CONNECT_SETTLE_MS`), in seconds
- **Labels**:
  - `key_custody`: single value `operator`
- **Cardinality**: 1
- **A CONFIG ECHO**, set once at boot from the same `ClientMediaConfig::connect_settle_window` the meeting actors enforce, so the published and enforced windows cannot drift (the `mc_media_receive_slot_cap` pattern). Env-test `27_mc_slot_placement.rs` derives its settle wait from this value.
- **Cost it names**: time-to-first-audio for a participant that does not reach every handler is delayed by up to this window; it never appears in `mc_session_join_duration_seconds` (which stops at the JoinResponse).
- **Recorded in**: `webtransport/server.rs::WebTransportServer::new` via `observability/metrics.rs::set_connect_settle_window`
- **Dashboard**: MC Media - Connect Settle Window (Media Connectivity row, stat panel)

### `mc_media_handler_set_divergence_total`
- **Expected-empty**: yes — any non-zero value is an MC-internal invariant violation
- **Type**: Counter
- **Description**: Joins whose handler assignment (read from Redis) differed from the meeting's handler set frozen at its first join
- **Labels**:
  - `key_custody`: single value `operator`
- **Cardinality**: 1
- **The frozen set is the meeting's authority.** The actor keeps it: no later join and no changed Redis entry can widen a live meeting's handler set or move anyone's edges. A non-zero value means Redis and the actor disagree — capture and escalate as an MC defect; restarting nothing fixes it. The ERROR line at `mc.actor.meeting` says the same.
- **Recorded in**: `actors/meeting_media.rs::install` via `observability/metrics.rs::record_handler_set_divergence`
- **Dashboard**: MC Media - Handler Set Divergence (Client Media Signalling row)

### `mc_media_receive_slot_cap`
- **Type**: Gauge
- **Description**: The configured maximum receive slots per declaration (`MC_MAX_RECEIVE_SLOTS`)
- **Labels**:
  - `key_custody`: single value `operator`
- **Cardinality**: 1
- **A CONFIG ECHO, NOT A UTILISATION GAUGE.** Set once, when the WebTransport server is built, from the very `ClientMediaConfig::max_receive_slots` every connection's capability parse compares a declaration against — so the published cap and the enforced cap are one value. There is no numerator: do not build a "slots used / cap" ratio on it.
- **Usage**: Read beside `mc_media_receive_capability_declarations_total{outcome="slot_count_over_cap"}` — a rising over-cap rate with a cap below the client's configured N is a configuration mismatch, not a client bug (R-1)
- **Recorded in**: `webtransport/server.rs::WebTransportServer::new` via `observability/metrics.rs::set_receive_slot_cap`
- **Dashboard**: MC Media - Receive Slot Cap (Client Media Signalling row, stat panel)

### `mc_media_unmatched_plan_slots_total` — RETIRED (story 2)
Retired rather than left as a permanent zero. It counted egress plans for a slot the subscriber never declared; since story 2 plans are DERIVED from the declared slots, so the count is identically zero on every path — a detector that structurally cannot observe anything, and a flat panel that reads as coverage. The invariant it guarded now lives in `media_routing::slots`' exhaustive model check, which fails the build instead. Its story-1 twin, `slot_id_not_planned`, retired with it. **Re-add condition**: if story 5's MH-selects shape returns and the forwarding plan decouples from the declaration again, re-derive the counter from that decoupling — not from this entry's old rationale.

---

## Participant Outbound Delivery Metrics

### `mc_participant_outbound_messages_dropped_total`
- **Expected-empty**: yes — a healthy outbound path drops nothing, so this reads zero when healthy
- **Type**: Counter
- **Description**: Server messages dropped because a participant's outbound channel was full or closed
- **Labels**:
  - `payload_kind`: `signaling_raw`, `participant_update_joined`, `participant_update_left`, `participant_update_muted`, `meeting_kek_update`
- **Cardinality**: bounded by one constant per `try_send` site in `actors/participant.rs` plus the roster-update labels `webtransport/handler.rs::encode_participant_update` returns with each wire-visible update (no restated count: it would be the third value of this line in one story)
- **`participant_update` was SPLIT into `_joined` / `_left`** (story 2 task 9). The rule applied is **demonstrated consumer need, not message-type taxonomy**: a named consumer — the client's R-18 rebind correlation — cannot do its job with the two merged, because a dropped LEAVE has a consequence a dropped join does not (the client keeps a stale roster entry and later counts a legitimate `sender_id` reissue as a rebind, `dt_client_media_roster_key_rebinds_total{outcome="rebind"}`). Non-zero `participant_update_left` over a window makes that explanation **supported** — not confirmed for any single rebind increment, since this counter is fleet-wide. `signaling_raw` also spans several message types and **stays merged**: that is the same rule returning the other answer, not an unfinished half of this split. The shared stem means `payload_kind=~"participant_update.*"` still recovers the old merged series.
- **`participant_update_muted` was split out at story 2 task 12, on the same DEMONSTRATED-CONSUMER-NEED rule.** A dropped `ParticipantMuteUpdate` has no re-sync path: join/leave state is rebuilt from the roster, but who-muted-whom is delivered ONLY by the live server-mute broadcast and the late-joiner replay, so a drop leaves that client rendering a wrong mute indicator until the next mute change on that participant — which may never come. Distinct consequence, distinct remedy. Past participle, matching `_joined` / `_left`, so `payload_kind=~"participant_update.*"` still recovers the merged series.
- **`meeting_kek_update`** is the third `try_send` site: a KEK push the outbound channel dropped. The same event is also `mc_meeting_kek_pushes_total{outcome="dropped_outbound"}` — two angles on one event, one as outbound loss, one as a rotation that did not reach a member.
- **The client did not receive something MC decided to send.**
- **EVERY DROP IS COUNTED HERE; ONLY THE FIRST IS LOGGED PER CONNECTION. A SINGLE WARN DOES NOT MEAN A SINGLE DROP.** The WARN at `mc.actor.participant` fires once per participant actor and is deliberately not repeated — a per-message log on a client-drivable path is a log-amplification vector, and a wedged outbound channel drops one message per roster broadcast, i.e. O(participants x events) from one bad connection. **So log-line volume understates drop volume by orders of magnitude, and this counter is the only complete record.** Triaging by `grep` first — which is what people actually do — shows one line and reads as an isolated blip; the truth is the opposite. Take the magnitude from here, never from the log.
- The one-shot WARN only became safe *because* this counter exists: before it, the repeated line **was** the record.
- **Deliberately a NEW metric, not a fourth `actor_type` on `mc_messages_dropped_total`.** That metric is fed by `MailboxMonitor` on the **inbound** actor path; a pseudo-`actor_type` value here would corrupt the `topk` on the cross-service `errors-overview.json` dashboard by mixing two different quantities.
- **No `key_custody` label**, deliberately: this is the generic outbound signalling choke point, not a media-path metric. Fleet-wide `key_custody` rollout is R-26 / story task 22. The key is not `reason` either — that key is spoken for by the frame-reject vocabulary.
- **RECIPROCAL WITH `mc_media_send_directives_total`.** That counter's `emitted` means a directive was **composed**, not delivered — it fires before the send. `payload_kind="signaling_raw"` is the stream-channel-FULL half of the evidence for what happened next; the participant-mailbox-CLOSED half is `mc_media_slot_view_emissions_total{outcome="delivery_failed"}` (a different, earlier hop — the two are disjoint). **A non-zero `signaling_raw` rate against a healthy `emitted` rate is the specific shape of "MC composed a send directive the client never received".**
- **Usage**: Detect a slow or wedged client connection losing server messages
- **Recorded in**: `actors/participant.rs::handle_send`, `handle_update` and `handle_kek_update` via `observability/metrics.rs::record_participant_outbound_dropped`
- **Dashboard**: MC Media - Dropped Outbound Messages (Client Media Signalling row)

---

## MH Coordination Metrics (R-15, R-20)

### `mc_mh_notifications_received_total`
- **Type**: Counter
- **Description**: Total MH→MC participant connection/disconnection notifications received
- **Labels**:
  - `event_type`: Notification event type (`connected`, `disconnected`)
- **Cardinality**: Low (2 event types)
- **Since story 2 task 20 these notifications ARE the source of truth for media visibility**: a participant hears a sender only if both are connected to a common handler, and "connected" is exactly what these notifications report (ADR-0036 §9). A stall here means participants silently hear nobody — while every slot state reads as a small meeting. Read with `mc_mh_notifications_unapplied_total` (notifications that did not become connectivity) and `mc_media_not_yet_connected_senders_total`.
- **Usage**: MH→MC notification volume; a flat `connected` rate while joins continue is the "nobody hears anybody" shape
- **Recorded in**: `grpc/media_coordination.rs` on notification receipt
- **Dashboard**: MC Overview - MH Notifications by Event (MH Coordination row)

### `mc_mh_notifications_unapplied_total`
- **Type**: Counter
- **Description**: MH notifications that did not become (or change) participant connectivity, by reason
- **Labels**:
  - `reason`: `meeting_unknown`, `participant_unknown`, `user_ambiguous`, `handler_not_in_set`, `unknown_connection`, `connection_bound_refused`, `retired_connection`
  - `key_custody`: single value `operator`
- **Cardinality**: bounded by `Unapplied::ALL`. Shares its first three spellings with `mc_media_sender_binding_responses_total` deliberately (connect path).
- **Per-value expectations — read per value, never as one rate**:
  - `unknown_connection` — **ROUTINE** (disconnect path): every connection MC declined to bind sends a Disconnected for a key MC never recorded; also a Disconnected naming a `sub` that resolves to no single roster entry. Never an error to MH.
  - `participant_unknown` — low-rate race (connect overtaking join). The connect is then forgotten for connectivity too; MH declines the session and the client's reconnect brings a fresh notification.
  - `user_ambiguous` — one user with two roster entries: connectivity recorded for NEITHER (fail closed). Never self-clears.
  - `meeting_unknown` — MH notifying about a meeting MC does not hold (both events).
  - `handler_not_in_set` — the `handler_id` is not byte-identical to a handler of the meeting's frozen set. **Sustained, it means an MH process restarted under a new id and story-4 re-registration has not happened**; the meeting's stale connectivity to the dead id does not clear on its own (Scenario 18).
  - `connection_bound_refused` — **expected-empty, alertable at `> 0`**: a participant exceeded the live-connection bound on one handler (a reconnect storm or leaked sessions).
  - `retired_connection` — a Connected delivered after its own Disconnected (MH abandoned the Connected, then sent the decline Disconnected); refused by MC's tombstone. Rare; non-zero means MH→MC RPCs are slow enough to be abandoned.
- **No meeting, participant, handler or connection identity.** `connection_id` is unbounded (one per MH session) and appears in the INFO line at `mc.grpc.media_coordination` as a correlation field only.
- **Usage**: Every way MH-reported connectivity failed to become routing input
- **Recorded in**: `grpc/media_coordination.rs` via `observability/metrics.rs::record_notification_unapplied`
- **Dashboard**: MC Media - MH Notifications Unapplied by Reason (Media Connectivity row; per-reason colours)

### `mc_mh_notifications_without_connection_id_total`
- **Type**: Counter
- **Description**: MH notifications carrying an EMPTY `connection_id` (an MH image that predates the field)
- **Labels**:
  - `key_custody`: single value `operator`
- **Cardinality**: 1
- **The degraded legacy path, counted so it is not silent.** An empty id maps to one implicit key per (participant, handler) — exactly the pre-field semantics, including the stale-disconnect hazard `connection_id` exists to close. Non-zero means an old MH is still in the fleet (a rolling deploy, or a stuck pod). **Retirement condition**: once this has been flat for a full MH rollout, the legacy branch in `media_routing/connectivity.rs` is deleted.
- **Usage**: Is any MH still running without `connection_id`?
- **Recorded in**: `grpc/media_coordination.rs` via `observability/metrics.rs::record_notification_without_connection_id`
- **Dashboard**: MC Media - Notifications Without Connection Id (Media Connectivity row)

---

### `mc_media_end_meeting_total`
- **Type**: Counter
- **Description**: Outcome of releasing a meeting on ONE media handler (`EndMeeting`, story 2 R-20) — when the meeting ENDED, and best-effort on graceful MC shutdown
- **Labels**:
  - `outcome`: `released`, `rejected_ownership`, `superseded_by_successor`, `unimplemented`, `unavailable_exhausted`, `invalid_argument`, `error`
  - `key_custody`: single value `operator`
- **Cardinality**: 7, bounded by `EndMeetingOutcome::ALL`; zero-initialised over the full set
- **COUNTING UNIT: ONE INCREMENT PER (MEETING, HANDLER)**, never per meeting — a handler missed from the release would otherwise be invisible. Every handler of the meeting's frozen set is released, including one never pushed to.
- **No handler label** — the handler id is a per-incarnation token (see this section's intro); the `mc.teardown` log line carries `mh_id` for the one-handler-vs-all question.
  - `released` — acknowledged: released, OR the meeting was unknown on that handler (one outcome by design; MC's action is identical).
  - `rejected_ownership` — `FAILED_PRECONDITION` on a teardown for a meeting that ENDED (emptied or closed): another MC's `mc_id` holds the meeting. **An MC defect: any non-zero value is a bug** — MC only sends its own id, and a release is sent only after the meeting's handlers are no longer being re-assigned (GC is told the meeting ended AFTER the release). Terminal; never retried. **The shutdown population was split out into `superseded_by_successor` on purpose** (story 2 task 12, O-24): folded in here it would fire this value's alert on every rolling deploy, get silenced, and hide the defect. Do not merge them back.
  - `superseded_by_successor` — `FAILED_PRECONDITION` on a GRACEFUL-SHUTDOWN teardown. The expected cause is a successor MC that already re-registered the meeting during a rollout: MH's ownership check refusing a release that would have cut a live meeting. **Consistent with supersession, not proof of it**: a wrong `mc_id` sent during shutdown lands on this value too and is indistinguishable at the recording site. Decided from MC's own teardown reason, never inferred from MH's reply. Expected non-zero after deploys; rising OUTSIDE a rollout window is worth a look (confirm mixed MC images as `mh-incident-response.md` does for `no_generation`). **No alert**, recorded: alerting would page on every deploy. Zero-initialised, so a flat baseline is distinguishable from an absent series.
  - `unimplemented` — the handler predates the RPC: a rollout in the wrong order (MH rolls forward first, MC rolls back first). Non-fatal, never retried; that handler keeps the meeting until it restarts.
  - `unavailable_exhausted` — transport, `UNAVAILABLE` or `DEADLINE_EXCEEDED` on every attempt (a release is idempotent, so these are retried, bounded). The meeting leaks on that handler exactly like a crash.
  - `invalid_argument`, `error` — terminal; not expected from MC's own values.
- **Cross-hop counterpart — for COMPARISON only, never a sum or a ratio**: `mh_media_meeting_teardowns_total{outcome="rejected_ownership"}` (`docs/observability/metrics/mh-service.md`). **The correspondence is ASYMMETRIC**: MH cannot tell why an MC was tearing down, so MH's `rejected_ownership` corresponds to MC's `rejected_ownership` **plus** `superseded_by_successor`, not to the like-named MC value alone. Comparing token to token shows a gap that grows on every rolling deploy and looks exactly like lost MC→MH messages. And the denominators differ even for the union — MC counts per (meeting, handler) terminal outcome, MH per request received (including retries of retryable failures) — so different denominators; do not sum, and do not build a ratio across them.
- **Usage**: Are ended meetings being released on their handlers? A sustained rate of failure outcomes (anything but `released` and `superseded_by_successor`) means MH edge budget and `MH_MAX_REGISTERED_MEETINGS` slots are leaking.
- **THE FAILURE PREDICATE IS STATED POSITIVELY**: `outcome=~"rejected_ownership|unimplemented|unavailable_exhausted|invalid_argument|error"`, **not** `outcome!~"released|superseded_by_successor"`. This section's standing rule (see `mc_media_mute_requests_total`) is that a negated predicate is correct only where the SUCCESS set is closed and small. **This metric's success set is demonstrably not closed** — it grew from `{released}` to `{released, superseded_by_successor}` inside one devloop, and under a negated predicate that new benign value would have joined the failure set silently and paged on every rolling deploy. So an eighth value defaults to NOT a failure and must be classified deliberately. **The classification obligation is recorded at `EndMeetingOutcome` in `media_routing/teardown.rs`**, because that is where a variant is added and nothing in Rust connects it to a PromQL rule.
- **`superseded_by_successor` is in NEITHER the numerator NOR the denominator** of `MCEndMeetingFailureRate`. **The denominator is `released` + the five failure values** — the releases that were MC's to make. It leaks nothing, and it arrives in bursts on every rolling deploy: left in the denominator it would dilute the ratio exactly while deploys are happening, the fail-quiet direction, masking a real failure during a rollout. (`released` stays in, as the non-zero-denominator guard requires.)
- **Alerts**: `MCEndMeetingOwnershipRejected` (`> 0`, `rejected_ownership` only), `MCEndMeetingFailureRate` (positive predicate, above). `superseded_by_successor` has **no alert**, recorded.
- **Recorded in**: `media_routing/teardown.rs::run` via `observability/metrics.rs::record_end_meeting`
- **Dashboard**: MC Overview - EndMeeting Outcomes (MH Coordination row)

### `mc_media_push_quiesce_total`
- **Type**: Counter
- **Description**: How one meeting teardown's DRAIN of its push workers ended — every in-flight `RegisterMeeting` must have RETURNED before `EndMeeting` goes out (`EndMeetingRequest`'s quiesce MUST)
- **Labels**:
  - `outcome`: `quiesced`, `timed_out`
  - `key_custody`: single value `operator`
- **Cardinality**: 2, bounded by `QuiesceOutcome::ALL`; zero-initialised
- **COUNTING UNIT: ONE INCREMENT PER MEETING TEARDOWN** (contrast `mc_media_end_meeting_total`, per (meeting, handler)). That is why the two are separate metrics: "the drain timed out" and "released on every handler" are routinely both true.
- **Unlike most of this family, this IS a clean denominator**: both values are server-driven and neither is client-inflatable, so `timed_out / (quiesced + timed_out)` is a legitimate ratio.
- **`timed_out` is the LEADING indicator of the edge-budget ratchet.** The drain waited `push_quiesce_bound_seconds` (MC's startup line), then waited once more before releasing (so an in-flight registration's deadline expires first), and still released. A late apply that was already queued at MH is refused there; a registration still in flight is only ORDERED by that wait — a property of the RPC layer that fails open. See the EndMeeting scenario in `docs/runbooks/mc-incident-response.md`.
- **Alert**: `MCPushQuiesceTimeouts` (warning, `> 0`)
- **Recorded in**: `media_routing/teardown.rs::quiesce` via `observability/metrics.rs::record_push_quiesce`
- **Dashboard**: MC Overview - Push Quiesce Outcomes (MH Coordination row)

### `mc_media_teardown_fence_backstop_total`
- **Expected-empty**: yes — reads zero forever on a healthy MC
- **Type**: Counter
- **Description**: A teardown fence lifted by its DEADLINE rather than by the teardown reporting completion
- **Labels**: `key_custody` (single value `operator`)
- **Cardinality**: 1; zero-initialised
- The controller FENCES a meeting id while its previous incarnation is being released (a create for it waits), because `EndMeeting` releases by meeting id alone and would otherwise release a re-created meeting. Completion is reported from a `Drop` guard on EVERY exit of the teardown task, a panic included — so this fires only if that task HUNG or vanished without unwinding. When it fires, creates for that id proceed, a still-running release may land on the new incarnation, and — for a meeting that ENDED rather than one removed at shutdown — GC is told it ended (the cause is recorded when the fence is raised), so the id stays re-assignable rather than stuck on `meeting_not_found`.
- **NOT mutually exclusive with `mc_media_push_quiesce_total{outcome="timed_out"}`**: a wedged teardown can record both. A backstop WITHOUT a matching timeout is a different fault — the task hung somewhere other than the drain.
- **Alert**: `MCTeardownFenceBackstop` (warning, `> 0`)
- **Recorded in**: `actors/controller.rs::teardown_complete` via `observability/metrics.rs::record_teardown_fence_backstop`
- **Dashboard**: MC Overview - Teardown Fence Backstops (MH Coordination row)

---

## Participant MH-Status Metrics (R-60)

Post-join client→MC reporting plane: a browser client sends
`ClientMessage{MediaConnectionUpdate}` reporting its per-MH connection outcomes,
which MC records on the participant actor.

### `mc_participant_mh_status_total`
- **Type**: Counter
- **Description**: Total per-MH connection statuses recorded from client
  `MediaConnectionUpdate` messages (one increment per status entry recorded)
- **Labels**:
  - `state`: Reported MH connection state — `connected`, `failed`,
    `disconnected`, `unspecified` (the proto enum is total-matched; an unknown
    wire value clamps to `unspecified`, never panics or drops)
- **Cardinality**: Low (4 states, bounded by the `MhState` enum)
- **NOT the connectivity source for visibility.** These statuses are
  CLIENT-ASSERTED and diagnostic only: MC never reads them to decide who hears
  whom, never narrows a participant's connectivity from them, and never takes a
  url from them (a client-controlled url reaching a send target is a redirect
  primitive). Connectivity is what the HANDLERS report —
  `mc_mh_notifications_received_total`. Since story 2 task 20.
- **Usage**: Observe client-perceived MH reachability; a rising `failed` share
  is an early signal of MH-edge connectivity problems the client sees before MC
  does
- **Recorded in**: `actors/participant.rs::handle_record_mh_statuses`, driven
  from `webtransport/connection.rs` `handle_media_connection_update`
- **Dashboard**: MC Overview - Client-Reported MH Status by State

### `mc_participant_mh_status_dropped_total`
- **Expected-empty**: yes — drops are cap/over-limit refusals of client-reported status, zero on a well-behaved client
- **Type**: Counter
- **Description**: Total per-MH status drops on the client→MC reporting path,
  by reason. Two independent security bounds:
  - `cap`: a per-entry drop when the participant is already tracking the
    per-participant cap (`MAX_MH_STATUSES_PER_PARTICIPANT = 16`) distinct MH
    URLs. Updates to already-tracked MH URLs are always allowed and never
    counted — recorded and cap-dropped are mutually exclusive per entry.
  - `over_limit`: a per-MESSAGE drop when a single `MediaConnectionUpdate`
    carries more than `MAX_MH_STATUSES_PER_UPDATE = 64` statuses (input
    amplification bound); the excess is refused before any per-entry work.
- **Labels**:
  - `reason`: Drop reason — `cap` (per-entry, map full) or `over_limit`
    (per-message, oversized batch)
- **Cardinality**: Low (2 reasons)
- **Usage**: Detect a client flooding distinct MH URLs (`cap`) or packing an
  oversized batch into one frame (`over_limit`) — both abuse/bug signals. No
  client-supplied strings are logged on either reject path (security control),
  so this counter is the only signal.
- **Recorded in**: `actors/participant.rs::handle_record_mh_statuses` (`cap`);
  `webtransport/connection.rs::handle_media_connection_update` (`over_limit`)
- **Dashboard**: MC Overview - Dropped Client MH Status (cap)

---

## Participant Departure Metrics (task #64 — roster leave latency)

These two counters make the disconnect/leave path observable, and distinguish a
clean tab-close from a crash/network-loss departure. Both labels are bounded
`&'static str`s mapped from internal enums via EXHAUSTIVE matches (a new variant is
a compile error, never an unbounded or mislabeled series); neither is ever a
client-controlled string, `mh_url`, participant-id, or raw close-reason text.

### `mc_participant_disconnects_total`
- **Type**: Counter
- **Description**: Total participant transport disconnects, by transport-authenticated
  cause, recorded at the moment the departure is detected (before the immediate-vs-grace
  decision). Derived ONLY from the WebTransport `Connection::closed()` classification.
- **Labels**:
  - `cause`: `client_closed` (clean tab/app close → immediate removal, grace skipped),
    `connection_lost` (idle-timeout/abrupt loss → grace period kept for reconnection),
    `server_initiated` (local close: shutdown/drain/already-handled leave)
- **Cardinality**: Low (3 causes)
- **Usage**: Observe the idle-timeout path directly (otherwise hidden inside
  `leaves{reason="timeout"}`), verify the configured `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS`
  bound is firing, and derive reconnect rate:
  `disconnects{connection_lost} − leaves{timeout} ≈ reconnected within grace`.
  A `client_closed` disconnect always pairs with a `leaves{reason="voluntary"}`; a
  `connection_lost` disconnect resolves LATER as either a reconnect or a
  `leaves{reason="timeout"}`.
- **Recorded in**: `actors/meeting.rs::handle_disconnect`

### `mc_participant_leaves_total`
- **Type**: Counter
- **Description**: Total participant roster REMOVALS — one per `ParticipantLeft`
  broadcast — by leave reason. Emitted at the single roster-removal choke-point, so a
  `Left` broadcast can never occur without a matching increment.
- **Labels**:
  - `reason`: `voluntary` (clean transport close OR explicit leave — grace skipped),
    `timeout` (disconnect grace period expired), `removed` (host-removed),
    `meeting_ended` (meeting ended)
  - **Emission honesty (ADR-0032 expected-vs-enforced):** `removed` is RESERVED —
    the enum variant, the `Kicked` wire-map (`handler.rs`), and the exhaustive label
    map are in place, but there is NO production emission site yet (no host-remove
    handler routes through `remove_and_broadcast_left`). Today only `voluntary`,
    `timeout`, and `meeting_ended` are actually emitted; `removed` will light up when
    the host-remove path lands.
- **Cardinality**: Low (4 reasons; `removed` reserved, see above)
- **Usage**: A clean tab-close shows as `voluntary`; a crash/network-loss that did not
  reconnect shows as `timeout`. A rising `timeout` share is the primary signal of
  involuntary departures (network issues / crashes) — see the disconnect-reason runbook
  section.
- **Recorded in**: `actors/meeting.rs::remove_and_broadcast_left` (voluntary/timeout/removed)
  and `actors/meeting.rs::handle_close_meeting` (meeting_ended — the MC-side meeting
  lifecycle close, renamed from `handle_end_meeting` in story 2 task 12 so that
  `EndMeeting` names only the MC→MH release RPC, which records no leave reason)

### `mc_join_display_name_resolved_total`
- **Type**: Counter
- **Description**: How a joining participant's roster display name was resolved — one per
  successful join, at the `handle_join` name sink. Complements the AC-side
  `ac_meeting_token_display_name_total`, which covers only the token ISSUANCE vector: this
  metric makes the MC CONSUMER side observable, so a name lost between issuance and roster
  render (e.g. an old `#[serde(default)]`-empty token) does not degrade silently.
- **Labels**:
  - `outcome`: `present` (the validated meeting-token claim carried a display name, used
    verbatim), `fallback` (the claim was empty → the generic `Participant N` label was
    substituted)
  - The label is one of two fixed `&'static str` literals; it is NEVER the display-name
    value itself (display_name is PII and must never become a metric label).
- **Cardinality**: Low (2 outcomes)
- **Usage**: A rising `fallback` share is the "fail loudly" signal that registered names are
  not reaching the roster (a truncation/empty-claim edge, or stale tokens minted before the
  display-name feature). Under healthy issuance this should be almost entirely `present`.
- **Recorded in**: `actors/meeting.rs::handle_join`

**Worst-case roster-remove latency (SSoT — derived from config, not a fixed number):**
- Clean tab-close: ≈ network RTT (sub-second). `Connection::closed()` fires immediately
  and the grace period is skipped.
- Crash / network-loss: `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS` + `MC_DISCONNECT_GRACE_PERIOD_SECONDS`
  + the 5s grace-check interval. With defaults: `10 + 30 + 5 = 45s`. See the MC incident
  runbook for the operational budget + PromQL.

---

## Token Manager Metrics (ADR-0010 Section 4a)

### `mc_token_refresh_total`
- **Type**: Counter
- **Description**: Total token refresh attempts
- **Labels**:
  - `status`: Refresh outcome (`success`, `error`)
- **Cardinality**: Low (2)
- **Usage**: Track token refresh rate and success
- **Recorded in**: `token_manager.rs` after refresh attempt completes

### `mc_token_refresh_duration_seconds`
- **Type**: Histogram
- **Description**: Token refresh operation duration
- **Labels**: None
- **Buckets**: [0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000]
- **Cardinality**: 1
- **Usage**: Monitor token refresh latency

### `mc_token_refresh_failures_total`
- **Expected-empty**: yes — every series is a token-refresh failure, so a healthy token manager reads zero
- **Type**: Counter
- **Description**: Token refresh failures by error type
- **Labels**:
  - `error_type`: Failure type (`http`, `auth_rejected`, `invalid_response`, `acquisition_failed`, `configuration`, `channel_closed`)
- **Cardinality**: Low (6)
- **Alert**: High rate indicates AC connectivity issues
- **Recorded in**: `token_manager.rs` on refresh failure only

---

## gRPC Auth Layer 2 Metrics (ADR-0003)

### `mc_caller_type_rejected_total`
- **Expected-empty**: yes — any caller-type rejection is a misconfiguration/bug, so this reads zero when healthy
- **Type**: Counter
- **Description**: Total Layer 2 service_type routing rejections (valid token, wrong caller for gRPC service)
- **Labels**:
  - `grpc_service`: Target gRPC service name (`MeetingControllerService`, `MediaCoordinationService`)
  - `expected_type`: Expected service_type for the gRPC service (`global-controller`, `media-handler`)
  - `actual_type`: Caller's `service_type` claim, **clamped** at the emit site (`common::service_type::service_type_metric_label`) to the recognized identities (`global-controller`, `media-handler`, `meeting-controller`), plus `unknown` (claim absent) and `other` (present-but-unrecognized — a forged/off-spec claim). A recognized-but-wrong identity keeps its real value; only genuinely-unrecognized strings collapse to `other`.
- **Cardinality**: Low — `actual_type` is bounded to 5 values by the emit-site clamp (3 identities + `unknown` + `other`), independent of the peer-controlled claim; `grpc_service` (2) and `expected_type` (2) are path-fixed literals.
- **Alert**: ANY non-zero value indicates a bug or misconfiguration
- **Usage**: Detect service-to-service routing errors, misconfigured tokens
- **Recorded in**: `grpc/auth_interceptor.rs` on Layer 2 rejection
- **Dashboard**: MC Overview - Caller Type Rejections (ADR-0003 Layer 2)

---

## Token Manager Metrics (ADR-0010 Section 4a)

### `mc_token_refresh_total`
- **Type**: Counter
- **Description**: Total token refresh attempts
- **Labels**:
  - `status`: Refresh outcome (success, error)
- **Cardinality**: 2
- **Usage**: Track token refresh rate and success
- **Example**:
  ```promql
  rate(mc_token_refresh_total{job="mc-service", status="error"}[5m])
  ```

### `mc_token_refresh_duration_seconds`
- **Type**: Histogram
- **Description**: Token refresh operation duration
- **Labels**: None (aggregated)
- **Buckets**: [0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000]
- **Cardinality**: 1
- **Usage**: Monitor token refresh latency
- **Example**:
  ```promql
  histogram_quantile(0.99,
    sum(rate(mc_token_refresh_duration_seconds_bucket{job="mc-service"}[5m])) by (le)
  )
  ```

### `mc_token_refresh_failures_total`
- **Type**: Counter
- **Description**: Token refresh failures by error type
- **Labels**:
  - `error_type`: Type of failure (http, auth_rejected, invalid_response, acquisition_failed, configuration, channel_closed)
- **Cardinality**: 6
- **Alert**: High rate indicates AC connectivity issues
- **Example**:
  ```promql
  sum(rate(mc_token_refresh_failures_total{job="mc-service"}[5m])) by (error_type)
  ```

---

## Error Metrics

### `mc_errors_total` — NOT EMITTED (no recording site exists)

> **This metric does not exist.** Nothing in `crates/` records it; `McError::error_type_label()`'s doc
> (`crates/mc-service/src/errors.rs`) only reserves the name for a future global error counter. Any
> query against it returns no data, which is indistinguishable from "healthy and flat" — so the
> example below is marked NON-FUNCTIONAL rather than deleted, and the row is kept so a grep for
> the name lands here. What to read instead today: `mc_session_join_failures_total{error_type}` for
> join-path errors (present at zero over its join-reachable values) and `mc_actor_panics_total` for
> actor faults. The fields below describe the reserved design, not a live series.

- **Type**: Counter
- **Description**: Total errors by operation and type
- **Labels**:
  - `operation`: Operation that failed (token_refresh, gc_heartbeat, redis_session, meeting_join, session_binding)
  - `error_type`: Error classification — the authoritative value set is `McError::error_type_label()`
    (`crates/mc-service/src/errors.rs`), a wildcard-free match; do NOT re-list it here (an inline copy drifted
    from 18 to the method's full range and is the exact restated-roster rot `mh-service.md` §header warns of).
  - `status_code`: Signaling error code as string (2, 3, 4, 5, 6, 7)
- **Cardinality**: Medium (~90 combinations, bounded by operations and error types)
- **Usage**: Track error rates by type, identify patterns in failures
- **Example** (NON-FUNCTIONAL — the series does not exist; see the note above):
  ```text
  sum(rate(mc_errors_total{job="mc-service"}[5m])) by (operation, error_type)
  ```

---

## Prometheus Query Examples

### Active Meetings
```promql
sum(mc_meetings_active)
```

### Redis p99 Latency
```promql
histogram_quantile(0.99,
  sum(rate(mc_redis_latency_seconds_bucket[5m])) by (le)
)
```

### GC Heartbeat Success Rate
```promql
sum(rate(mc_gc_heartbeats_total{status="success"}[5m])) /
sum(rate(mc_gc_heartbeats_total[5m]))
```

### Join Success Rate
```promql
sum(rate(mc_session_joins_total{status="success"}[5m])) /
sum(rate(mc_session_joins_total[5m]))
```

### Join Duration p95
```promql
histogram_quantile(0.95,
  sum(rate(mc_session_join_duration_seconds_bucket{status="success"}[5m])) by (le)
)
```

### JWT Validation Failure Rate
```promql
sum(rate(mc_jwt_validations_total{result="failure"}[5m])) /
sum(rate(mc_jwt_validations_total[5m]))
```

### JWT Validation Failures by Reason
```promql
sum(rate(mc_jwt_validations_total{result="failure"}[5m])) by (failure_reason)
```

### Connection Rejection Rate
```promql
sum(rate(mc_webtransport_connections_total{status="rejected"}[5m])) /
sum(rate(mc_webtransport_connections_total[5m]))
```

### Join Failures by Error Type
```promql
sum(rate(mc_session_join_failures_total[5m])) by (error_type)
```

### RegisterMeeting RPC Success Rate
```promql
sum(rate(mc_register_meeting_total{status="success"}[5m])) /
sum(rate(mc_register_meeting_total[5m]))
```

### RegisterMeeting RPC Latency p99
```promql
histogram_quantile(0.99,
  sum(rate(mc_register_meeting_duration_seconds_bucket[5m])) by (le)
)
```

### MH Notification Rate by Event
```promql
sum(rate(mc_mh_notifications_received_total[5m])) by (event_type)
```

### Token Refresh Failures by Reason
```promql
sum(rate(mc_token_refresh_failures_total[5m])) by (error_type)
```

### Involuntary Departure Share (timeout leaves)
```promql
sum(rate(mc_participant_leaves_total{reason="timeout"}[5m])) /
sum(rate(mc_participant_leaves_total[5m]))
```
A rising share indicates crashes / network loss (not clean tab-closes). Gate on a
volume floor before alerting — see the MC incident runbook.

### Disconnect Cause Breakdown
```promql
sum(rate(mc_participant_disconnects_total[5m])) by (cause)
```

### Reconnect Rate (within grace)
```promql
sum(rate(mc_participant_disconnects_total{cause="connection_lost"}[5m])) -
sum(rate(mc_participant_leaves_total{reason="timeout"}[5m]))
```

---

## SLO Definitions

### Redis Latency
- **SLI**: p99 Redis operation duration
- **Threshold**: < 10ms
- **Window**: 30 days
- **Objective**: 99.9% of operations under threshold

### Token Refresh Latency
- **SLI**: p99 token refresh duration
- **Threshold**: < 5s
- **Window**: 30 days
- **Objective**: 99.9% of refreshes under threshold

---

## Cardinality Management

All MC service metrics follow strict cardinality bounds per ADR-0011:

| Label | Bound | Values |
|-------|-------|--------|
| `actor_type` | 3 | `controller`, `meeting`, `connection` |
| `operation` | ~10 | Bounded by Redis commands |
| `state` | 4 | `connected`, `failed`, `disconnected`, `unspecified` (participant MH status) |
| `reason` (fencing) | 2-3 | `stale_generation`, `concurrent_write` |
| `reason` (mh-status drop) | 2 | `cap` (per-entry, map full), `over_limit` (per-message, oversized batch) |
| `status` | 2-3 | `success`, `error`/`failure`, `accepted`/`rejected` |
| `heartbeat_type` | 2 | `fast`, `comprehensive` |
| `result` | 2 | `success`, `failure` (JWT validation) |
| `token_type` | 3 | `meeting`, `guest`, `service` (JWT validation) |
| `failure_reason` | 6 | `none`, `signature_invalid`, `expired`, `missing_token`, `scope_mismatch`, `malformed` (JWT validation) |
| `error_type` (join failures) | ~19 | Bounded by `McError` enum variants |
| `error_type` (token refresh) | 6 | `http`, `auth_rejected`, `invalid_response`, `acquisition_failed`, `configuration`, `channel_closed` |
| `event_type` | 2 | `connected`, `disconnected` (MH notifications) |
| `grpc_service` | 2 | `MeetingControllerService`, `MediaCoordinationService` (Layer 2 auth) |
| `expected_type` | 3 | `global-controller`, `media-handler`, `meeting-controller` (Layer 2 auth) |
| `actual_type` | 5 | `global-controller`, `media-handler`, `meeting-controller`, `unknown`, `other` — emit-site clamp (`common::service_type::service_type_metric_label`); `other` = present-but-unrecognized claim |

**Total Estimated Cardinality**: ~115 time series (well within Prometheus limits) — +4 `state` (participant MH status) and +2 `reason` (mh-status drop: `cap`, `over_limit`) over the prior ~105, plus a bounded `actual_type` clamp domain (`other` bucket + `meeting-controller` now zero-init'd) on `mc_caller_type_rejected_total`.

---

## References

- **ADR-0011**: Observability standards and metric naming conventions
- **ADR-0023**: Meeting Controller design (Section 11: Observability)
- **Implementation**: `crates/mc-service/src/observability/metrics.rs`
- **Dashboard**: `infra/grafana/dashboards/mc-overview.json`
