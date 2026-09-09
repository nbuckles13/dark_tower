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
  - `status`: Connection outcome — `accepted`, `rejected`, or `error`.
    - `accepted` is incremented at accept time (`webtransport/server.rs`), **before** the media-session decision, so it counts connections that were later declined too. This is why `sum(accepted)` is **not** a denominator for started media sessions — see the "Do not build a ratio against accepted connections" note under `mh_media_session_starts_total`, whose mechanism this is.
    - `error` means the connection **handler failed**. A deliberate, fail-closed **media-session decline is NOT an error** — it is counted at full resolution on `mh_media_session_starts_total{outcome}` and must never also land here. A decline returning `Err` from `close_declined_connection` would double-count it as `error` and, worse, feed the `{status!="accepted"}` immediate-rollback gate (`docs/runbooks/mh-deployment.md`) with expected fail-closed behaviour — a control that inverts under the fault it exists to catch (F9). The regression guard is `media_session_binding_integration.rs`'s decline arms asserting `status="error"` delta 0.
- **Cardinality**: Low — the three `status` values above.
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
  - `outcome`: what MH did with the policy (the `PolicyApplyOutcome` variants, table below)
  - `key_custody`: `operator` (single value)
- **Cardinality**: Low — bounded at the type level by the compile-checked `PolicyApplyOutcome::ALL` × one `key_custody` value; the table below is the operator-facing list, and a second encoding of its length only rots.
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

**The rule covers logs and span attributes, not only labels.** ADR-0036 §11 bars
participant and stream identity — `sender_id`, `slot_id`, stream number — from
metric labels, span attributes **and** media-path log fields alike; MC's catalog
states the same rule at the same width (`mc-service.md` §"Identity rules"). This
file used to state only the label half, which left an auditor comparing the two
free to conclude MH held the rule more narrowly.

Naming the colliding ordinal in the `declined_sender_binding_conflict` log was
proposed and declined. Two reasons, and the first disproved the premise the
proposal rested on: (1) the "bounded by an MC bug, not by traffic" rate argument
fails under the broken-or-racing-unbind cause, where **every reconnect collides**
and the line becomes a connect/reconnect trace for one identifiable stream —
safe only in the case nobody would read it; (2) without the two `participant_id`s,
which are barred under every reading of §11, a bare ordinal cannot be resolved to
who held it, so it does not answer the question it would be added for. The
range-vs-uniqueness distinction lives in the `outcome` label instead.

One counter here is a **lifecycle** counter rather than a control-plane or a
per-frame one: `mh_media_session_starts_total` records whether a connection ever
got as far as being able to forward. It is read first, which is why it is
catalogued first.

**A blind spot that applies to this whole section, stated first because a
reader who takes any single counter at face value will be misled.** MH is
keyless and never opens a frame, so it cannot observe any crypto- or key-layer
condition; and quinn's datagram **send** buffer evicts silently one layer beneath
MH's own egress queue. Both are elaborated at the entries below.
**The two directions are no longer symmetric, and the asymmetry is load-bearing:**
on the **receive** side quinn's eviction is observable **after the fact, per
connection, at teardown only** — `frame_rx.datagram`
counts every DATAGRAM frame it decoded, evicted ones included, which is what
`transport_receive_dropped` differences against — while on the **send** side
there is still no accessor and no counter, only a trace-level log. So an absent ingress-side signal means something **once the
connection has closed**, and nothing at all before then; an absent egress-side
signal still means nothing, ever. Neither direction gives MH a live detector,
and this change did not create one.
**Absence of a drop signal is not evidence that forwarding is healthy.**

> **READ THIS BEFORE READING ANY PANEL IN THIS SECTION: a flat zero across this
> whole family is not "no load", and the counter that tells you which it is
> comes first.**
>
> MH declines to start its media tasks for a connection it cannot bind to a
> `sender_id`, and it closes that connection rather than holding it open. Every
> handle in this family is resolved at process start, so the series all exist and
> render **present and flat at zero** whether MH is idle or failing —
> `frames_forwarded` 0, every `frames_dropped{reason}` 0, the latency histogram
> empty. That is **byte-identical between a healthy idle handler and one that has
> never started a single session**. This section's own rule — *absence of a drop
> signal is not evidence that forwarding is healthy* — is what condemns reading
> it as health.
>
> **`mh_media_session_starts_total` is the discriminator, and its `outcome` value
> names the cause without a hunt:**
>
> | Reading | Meaning, and where to look next |
> |---|---|
> | `outcome="started"` climbing, `mh_media_frames_forwarded_total{direction="ingress"}` flat at 0, **and `transport_receive_dropped` + `no_media_session` also flat** | **Ask this BEFORE client-vs-MH — but only after checking whether connections have closed.** The two drop tokens are emitted at connection teardown, so during an incident whose connections are still open they read flat **whatever is happening**, and this row cannot be used at all. With connections that have since closed, all three flat means no datagram reached this handler; confirm the publisher is transmitting before opening either service. **If sessions are still open, you have no ingress-loss signal — say so and escalate rather than concluding from the flat.** **This row previously read "Sessions start; no uplink arrives. Client-side, not MH." and that instruction misdirected three consecutive triage sessions in 2026-09** — the observed cause was that the only test in the tree which sends a media datagram was `#[ignore]`d, so every session in that fleet legitimately contributed zero frames. |
> | `outcome="started"` climbing, `forwarded{direction="ingress"}` flat at 0, **`transport_receive_dropped` moving** | Datagrams arrived and **MH never read them**. At or after the ingress-loop spawn: the loop is not draining, or quinn evicts faster than it drains. **Forensic, not live** — this token only increments at connection close, so during an ongoing incident it lags by the session length. Read its entry below before attributing: it is an upper bound and is client-influenceable. |
> | `outcome="started"` climbing, `forwarded{direction="ingress"}` flat at 0, **`transport_receive_dropped` flat but `no_media_session` moving** | Frames are arriving on connections MH **declined**. Not this row’s fault path at all — read the decline outcomes below; the remedy is upstream. |
> | `declined_no_sender_binding` climbing | MC answered with no usable ordinal. Several causes — including MC's per-meeting connection registry at capacity, which presents as an elevated-but-flat rate rather than a spike. `mc_media_sender_binding_responses_total` splits them; MH cannot. |
> | `declined_sender_binding_out_of_range` > 0 | MC's allocator **range**. Should read zero forever. Field corruption in transit, or a peer that is not MC. |
> | `declined_sender_binding_conflict` > 0 | Should read zero forever, and it is the only decline where the refused binding could have crossed media between participants. **Check MH's unbind path for that meeting before paging MC** — a broken or racing unbind produces this with MC entirely correct. |
> | `declined_mc_unavailable` climbing | MH→MC **reachability**. Cross-check `mh_mc_notifications_total{event_type="connected",status="error"}` for the attempt-level view. Declines after the full retry budget. |
> | `declined_mc_auth_rejected` climbing | MH's outbound credential failed: MC **refused** it, or MH could not **build** one. **MH's outbound auth — do not open MC's health.** Discriminate on the log: `"Authorization header parse failed"` present **on target `mh.grpc.mc_client`** ⇒ build failure, stay in MH; absent on that target ⇒ refusal, read MC's `mc_caller_type_rejected_total`. **The target qualifier is load-bearing** — the same message is emitted on `mh.grpc.gc_client` from the same pod, off the same service token, so an unqualified grep fails open. `mh_token_refresh_total{status="error"}` (what `MHTokenRefreshFailures` fires on), then `mh_token_refresh_failures_total` by `error_type`, are context for both and separate neither. **No timing tell** — see the entry below. |
> | `declined_mc_endpoint_unknown` climbing | MH has no usable MC endpoint for the meeting and never reached the network. A **registration** fault, not a reachability one — check `mc_grpc_endpoint` on the `RegisterMeeting` that programmed this handler, then `mh_grpc_requests_total{method="register_meeting"}` and MC's `mc_media_policy_pushes_total`. |
> | **Every outcome flat** while `mh_webtransport_connections_total{status="accepted"}` climbs | Connections never reach the binding boundary at all. The only remaining pre-boundary terminus is the **JWT gate** — read `mh_jwt_validations_total{result="failure"}`. |
>
> **A corroborating read, one row up rather than a hunt.** On the MH overview
> dashboard, `mh_media_policy_applies_total{outcome="applied"}` sits **one row
> above this section, under `MC Coordination & Outbound Auth`**. Control-plane
> healthy while every forward-path panel reads zero means MH was told what to
> forward and is not forwarding it — which the table above then attributes. The
> two rows are adjacent and neither is collapsed by default. (The row is named
> rather than described as "the same section" because row collapse is a one-click
> change nothing guards, and a responder who looks inside `Media Forward Path`
> for a panel that is not there concludes the doc describes a different
> dashboard.)

### `mh_media_session_starts_total`
- **Type**: Counter
- **Description**: Connections that attempted to start a media session, by terminal outcome — one increment per connection.
- **Labels**:
  - `outcome`: the values in the table below
  - `key_custody`: `operator` (single value)
- **Cardinality**: bounded at the type level by `MediaSessionStartOutcome::ALL` x one `key_custody` value. Deliberately no restated integer — the outcome table below is the operator-facing artifact, and a second encoding of its length only rots. (The written-out length in `ALL` stays: that one is compile-checked.)
- **Usage**: **Read this first when triaging "no audio".** It is the precondition for every other metric in this section: a handler with no started sessions cannot produce a single frame, forwarded or dropped.

| `outcome` | Condition | Responder's first move |
|---|---|---|
| `started` | Bound; the three media loops were spawned. | Nothing. Health. |
| `declined_no_sender_binding` | MC answered with no usable `sender_id`. | **Upstream in MC.** The union of MC's `0`-answering outcomes, which MH structurally cannot split: `meeting_unknown` (routing/registration fault), `participant_unknown` (join race — transient and self-clearing), `registry_full` (MC's per-meeting connection cap — capacity, **never** self-clears), `user_ambiguous` (one user, two roster entries — **never** self-clears, and reconnecting *causes* it, so the `participant_unknown` remedy is actively harmful here). `mc_media_sender_binding_responses_total` splits them, which is why neither series is redundant. **Discriminate on the ratio, not the label**: a decaying fraction is a race; flat-and-total is a systematic identity mismatch between what MH names and what MC keys on — the shape of the defect this contract's own first cluster run found. |
| `declined_sender_binding_out_of_range` | MC answered above the 16-bit ordinal range. | **Should read zero forever.** MC's allocator **range**. Field corruption in transit, or a mis-versioned or foreign peer answering. |
| `declined_sender_binding_conflict` | The ordinal is already held by a **different** participant in that meeting; MH refused rather than overwrote. | **Should read zero forever.** Two candidate causes MH cannot separate: MC's allocator **uniqueness / non-recycling**, or MH failing to unbind a previous holder. **Check MH's unbind path for that meeting first.** |
| `declined_mc_unavailable` | The RPC reached MC (or the network) and yielded no usable response — timeout, transport failure, or a non-auth error status. | **MC availability / network.** Declines only after the whole retry budget, unlike the two fast reachability outcomes below. |
| `declined_mc_auth_rejected` | MH's outbound credential failed, by **either** of two routes: MC was reachable and **refused** it (`UNAUTHENTICATED` / `PERMISSION_DENIED`), **or** MH could not **build** one from the token it holds — in which case MC's reachability is unknown, MH never sent a request, and MC refused nothing. | **MH's outbound auth, not MC's health**, on both routes. Discriminate on the log line `"Authorization header parse failed"` **emitted on target `mh.grpc.mc_client`** — the qualifier is load-bearing, because MH emits the same message on `mh.grpc.gc_client` from the same pod off the same service token, so an unqualified grep is right for the wrong reason under this very fault and simply wrong when a GC-path parse failure coincides with a genuine refusal. Present on that target ⇒ build failure, stay in MH and escalate to auth-controller, because AC issued a token containing bytes illegal in an HTTP header value; absent ⇒ refusal, read MC's `mc_caller_type_rejected_total`. `mh_token_refresh_total{status="error"}` — the expression `MHTokenRefreshFailures` fires on, so a responder arriving from that alert sees the same query — then `mh_token_refresh_failures_total` broken down by `error_type` for *why*. Both are context on both routes and **separate neither** — a refresh can succeed and return a token MH cannot put on the wire, and a refresh can fail while the previously cached token still builds. An expired service token or a JWKS rotation fires the refusal route for every connection on the handler at once. |
| `declined_mc_endpoint_unknown` | MH has no *usable* MC endpoint for the meeting — none recorded, or one that will not parse as an endpoint — so it never reached the network. | **Registration**, not reachability. Read what the `RegisterMeeting` that programmed this handler carried in `mc_grpc_endpoint`; MH never dialled, so MC's health is not the question. Not retried: every attempt re-parses the same string. |

**`started` means the loops were spawned, not that media flowed.** It is measured
strictly upstream of the frame counters, and that gap is the point — but it
**localises** a fault rather than attributing one. `started` climbing while
`frames_forwarded{direction="ingress"}` stays flat says only that the fault is at
or after the spawn; it does **not** say which side. It is specifically NOT a
client-side discriminator, and reading it as one is a documented incident: see
the first row of the table above. The discriminator is
`mh_media_frames_dropped_total{reason="transport_receive_dropped"}` — **but it
is a teardown-emitted counter and is therefore usable only over connections that
have already closed.** Moving, it means MH received datagrams and never read
them. Flat, it means *either* nothing was sent *or* the connections carrying the
loss are still open — those two are indistinguishable here, and treating flat as
"nothing was sent" during a live incident is the same fail-open mistake as the
retired row above, with a different mechanism. When sessions are still open, MH
has **no** ingress-loss signal at all; that is the documented gap in limitation
1 below, not a reading. A counter that waited for the
first forwarded frame would be a lagging duplicate of the frame counter and would
lose the exactly-once-per-connection property the sum depends on.

> **The ingress identity: what the sum now means, and the bounded residue.**
> With `transport_receive_dropped` and `no_media_session` added — both
> `direction="ingress"` — the sum
> `forwarded{direction="ingress"} + dropped{direction="ingress"}` **changed
> meaning**. It used to be *every datagram the ingress loop read*; it is now
> *every DATAGRAM frame the connection received*, which is what
> `forward.rs`'s comment always claimed and what was not previously true. Any
> panel or rule written against the old reading is now measuring something
> wider — in particular, **the read set requires subtracting the two new
> tokens back out.** The sum accounts for every frame received **except** frames
> still sitting in the ingress ring when the connection is cancelled: the
> forward loop returns on cancellation without draining, so those were counted
> as read (and are therefore excluded from `transport_receive_dropped`) but were
> never forwarded and never dropped. The residue is bounded by the ring itself —
> **at most `INGRESS_QUEUE_FRAMES` frames per connection, once, at
> teardown** — so it cannot accumulate or scale with traffic.
> **Counting the residue as its own drop reason was considered and REJECTED — do
> not re-derive it.** Two grounds. Draining on cancellation to eliminate the
> residue would delay shutdown in order to forward frames no subscriber is
> waiting for, against ADR-0036 §11's "MH sheds media sessions on restart, and
> that is the decision, not merely the current behaviour". And a token for it
> would fire exactly once per connection close with a value bounded by
> `INGRESS_QUEUE_FRAMES`,
> making it a fixed per-teardown tick whose rate tracks connection churn and
> nothing else — noise beside the real reasons.
> **This identity is documented but NOT machine-checked**, and will stay that way
> until the live detector above exists: no test can assert it as an equality,
> because the received term (`frame_rx.datagram`) is MH-side quinn state with no
> metric export, and asserting against a *sent* count is non-deterministic —
> QUIC datagrams are unreliable, so ordinary network loss would red a correct
> implementation. The integration coverage is therefore a one-sided sandwich
> (accounted + unread <= sent, plus a deterministic lower bound on the unread
> counter, plus a non-vacuity floor), which catches over-counting but cannot
> confirm the equality. Treat the identity as a design intent with partial
> coverage, not as a verified invariant. It is documented
> rather than eliminated because draining on cancellation would delay shutdown
> to forward frames nobody is waiting for. Expect the sum to fall short of the
> received count by a small multiple of connection count; that is this residue
> and not a defect.

**This is NOT a frame drop and must never be folded into
`mh_media_frames_dropped_total`.** No frame was dropped. This section defines
`forwarded + dropped = attempts`; a decline counted as a drop puts a non-frame
event in the drop-rate numerator and corrupts the denominator identity. The two
counters have disjoint units — sessions here, frames there.

**Increment boundary, and why it is not one step later.** A connection counts here
once it has passed the JWT gate and **entered the media-session start sequence**,
whose first action is the MC-endpoint lookup — the same "first read of the thing
this metric is about" boundary `mh_media_policy_applies_total` uses. Pre-boundary
rejects (JWT failure, handshake failure, framing errors) stay on
`mh_jwt_validations_total` / `mh_webtransport_connections_total` alone: "this
caller's token is bad" and "MC could not name this participant's sender" have
different owners. Putting the boundary one step later, at "MH issued the RPC",
was considered and rejected: a meeting never registered on this handler would then
close every connection while incrementing only an undifferentiated
`mh_webtransport_connections_total{status="error"}` plus a log line — MH healthy,
forwarding nothing, detectable only in logs, which is the exact failure this
counter exists to close, reproduced one condition earlier.

**Do not build a ratio against accepted connections.**
`sum(mh_media_session_starts_total)` is **not** equal to
`mh_webtransport_connections_total{status="accepted"}`. Connections that hang up
during the binding await, and connections kicked by the provisional-accept
timeout (`mh_register_meeting_timeouts_total`), pass the JWT gate and never reach
the decision. Those two counters are that gap's home.

Cancellation during shutdown does **not** increment: this counter records
decisions reached, and a cancelled await is not a decision.

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
- **Cardinality**: Low — bounded at the type level by the compile-checked `MediaDropReason::ALL` plus `media_protocol`’s `ALL_REJECT_REASONS`, × one `key_custody` value. **Deliberately no restated integer**: the two vocabularies below are the operator-facing artifact, and a second encoding of their combined length only rots — `mc-service.md` §`mc_media_sender_binding_responses_total` records one drifting four times inside a single devloop. The codec half is not merely tedious to restate but **unrestatable in principle**: `resolve_media_handles` builds it by iterating `ALL_REJECT_REASONS`, so a ninth codec token added upstream gets an MH series with no MH edit. (The written-out lengths in `MediaDropReason::ALL` and in the handle array stay: those are compile-checked, and they are the guard.)
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
| `transport_receive_dropped` | ingress | Datagrams the QUIC connection received that **no MH reader ever saw**. Computed exactly at connection teardown as quinn's `frame_rx.datagram` minus the count the ingress loop actually read. | **The client fleet or MH session-start latency — not MH's forward path.** Read the three limitations in the blockquote below this table BEFORE acting on it: it is forensic rather than live, it is an upper bound, and it is client-influenceable. Its ordinary contributor is a client publishing before its `SendDirective` arrives, or reconnecting into a handler it already holds a directive for and racing the new connection's setup. |
| `no_media_session` | ingress | **Not redundant with the row above — see the note beneath this table.** Datagrams received on a connection whose media session MH **declined**, so no ingress loop was ever spawned and every one was discarded. Exact in the sense that matters — there is no reader to overlap with, so the whole received count IS the loss and no differencing is needed. **"Exact" qualifies the no-reader property, not a race with the wire**: the sample is taken *after* the connection closes (deliberately — the MC-unavailable arm sleeps a jittered interval before closing, and a publishing client sends throughout, so sampling earlier would under-count by exactly the window the slowest decline arm holds open), which leaves one residue — a datagram already in flight when the close frame goes out is discarded uncounted. Bounded by one RTT, per connection, once. | **Diagnostic only; do not alert on it.** It is fully explained one entry up by `mh_media_session_starts_total{outcome="declined_*"}`, which names the cause; this token only says how much media the declined connections were carrying. A large value during an MC outage is expected, not additional signal: MH's MC-client retry budget keeps each declined publisher transmitting for tens of seconds before its close lands. |

> **The `transport_send_refused` / `transport_receive_dropped` pairing is
> LEXICAL, NOT SEMANTIC — do not import one's discipline onto the other.** The
> shared `transport_*` prefix says only "the layer below us", in opposite
> directions. Their *groups* are opposite too: `transport_send_refused` is a
> should-read-zero invariant violation, alertable at any non-zero value;
> `transport_receive_dropped` sits in this saturation-or-input table and has a
> legitimately non-zero tail. A reader who infers symmetry from the surface will
> either alert on ordinary noise, or — worse — dismiss a genuine rise as "the
> usual tail" because they half-remember the counter as the routine one. The
> names pair; the readings do not.
>
> **Three limitations on `transport_receive_dropped`, stated here because a
> panel or alert built on it without them will over-claim.**
>
> **1. Forensic, not detective.** The delta is only computable once the ingress
> loop's read count is final, so **nothing increments until a connection
> closes**. Meetings run for an hour; during an ongoing incident this token is
> silent. It does **not** close the "detect silent media loss while it is
> happening" gap, and no dashboard panel, alert annotation or runbook sentence
> may imply that it does. The live form needs one more series —
> `mh_media_datagrams_received_total`, a periodic sum of `frame_rx.datagram`
> over live connections — because the **accounted** side is already live today:
> the **read** set is live today as
> `forwarded{direction="ingress"} + dropped{direction="ingress"} −
> dropped{reason="transport_receive_dropped"} −
> dropped{reason="no_media_session"}`, less the bounded residue described below.
> The two subtractions are load-bearing and easy to omit: both new tokens are
> themselves `direction="ingress"`, so the unsubtracted sum is the **received**
> set, not the read set. (`crates/mh-service/src/media/forward.rs` increments
> `forwarded{ingress}` on the `no_subscriber` path precisely to keep the sum
> whole.) **Why this is worth
> building and not merely nice to have**: without it MH has no detective signal
> for silent ingress loss at all — only a forensic one — and the nearest
> substitute requires a responder to already know the publisher's expected frame
> rate, which is not recorded anywhere. Tracked in `docs/TODO.md`
> §Media Path Obligations, under the R-15 entry.
>
> **1b. It is a union of three windows and cannot be split.** The delta covers
> datagrams that arrived *before* the ingress loop was live, datagrams evicted
> *mid-session* while the loop was behind, and datagrams that arrived *after*
> teardown began. MH cannot separate them without further sampling. The
> pre-session window is expected to dominate, but **do not assume it**: the day
> it does not is the day the mid-session case matters most, and a responder who
> read this token as purely pre-session will look at the client fleet while MH's
> forward loop is the one falling behind.
>
> **1c. There is deliberately no per-connection or per-participant breakdown of
> this counter, and there never can be.** A per-connection count of media frames
> at ~50/s is talk duration for a named participant, and `connection_id` is
> logged beside `participant_id`, so such a breakdown would be joinable back to
> a person. The count is carried in a newtype implementing neither `Display` nor
> `Debug`, which makes exposing it a compile error rather than a review comment.
> There is no getter: the newtype's only method consumes the value and returns
> the difference against the received count, so the per-connection number never
> leaves the module in a form anything could label, log or export. The bar is
> structural, not a comment asking reviewers to be vigilant. Do not propose a
> `sender_id`, `connection_id` or `meeting_id` dimension here.
>
> **2. An upper bound on genuine loss, not a measurement of it.** The delta
> counts every DATAGRAM frame quinn decoded and MH did not read — which includes
> frames `wtransport` itself discarded for a WebTransport session-id mismatch,
> because quinn records `stats.frame_rx` when it decodes the frame and
> `wtransport` drops the mismatch afterwards. Those never had an MH reader and
> never could have.
>
> **3. Therefore client-influenceable, which is why it sits in the
> saturation-or-input group and not the should-read-zero one.** An authenticated
> client can raise this series at will. A should-read-zero classification would
> have handed any such client a lever to force an on-call page, so **no alert on
> this token may use a bare `> 0` trigger** — it needs a rate over a sustained
> window, like every other member of its group.

**Why `transport_receive_dropped` and `no_media_session` are two tokens and
neither may be deleted as redundant.** They share a mechanism — datagrams quinn
received that no MH reader consumed — and differ only by the connection's
outcome: a session that *started* versus one MH *declined*. That is enough,
because the remedies diverge completely (the client fleet and MH's own
session-start latency, versus the control plane and MC), and because **neither
series is a function of the other**: a fleet can produce either with the other
flat, so no arithmetic recovers one from the other. They are also not
interchangeable in volume — MH's MC-client retry budget keeps each declined
publisher transmitting for tens of seconds, so during an MC outage
`no_media_session` can exceed `transport_receive_dropped` by orders of
magnitude, and a single merged series would bury the client-behaviour signal
under a control-plane one exactly when both matter.

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
  - `error_type`: Error variant, bounded by the `MhError` variants — `grpc`, `not_registered`, `config`, `internal`, `token_acquisition`, `token_acquisition_timeout`, `jwt_validation`, `webtransport`, `meeting_not_registered`, `mc_endpoint_invalid`, `outbound_auth_unavailable`. The SSoT is `MhError::error_type_label` (a wildcard-free match, so a new variant forces a new label); `metrics.rs`'s `test_cardinality_bounds` derives the set from the enum rather than a hand-list.
  - `status_code`: gRPC-compatible status code
- **Cardinality**: Low — bounded by the `error_type` set above × the finite `operation` code paths.
- **Usage**: Global error tracking, alerting on error spikes

**PromQL example** - total error rate:
```promql
rate(mh_errors_total[5m])
```
