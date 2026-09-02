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

> **Expected steady state until story task 13 — read this before treating `no_generation` as an incident.**
>
> MC does not emit `policy_generation` >= 1 until story task 13. Until then **every** registration MC sends carries 0, so the correct and expected shape is:
> `no_generation` counting up at the registration rate, `applied` flat at **zero**, and `RegisterMeetingResponse.applied_generation` pinned at **0**. None of that is an outage.
>
> MH deliberately does **not** reject `policy_generation` 0 yet. `internal.proto::RegisterMeetingRequest.policy_generation` carries the ordering constraint in full: enforcing the rejection before MC emits >= 1 would reject every registration for the whole window — no meeting registered, every client provisionally accepted and kicked at the registration timeout, which is ADR-0036 §8's opening paragraph almost verbatim, "a permanent media blackhole for that meeting until it emptied, reached through an ordinary rolling deploy".
>
> **The shape inverts at story task 13**, and that is the signal to watch for: `no_generation` should fall to zero and `applied` should take over. `no_generation` is deliberately the same value the *rejection* will land on once enforcement turns on — two eras, one value, no rename, no dashboard edit.

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

No alert rule ships with this metric. A useful threshold depends on MC's re-assert cadence, which lands with story task 13; alerting is story task 21's scope.

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
