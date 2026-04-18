# Global Controller Metrics Catalog

**Service**: Global Controller (gc-service)
**Implementation**: `crates/gc-service/src/observability/metrics.rs`
**Job Label**: `gc-service-local` (local development), `gc-service` (production)

All GC service metrics follow ADR-0011 naming conventions with the `gc_` prefix.

---

## HTTP Metrics

### `gc_http_requests_total`
- **Type**: Counter
- **Description**: Total HTTP requests received
- **Labels**:
  - `method`: HTTP method (GET, POST, PATCH, DELETE, etc.)
  - `endpoint`: Normalized endpoint path (e.g., `/api/v1/meetings/{code}`)
  - `status_code`: HTTP response status code — raw code as a string
    (`"200"`, `"400"`, `"401"`, `"403"`, `"404"`, `"500"`, etc.)
- **Cardinality**: nominal worst-case ~1,050 combinations (7 methods × 10 endpoints
  × ~15 realistic status codes) notionally nudges the ADR-0011 §62-63 1,000/metric
  ceiling, but observed series stay well under 300 because no single
  (method, endpoint) pair surfaces more than ~3 codes in practice (e.g., `POST
  /api/v1/meetings` emits roughly 200/400/401/500, not 15 codes). The catalog
  ceiling holds in practice; revisit if a new endpoint surfaces >5 distinct codes.
- **Usage**: Track request rate and error distribution by endpoint. Derive
  category breakdowns via PromQL regex on `status_code` (e.g., `"2.."` for
  success, `"[45].."` for errors, `"5.."` for server errors). No separate
  category label is emitted — aligns with AC's canonical shape per ADR-0031.
- **Example**:
  ```promql
  # Total request rate
  rate(gc_http_requests_total{job="gc-service-local"}[5m])

  # Server-error rate (5xx)
  sum(rate(gc_http_requests_total{status_code=~"5.."}[5m]))
  ```

### `gc_http_request_duration_seconds`
- **Type**: Histogram
- **Description**: HTTP request duration in seconds
- **Labels**:
  - `method`: HTTP method
  - `endpoint`: Normalized endpoint path
  - `status_code`: HTTP response status code — raw code as a string
    (matches `gc_http_requests_total`)
- **Buckets**: [0.005, 0.010, 0.025, 0.050, 0.100, 0.150, 0.200, 0.300, 0.500, 1.000, 2.000]
- **SLO Target**: p95 < 200ms (per ADR-0010)
- **Cardinality**: same worst-case framing as `gc_http_requests_total` — nominal
  ~1,050 label combinations nudges the 1,000/metric ceiling, observed stays well
  under 300 in practice. Note: `_bucket` time series ≈ 12× the label cardinality
  per the metrics library's histogram expansion, so the bucket surface is a
  separate (larger) concern tracked by `validate-histogram-buckets.sh`.
- **Usage**: Monitor request latency, calculate percentiles for SLO tracking.
  Filter by `status_code=~"2.."` to exclude error paths from latency SLOs if
  desired.
- **Example**:
  ```promql
  histogram_quantile(0.95,
    sum(rate(gc_http_request_duration_seconds_bucket{job="gc-service-local"}[5m])) by (le)
  ) * 1000
  ```

---

## MC Assignment Metrics

### `gc_mc_assignments_total`
- **Type**: Counter
- **Description**: Total MC assignment attempts
- **Labels**:
  - `status`: Assignment outcome (success, rejected, error)
  - `rejection_reason`: Reason for rejection (at_capacity, draining, unhealthy, rpc_failed, none)
- **Cardinality**: Low (~15 combinations)
- **Usage**: Track assignment success rate and failure patterns
- **Example**:
  ```promql
  sum(rate(gc_mc_assignments_total{status="success"}[5m])) /
  sum(rate(gc_mc_assignments_total[5m]))
  ```

### `gc_mc_assignment_duration_seconds`
- **Type**: Histogram
- **Description**: Time to assign meeting to MC
- **Labels**:
  - `status`: Assignment outcome (success, rejected, error)
- **Buckets**: [0.005, 0.010, 0.015, 0.020, 0.030, 0.050, 0.100, 0.250, 0.500]
- **SLO Target**: p95 < 20ms (per ADR-0010)
- **Cardinality**: Low (3 statuses)
- **Usage**: Monitor assignment latency for SLO compliance
- **Example**:
  ```promql
  histogram_quantile(0.95,
    sum(rate(gc_mc_assignment_duration_seconds_bucket{status="success"}[5m])) by (le)
  ) * 1000
  ```

---

## Database Metrics

### `gc_db_queries_total`
- **Type**: Counter
- **Description**: Total database queries executed
- **Labels**:
  - `operation`: Query operation (select_mc, get_healthy_assignment, get_candidate_mcs, atomic_assign, end_assignment, update_heartbeat, etc.)
  - `status`: Query outcome (success, error)
- **Cardinality**: Low (~30 combinations)
- **Usage**: Track database query rates and failures by operation
- **Example**:
  ```promql
  sum(rate(gc_db_queries_total{status="error"}[5m])) by (operation)
  ```

### `gc_db_query_duration_seconds`
- **Type**: Histogram
- **Description**: Database query duration
- **Labels**:
  - `operation`: Query operation type
- **Buckets**: [0.001, 0.002, 0.005, 0.010, 0.020, 0.050, 0.100, 0.250, 0.500, 1.000]
- **SLO Target**: p99 < 50ms
- **Cardinality**: Low (~15 operations)
- **Usage**: Monitor database query latency, identify slow queries
- **Example**:
  ```promql
  histogram_quantile(0.99,
    sum(rate(gc_db_query_duration_seconds_bucket{job="gc-service-local"}[5m])) by (le, operation)
  ) * 1000
  ```

---

## Token Manager Metrics (ADR-0010 Section 4a)

### `gc_token_refresh_total`
- **Type**: Counter
- **Description**: Total token refresh attempts
- **Labels**:
  - `status`: Refresh outcome (success, error)
- **Cardinality**: Low (2 statuses)
- **Usage**: Track token refresh rate and success
- **Example**:
  ```promql
  rate(gc_token_refresh_total{status="error"}[5m])
  ```

### `gc_token_refresh_duration_seconds`
- **Type**: Histogram
- **Description**: Token refresh operation duration
- **Labels**: None (aggregated)
- **Buckets**: [0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000]
- **Cardinality**: 1 (no labels)
- **Usage**: Monitor token refresh latency
- **Example**:
  ```promql
  histogram_quantile(0.99,
    sum(rate(gc_token_refresh_duration_seconds_bucket[5m])) by (le)
  )
  ```

### `gc_token_refresh_failures_total`
- **Type**: Counter
- **Description**: Token refresh failures by error type
- **Labels**:
  - `error_type`: Type of failure (http, auth_rejected, invalid_response, acquisition_failed, configuration, channel_closed)
- **Cardinality**: Low (6 error types)
- **Alert**: High rate indicates AC connectivity issues
- **Usage**: Diagnose token refresh failures
- **Example**:
  ```promql
  sum(rate(gc_token_refresh_failures_total[5m])) by (error_type)
  ```

---

## Meeting Creation Metrics

### `gc_meeting_creation_total`
- **Type**: Counter
- **Description**: Total meeting creation attempts
- **Labels**:
  - `status`: Creation outcome (success, error)
- **Cardinality**: Low (2 statuses)
- **Usage**: Track meeting creation rate and success
- **Example**:
  ```promql
  rate(gc_meeting_creation_total{status="error"}[5m])
  ```

### `gc_meeting_creation_duration_seconds`
- **Type**: Histogram
- **Description**: Meeting creation operation duration (end-to-end handler time)
- **Labels**:
  - `status`: Creation outcome (success, error)
- **Buckets**: [0.005, 0.010, 0.025, 0.050, 0.100, 0.150, 0.200, 0.300, 0.500, 1.000]
- **Cardinality**: Low (2 statuses)
- **Usage**: Monitor meeting creation latency, identify slow DB or code-generation paths
- **Example**:
  ```promql
  histogram_quantile(0.95,
    sum(rate(gc_meeting_creation_duration_seconds_bucket{status="success"}[5m])) by (le)
  )
  ```

### `gc_meeting_creation_failures_total`
- **Type**: Counter
- **Description**: Meeting creation failures by error type
- **Labels**:
  - `error_type`: Type of failure (bad_request, forbidden, unauthorized, code_collision, db_error, internal)
- **Cardinality**: Low (6 error types)
- **Alert**: High rate may indicate org limit exhaustion or DB issues
- **Usage**: Diagnose meeting creation failures
- **Example**:
  ```promql
  sum(rate(gc_meeting_creation_failures_total[5m])) by (error_type)
  ```

---

## Meeting Join Metrics

> **Note**: The `gc_meeting_join_*` family is **shared between two handlers** —
> `join_meeting` (authenticated user, `participant=user`) and `get_guest_token`
> (public guest, `participant=guest`). The `participant` label discriminates so
> operators can triage user-vs-guest failures without log-diving (added per
> ADR-0032 Step 5, 2026-04-27). Do NOT introduce a parallel `gc_guest_token_*`
> family — both paths produce a join token + MC assignment with the same
> outcome shape; the `participant` axis is the canonical discriminator.

### `gc_meeting_join_total`
- **Type**: Counter
- **Description**: Total meeting join attempts (both `join_meeting` and `get_guest_token` handlers)
- **Labels**:
  - `participant`: `user` (authenticated) or `guest` (public). ADR-0032 Step 5.
  - `status`: Join outcome (success, error)
- **Cardinality**: Low (2 participants × 2 statuses = 4)
- **Usage**: Track meeting join rate and success per participant type
- **Dashboard**: "Meeting Join Rate by Status" and "Meeting Join Success Rate (%)" panels in `gc-overview.json`
- **Alerts**: `GCHighJoinFailureRate` (warning, >5% failure rate for 5m)
- **Example**:
  ```promql
  rate(gc_meeting_join_total{status="error"}[5m])
  # User-only success rate
  sum(rate(gc_meeting_join_total{participant="user", status="success"}[5m])) /
  sum(rate(gc_meeting_join_total{participant="user"}[5m]))
  ```

### `gc_meeting_join_duration_seconds`
- **Type**: Histogram
- **Description**: Meeting join operation duration (end-to-end handler time including MC assignment and AC token request)
- **Labels**:
  - `participant`: `user` (authenticated) or `guest` (public). ADR-0032 Step 5.
  - `status`: Join outcome (success, error)
- **Buckets**: [0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.000, 5.000]
- **Cardinality**: Low (2 participants × 2 statuses = 4)
- **Usage**: Monitor meeting join latency, identify slow MC assignment or AC token paths; compare guest vs user latency
- **Dashboard**: "Meeting Join Latency (P50/P95/P99)" panel in `gc-overview.json`
- **Alerts**: `GCHighJoinLatency` (info, p95 >2s for 5m)
- **Example**:
  ```promql
  histogram_quantile(0.95,
    sum(rate(gc_meeting_join_duration_seconds_bucket{status="success"}[5m])) by (le)
  )
  ```

### `gc_meeting_join_failures_total`
- **Type**: Counter
- **Description**: Meeting join failures by error type and participant type
- **Labels**:
  - `participant`: `user` or `guest`. ADR-0032 Step 5.
  - `error_type`: Type of failure (bounded set):
    - `not_found` — meeting doesn't exist (both)
    - `bad_status` — meeting cancelled/ended (both)
    - `forbidden` — cross-org denied (`user` only) or `allow_guests=false` is reported as `guests_disabled` (`guest` only — distinct value)
    - `unauthorized` — JWT parse fails (`user` only — guest path is public)
    - `guests_disabled` — `meeting.allow_guests=false` (`guest` only). ADR-0032 Step 5.
    - `bad_request` — body validation fails (`guest` only — user path has no body)
    - `mc_assignment` — MC assignment service failed (both)
    - `ac_request` — AC token request failed (both)
    - `internal` — RNG/AC client construction failure (both)
- **Cardinality**: Low (~2 participants × ~9 error types ≈ 18 series; bounded under ADR-0011 cap-10-per-label)
- **Alert**: High rate may indicate MC capacity issues or AC connectivity problems; `participant=guest, error_type=guests_disabled` spikes mean meeting hosts are receiving guest-join attempts on guest-disabled meetings
- **Usage**: Diagnose meeting join failures, distinguishing user vs guest paths for triage
- **Dashboard**: "Meeting Join Failures by Type" panel in `gc-overview.json`
- **Example**:
  ```promql
  sum(rate(gc_meeting_join_failures_total[5m])) by (participant, error_type)
  ```

---

## AC Client Metrics

### `gc_ac_requests_total`
- **Type**: Counter
- **Description**: Total requests to Auth Controller internal endpoints
- **Labels**:
  - `operation`: Token operation (meeting_token, guest_token)
  - `status`: Request outcome (success, error)
- **Cardinality**: Low (4 combinations)
- **Usage**: Track AC request rate and errors by operation
- **Example**:
  ```promql
  rate(gc_ac_requests_total{status="error"}[5m])
  ```

### `gc_ac_request_duration_seconds`
- **Type**: Histogram
- **Description**: AC client request duration
- **Labels**:
  - `operation`: Token operation (meeting_token, guest_token)
- **Buckets**: Default histogram buckets
- **Cardinality**: Low (2 operations)
- **Usage**: Monitor AC request latency, detect degraded AC performance
- **Example**:
  ```promql
  histogram_quantile(0.95,
    sum(rate(gc_ac_request_duration_seconds_bucket[5m])) by (le, operation)
  )
  ```

---

## gRPC Metrics

### `gc_grpc_mc_calls_total`
- **Type**: Counter
- **Description**: Total gRPC calls to Meeting Controllers
- **Labels**:
  - `method`: gRPC method (assign_meeting)
  - `status`: Call outcome (success, rejected, error)
- **Cardinality**: Low (~9 combinations)
- **Usage**: Track GC-MC communication rate and errors
- **Example**:
  ```promql
  rate(gc_grpc_mc_calls_total{status="error"}[5m])
  ```

### `gc_grpc_mc_call_duration_seconds`
- **Type**: Histogram
- **Description**: gRPC call duration to MCs
- **Labels**:
  - `method`: gRPC method
- **Buckets**: [0.005, 0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.500]
- **Cardinality**: Low (~3 methods)
- **Usage**: Monitor GC-MC latency
- **Example**:
  ```promql
  histogram_quantile(0.95,
    sum(rate(gc_grpc_mc_call_duration_seconds_bucket[5m])) by (le, method)
  )
  ```

---

## gRPC Auth Metrics (ADR-0003)

### `gc_jwt_validations_total`
- **Type**: Counter
- **Description**: Total JWT validation attempts at the gRPC auth layer by result, token type, and failure reason.
- **Labels**:
  - `result`: Validation outcome (`success`, `failure`)
  - `token_type`: Token type (`service` — GC's gRPC layer only sees service tokens; user/guest tokens flow through HTTP middleware)
  - `failure_reason`: Reason for failure (`none`, `signature_invalid`, `expired`, `missing_token`, `scope_mismatch`, `malformed`)
- **Cardinality**: Low (bounded, 2 x 1 x 6 = 12 max with headroom if `token_type` expands)
- **Usage**: Monitor gRPC auth health, detect service token validation failures, diagnose failure causes.
- **Recorded in**: `grpc/auth_layer.rs` on every validation that reaches the cryptographic layer. Structural rejects (missing/invalid-format/empty/oversized) return early without incrementing.
- **Dashboard**: GC Overview - JWT Validations by Result

### `gc_caller_type_rejected_total`
- **Type**: Counter
- **Description**: Total Layer 2 `service_type` routing rejections (valid token, wrong caller for the target gRPC service). ADR-0003.
- **Labels**:
  - `grpc_service`: Target gRPC service name (`GlobalControllerService`, `MediaHandlerRegistryService`)
  - `expected_type`: Expected `service_type` for the gRPC service (`meeting-controller`, `media-handler`)
  - `actual_type`: Actual `service_type` from the token (`meeting-controller`, `media-handler`, `global-controller`, `unknown`)
- **Cardinality**: Low (2 x 2 x 4 = 16 max, bounded by gRPC services and service types + "unknown")
- **Cardinality note**: The label values listed are the expected/legitimate set. The emission site at `grpc/auth_layer.rs:241` does not currently allowlist-clamp the `claims.service_type` value before recording, so a forged or off-spec JWT presenting an arbitrary string would inject that value as a label. Tracked in `docs/TODO.md` for clamping fix (ADR-0032 Step 5 finding F1); bound in production is enforced by JWKS auth (only legitimately-issued tokens reach this site, and AC issues only the 4 enumerated values).
- **Alert**: ANY non-zero value indicates a bug or misconfiguration — a service is presenting a valid token but calling the wrong gRPC endpoint.
- **Usage**: Detect service-to-service routing errors, misconfigured tokens.
- **Recorded in**: `grpc/auth_layer.rs` on Layer 2 rejection.
- **Dashboard**: GC Overview - Caller Type Rejections

---

## MH Selection Metrics

### `gc_mh_selections_total`
- **Type**: Counter
- **Description**: Total MH selection attempts
- **Labels**:
  - `status`: Selection outcome (success, error)
  - `has_multiple`: Whether multiple MH peers were selected (true, false)
- **Cardinality**: Low (4 combinations)
- **Usage**: Track MH selection patterns
- **Example**:
  ```promql
  sum(rate(gc_mh_selections_total{has_multiple="true"}[5m])) /
  sum(rate(gc_mh_selections_total[5m]))
  ```

### `gc_mh_selection_duration_seconds`
- **Type**: Histogram
- **Description**: MH selection operation duration
- **Labels**:
  - `status`: Selection outcome (success, error)
- **Buckets**: [0.002, 0.005, 0.010, 0.020, 0.050, 0.100, 0.250]
- **Cardinality**: Low (2 statuses)
- **Usage**: Monitor MH selection latency
- **Example**:
  ```promql
  histogram_quantile(0.95,
    sum(rate(gc_mh_selection_duration_seconds_bucket{status="success"}[5m])) by (le)
  )
  ```

---

## Fleet Health Metrics

### `gc_registered_controllers`
- **Type**: Gauge
- **Description**: Number of registered controllers by type and health status
- **Labels**:
  - `controller_type`: Type of controller (`meeting`, `media`)
  - `status`: Health status (`pending`, `healthy`, `degraded`, `unhealthy`, `draining`)
- **Cardinality**: Low (2 types x 5 statuses = 10 series)
- **Usage**: Monitor fleet composition and health distribution. Detect capacity issues when healthy count drops.
- **Update Triggers**: GC startup, MC/MH registration, heartbeat status changes, health checker stale detection
- **Example**:
  ```promql
  sum by(controller_type, status) (gc_registered_controllers)
  ```

---

## Error Metrics

### `gc_errors_total`
- **Type**: Counter
- **Description**: Total errors by operation and type
- **Labels**:
  - `operation`: Operation that failed (join_meeting, guest_token, update_settings, mc_assignment, ac_meeting_token, ac_guest_token, mc_grpc)
  - `error_type`: Error classification (not_found, forbidden, unauthorized, rate_limit, service_unavailable, internal, bad_request, database, invalid_token, conflict)
  - `status_code`: HTTP status code
- **Cardinality**: Medium (~80 combinations, bounded by operations and error types)
- **Usage**: Track error rates by type, identify patterns in failures
- **Note**: Operation labels use subsystem prefixes (e.g., `ac_meeting_token`, `mc_grpc`) unlike per-subsystem metrics (`gc_ac_requests_total`) which use unprefixed operation names (`meeting_token`). Use prefixed values when filtering this metric.
- **Example**:
  ```promql
  sum(rate(gc_errors_total[5m])) by (operation, error_type)
  ```

---

## Prometheus Query Examples

### Request Rate (Total)
```promql
sum(rate(gc_http_requests_total{job="gc-service-local"}[5m]))
```

### Error Rate
```promql
sum(rate(gc_http_requests_total{job="gc-service-local",status_code=~"4..|5.."}[5m])) /
sum(rate(gc_http_requests_total{job="gc-service-local"}[5m]))
```

### HTTP p95 Latency
```promql
histogram_quantile(0.95,
  sum(rate(gc_http_request_duration_seconds_bucket{job="gc-service-local"}[5m])) by (le)
) * 1000
```

### MC Assignment Success Rate
```promql
sum(rate(gc_mc_assignments_total{status="success"}[5m])) /
sum(rate(gc_mc_assignments_total[5m]))
```

### MC Assignment p95 Latency
```promql
histogram_quantile(0.95,
  sum(rate(gc_mc_assignment_duration_seconds_bucket{status="success"}[5m])) by (le)
) * 1000
```

### Meeting Creation Success Rate
```promql
sum(rate(gc_meeting_creation_total{status="success"}[5m])) /
sum(rate(gc_meeting_creation_total[5m]))
```

### Meeting Creation p95 Latency
```promql
histogram_quantile(0.95,
  sum(rate(gc_meeting_creation_duration_seconds_bucket{status="success"}[5m])) by (le)
)
```

### Meeting Join Success Rate
```promql
sum(rate(gc_meeting_join_total{status="success"}[5m])) /
sum(rate(gc_meeting_join_total[5m]))
```

### Meeting Join p95 Latency
```promql
histogram_quantile(0.95,
  sum(rate(gc_meeting_join_duration_seconds_bucket{status="success"}[5m])) by (le)
)
```

### DB Query Latency by Operation
```promql
histogram_quantile(0.99,
  sum(rate(gc_db_query_duration_seconds_bucket[5m])) by (le, operation)
) * 1000
```

---

## SLO Definitions

### HTTP Request Latency
- **SLI**: p95 HTTP request duration
- **Threshold**: < 200ms
- **Window**: 30 days
- **Objective**: 99% of requests under threshold

### MC Assignment Latency
- **SLI**: p95 MC assignment duration
- **Threshold**: < 20ms
- **Window**: 30 days
- **Objective**: 99.5% of assignments under threshold

### Meeting Join Availability
- **SLI**: Successful meeting joins / Total join attempts
- **Threshold**: 99.9%
- **Window**: 30 days

---

## Cardinality Management

All GC service metrics follow strict cardinality bounds per ADR-0011:

| Label | Bound | Values |
|-------|-------|--------|
| `method` | 7 max | GET, POST, PATCH, DELETE, PUT, HEAD, OPTIONS |
| `endpoint` | ~10 | /health, /metrics, /api/v1/me, /api/v1/meetings/{code}, etc. |
| `status_code` | ~15 realistic | 200, 201, 400, 401, 403, 404, 429, 500, 503, etc. (HTTP metrics only) |
| `status` | 5 | success, error, timeout, rejected, accepted (non-HTTP outcome metrics: mc_assignments, db_queries, token_refresh, ac_requests, grpc_mc_calls, mh_selections, meeting_creation, meeting_join) |
| `operation` | ~18 | select_mc, atomic_assign, update_heartbeat, ac_meeting_token, ac_guest_token, mc_grpc, etc. |
| `rejection_reason` | 5 | at_capacity, draining, unhealthy, rpc_failed, none |
| `error_type` | ~10 | not_found, forbidden, unauthorized, rate_limit, service_unavailable, internal, etc. |

**Total Estimated Cardinality**: HTTP metrics ~1,050 worst-case (realistically a few hundred), plus ~200 non-HTTP series — well within Prometheus limits.

---

## References

- **ADR-0011**: Observability standards and metric naming conventions
- **ADR-0010**: GC-MC integration and SLO requirements
- **Implementation**: `crates/gc-service/src/observability/metrics.rs`
- **Dashboard**: `infra/grafana/dashboards/gc-overview.json`
- **Alerts**: `infra/docker/prometheus/rules/gc-alerts.yaml`
