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
- **Dashboard**: MC Overview - JWT Validations by Result (Join Flow row)

### `mc_session_joins_total`
- **Type**: Counter
- **Description**: Total session join attempts by outcome
- **Labels**:
  - `status`: Join outcome (`success`, `failure`)
- **Cardinality**: Low (2 status values)
- **Usage**: Monitor join success rate, overall join volume
- **Alert**: `MCHighJoinFailureRate` (warning, failure rate >5% for 5m)
- **Dashboard**: MC Overview - Session Join Rate by Status (Join Flow row)

### `mc_session_join_duration_seconds`
- **Type**: Histogram
- **Description**: Duration from WebTransport session accept to JoinResponse sent (or error)
- **Labels**:
  - `status`: Join outcome (`success`, `failure`)
- **Buckets**: [0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.000, 5.000]
- **Cardinality**: Low (2 status values)
- **Usage**: Monitor join latency, identify slow joins. Extended to 5s because join includes actor processing.
- **Recorded in**: `connection.rs` measuring full join flow
- **Alert**: `MCHighJoinLatency` (info, p95 >2s for 5m, success only)
- **Dashboard**: MC Overview - Session Join Latency P50/P95/P99 (Join Flow row)

### `mc_session_join_failures_total`
- **Type**: Counter
- **Description**: Total session join failures by error type
- **Labels**:
  - `error_type`: Bounded by `McError` enum variants (e.g., `jwt_validation`, `internal`, `meeting_not_found`, `mc_capacity_exceeded`, `meeting_capacity_exceeded`, `identity_key_invalid`, `sender_id_space_exhausted`)
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
  - `sender_id_space_exhausted` — the meeting consumed all 65535 `sender_id`s and MC refused the
    admission rather than wrapping onto a live id (invariant R-35). **Cumulative lifetime
    admissions, not concurrent participants**, so `MC_MAX_PARTICIPANTS` does not bound it. See
    `mc-incident-response.md` Scenario 8.
- **Usage**: Diagnose join failure root causes, alert on specific failure patterns
- **Recorded in**: `connection.rs` on join failure only
- **Alert**: Used indirectly via `MCHighJoinFailureRate` (this metric provides error type breakdown for diagnosis)
- **Dashboard**: MC Overview - Join Failures by Error Type (Join Flow row)

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

## Media Key Custody Metrics (ADR-0036 §4, §11)

### `mc_meeting_kek_generated_total`
- **Type**: Counter
- **Description**: Meeting key-encryption keys generated. One per meeting-actor creation.
- **Labels**:
  - `key_custody`: single permitted value `operator`. Adding a second requires an ADR-0036 §4
    amendment — this is a *constraint*, not a snapshot of today's deployment.
- **Cardinality**: 1
- **Usage**: KEK issuance rate, and the carrier for the `key_custody` label. **That is the whole of
  what it measures.**
- **NOT a "missing key material" signal.** It increments unconditionally, so it is identically the
  meeting-creation count — there is no path where the meeting actor exists and this does not fire.
  Detecting *absent* key material would need a second series to divide by, and MC has no
  meeting-creation counter (`mc_meetings_active` is a gauge), so the inference is not computable
  even in principle. The real not-provisioned condition is per-join and defined on the response
  side (`signaling.proto`: "the not-provisioned signal is `meeting_kek` not being exactly 32
  bytes"). Do not build an alert on the absence of this counter moving.
- **Emits no key material**, and no `meeting_id` / `meeting_id_hash` — ADR-0036 §11 bars any meeting
  identifier on any metric in this design. *Which* meeting is answered by the meeting actor's span.
- **Recorded in**: `actors/meeting.rs` at meeting-actor creation
- **Dashboard**: MC Overview - Meeting KEK Issuance (Join Flow row)
- **Forward compatibility**: when KEK rotation lands this counts rotations too and genuinely
  diverges from the meeting-creation count.

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
- **Dashboard**: MC Overview - RegisterMeeting RPC Latency P50/P95/P99 (MH Coordination row)

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
- **Counts response evaluations, NOT meetings.** A retried push contributes one sample per attempt; a terminal outcome exactly one. A divergence *ratio* over this denominator is therefore per-attempt, not per-meeting.
- **This is the detection signal.** `mc_media_generation_divergence` is the magnitude a responder reads next — see that entry for why the gauge cannot carry detection.
- **`handler_id_mismatch` is a DIAGNOSTIC, not an alerting signal**, until `MH_HANDLER_ID` is stable per deployment: the id is per-incarnation, so an ordinary MH pod restart produces that outcome by construction and an `outcome!="match"` page would fire on every MH rollout. Alert expressions must read `outcome!~"match|handler_id_mismatch"`. **Three sites hold that one expression** — the alert rule (story task 21), this entry, and `docs/runbooks/mc-deployment.md`'s post-deploy checklist — and they are **one decision with one revert trigger**, `2026-09-02-mh-stable-handler-id`.
- **Label key is `outcome`, not `status`**, matching MH's `mh_media_policy_applies_total{outcome,key_custody}` so both ends of one handshake sit side by side in a query.
- **Usage**: Detect a meeting whose forwarding policy MC could not confirm as live
- **Recorded in**: `grpc/mh_client.rs::confirm` via `observability/metrics.rs::record_media_policy_push`
- **Dashboard**: MC Overview - Media Policy Pushes by Outcome (Media Routing row)

### `mc_media_sender_binding_responses_total`
- **Type**: Counter
- **Description**: Whether MC could resolve a participant's `sender_id` for the Media Handler that asked, and if not, why (ADR-0036 §2, §4; R-15)
- **Labels**:
  - `outcome`: `resolved`, `meeting_unknown`, `participant_unknown`, `registry_full`, `user_ambiguous`
  - `key_custody`: single value `operator` (ADR-0036 §4 — MC can read media keys; that is accepted operator custody, never described as end-to-end)
- **Cardinality**: bounded at the type level by `SenderBindingOutcome::ALL`. **Deliberately no restated integer** — the list above is the operator-facing artifact and a second encoding of its length only rots. (This number drifted four times during the devloop that added the metric; the compile-checked length in `ALL: [Self; N]` is the guard, a prose count is an unchecked copy.)
- **Increment boundary**: once per `NotifyParticipantConnected` **response MC emits**, on every path that produces a response. If the RPC fails before a response is formed, nothing increments here — that population is MH's `mh_media_session_starts_total{outcome="declined_mc_unavailable"}` plus MC's transport-level metrics.
- **THE `sender_id` VALUE IS NEVER A LABEL.** ADR-0036 §11 bars it as a per-participant metric dimension, span attribute or structured log field. The bounded `outcome` token carries every bit of triage signal the value would, at the `SenderBindingOutcome::ALL` cardinality instead of 65535.
- **Why this is NOT redundant with MH's counter — read this before deleting either.** MH observes only `sender_id == 0` and **structurally cannot** tell which unresolved outcome produced it; its `mh_media_session_starts_total{outcome="declined_no_sender_binding"}` is the **union of MC's `0`-answering outcomes**: `meeting_unknown` (routing/registration fault, remedy in the MC↔MH registration path), `participant_unknown` (join race — **self-clearing**), `registry_full` (capacity — **never self-clears**; raise the per-meeting cap or add MC capacity), `user_ambiguous` (one user, two roster entries — **never self-clears, and reconnecting CAUSES it**). Only MC can split them. **This series therefore carries strictly more information than any function of the MH series.** The values are named rather than counted deliberately: a restated count is an unchecked copy that goes stale on the next addition, while naming them cannot.
- **The discriminator for this whole vocabulary is REMEDY, not cause.** Three of the five values exist as separate series because acting on a neighbour's remedy would be wrong or harmful, not because their causes differ. When adding a value, the question is "does a responder do something different?", not "is this a distinct cause?".
- **`user_ambiguous`: the folded label would have been actively harmful, which is why it is separate.** MC mints a fresh `participant_id` per join and does not bar a second join, so **a user who reconnects produces a second roster entry for the same `sub`**. If this were folded into `participant_unknown`, the union's documented remedy is "benign race — reconnect", and following that instruction on this cause **converts a transient failure into a permanent one**. This is not merely two remedies in one series; it is a series whose documented remedy is *harmful* for one of its members.
- **`registry_full` is separate from `participant_unknown` on purpose.** The participant may be perfectly well known; the refusal is about the per-meeting connection cap, and its remedy is capacity, not a lifecycle investigation. It exists because answering a real ordinal for a connection MC declined to register would let MH forward media for a connection MC is not tracking — and will never send `NotifyParticipantDisconnected` for — so the two sides would disagree about whether the connection exists with nothing surfacing the disagreement.
- **Shared denominator, recorded so it is not alerted on twice.** This increments on the same path as `record_mh_notification("connected")`, after the same validation gate, so `sum(mc_media_sender_binding_responses_total)` equals `mc_mh_notifications_received_total{event_type="connected"}` **by construction**. Not duplication — this one carries the resolution dimension the other lacks — but two series encoding one count invite either a double alert or the deletion of the wrong one.
- **Label key is `outcome`, not `status`**, matching the sibling `mc_media_policy_pushes_total` and MH's counterpart so both ends of one handshake sit side by side in a query.
- **Usage**: Detect a participant MC cannot bind for MH — and distinguish a routing fault from a join race, which MH alone cannot
- **Recorded in**: `grpc/media_coordination.rs::notify_participant_connected` via `observability/metrics.rs::record_sender_binding_response`
- **Dashboard**: MC Overview - Sender Binding Responses by Outcome (Media Routing row)

### `mc_media_generation_divergence`
- **Type**: Gauge
- **Description**: Last-observed magnitude by which a handler's live forwarding policy differed from the one MC pushed
- **Labels**:
  - `key_custody`: single value `operator`
- **Cardinality**: 1
- **Value is an unsigned MAGNITUDE, not a difference**: `|sent - applied|`, computed from the **applied** value in the response. Feeding it from the sent value yields a constant 0 — the silent regression ADR-0036 §8 names. `saturating_sub` is equally wrong: `applied > sent` is reachable (an MC restart re-deriving from 1 against a handler holding a higher generation, or the `policy_generation: u64::MAX` ratchet wedge) and would render as a healthy 0 on the one response shape that proves the two ends disagree. Direction is not lost — the `error!` line carries both numbers.
- **Last-write-wins, pod-level.** Its clearing path is the next push of any (meeting, handler) on this pod, so it is overwritten rather than latched and cannot wedge above zero for a pod's lifetime. The price: a healthy push can erase a diverged reading. MC also inherits the global `scrape_interval`, which is coarse relative to a one-shot-per-registration write, so a divergence can be overwritten before it is ever scraped. **Both are independent reasons this gauge is not the detection signal** — that is `mc_media_policy_pushes_total`.
- **Written ONLY on a registration push.** With the ADR-0036 §8 re-assert cadence deferred, **this gauge does not observe a handler restart**: a handler can lose all forwarding policy while this value holds its last healthy reading.
- **Usage**: Read the magnitude after `mc_media_policy_pushes_total{outcome!~"match|handler_id_mismatch"}` has fired; never page on it
- **Recorded in**: `grpc/mh_client.rs::confirm` via `observability/metrics.rs::record_media_policy_push`
- **Dashboard**: MC Overview - Media Generation Divergence (Media Routing row)

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
  convention throughout; the media-signalling WARNs and DEBUGs in
  `webtransport/connection.rs` carry it deliberately, because it is what makes
  the context-unavailable WARN actionable at all.
- **Participant id and stream identity** — `slot_id`, `sender_id`, stream
  number, `switch_command_id` — barred from labels, span attributes **and**
  logs. These are the per-frame/per-stream dimensions §11's log bullet names.

Every label domain is an exhaustive `match` over a Rust enum with an `ALL`
constant, so a new value is a compile error rather than an unbounded series. There is deliberately
no `#[instrument]` anywhere in `media_signaling`: its functions take
`SlotId`/`SenderId`/`&ReceiveCapability` parameters that an auto-instrumented
span would record as attributes, on a surface no guard covers.

**Scrape periodicity**: MC inherits the global 10 s `scrape_interval` (R-36).

### `mc_media_receive_capability_declarations_total`
- **Type**: Counter
- **Description**: Client receive-capability declarations, by disposition (ADR-0036 §6)
- **Labels**:
  - `outcome`: `accepted`, `accepted_unchanged`, `duplicate_slot_id`, `slot_count_over_cap`, `slot_id_out_of_range`, `pinned_sender_id_zero`, `pinned_sender_id_out_of_range`, `media_kind_unspecified`, `slot_id_not_planned`, `declaration_budget_exhausted`
  - `key_custody`: single value `operator` (ADR-0036 §4 — MC can read media keys; that is accepted operator custody, never described as end-to-end)
- **Cardinality**: 10 (bounded at the type level by `CapabilityOutcome::ALL`; an eleventh value is a compile error)
- **This metric PARTITIONS declarations.** Every declaration lands in exactly one bucket of exactly one metric. A rejection reported on some other series would silently break that relationship — which is why `slot_id_not_planned` lives here despite its remedy differing in kind from its neighbours, and why the identical-re-declaration no-op is *counted* rather than dropped.
- **THE SUCCESS SET IS `{accepted, accepted_unchanged}`.** The failure predicate is `outcome!~"accepted|accepted_unchanged"`. Same shape as `mc_media_send_directives_total`'s `{emitted, emitted_empty_targets}`: one pattern across both metrics in this family — the success set is the prefix-shared pair, the failure predicate is the negated alternation.
- **PERMANENT — no revert trigger.** This alternation is a correct, permanent classification, not a defect workaround. If you have seen a similar-looking alternation elsewhere in this catalog that carries a revert trigger, that one is a temporary carve-out and this is not; do not fold them together, and do not strip this alternation as part of any cleanup.
- **`accepted` ALONE means "declarations MC acted on", and that distinction is a security property.** `accepted_unchanged` records a re-declaration identical to the one already in force: MC does no work and sends nothing. It is therefore **client-inflatable at near-zero server cost**, and **must not appear in the denominator of any ratio a client has an incentive to deflate** — a rejection ratio computed over it is an evadable alert. Use `accepted` alone as that denominator. There is no alert on this metric today, which is exactly why the rule is written down now rather than discovered by whoever builds one.
- **A non-zero `accepted_unchanged` is also real information**: nothing in story 1 re-declares at all, so a sustained rate means a client is re-declaring pointlessly.
- **Rejection is always WHOLE-DECLARATION.** MC never accepts a prefix, never applies last-write-wins to a duplicate, and never clamps an out-of-range id.
- **Remedies differ by value, and one of them is not a client bug:**
  - `duplicate_slot_id`, `slot_count_over_cap`, `slot_id_out_of_range`, `pinned_sender_id_zero`, `pinned_sender_id_out_of_range` — **client defects.** Fix the client. `slot_count_over_cap` may instead mean `MC_MAX_RECEIVE_SLOTS` is set below what legitimate clients need.
  - `media_kind_unspecified` — **read version skew first, not a forgotten field.** The proto's zero is what a pre-ADR-0036 peer's `AUDIO = 0` decodes to, so the likeliest producer is a client that *meant audio*. A fleet-wide rise correlates with a client rollback or a partial rollout. Treating it as "a kind we happen not to have" would have made that present as "everyone's meetings are empty" with no signal naming skew.
  - `slot_id_not_planned` — **an MC limitation, NOT a client defect.** The declaration is well-formed and the wire contract permits it. MC cannot serve it because task 13's join-time forwarding-policy push fixes the egress slot id *before* the client can declare, and this story adds no re-push, so MC cannot address a slot it did not plan. **Do not send an operator into the SDK on this value.** The fix is the capability-triggered policy re-push (or making declared slots an input to the assignment computation) — see the devloop plan's §Open question options 1 and 2, and `docs/TODO.md` §Media Path Obligations. **Its rate carries roadmap information**: rising means real clients in the field are choosing slot ids MC does not plan for, i.e. the story-1 limitation is biting and the re-push needs to land.
  - `declaration_budget_exhausted` — a client re-declaring more than `MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS` times on one connection. Nothing in story 1 re-declares at all, so any occurrence now is a client bug or an attack.
- **PAIRED WITH `mc_media_unmatched_plan_slots_total`.** `slot_id_not_planned` (the client's slot has no plan) and that counter (the plan's slot was not declared) are the **two directions of one join**. They are easy to transpose under pressure; read the direction off the name.
- **Logging**: every rejection increments this counter; the WARN fires **once per connection**, carries the outcome token, and never carries the offending `slot_id`/`pinned_sender_id` value.
- **Usage**: Are clients' capability declarations landing, and if not, whose bug is it?
- **Recorded in**: `webtransport/connection.rs::handle_receive_capability` via `observability/metrics.rs::record_receive_capability`
- **Dashboard**: MC Overview - Receive Capability Declarations by Outcome (Client Media Signalling row)

### `mc_media_send_directives_total`
- **Type**: Counter
- **Description**: Send-directive compositions, by disposition (ADR-0036 §5)
- **Labels**:
  - `outcome`: `emitted`, `emitted_empty_targets`, `unknown_stream_number`, `transport_mode_unspecified`, `handler_url_unresolved`, `meeting_state_unavailable`, `assignment_failed`, `no_planned_egress_slot`
  - `key_custody`: single value `operator`
- **Cardinality**: bounded at the type level by `DirectiveOutcome::ALL`. Deliberately no restated integer — the list above is the operator-facing artifact and a second encoding of its length only rots.
- **THE SUCCESS SET IS `{emitted, emitted_empty_targets}`, NOT `{emitted}`.** ADR-0036 §5 makes an empty target set a *specified success* — "A target set may be empty. That means send nothing." A failure predicate must read `outcome!~"emitted|emitted_empty_targets"`; `outcome!="emitted"` would page on a legal state. `emitted_empty_targets` also becomes **routine rather than anomalous** once §7 turns off publishers nobody watches, so its meaning does not move even though its rate will.
- **PERMANENT — no revert trigger.** This alternation is a correct, permanent classification grounded in ADR-0036 §5, not a workaround. `mc_media_policy_pushes_total`'s superficially identical `outcome!~"match|handler_id_mismatch"` is a *temporary* carve-out with a recorded revert trigger; **the two share a shape and nothing else.** Do not fold this entry into that decision, and do not strip this alternation as part of any cleanup: doing so starts paging on a routine success while appearing to complete a documented task.
- **Two values are MC defects — ANY non-zero value indicates a bug**: `unknown_stream_number` (a forwarding plan named a stream number MC has no policy entry for) and `transport_mode_unspecified` (a plan carried no transport mode; MC fails closed rather than defaulting to datagram). The remaining **four** failure values are environmental: `handler_url_unresolved`, `meeting_state_unavailable`, `assignment_failed` and `no_planned_egress_slot`.
- **`no_planned_egress_slot` is the MOST SEVERE value on this metric, and the only one that is JOIN-TIME ONLY.** It means the forwarding assignment computed *successfully* and simply contains no egress plan naming this subscriber, so MC has no slot to route its audio into and cannot validate a declaration against one. The connection then never receives a send directive **for its entire session** — not for one declaration — and stays otherwise healthy, so nothing else about it looks wrong.
  - **Not `assignment_failed`.** "MC could not compute a plan" and "MC computed a plan that does not include you" have different causes and different remedies; collapsing them hides the difference.
  - **Not `slot_id_not_planned`** on the capability counter, which is a *client* naming a slot MC did not plan. This is MC planning no slot at all. Easy to transpose under pressure.
  - Recorded once per connection at join, never per message.
- **Recorded at TWO sites, and that is the point of the vocabulary.** Per composition in `compose_and_emit`, and once per connection at join in `handle_connection`'s context-resolution step. A join-time failure is the more severe of the two, so it must not be the one that moves no counter: every reason MC did not **compose** a directive lands on this one series, which is what makes "did MC compose a directive, and if not why" a single query rather than a join across two.
- **Failure values are DISJOINT BY CONSTRUCTION.** The emit path is sequential stages — read meeting state, compute the assignment, resolve handler urls, build streams — and the first failing stage returns its own value. An assignment failure can never also be reported as an unresolved handler url, because url resolution is never reached.
- **`emitted` MEANS COMPOSED, NOT DELIVERED — and this is the one caveat an operator must read before concluding anything from a healthy graph.** `record_send_directive` fires on the `Ok` branch of `compose_and_emit` **before** `send_signaling` is called, so the counter is incremented by the act of composing. **Two hops follow it, and neither is covered by this series:**
  1. `ParticipantActorHandle::send`, an awaited mpsc — an `Err` means the mailbox is **CLOSED** (the participant actor is gone). **WARN only, deliberately not counted**, because the FULL case below already is; the WARN is at `mc.webtransport.connection` and says in terms that the client was not told what the counter says it was told.
  2. the stream outbound `try_send` in `actors/participant.rs` — mailbox **FULL** or closed. Counted, but on a **different** metric: `mc_participant_outbound_messages_dropped_total{payload_kind="signaling_raw"}`.
- **So a directive composed and then lost reads as `emitted` here, with `accepted` healthy on the capability counter and nothing anywhere disagreeing.** That is precisely the scenario this entry exists for — "a client that is never told to send simply produces nothing; there is no error and no absent-frame signal to notice" — so **do not treat `emitted` alone as evidence that a client was directed.** Check `mc_participant_outbound_messages_dropped_total{payload_kind="signaling_raw"}` for the mailbox-full case and the `mc.webtransport.connection` delivery WARN for the actor-gone case.
- **The counter is NOT moved below `send_signaling` to fix this, on purpose.** Doing so would conflate composition failure with delivery failure and destroy the disjoint-by-construction property above — the vocabulary describes *stages of composition*, and delivery is a separate hop with separate evidence. The completeness caveat is the correct fix; a merged counter would be a worse one.
- **Usage**: Is MC actually directing clients to send, and when it silently is not, why?
- **Recorded in**: `webtransport/connection.rs::compose_and_emit` (per composition) and `webtransport/connection.rs::handle_connection` on the `build_media_signaling_context` error path (`no_planned_egress_slot` and the join-time environmental values), both via `observability/metrics.rs::record_send_directive`
- **Dashboard**: MC Overview - Send Directives by Outcome (Client Media Signalling row)

### `mc_media_slot_states_total`
- **Type**: Counter
- **Description**: Slot states conveyed to subscribers (ADR-0036 §6)
- **Labels**:
  - `slot_state`: `unspecified`, `active`, `source_muted`, `withheld_by_congestion`, `fewer_sources_than_slots`, `zero_requested`, `source_unreachable`, `switch_pending`
  - `key_custody`: single value `operator`
- **Cardinality**: bounded at the type level by the proto `SlotState` enum, mirrored exhaustively by `media_signaling::slot_state_label`
- **The bucketed slot-state signal ADR-0036 §11 mandates**, joined to no identity: it answers *how many and how bad*; **MC's own assignment state answers *who***, at investigation time, in a system that legitimately holds that mapping. There is no per-slot series and must not be one.
- **The label domain MIRRORS THE WIRE ENUM EXHAUSTIVELY** — all eight `SlotState` variants, not the four MC can reach today. A hand-picked subset needs editing the moment §7 makes `switch_pending` live, and a `SLOT_STATE_UNSPECIFIED` reaching the wire is an MC defect that must be *visible* rather than absent. Keeping the vocabulary identical to the wire's is also what makes this distribution comparable with the client's.
- **Reachable in story 1**: `active`, `source_muted`, `fewer_sources_than_slots`, `source_unreachable`. `withheld_by_congestion` is MH-observed (§6) and arrives with the slot-state notification; `switch_pending` arrives with §7 switching; `zero_requested` may be **unemittable in principle** — it means "requested zero of this kind" but rides a message whose `slot_id` echoes a slot the subscriber *declared*, and a subscriber who declared a slot of that kind did not request zero of it. That is an open protocol question recorded in `docs/TODO.md`, not a settled "reachable later".
- **`source_muted` is client mute (§5)** — the source is present and has muted itself at capture. MC does **not** withdraw or re-issue that source's send directive; the slot state is the entire signal. A `source_muted`/`active` oscillation with a flat `mc_media_send_directives_total` is the healthy shape.
- **THIS COUNTER IS PER-COMPOSITION, AND COMPOSITIONS ARE PARTLY CLIENT-TRIGGERED.** One increment per declared slot per composition, and a composition runs on an accepted declaration *or* on a mute report whose **audio** flag changed. So a client toggling mute weights this distribution, and a raw fleet-wide ratio over it is skewable by one participant. **Any SLO or ratio built on it must be per-connection-normalised, or restricted to the capability path.**
  - The skew is **bounded**, not open-ended: mute-driven compositions are rate-limited per connection (`mc_media_mute_requests_total{outcome="rate_limited"}` is the visible edge of that bound), and a *video-only* toggle does not recompose at all, because the slot view derives from `audio_self_muted` alone. So a single connection can weight this counter at a few compositions per second, not at line rate.
  - This is a weaker property than `mc_media_receive_capability_declarations_total`'s: there, `accepted_unchanged` is called out as client-inflatable and excluded from the denominator. Here there is no single value to exclude — the weighting is spread across whichever states that subscriber's slots are in — which is precisely why the normalisation has to happen in the query.
- **Usage**: How many slots are filled versus dark, and in what way
- **Recorded in**: `webtransport/connection.rs::compose_and_emit` via `observability/metrics.rs::record_slot_state`
- **Dashboard**: MC Overview - Slot States (Client Media Signalling row)

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
  - `applied` — reported to the meeting actor, and recomposition **attempted**. Deliberately not "and recomposed": the counter is recorded before `compose_and_emit` runs and that call's result is discarded, so a recomposition that then fails on the meeting-state read or the assignment computation is still counted here. **A failed mute-path recomposition is currently visible only in logs** — every `record_send_directive` call in `compose_and_emit` is inside the `if emit_directive` guard, so the mute path records none of them. (No restated integer: the number of guarded call sites is not the number of outcomes they carry, and a second encoding of either only rots.) That gap is a tracked deferral in `docs/TODO.md`; moving this record site below `compose_and_emit` would force a sixth variant for "applied but recomposition failed", which is exactly the design question the deferral parks.
  - `applied_no_recompose` — reported to the actor, with nothing to re-convey: either the subscriber has not declared a receive capability yet, or **only `video_muted` changed**. The slot view derives from `audio_self_muted` alone, so recomposing would spend an O(N) roster read and an assignment computation to emit a byte-identical `StreamAssignments`. **This is what an ordinary camera button produces** — expect it to be the largest bucket once clients wire video controls.
  - `unchanged` — identical to the report already in force. No actor hop, no recomposition. Not a failure: MC correctly did nothing.
- **`unchanged` IS CLIENT-INFLATABLE AT NEAR-ZERO SERVER COST, and must not appear in a ratio denominator.** Same property as `accepted_unchanged` on the capability counter, and the same rule: **the denominator is `applied` + `applied_no_recompose`**. The short-circuit fires before any actor hop, roster read or recomposition, so a client repeating one state drives this value at line rate for a tuple compare and a counter increment. "What fraction of mute reports are being applied", computed over `unchanged`, is a number one participant can drive to zero.
- **`unchanged` is exempt from the rate limiter ON PURPOSE — do not "fix" the ordering.** The no-op check runs *before* the token is spent, so identical repeats do not drain the bucket. Reversing that would let a client spamming a steady state exhaust its own budget on messages that do no work and thereby suppress its next **genuine** toggle, while the metric reported `rate_limited` for an expensive path that was never approached.
- **`rate_limited` is the visible edge of the mute-work bound, and is NOT by itself an incident.** Client mute is the only repeatable client-driven path that puts work on the **shared meeting actor's mailbox** — one `GetState` roster snapshot per composition. A per-connection token bucket (burst 8, sustained 4/s) bounds it.
  - **This is NOT a meeting-wide contention signal, and it was documented as one.** The bound was originally sized against `handle_self_mute`'s O(N) awaited `broadcast_update`, which head-of-line-blocked joins, leaves and every other connection's state read. That fan-out was **removed** during the same devloop (security's S-2: `MuteChanged` has no consumer, so it delivered zero bytes to zero clients). What remains on the shared actor is a queue slot, not head-of-line blocking; the dominant cost is now the per-connection roster read, assignment computation and outbound message. The bound is still required — the work is still unbounded and client-driven — but do not route a meeting-wide latency investigation here.
  - **A human never reaches the limiter.** A sustained non-zero rate means one connection is toggling far above human rates: a client-side repeat loop or a reactive-state bug first, an abusive peer second. It is bounded to that connection either way.
  - **Why a rate limit and not a budget.** A cumulative budget would permanently deny a repeatable steady-state user action: once spent, that participant's `audio_self_muted` freezes and every other client renders a live speaker as muted for the rest of the session. That is a correctness failure strictly worse than the amplification it would prevent. See `media_signaling`'s module doc for the criterion in general form.
  - **Dropping a report is safe, and here is why it is safe even for a merely broken client.** ADR-0036 §5 enforces client mute at **capture on the client**, so a suppressed report means the audio genuinely stopped and only the indicator other participants see is stale. There is no window in which someone believes they have stopped transmitting and has not. A suppressed report deliberately does not update MC's cache of the last reported pair, so the client's next toggle re-attempts rather than being swallowed — the staleness is bounded by that next action, not by the session.
- **`actor_unavailable` is environmental and terminal**: the meeting actor's mailbox is closed, so the connection is already going away.
- **Usage**: Is client mute being applied, and when it is not, why? Is any connection driving mute recomposition hard enough to be clamped? (Per-connection cost, not meeting-wide — see the `rate_limited` bullet.)
- **Recorded in**: `webtransport/connection.rs::handle_mute_request` via `observability/metrics.rs::record_mute_request`
- **Dashboard**: MC Overview - Mute Requests by Outcome (Client Media Signalling row)

### `mc_media_unmatched_plan_slots_total`
- **Type**: Counter
- **Description**: Planned egress slots a subscriber never declared (ADR-0036 §6)
- **Labels**:
  - `key_custody`: single value `operator`
- **Cardinality**: 1
- **What it means operationally**: MH is forwarding egress the subscriber will never accept — wasted uplink and handler work for media nobody opened a slot for.
- **A non-zero value is CLIENT-CONFORMANCE-DRIVEN, not necessarily an MC defect.** Reachable in story 1 through the zero-audio exemption: a subscriber declaring no audio slot (`{}` or video-only) is legitimately saying "I want to send but not receive audio", MC accepts it, and MC's planned audio slot then matches nothing.
- **PAIRED WITH `mc_media_receive_capability_declarations_total{outcome="slot_id_not_planned"}`.** These are the **two directions of one join**: *the plan's slot was not declared* here, *the client's slot has no plan* there. Under whole-declaration rejection the two are mutually exclusive per declaration — a declaration asking for audio in an unplanned slot is rejected before composition, so it never reaches this counter. Read the direction off the name; they are easy to transpose.
- **Its denominator is per-planned-slot, not per-directive**, which is why it is a separate series rather than a value on `mc_media_slot_states_total` — and why mixing it in would have broken that counter's "metric and wire cannot disagree" property, since it corresponds to no wire value.
- **Usage**: Is MC programming handlers to forward media no subscriber will take?
- **Recorded in**: `webtransport/connection.rs::compose_and_emit` via `observability/metrics.rs::record_unmatched_plan_slots`
- **Dashboard**: MC Overview - Unmatched Plan Slots and Dropped Outbound Messages (Client Media Signalling row)

---

## Participant Outbound Delivery Metrics

### `mc_participant_outbound_messages_dropped_total`
- **Type**: Counter
- **Description**: Server messages dropped because a participant's outbound channel was full or closed
- **Labels**:
  - `payload_kind`: `signaling_raw`, `participant_update`
- **Cardinality**: 2
- **The client did not receive something MC decided to send.**
- **EVERY DROP IS COUNTED HERE; ONLY THE FIRST IS LOGGED PER CONNECTION. A SINGLE WARN DOES NOT MEAN A SINGLE DROP.** The WARN at `mc.actor.participant` fires once per participant actor and is deliberately not repeated — a per-message log on a client-drivable path is a log-amplification vector, and a wedged outbound channel drops one message per roster broadcast, i.e. O(participants x events) from one bad connection. **So log-line volume understates drop volume by orders of magnitude, and this counter is the only complete record.** Triaging by `grep` first — which is what people actually do — shows one line and reads as an isolated blip; the truth is the opposite. Take the magnitude from here, never from the log.
- The one-shot WARN only became safe *because* this counter exists: before it, the repeated line **was** the record.
- **Deliberately a NEW metric, not a fourth `actor_type` on `mc_messages_dropped_total`.** That metric is fed by `MailboxMonitor` on the **inbound** actor path; a pseudo-`actor_type` value here would corrupt the `topk` on the cross-service `errors-overview.json` dashboard by mixing two different quantities.
- **No `key_custody` label**, deliberately: this is the generic outbound signalling choke point, not a media-path metric. Fleet-wide `key_custody` rollout is R-26 / story task 22. The key is not `reason` either — that key is spoken for by the frame-reject vocabulary.
- **RECIPROCAL WITH `mc_media_send_directives_total`.** That counter's `emitted` means a directive was **composed**, not delivered — it fires before the send. `payload_kind="signaling_raw"` is the mailbox-FULL half of the evidence for what happened next; the mailbox-CLOSED half is a WARN at `mc.webtransport.connection` and is deliberately uncounted, since the FULL case is covered here. **A non-zero `signaling_raw` rate against a healthy `emitted` rate is the specific shape of "MC composed a send directive the client never received".**
- **Usage**: Detect a slow or wedged client connection losing server messages
- **Recorded in**: `actors/participant.rs::handle_send` and `handle_update` via `observability/metrics.rs::record_participant_outbound_dropped`
- **Dashboard**: MC Overview - Unmatched Plan Slots and Dropped Outbound Messages (Client Media Signalling row)

---

## MH Coordination Metrics (R-15, R-20)

### `mc_mh_notifications_received_total`
- **Type**: Counter
- **Description**: Total MH→MC participant connection/disconnection notifications received
- **Labels**:
  - `event_type`: Notification event type (`connected`, `disconnected`)
- **Cardinality**: Low (2 event types)
- **Usage**: Monitor MH→MC notification volume, detect MH connectivity issues
- **Recorded in**: `grpc/media_coordination.rs` on notification receipt
- **Dashboard**: MC Overview - MH Notifications by Event (MH Coordination row)

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
- **Usage**: Observe client-perceived MH reachability; a rising `failed` share
  is an early signal of MH-edge connectivity problems the client sees before MC
  does
- **Recorded in**: `actors/participant.rs::handle_record_mh_statuses`, driven
  from `webtransport/connection.rs` `handle_media_connection_update`
- **Dashboard**: MC Overview - Client-Reported MH Status by State

### `mc_participant_mh_status_dropped_total`
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
  and `actors/meeting.rs::handle_end_meeting` (meeting_ended)

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
- **Type**: Counter
- **Description**: Total Layer 2 service_type routing rejections (valid token, wrong caller for gRPC service)
- **Labels**:
  - `grpc_service`: Target gRPC service name (`MeetingControllerService`, `MediaCoordinationService`)
  - `expected_type`: Expected service_type for the gRPC service (`global-controller`, `media-handler`)
  - `actual_type`: Actual service_type from the token (`global-controller`, `media-handler`, `meeting-controller`, `unknown`)
- **Cardinality**: Low (2 x 3 x 4 = 24 max, bounded by service types)
- **Alert**: ANY non-zero value indicates a bug or misconfiguration
- **Usage**: Detect service-to-service routing errors, misconfigured tokens
- **Recorded in**: `grpc/auth_interceptor.rs` on Layer 2 rejection
- **Dashboard**: MC Overview - Caller Type Rejections

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

### `mc_errors_total`
- **Type**: Counter
- **Description**: Total errors by operation and type
- **Labels**:
  - `operation`: Operation that failed (token_refresh, gc_heartbeat, redis_session, meeting_join, session_binding)
  - `error_type`: Error classification from `McError::error_type_label()` (redis, grpc, not_registered, config, session_binding, meeting_not_found, participant_not_found, meeting_capacity_exceeded, mc_capacity_exceeded, draining, migrating, fenced_out, conflict, jwt_validation, permission_denied, internal, token_acquisition, token_acquisition_timeout)
  - `status_code`: Signaling error code as string (2, 3, 4, 5, 6, 7)
- **Cardinality**: Medium (~90 combinations, bounded by operations and error types)
- **Usage**: Track error rates by type, identify patterns in failures
- **Example**:
  ```promql
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
| `actual_type` | 4 | `global-controller`, `media-handler`, `meeting-controller`, `unknown` (Layer 2 auth) |

**Total Estimated Cardinality**: ~111 time series (well within Prometheus limits) — +4 `state` (participant MH status) and +2 `reason` (mh-status drop: `cap`, `over_limit`) over the prior ~105.

---

## References

- **ADR-0011**: Observability standards and metric naming conventions
- **ADR-0023**: Meeting Controller design (Section 11: Observability)
- **Implementation**: `crates/mc-service/src/observability/metrics.rs`
- **Dashboard**: `infra/grafana/dashboards/mc-overview.json`
