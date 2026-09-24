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
- **Expected-empty**: yes — every series is a join failure, so a healthy join path reads zero
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
- **Counts response evaluations, NOT meetings — with ONE exception.** A retried push contributes one sample per attempt; a terminal outcome exactly one. A divergence *ratio* over this denominator is therefore per-attempt, not per-meeting. The exception: an MC-restart floor on the FIRST confirm of a (meeting, handler) in an MC process is recorded on `mc_media_policy_generation_adoptions_total` INSTEAD (or here as `generation_mismatch` if adoption fails). **Total evaluated replies = `sum(mc_media_policy_pushes_total) + sum(mc_media_policy_generation_adoptions_total)`.**
- **This is the detection signal.** `mc_media_generation_divergence` is the magnitude a responder reads next — see that entry for why the gauge cannot carry detection.
- **`handler_id_mismatch` is a DIAGNOSTIC, not an alerting signal**, until `MH_HANDLER_ID` is stable per deployment: the id is per-incarnation, so an ordinary MH pod restart produces that outcome by construction and an `outcome!="match"` page would fire on every MH rollout. Alert expressions must read `outcome!~"match|handler_id_mismatch"`. **Three sites hold that one expression** — the alert rule (story task 21), this entry, and `docs/runbooks/mc-deployment.md`'s post-deploy checklist — and they are **one decision with one revert trigger**, `2026-09-02-mh-stable-handler-id`.
- **Label key is `outcome`, not `status`**, matching MH's `mh_media_policy_applies_total{outcome,key_custody}` so both ends of one handshake sit side by side in a query.
- **Usage**: Detect a meeting whose forwarding policy MC could not confirm as live
- **Recorded in**: `grpc/mh_client.rs::confirm` via `observability/metrics.rs::record_media_policy_push` (and `media_routing/pusher.rs` for a FAILED restart-floor adoption, as `generation_mismatch`)
- **Dashboard**: MC Overview - Media Policy Pushes by Outcome (Media Routing row)

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
- **Dashboard**: MC Overview - Policy Generation Floor Adoptions (Media Routing row)

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
- **Written on every registration push, which since story 2 means every structural change.** Joins, leaves, capability declarations and mutes each re-render the meeting and re-push any handler whose snapshot changed, so this gauge is written far more often than the story-1 once-per-meeting — and the last-write-wins caveat above gets WORSE, not better: a diverged reading is overwritten by the next healthy push of ANY (meeting, handler) on the pod. With the ADR-0036 §8 re-assert cadence still deferred (story 4), **this gauge does not observe a handler restart**: a handler can lose all forwarding policy while this value holds its last healthy reading.
- **An MC restart against a live handler is now self-correcting.** A restarted MC re-derives generations from 1 while MH still holds K; the push worker adopts MH's truthfully echoed K as a floor and re-pushes at K+1 (`PolicyGenerations::adopt_floor`), and it is recorded on `mc_media_policy_generation_adoptions_total`, NOT here: a successful adoption never writes this gauge. Only a FAILED adoption (recorded as `generation_mismatch`) writes a magnitude.
- **Usage**: Read the magnitude after `mc_media_policy_pushes_total{outcome!~"match|handler_id_mismatch"}` has fired; never page on it
- **Recorded in**: `grpc/mh_client.rs::confirm` via `observability/metrics.rs::record_media_policy_push` (and `media_routing/pusher.rs` for a FAILED restart-floor adoption, as `generation_mismatch`)
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

**Scrape periodicity**: MC inherits the global 10 s `scrape_interval` (R-36).

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
- **Dashboard**: MC Overview - Receive Capability Declarations by Outcome (Client Media Signalling row)

### `mc_media_send_directives_total`
- **Type**: Counter
- **Description**: Send-directive compositions, by disposition (ADR-0036 §5)
- **Labels**:
  - `outcome`: `emitted`, `emitted_empty_targets`, `unknown_stream_number`, `transport_mode_unspecified`, `handler_url_unresolved`, `meeting_state_unavailable`, `assignment_failed`
  - `key_custody`: single value `operator`
- **Cardinality**: bounded at the type level by `DirectiveOutcome::ALL`. Deliberately no restated integer — the list above is the operator-facing artifact and a second encoding of its length only rots.
- **THE SUCCESS SET IS `{emitted, emitted_empty_targets}`, NOT `{emitted}`.** ADR-0036 §5 makes an empty target set a *specified success* — "A target set may be empty. That means send nothing." A failure predicate must read `outcome!~"emitted|emitted_empty_targets"`; `outcome!="emitted"` would page on a legal state. **Since story 2, `emitted_empty_targets` is ROUTINE**: a solo participant, and any participant no co-handler subscriber holds in a slot, is directed to send nothing — and is re-sent a non-empty directive the moment someone starts holding it.
- **PERMANENT — no revert trigger.** This alternation is a correct, permanent classification grounded in ADR-0036 §5, not a workaround. `mc_media_policy_pushes_total`'s superficially identical `outcome!~"match|handler_id_mismatch"` is a *temporary* carve-out with a recorded revert trigger; **the two share a shape and nothing else.** Do not fold this entry into that decision, and do not strip this alternation as part of any cleanup: doing so starts paging on a routine success while appearing to complete a documented task.
- **Three values are MC defects — ANY non-zero value indicates a bug**: `unknown_stream_number` (a forwarding plan named a stream number MC has no policy entry for), `transport_mode_unspecified` (a plan carried no transport mode; MC fails closed rather than defaulting to datagram) and — since story 2 — `handler_url_unresolved` (every handler url a client sees comes from one frozen `MeetingHandlers` value, so a miss cannot happen by construction; it fails CLOSED — nothing is emitted for that participant, never an `ACTIVE` slot or a send target with an empty url). The remaining **two** failure values are environmental: `meeting_state_unavailable` and `assignment_failed`. (`no_planned_egress_slot`, story 1's join-time "MC planned no slot for you", is retired: declared slots are now the assignment's input.)
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
- **Dashboard**: MC Overview - Send Directives by Outcome (Client Media Signalling row)

### `mc_media_slot_states_total`
- **Type**: Counter
- **Description**: Slot states conveyed to subscribers (ADR-0036 §6)
- **Labels**:
  - `slot_state`: `unspecified`, `active`, `source_muted`, `withheld_by_congestion`, `fewer_sources_than_slots`, `zero_requested`, `source_unreachable`, `switch_pending`
  - `key_custody`: single value `operator`
- **Cardinality**: bounded at the type level by the proto `SlotState` enum, mirrored exhaustively by `media_signaling::slot_state_label`
- **The bucketed slot-state signal ADR-0036 §11 mandates**, joined to no identity: it answers *how many and how bad*; **MC's own assignment state answers *who***, at investigation time, in a system that legitimately holds that mapping. There is no per-slot series and must not be one.
- **The label domain MIRRORS THE WIRE ENUM EXHAUSTIVELY** — all eight `SlotState` variants, not the three MC can reach today. A hand-picked subset needs editing the moment §7 makes `switch_pending` live, and a `SLOT_STATE_UNSPECIFIED` reaching the wire is an MC defect that must be *visible* rather than absent. Keeping the vocabulary identical to the wire's is also what makes this distribution comparable with the client's.
- **Reachable since story 2**: `active`, `source_muted`, `fewer_sources_than_slots`. **Every declared-but-unfilled slot is `fewer_sources_than_slots`** — a solo participant (loopback is removed, R-3), a meeting smaller than N, a participant alone on its handler. `source_unreachable` is **no longer emitted**: a participant on ANOTHER handler consumes no slot and is named in `StreamAssignments.unreachable_sender_ids` instead (see `mc_media_unreachable_senders_total`); the state is reserved for a PINNED source on another handler, which static fill cannot produce. It stays in the label domain because the domain mirrors the wire. `withheld_by_congestion` is MH-observed (§6) and arrives with the slot-state notification; `switch_pending` arrives with §7 switching; `zero_requested` may be **unemittable in principle** — it means "requested zero of this kind" but rides a message whose `slot_id` echoes a slot the subscriber *declared*, and a subscriber who declared a slot of that kind did not request zero of it. That is an open protocol question recorded in `docs/TODO.md`, not a settled "reachable later".
- **`source_muted` is client mute (§5) OR server mute (§7)** — one wire state covers both causes (who muted whom rides `ParticipantMuteUpdate`). MC does **not** withdraw or re-issue that source's send directive; the slot state is the entire signal. A `source_muted`/`active` oscillation with a flat `mc_media_send_directives_total` is the healthy shape.
- **THIS COUNTER IS PER-EMISSION, AND EMISSIONS ARE BOTH CLIENT- AND SERVER-DRIVEN.** One increment per declared slot per `StreamAssignments` the actor actually SENDS (only changed views are sent). Since story 2 the actor re-emits on a PEER's join, leave, declaration or audio-mute change, not only on the subscriber's own declaration, so the distribution is weighted by meeting churn as well as by any one client. **Any SLO or ratio built on it must be per-connection-normalised.**
  - The client-driven share is bounded: mute-driven re-emits are rate-limited per connection (`mc_media_mute_requests_total{outcome="rate_limited"}`), a video-only toggle re-emits nothing, and each re-emit reaches only the subscribers HOLDING the muted source. The server-driven share is bounded per meeting by the actor's per-turn flush bound (`mc_media_slot_view_emissions_total{outcome="deferred"}`).
- **Usage**: How many slots are filled versus dark, and in what way
- **Recorded in**: `actors/meeting_media.rs::flush_one` via `observability/metrics.rs::record_slot_state`
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
  - `applied` — the audio flag moved on a declared connection and was reported to the meeting actor, which re-emits the changed slot view to the subscribers HOLDING this source. Any composition failure on that re-emit is counted by the actor on `mc_media_send_directives_total` and `mc_media_slot_view_emissions_total{outcome="composition_failed"}` — it is no longer visible only in logs.
  - `applied_no_recompose` — reported to the actor, with nothing to re-convey: either this connection has not declared a receive capability yet, or **only `video_muted` changed**. The slot view reads only audio mute, so no subscriber's view changes. **This is what an ordinary camera button produces** — expect it to be the largest bucket once clients wire video controls.
  - `unchanged` — identical to the report already in force. No actor hop, no recomposition. Not a failure: MC correctly did nothing.
- **`unchanged` IS CLIENT-INFLATABLE AT NEAR-ZERO SERVER COST, and must not appear in a ratio denominator.** Same property as `accepted_unchanged` on the capability counter, and the same rule: **the denominator is `applied` + `applied_no_recompose`**. The short-circuit fires before any actor hop, roster read or recomposition, so a client repeating one state drives this value at line rate for a tuple compare and a counter increment. "What fraction of mute reports are being applied", computed over `unchanged`, is a number one participant can drive to zero.
- **`unchanged` is exempt from the rate limiter ON PURPOSE — do not "fix" the ordering.** The no-op check runs *before* the token is spent, so identical repeats do not drain the bucket. Reversing that would let a client spamming a steady state exhaust its own budget on messages that do no work and thereby suppress its next **genuine** toggle, while the metric reported `rate_limited` for an expensive path that was never approached.
- **`rate_limited` is the visible edge of the mute-work bound, and is NOT by itself an incident.** Client mute is the only repeatable client-driven path that puts work on the **shared meeting actor's mailbox** — one actor hop plus a re-emit to the source's holders. A per-connection token bucket (burst 8, sustained 4/s) bounds it; the resulting fan-out is bounded per meeting by the actor's flush bound.
  - **This is NOT a meeting-wide contention signal.** The roster `broadcast_update` the bound was first sized against was removed (security's S-2: `MuteChanged` has no consumer). The meeting-wide cost of a mute is its re-emit to the source's holders, which the actor bounds per turn; do not route a meeting-wide latency investigation here.
  - **A human never reaches the limiter.** A sustained non-zero rate means one connection is toggling far above human rates: a client-side repeat loop or a reactive-state bug first, an abusive peer second. It is bounded to that connection either way.
  - **Why a rate limit and not a budget.** A cumulative budget would permanently deny a repeatable steady-state user action: once spent, that participant's `audio_self_muted` freezes and every other client renders a live speaker as muted for the rest of the session. That is a correctness failure strictly worse than the amplification it would prevent. See `media_signaling`'s module doc for the criterion in general form.
  - **Dropping a report is safe, and here is why it is safe even for a merely broken client.** ADR-0036 §5 enforces client mute at **capture on the client**, so a suppressed report means the audio genuinely stopped and only the indicator other participants see is stale. There is no window in which someone believes they have stopped transmitting and has not. A suppressed report deliberately does not update MC's cache of the last reported pair, so the client's next toggle re-attempts rather than being swallowed — the staleness is bounded by that next action, not by the session.
- **`actor_unavailable` is environmental and terminal**: the meeting actor's mailbox is closed, so the connection is already going away.
- **Usage**: Is client mute being applied, and when it is not, why? Is any connection driving mute recomposition hard enough to be clamped? (Per-connection cost, not meeting-wide — see the `rate_limited` bullet.)
- **Recorded in**: `webtransport/connection.rs::handle_mute_request` via `observability/metrics.rs::record_mute_request`
- **Dashboard**: MC Overview - Mute Requests by Outcome (Client Media Signalling row)

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
- **Dashboard**: MC Overview - Slot View Emissions by Outcome (Client Media Signalling row)

### `mc_media_unreachable_senders_total`
- **Type**: Counter
- **Description**: Roster participants named unreachable (on a different media handler, ADR-0036 §9; story 2 R-33) across emitted `StreamAssignments`
- **Labels**:
  - `key_custody`: single value `operator`
- **Cardinality**: 1
- **Increment boundary**: by the length of `unreachable_sender_ids` on each `StreamAssignments` the actor SENDS — recorded only after the handover succeeded, alongside `mc_media_slot_view_emissions_total{outcome="sent"}`, so a delivery failure moves neither side of the ratio below. No sender, meeting or handler identity, ever.
- **EMISSION-WEIGHTED, NOT A POPULATION.** It rises with churn in a split meeting, and a stable split meeting emits nothing. It does NOT answer "how many people are currently cross-handler" — that is the placement INFO log (`mc.actor.meeting`, "Participant placed on media handler"). Mean unreachable senders per emitted view: `rate(mc_media_unreachable_senders_total[5m]) / rate(mc_media_slot_view_emissions_total{outcome="sent"}[5m])`.
- **Permanently zero on a single-handler deployment** — by construction, not a vacuous detector: with one handler nobody is unreachable.
- **Usage**: How often are participants told that some of the roster is on another handler (split meetings under round-robin placement)?
- **Recorded in**: `actors/meeting_media.rs::flush_one` via `observability/metrics.rs::record_unreachable_senders`
- **Dashboard**: MC Overview - Unreachable Senders Named (Client Media Signalling row)

### `mc_media_handler_set_divergence_total`
- **Expected-empty**: yes — any non-zero value is an MC-internal invariant violation
- **Type**: Counter
- **Description**: Joins whose handler assignment (read from Redis) differed from the meeting's handler set frozen at its first join
- **Labels**:
  - `key_custody`: single value `operator`
- **Cardinality**: 1
- **The frozen set is the meeting's authority.** The actor keeps it: no later join and no changed Redis entry can widen a live meeting's handler set or move a placed participant. A non-zero value means Redis and the actor disagree — capture and escalate as an MC defect; restarting nothing fixes it. The ERROR line at `mc.actor.meeting` says the same.
- **Recorded in**: `actors/meeting_media.rs::install` via `observability/metrics.rs::record_handler_set_divergence`
- **Dashboard**: MC Overview - Handler Set Divergence (Client Media Signalling row)

### `mc_media_receive_slot_cap`
- **Type**: Gauge
- **Description**: The configured maximum receive slots per declaration (`MC_MAX_RECEIVE_SLOTS`)
- **Labels**:
  - `key_custody`: single value `operator`
- **Cardinality**: 1
- **A CONFIG ECHO, NOT A UTILISATION GAUGE.** Set once, when the WebTransport server is built, from the very `ClientMediaConfig::max_receive_slots` every connection's capability parse compares a declaration against — so the published cap and the enforced cap are one value. There is no numerator: do not build a "slots used / cap" ratio on it.
- **Usage**: Read beside `mc_media_receive_capability_declarations_total{outcome="slot_count_over_cap"}` — a rising over-cap rate with a cap below the client's configured N is a configuration mismatch, not a client bug (R-1)
- **Recorded in**: `webtransport/server.rs::WebTransportServer::new` via `observability/metrics.rs::set_receive_slot_cap`
- **Dashboard**: MC Overview - Receive Slot Cap (Client Media Signalling row, stat panel)

### `mc_media_unmatched_plan_slots_total` — RETIRED (story 2)
Retired rather than left as a permanent zero. It counted egress plans for a slot the subscriber never declared; since story 2 plans are DERIVED from the declared slots, so the count is identically zero on every path — a detector that structurally cannot observe anything, and a flat panel that reads as coverage. The invariant it guarded now lives in `media_routing::slots`' exhaustive model check, which fails the build instead. Its story-1 twin, `slot_id_not_planned`, retired with it. **Re-add condition**: if story 5's MH-selects shape returns and the forwarding plan decouples from the declaration again, re-derive the counter from that decoupling — not from this entry's old rationale.

---

## Participant Outbound Delivery Metrics

### `mc_participant_outbound_messages_dropped_total`
- **Expected-empty**: yes — a healthy outbound path drops nothing, so this reads zero when healthy
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
- **RECIPROCAL WITH `mc_media_send_directives_total`.** That counter's `emitted` means a directive was **composed**, not delivered — it fires before the send. `payload_kind="signaling_raw"` is the stream-channel-FULL half of the evidence for what happened next; the participant-mailbox-CLOSED half is `mc_media_slot_view_emissions_total{outcome="delivery_failed"}` (a different, earlier hop — the two are disjoint). **A non-zero `signaling_raw` rate against a healthy `emitted` rate is the specific shape of "MC composed a send directive the client never received".**
- **Usage**: Detect a slow or wedged client connection losing server messages
- **Recorded in**: `actors/participant.rs::handle_send` and `handle_update` via `observability/metrics.rs::record_participant_outbound_dropped`
- **Dashboard**: MC Overview - Dropped Outbound Messages (Client Media Signalling row)

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
  - `error_type`: Error classification — the authoritative value set is `McError::error_type_label()`
    (`crates/mc-service/src/errors.rs`), a wildcard-free match; do NOT re-list it here (an inline copy drifted
    from 18 to the method's full range and is the exact restated-roster rot `mh-service.md` §header warns of).
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
| `actual_type` | 5 | `global-controller`, `media-handler`, `meeting-controller`, `unknown`, `other` — emit-site clamp (`common::service_type::service_type_metric_label`); `other` = present-but-unrecognized claim |

**Total Estimated Cardinality**: ~115 time series (well within Prometheus limits) — +4 `state` (participant MH status) and +2 `reason` (mh-status drop: `cap`, `over_limit`) over the prior ~105, plus a bounded `actual_type` clamp domain (`other` bucket + `meeting-controller` now zero-init'd) on `mc_caller_type_rejected_total`.

---

## References

- **ADR-0011**: Observability standards and metric naming conventions
- **ADR-0023**: Meeting Controller design (Section 11: Observability)
- **Implementation**: `crates/mc-service/src/observability/metrics.rs`
- **Dashboard**: `infra/grafana/dashboards/mc-overview.json`
