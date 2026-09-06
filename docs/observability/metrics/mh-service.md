# Media Handler Metrics Catalog

**Service**: Media Handler (mh-service)
**Implementation**: `crates/mh-service/src/observability/metrics.rs`
**Job Label**: `mh-service-local` (local development), `mh-service` (production)

All MH service metrics follow ADR-0011 naming conventions with the `mh_` prefix.

---

## GC Registration Metrics

### `mh_gc_registration_total`
- **Type**: Counter
- **Description**: Total GC registration (RegisterMH) attempts
- **Labels**:
  - `status`: Outcome (`success`, `error`)
- **Cardinality**: Low (2 values)
- **Usage**: Monitor registration success rate, detect GC connectivity issues

### `mh_gc_registration_duration_seconds`
- **Type**: Histogram
- **Description**: GC registration RPC latency
- **Labels**: None
- **Buckets**: [0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000]
- **Usage**: Monitor registration latency, detect slow GC responses

**PromQL example** - registration error rate:
```promql
sum(rate(mh_gc_registration_total{status="error"}[5m]))
  / sum(rate(mh_gc_registration_total[5m]))
```

---

## GC Heartbeat Metrics

### `mh_gc_heartbeats_total`
- **Type**: Counter
- **Description**: Total GC heartbeat (SendLoadReport) attempts
- **Labels**:
  - `status`: Outcome (`success`, `error`)
- **Cardinality**: Low (2 values)
- **Usage**: Monitor heartbeat success rate, detect staleness risk

### `mh_gc_heartbeat_latency_seconds`
- **Type**: Histogram
- **Description**: GC heartbeat RPC latency
- **Labels**: None
- **Buckets**: [0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000]
- **SLO Target**: p95 < 100ms
- **Usage**: Monitor heartbeat latency, detect network issues

**PromQL example** - heartbeat p95 latency:
```promql
histogram_quantile(0.95, rate(mh_gc_heartbeat_latency_seconds_bucket[5m]))
```

---

## Token Refresh Metrics

### `mh_token_refresh_total`
- **Type**: Counter
- **Description**: Total OAuth token refresh attempts
- **Labels**:
  - `status`: Outcome (`success`, `error`)
- **Cardinality**: Low (2 values)
- **Usage**: Monitor token refresh health

### `mh_token_refresh_duration_seconds`
- **Type**: Histogram
- **Description**: OAuth token refresh latency
- **Labels**: None
- **Buckets**: [0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000]
- **Usage**: Monitor AC response times

### `mh_token_refresh_failures_total`
- **Type**: Counter
- **Description**: Token refresh failures by error type
- **Labels**:
  - `error_type`: Error category (`http`, `auth_rejected`, `invalid_response`, `acquisition_failed`, `configuration`, `channel_closed`)
- **Cardinality**: Low (6 values)
- **Usage**: Diagnose token refresh failures

**PromQL example** - token refresh error rate:
```promql
rate(mh_token_refresh_total{status="error"}[5m])
```

---

## WebTransport Connection Metrics

### `mh_active_connections`
- **Type**: Gauge
- **Description**: Number of active WebTransport connections on this MH instance
- **Labels**: None
- **Usage**: Monitor connection load and capacity utilization
- **Dashboard**: MH Overview - Active Connections gauge

### `mh_webtransport_connections_total`
- **Type**: Counter
- **Description**: Total WebTransport connection attempts by outcome
- **Labels**:
  - `status`: Connection outcome (`accepted`, `rejected`, `error`)
- **Cardinality**: Low (3 values)
- **Usage**: Monitor connection acceptance rate, capacity rejections, and connection errors
- **Dashboard**: MH Overview - WebTransport Connections by Status

### `mh_webtransport_handshake_duration_seconds`
- **Type**: Histogram
- **Description**: Duration from WebTransport session accept through JWT validation
- **Labels**: None
- **Buckets**: [0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.000, 5.000]
- **Usage**: Monitor handshake latency, detect slow JWKS lookups or TLS issues
- **Dashboard**: MH Overview - Handshake Latency P50/P95/P99

**PromQL example** - connection rejection rate:
```promql
sum(rate(mh_webtransport_connections_total{status="rejected"}[5m])) /
sum(rate(mh_webtransport_connections_total[5m]))
```

---

## JWT Validation Metrics

### `mh_jwt_validations_total`
- **Type**: Counter
- **Description**: Total JWT validation attempts by result, token type, and failure reason
- **Labels**:
  - `result`: Validation outcome (`success`, `failure`)
  - `token_type`: Token type (`meeting`, `service`)
  - `failure_reason`: Reason for failure (`none`, `signature_invalid`, `expired`, `scope_mismatch`, `malformed`, `validation_failed`)
- **Cardinality**: Low (2 x 2 x 6 = 24 max, sparse in practice)
- **Usage**: Monitor authentication health, detect token validation failures, diagnose failure causes
- **Dashboard**: MH Overview - JWT Validations by Result

**PromQL example** - JWT failure rate:
```promql
sum(rate(mh_jwt_validations_total{result="failure"}[5m])) /
sum(rate(mh_jwt_validations_total[5m]))
```

**PromQL example** - failures by reason:
```promql
sum by(failure_reason) (rate(mh_jwt_validations_total{result="failure"}[5m]))
```

---

## gRPC Auth Layer 2 Metrics (ADR-0003)

### `mh_caller_type_rejected_total`
- **Type**: Counter
- **Description**: Total caller service_type rejections by Layer 2 routing
- **Labels**:
  - `grpc_service`: Target gRPC service name (`MediaHandlerService`)
  - `expected_type`: Expected caller service_type (`meeting-controller`)
  - `actual_type`: Actual caller service_type (e.g., `global-controller`, `unknown`)
- **Cardinality**: Low (1 x 1 x 3 = 3 max)
- **Usage**: Detect misconfigured services calling wrong gRPC endpoints
- **ALERT**: Any non-zero value indicates a bug or misconfiguration

**PromQL example** - caller type rejection rate:
```promql
rate(mh_caller_type_rejected_total[5m])
```

---

## MC Notification Metrics

### `mh_mc_notifications_total`
- **Type**: Counter
- **Description**: Total MH→MC notification delivery attempts by event type and outcome
- **Labels**:
  - `event_type`: Notification event type (`connected`, `disconnected`)
  - `status`: Delivery outcome (`success`, `error`)
- **Cardinality**: Low (2 event types x 2 statuses = 4 series)
- **Usage**: Monitor MH→MC notification delivery health, detect MC connectivity issues
- **Dashboard**: MH Overview - MC Notification Delivery

**PromQL example** - notification failure rate:
```promql
sum(rate(mh_mc_notifications_total{status="error"}[5m])) /
sum(rate(mh_mc_notifications_total[5m]))
```

---

## RegisterMeeting Metrics

### `mh_register_meeting_timeouts_total`
- **Type**: Counter
- **Description**: Count of provisional WebTransport connections disconnected because `RegisterMeeting` did not arrive within the configured timeout (R-26). Fires only from the provisional-accept timeout arm in `webtransport::connection::handle_connection`; shutdown-driven cancellation does not increment this counter.
- **Labels**: None
- **Cardinality**: 1
- **Usage**: Detect MC → MH `RegisterMeeting` latency issues or client-side sequencing bugs (client connected before MC finished assignment). A non-zero value is actionable.

**PromQL example** (per ADR-0029 — low-rate lifecycle counter, prefer `increase`):
```promql
# Count panel — how many timeouts in the window? (ADR-0029 Category A)
increase(mh_register_meeting_timeouts_total[$__rate_interval])
```

> **R-26 RegisterMeeting receipt signal**: the user story mentions an `mh_register_meeting_total` metric. That signal is served by the transport-level `mh_grpc_requests_total{method="register_meeting"}` counter (see below) — no separate business-level counter was added, since it would duplicate call-site recordings with identical totals. Note that receipt is *not* application: `mh_media_policy_applies_total` is the signal for whether forwarding policy took effect.

---

## Incoming gRPC Metrics

### `mh_grpc_requests_total`
- **Type**: Counter
- **Description**: Total incoming gRPC requests from MC. Also serves as R-26's `RegisterMeeting` receipt counter when filtered by `method="register_meeting"`.
- **Labels**:
  - `method`: RPC method. **Single-valued by design**: `register_meeting`. `proto/dark_tower/internal/v1/internal.proto`'s `MediaHandlerService` block is the single source of truth for this value set — ADR-0036 §8 makes meeting registration the control plane, so the service gains *fields* rather than sibling RPCs and is expected to keep exactly one method. The `register`, `route_media` and `stream_telemetry` values this entry previously listed were retired with the 2026-09-01 `internal.proto` reshape and **can never appear again**; that file's tombstone block forbids resurrecting the names, because a gRPC method name is a wire path component.
  - `status`: Outcome (`success`, `error`)
- **Cardinality**: Low (2 = 1 method x 2 statuses)
- **Usage**: Monitor MC→MH traffic volume and error rate.

The label is retained rather than dropped even though it carries one value: runbook queries and dashboard panels select on `method="register_meeting"`, and an unmatched label selector yields an *empty series* rather than an error — removing it would silently blank those panels during the incident they exist for. What changed is that `record_grpc_request` no longer takes a `method` parameter; the value is a constant inside the recorder, so a second value cannot be introduced from a call site.

**Scope note.** This counter covers the *transport* outcome — did the RPC succeed. It does **not** tell you whether forwarding policy took effect; that is `mh_media_policy_applies_total` below, and ADR-0036 §8 is explicit that `accepted == true` means only "received and parsed".

**PromQL examples** (per ADR-0029):
```promql
# Count panel — how many RegisterMeeting receipts by outcome?
increase(mh_grpc_requests_total{method="register_meeting"}[$__rate_interval])

# Ratio panel — error rate (ratio requires rate/rate)
#
# `sum()` on BOTH sides is load-bearing, not style. Prometheus matches binary
# operands on their full label sets: without it the numerator's
# {status="error"} series matches only itself, the {status="success"} series
# has no partner and is dropped, and the panel reads a flat 100% whenever any
# error exists and "No data" when none does.
sum(rate(mh_grpc_requests_total{status="error"}[5m]))
  / sum(rate(mh_grpc_requests_total[5m]))
```

---

## Media Policy Metrics

The ADR-0036 §8 MC→MH control plane. Deliberately adjacent to
`mh_grpc_requests_total` above rather than filed at the end of the document: a
responder comparing *receipt* against *application* should not have to jump
sections. The heading exists because these are not gRPC transport metrics, and
because story task 21's media metrics need a section to extend.

### `mh_media_policy_applies_total`
- **Type**: Counter
- **Description**: Outcome of every ADR-0036 §8 forwarding-policy application MH considered. Incremented exactly once per `RegisterMeeting` call that reaches the **policy boundary** — the first read of a policy-bearing field (`egress_streams`, `selection_rules`, `policy_generation`) — on every terminal path from there onward.
- **Labels**:
  - `outcome`: what MH did with the policy (5 values, table below)
  - `key_custody`: `operator` (single value)
- **Cardinality**: Low (5 = 5 outcomes x 1 custody)
- **Usage**: The primary health signal for the MC→MH control plane. `rejected_stale`, `rejected_invalid` and `apply_failed` each indicate a different broken thing with a different owner.

**Why `outcome` and not `status`.** `status` is this repo's *coarse, fleet-wide shared* classification (`success`/`error`/…) that a responder compares across services. This label is a *fine-grained, metric-local* taxonomy in which **each value names a distinct remedy**. `internal.proto` already names the MC-side counterpart of this same RPC `outcome`, so after MC's half lands both ends of one handshake carry the same label key and the two series can be read side by side. See `docs/observability/label-taxonomy.md` §Shared Label Names.

| `outcome` | Condition | What a responder does |
|---|---|---|
| `applied` | The live forward path reflects the generation MC sent — a fresh install, **or** an idempotent re-assert of an already-installed generation (no snapshot swap). | Nothing. This is health. |
| `rejected_stale` | A generation strictly lower than the installed one was ignored. | Investigate reordering or retry on the MC→MH path. Policy is not rolled back. |
| `no_generation` | `policy_generation` was 0 — MC named no generation. | **See the window note below.** Today: nothing. |
| `rejected_invalid` | The policy failed structural validation (duplicate `egress_stream_id`, duplicate subscriber slot, malformed identifier, count bound, unspecified or heterogeneous transport mode). | MC sent bad policy; the remedy is upstream in MC's assignment computation. |
| `apply_failed` | MH could not install it: config-apply mailbox full, apply timed out, session actor gone, or the aggregate egress-edge bound. **The prior generation stays live.** | MH-side. Read the `reason` field on the accompanying `mh.session.policy` WARN log line to tell the causes apart — it is deliberately not a label. |

The idempotent re-assert counts as `applied`, not `rejected_stale`: §8's cadence re-asserts every meeting every ≤10 s in perfect health, so counting it as a rejection would drive that series monotonically upward in the steady state and make any alert on it dead on arrival.

> **`no_generation` is now a ROLLOUT signal, not a steady state — read this before treating it as an incident, and before treating it as always-benign.**
>
> MC emits `policy_generation` >= 1 as of story task 13 (2026-09-02), so the healthy steady state is now `applied` carrying the traffic and `no_generation` at **zero**.
>
> **`no_generation` counting up is expected ONLY while an MC rollout is in progress.** A not-yet-upgraded MC pod legitimately still sends 0, so during a rolling deploy the two series coexist and the mix shifts as pods cycle. That is not an outage and is not worth an incident.
>
> **Sustained `no_generation` after the rollout completes means MC failed to compute a generation** for those meetings — an MC-side fault, not an MH one. The remedy is upstream: check MC's `mc_media_policy_pushes_total` and MC's `mc.register_meeting.trigger` logs for an assignment that could not be computed.
>
> MH still deliberately does **not** reject `policy_generation` 0. `internal.proto::RegisterMeetingRequest.policy_generation` carries the ordering constraint in full, and MC and MH roll independently: flipping the rejection in the same change would let whichever side rolls first decide, and MH-first rejects every registration from a not-yet-upgraded MC pod — no meeting registered, every client provisionally accepted and kicked at the registration timeout, which is ADR-0036 §8's opening paragraph almost verbatim, "a permanent media blackhole for that meeting until it emptied, reached through an ordinary rolling deploy". Enforcement is a separate change whose precondition — all MC pods emitting >= 1 — task 13 supplies but does not itself verify.
>
> `no_generation` is deliberately the same value the *rejection* will land on once that enforcement turns on — two eras, one value, no rename, no dashboard edit.

**Not label material** (ADR-0036 §11): `policy_generation`, `applied_generation` and `process_start_epoch_ms` are unbounded — one new series per policy change or per restart — and go in the log line and in this metric's *value*, never in a label. Neither is any stream or meeting identity: `egress_stream_id`, `sender_id`, `slot_id`, `stream_number`, `participant_id`, or a meeting id **raw or hashed**. A `sender_id` label would be specifically hazardous: it is a per-meeting ordinal, so the same value recurs across every meeting on the handler and emitting it unqualified is a cross-meeting correlation handle. `handler_id` would be *permitted* (pod level is §11's stated aggregation floor) but is deliberately not added — the scrape already carries `instance`/`pod`.

**`key_custody`** records that ADR-0036 §4 media is encrypted between clients, that MH cannot read it, and that MC can — accepted operator custody, recorded as the user's risk decision. It exists **in place of** an end-to-end or zero-trust boolean, which no metric, log, dashboard or document may carry: the label states a fact, a boolean would state a product claim an operator might repeat to a customer.

**PromQL examples** (per ADR-0029 — low-rate control-plane counter, prefer `increase`):
```promql
# Count panel — policy applies by outcome
sum by(outcome) (increase(mh_media_policy_applies_total[$__rate_interval]))

# Ratio panel — share of considered policies that did not install
sum(rate(mh_media_policy_applies_total{outcome=~"rejected_stale|rejected_invalid|apply_failed"}[5m]))
  / sum(rate(mh_media_policy_applies_total[5m]))
```

The denominator is meaningful because the counter increments on every post-boundary path: `sum(mh_media_policy_applies_total)` is "registrations whose policy MH considered". Pre-boundary rejects (a malformed `mc_grpc_endpoint`, an over-length `mc_id`) are **not** counted here — they land on `mh_grpc_requests_total{status="error"}` alone, because "MC's assignment computation produced a policy MH will not apply" and "this caller's endpoint is malformed" have different owners and different remedies.

No alert rule ships with this metric. A useful threshold depends on MC's re-assert cadence, which task 13 explicitly deferred to the handler-restart story; alerting is story task 21's scope.

---

## Media Forward Path

The ADR-0036 §2/§7 audio datagram forward path and its §11 telemetry. Placed
immediately after the control plane above because a responder triaging "no
audio" reads them together: the control plane says *whether MH was told what to
forward*, and these say *what it did with the frames*.

Every metric here carries `key_custody=operator` and none carries a
participant, stream-identity or meeting dimension. See §"Not label material"
under Media Policy Metrics — that block is **extended** by this section, not
restated.

**A blind spot that applies to this whole section, stated first because a
reader who takes any single counter at face value will be misled.** MH is
keyless and never opens a frame, so it cannot observe any crypto- or key-layer
condition; and quinn's datagram send buffer evicts silently one layer beneath
MH's own egress queue. Both are elaborated at the entries below.
**Absence of a drop signal is not evidence that forwarding is healthy.**

> **READ THIS BEFORE READING ANY PANEL IN THIS SECTION: every metric here reads
> ZERO on a production pod today, and that is not "no load".**
>
> MH declines to start its media tasks for a connection whose participant it
> cannot resolve to a `sender_id`, and **no contract in the tree carries that
> association yet** — so today it declines for *every* connection. The forward
> path is complete, tested and unreached in production.
>
> Every handle in this family is resolved at process start, so the series all
> exist and render **present and flat at zero**: `frames_forwarded` 0, every
> `frames_dropped{reason}` 0, the latency histogram empty. That is
> **byte-identical to a healthy idle handler**. This section's own rule —
> *absence of a drop signal is not evidence that forwarding is healthy* — is
> what condemns reading it as health.
>
> **The sharpest read, and it is a scroll rather than a hunt.** On the MH
> overview dashboard, `mh_media_policy_applies_total{outcome="applied"}` sits
> **one row above this section, under `MC Coordination & Outbound Auth`**, and
> reads **genuinely healthy** in this state — MC did program this handler, and
> MH did apply the policy. So **control-plane healthy while every forward-path
> panel reads zero** is the signature: MH was told what to forward and is not
> forwarding it. That is this section's opening framing used as a diagnostic.
> The two rows are adjacent and neither is collapsed by default. (The row is
> named rather than described as "the same section" because row collapse is a
> one-click change nothing guards, and a responder who looks inside
> `Media Forward Path` for a panel that is not there concludes the doc describes
> a different dashboard.)
>
> **The confirming discriminator, if the panels above are not to hand:**
>
> ```promql
> sum(rate(mh_webtransport_connections_total{status="accepted"}[5m])) > 0
> ```
> while `mh_media_frames_forwarded_total{direction="ingress"}` is flat at 0 **and**
> every `mh_media_frames_dropped_total` reason is 0
> ⇒ the media path never started for any connection, rather than an idle handler.
> Confirm in the pod log: `Media forward path not started: no sender_id is bound`.
>
> **Obligation (open)** — the honest positive signal is a counter of media
> sessions that declined to start, e.g.
> `mh_media_sessions_total{outcome="started"|"declined_no_sender_binding"}`. It
> must **not** be folded into `mh_media_frames_dropped_total`: no frame was
> dropped, and this section defines forwarded + dropped = attempts, so a decline
> would corrupt the drop-rate denominator. Owner: observability for the name and
> label space, media-handler for emission; lands with whichever task makes the
> binding real, and **lands first if MH is to be deployed anywhere real before
> that contract does**. Tracked in `docs/TODO.md` §Media Path Obligations under
> "R-15 IS NOT SATISFIED IN PRODUCTION".

### `mh_media_frames_forwarded_total`
- **Type**: Counter
- **Description**: Frames MH accepted into the forward path (`direction=ingress`) and frames the transport accepted for a subscriber (`direction=egress`).
- **Labels**:
  - `direction`: `ingress` (publisher→relay) or `egress` (relay→subscriber)
  - `key_custody`: `operator` (single value)
- **Cardinality**: Low (2)
- **Usage**: **This is the drop-rate denominator.** The bytes counter cannot denominate a frame count, which is the first thing a future reader will try to delete this as redundant with.

`direction` is **pipeline-relative and never participant-relative**. `uplink`
and `downlink` are barred: both readings are 2-valued and a catalog entry cannot
tell them apart, but only the pipeline-relative one is structurally incapable of
growing a third value that individuates a participant.

**Build the drop rate on the attempts SUM, never on one addend**
(`alert-conventions.md` §"the attempts-denominator inversion"):

```promql
# Egress drop rate. The denominator is forwarded + dropped, i.e. ATTEMPTS.
sum(rate(mh_media_frames_dropped_total{direction="egress"}[$__rate_interval]))
  / (
      sum(rate(mh_media_frames_forwarded_total{direction="egress"}[$__rate_interval]))
    + sum(rate(mh_media_frames_dropped_total{direction="egress"}[$__rate_interval]))
    )
```

A frame that decoded cleanly but reached no edge counts on
`forwarded{direction="ingress"}` **and** on `dropped{reason="no_subscriber"}`:
it was ingested successfully and delivered nowhere. That keeps the ingress
attempts sum whole — every datagram received is either an ingress drop or an
ingress forward — while attributing the failure to the egress-side cause it
actually has.

**EVERY egress series is counted per EDGE except one.** `forwarded{direction="egress"}`
and every `direction=egress` drop reason — `egress_queue_overflow`,
`connection_closed`, `transport_send_refused`, `no_local_subscriber`,
`relay_rewrite_failed`, `partial_frame_discard` — count once per
(frame × subscriber), because that is the unit at which each outcome happens.
**`no_subscriber` is the exception and counts once per FRAME**, because the frame
reached *zero* edges and there is no per-edge event to count. Two consequences a reader of the drop-rate expression needs: under
fan-out the numerator mixes the two units, and in a meeting where nobody is
subscribed the egress drop rate reads 100% off a single frame. Both are
defensible and neither is inferable from the series names, which is why they are
stated here.

### `mh_media_frames_dropped_total`
- **Type**: Counter
- **Description**: Frames MH did not forward, by reason.
- **Labels**:
  - `reason`: one layered label space with two families, below
  - `direction`: `ingress` or `egress`; each reason has exactly one
  - `key_custody`: `operator` (single value)
- **Cardinality**: Low (19 = 11 MH-local + 8 codec)
- **Usage**: The primary "why is there no audio" signal. Read it with the forwarded counter, never alone.

**`reason` is ONE layered label space, not two vocabularies.**
`media-protocol`'s `reject_reasons!` macro documents its eight tokens as "the
structural / parse subset of a **shared** `reason` label space", and **MH is a
first-class member of the codec layer** — `RejectReason::producible_by()` names
`rewrite_relay_region` as a producing entry point. MH emits the codec tokens
**verbatim** via `RejectReason::as_str()`, iterating the codec's own generated
list so a ninth token upstream gets an MH series automatically. It never
re-spells one, and it never invents an MH spelling for a condition the codec
already names.

**Codec family** (`direction=ingress`, always). Every codec token MH emits comes
from `decode_datagram`, i.e. names a **sender-side** condition. There is
deliberately **no `entry_point` label**: a failure from `rewrite_relay_region`
on an already-decoded frame is not a sender fault at all and carries its own
`relay_rewrite_failed` token instead, which resolves the mis-attribution
structurally rather than with a dimension. `RejectReason::operator_meaning()`
carries one triage sentence per token, and
`RejectReason::reachable_on_stream_carried_frames()` — not a token's name — is
what a future video panel or alert must consult to decide which series it can
ever populate.

**MH-local family.** Grouped by what a responder does, because "this counter
moved" means *investigate the sender or the load* for one group and *we have a
bug* for the other:

*Should-read-zero invariant-violation tokens.* A counter that should read zero
forever is a good counter; one that mixes expected shedding with an invariant
violation can never be alerted on.

| `reason` | `direction` | Condition | What a responder does |
|---|---|---|---|
| `transport_send_refused` | egress | The seam refused a datagram it should have accepted (`TooLarge`, `DatagramsUnsupported`, or the test double's `WouldBlock`). | The `EgressQueueDoesNotBindFirst` ordering premise broke, or MH built an oversize datagram. Check `MH_DATAGRAM_BUFFER_AUDIO_FRAMES` against the application bound. |
| `relay_rewrite_failed` | egress | `rewrite_relay_region` failed on a frame that had already decoded cleanly. | **An MH bug.** Not a sender fault: the frame passed full header validation moments earlier. |
| `partial_frame_discard` | egress | A partially-read stream frame was given up on. | **Unreachable until video.** Audio is one frame per datagram, so this has no producer today; it exists because the `decode_stream_frame` `Ok(None)` caller obligation names MH as an owner and a counted give-up is the whole point of it. |

*Saturation-or-input tokens.* Expected under load or under a misbehaving
sender; none is a bug on its own.

| `reason` | `direction` | Condition | What a responder does |
|---|---|---|---|
| `ingress_queue_overflow` | ingress | MH's bounded ingress ring shed its oldest frame. | MH ingest is saturated — the forward loop is not keeping up with the receive loop. Look at CPU and at handler load, not at the sender. |
| `egress_queue_overflow` | egress | MH's bounded egress ring shed its oldest frame. | A slow subscriber. **This is expected load shedding**, not a fault; read the blind-spot note below before concluding anything from its level. |
| `connection_closed` | egress | The subscriber's connection closed mid-flight. | **Routine** — a participant leaves every meeting, many times. A *spike* is real signal (a client-side crash loop, a network event); a steady trickle is normal. Deliberately split from `transport_send_refused` so that counter stays alertable. |
| `oversize_datagram` | ingress | The received datagram's byte length exceeded the wire-format frame maximum, rejected before any parse. | A sender build defect or a hostile peer. **Not** the codec's `payload_length_exceeds_max`, which is the declared *field* exceeding the max during header validation — same constant, two checks, two tokens, and the distinction tells you whether anything was parsed at all. |
| `stream_rate_limited` | ingress | A connection exceeded its per-connection stream creation-rate cap. | **UNREACHABLE UNTIL UNI-STREAM/VIDEO — this token cannot fire today and is not a live detector.** MH opens no unidirectional-stream accept loop and audio is one frame per datagram, so there is no stream-creation event to limit. The token is defined now so the author of the accept path inherits the spelling rather than inventing a second one; a fixed-window limiter was built for it at task 16 and **removed at review** as unreachable enforcement machinery. Same posture as `partial_frame_discard`. |
| `no_policy` | ingress | No forwarding policy is installed for this meeting. | **The remedy is the control plane, not this handler.** Read `mh_media_policy_applies_total` and MC's `mc_media_policy_pushes_total`. |
| `no_subscriber` | egress | A policy is installed but no egress edge names this sender. | MC's assignment. The publisher is connected and nobody is subscribed to it. |
| `no_local_subscriber` | egress | An edge names a subscriber with no connection on this handler. | Ordinary under multi-handler assignment (§9), where a meeting's participants are spread across handlers. Sustained and unexpected means a stale assignment. |

**MH must never emit a crypto- or key-layer token** — `signature_invalid`,
`decrypt_failed`, `unwrap_failed`, `replay_detected`, `wrap_key_id_mismatch`,
`no_kek_for_generation`, `no_roster_entry`, `no_transmit_key`. MH is keyless and
never opens a frame, so such a series would assert a verification MH is
structurally incapable of performing — and an operator reads it as "MH validates
frames", after which someone relies on a control that does not exist. Those
tokens are the client's (story task 19). The bar is executable:
`crates/mh-service/tests/media_metrics_integration.rs` asserts MH's own
vocabulary is disjoint from all sixteen tokens in
`proto/test-vectors/frame-v2.vectors.json`.

> **Blind spot 1 — relay-side corruption has NO MH-side signal, by
> construction.**
>
> The relay-region offset is derived per frame from the frame's own header,
> never precomputed, because it moves with the key-bearing flag and the
> extension length — both of which are live on audio. If a relay-side offset
> defect ever overwrote part of the *signed* publisher region, **every receiver
> would count `signature_invalid` and MH would count nothing at all** — correctly
> so, since MH cannot observe crypto-layer conditions. So the structural
> guarantee in the code is the **only** control here; there is no detector
> behind it. **Absence of a drop signal is not evidence that forwarding is
> healthy.**
>
> **Runbook obligation (open)** — owner: operations, story task 21. The
> media-datagram-drop scenario must carry the correlation rule: *a client-side
> `signature_invalid` rate with a flat MH drop series points at the relay, not
> at the sender.* Recorded here rather than left in a review thread because
> reviewer messages do not survive to task 21 and this file does.

> **Blind spot 2 — `egress_queue_overflow` reads FLAT at exactly the moment loss
> is worst, and it is UNMITIGATED today.**
>
> This counter observes **MH's own application queue and nothing else**. It is
> never a transport signal: quinn's datagram send buffer sits one layer beneath
> it and evicts the oldest queued datagram silently — no `ConnectionStats`
> field, no lost-packet accounting (an evicted datagram was never transmitted),
> only a trace-level log line, which ADR-0036 §11 rules out in terms ("A log
> level is not an acceptable gate"). Under congestion severe enough to saturate
> quinn's buffer but not MH's, **"no loss" and "loss we cannot see" are
> indistinguishable on this signal.**
>
> ADR-0036 §1 decided the mitigation — size the transport buffer so the
> application bound trips first — and MH startup-validates that relationship
> (`ConfigError::EgressQueueDoesNotBindFirst`). But that holds only *while the
> configured relationship holds*: it does not make the counter honest under
> misconfiguration, nor if quinn's buffer is pressured by anything other than
> MH's queue depth.
>
> **Obligation (open)** — owner: client, story task 19. The honest compensating
> control is the **far-end hop-sequence gap** counter: MH assigns
> `hop_sequence` before handing the datagram to the transport, so a
> quinn-evicted datagram consumes a hop number and never arrives, and the
> receiver observes a genuine gap. **That counter does not exist anywhere in the
> tree today** — `hop_sequence` appears only as a codec field and in test
> vectors — so this blind spot is currently **unmitigated**, and this entry says
> so rather than cross-referencing a control nobody built. An MH-side *ingress*
> hop-gap counter would not substitute: the eviction is on MH's **downlink**, so
> only the client can see it.

### `mh_media_forward_latency_seconds`
- **Type**: Histogram
- **Description**: MH-internal forwarding latency, decomposed. Ingress-from-network to egress-to-network, split into the three stages that have different remedies.
- **Labels**:
  - `phase`: `receive_buffer` | `processing` | `transmit_buffer` | `total`
  - `key_custody`: `operator` (single value)
- **Buckets**: 50 µs → 100 ms (`[0.00005, 0.0001, 0.00025, 0.0005, 0.001, 0.0025, 0.005, 0.010, 0.030, 0.050, 0.100]`)
- **Cardinality**: Low (4 series × bucket count)
- **Usage**: Which stage to pursue. An undifferentiated total does not tell an operator whether the problem is ingest, routing, or a slow subscriber.

| `phase` | Spans | A rise means |
|---|---|---|
| `receive_buffer` | datagram received → popped off the ingress ring | The forward loop is not keeping up with the receive loop. Pairs with `ingress_queue_overflow`. |
| `processing` | ingress pop → pushed onto a subscriber's egress ring | Routing, the relay rewrite and fan-out. A rise here is MH's own work getting slower — an unusually wide fan-out, or CPU starvation. |
| `transmit_buffer` | egress push → the transport send call returned | A slow subscriber. Pairs with `egress_queue_overflow`. |
| `total` | received → send returned | The headline. Always read with the three components. |

**Sampled, and the sampling is deliberate.** Timestamps are taken on **every**
frame (a monotonic clock read is a ~25 ns vDSO call and the ingress timestamp is
needed for frame age regardless); the histogram observation happens one in N,
where N is published by `mh_media_latency_sample_ratio`. The sampling is
**random per frame, never deterministic per stream** — ADR-0036 §11 bars the
natural modulo implementation because it perfectly reconstructs which frames of
a single stream were observed, which is the voice-activity trace.

**`mh_media_forward_latency_seconds_count` is 1/N of the forwarded counter. Do
not divide the two directly.** They count different populations by
construction, and a ratio of them measures the sample ratio rather than
anything about the media path.

**The objective is provisional.** `MEDIA_FORWARD_OBJECTIVE_SECONDS` = 30 ms is
recorded in code and in `slos.md` as *provisional pending story 8*: ADR-0011's
`< 30 ms` figure is not ratified against this measurement point, which is
MH-internal rather than end-to-end. **No burn-rate alert may rest on it until it
is ratified.** A unit test pins the objective to an exact bucket edge, because a
quantile read at a non-edge value is interpolated and an off-edge objective
would silently become an estimate of an estimate.

### `mh_media_latency_sample_ratio`
- **Type**: Gauge
- **Description**: The fraction of forwarded frames whose latency is observed.
- **Labels**: `key_custody`: `operator`
- **Cardinality**: 1
- **Usage**: Interpretation input for the histogram above. Read it before reading a rate off `_count`.

Published from **the same configuration field the sampler draws against**
(`MH_MEDIA_LATENCY_SAMPLE_RATIO`, optional; the default is
`mh_service::config::DEFAULT_MEDIA_LATENCY_SAMPLE_RATIO` and is deliberately not
restated here, because a number written into a document is a second encoding of
a config value). A ratio computed
separately from the one in force is a gauge that lies exactly when someone is
using it to interpret a histogram. Not to be confused with `OTEL_SAMPLE_RATE`,
which is trace head-sampling with a different owner and a different consumer.

### `mh_media_egress_queue_depth`
- **Type**: Gauge
- **Description**: Occupancy of **one** subscriber's egress queue, in frames — a single process-wide gauge, last-writer-wins across queues. See below.
- **Labels**: `key_custody`: `operator`
- **Cardinality**: 1
- **Usage**: Trend only. Pairs with `egress_queue_overflow` and the `transmit_buffer` phase.

**This is MH's application queue, NOT quinn's send buffer.** `wtransport`
exposes no send-buffer accessor on its `Connection` (its surface is
`send_datagram`, `close`, `session_id`, `remote_address`, `stable_id`,
`max_datagram_size`, `rtt`, `export_keying_material`, `peer_identity`,
`handshake_data`); quinn's own `datagram_send_buffer_space` **does** exist and
is reachable through the raw-QUIC escape hatch, which MH deliberately does not
take — and it reports remaining space in a **connection-wide** buffer, a
different quantity from the per-subscriber media queue this gauge reports, so
taking the hatch would not produce this number anyway.

**It is ONE process-wide gauge fed by N per-subscriber queues, and the value is
last-writer-wins.** MH runs one egress queue per subscriber connection, and
every push and every drain on every connection writes this single series. **A
saturated queue on one subscriber is therefore invisible if an idle subscriber
was written a microsecond later.** The `max(...)` in the dashboard expressions
maxes across Prometheus *instances*, not across queues — a reader who sees `max`
and a title of "Egress Queue Depth" will otherwise conclude it is the deepest
queue on the handler, and it is not. A true cross-connection maximum needs
shared state and is deliberately not built here. This is the second and stronger
reason the gauge is trend-only.

> **NO ALERT MAY REST ON THIS GAUGE ALONE.** The scrape interval is orders of
> magnitude longer than this queue's fill-and-drain time, so an instantaneous
> gauge can read healthy across an entire incident. It is a trend input. The
> countable signal is `mh_media_frames_dropped_total{reason="egress_queue_overflow"}`,
> which is a counter and cannot be missed between scrapes.

**PromQL examples**:
```promql
# Where is the time going? All four phases, p95.
histogram_quantile(0.95, sum by(le, phase) (rate(mh_media_forward_latency_seconds_bucket[$__rate_interval])))

# Drops by reason — read WITH the forwarded counter, never alone.
sum by(reason) (rate(mh_media_frames_dropped_total[$__rate_interval]))
```

No alert rule ships with these metrics. Media alerting — including the drop-rate
rule and its fire/apply table — is story task 21's scope, and `slos.md` forbids
a latency burn-rate alert until the objective is ratified.

---

## Error Metrics

### `mh_errors_total`
- **Type**: Counter
- **Description**: Total errors by operation and type
- **Labels**:
  - `operation`: Code path (`registration`, `heartbeat`, `grpc_service`)
  - `error_type`: Error variant (bounded by MhError: `grpc`, `not_registered`, `config`, `internal`, `token_acquisition`, `token_acquisition_timeout`)
  - `status_code`: gRPC-compatible status code
- **Cardinality**: Low (~30 combinations max)
- **Usage**: Global error tracking, alerting on error spikes

**PromQL example** - total error rate:
```promql
rate(mh_errors_total[5m])
```
