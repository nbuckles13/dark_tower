# Global Controller Metrics Catalog

**Service**: Global Controller (global-controller)
**Implementation**: `crates/global-controller/src/observability/metrics.rs`
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
  - `status_code`: HTTP response status code (200, 400, 401, 403, 404, 500, etc.)
- **Cardinality**: ~210 (7 methods x 10 endpoints x 3 status categories)
- **Usage**: Track request rate and error distribution by endpoint
- **Example**:
  ```promql
  rate(gc_http_requests_total{job="gc-service-local"}[5m])
  ```

### `gc_http_request_duration_seconds`
- **Type**: Histogram
- **Description**: HTTP request duration in seconds
- **Labels**:
  - `method`: HTTP method
  - `endpoint`: Normalized endpoint path
  - `status`: Status category (success, error, timeout)
- **Buckets**: [0.005, 0.010, 0.025, 0.050, 0.100, 0.150, 0.200, 0.300, 0.500, 1.000, 2.000]
- **SLO Target**: p95 < 200ms (per ADR-0010)
- **Cardinality**: ~210 (7 methods x 10 endpoints x 3 statuses)
- **Usage**: Monitor request latency, calculate percentiles for SLO tracking
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
  - `error_type`: Type of failure (http_error, auth_rejected, invalid_response, timeout)
- **Cardinality**: Low (~4 error types)
- **Alert**: High rate indicates AC connectivity issues
- **Usage**: Diagnose token refresh failures
- **Example**:
  ```promql
  sum(rate(gc_token_refresh_failures_total[5m])) by (error_type)
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

## MH Selection Metrics

### `gc_mh_selections_total`
- **Type**: Counter
- **Description**: Total MH selection attempts
- **Labels**:
  - `status`: Selection outcome (success, error)
  - `has_backup`: Whether backup MH was selected (true, false)
- **Cardinality**: Low (4 combinations)
- **Usage**: Track MH selection patterns
- **Example**:
  ```promql
  sum(rate(gc_mh_selections_total{has_backup="true"}[5m])) /
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

## Error Metrics

### `gc_errors_total`
- **Type**: Counter
- **Description**: Total errors by operation and type
- **Labels**:
  - `operation`: Operation that failed (join_meeting, guest_token, update_settings, mc_assignment)
  - `error_type`: Error classification (not_found, forbidden, unauthorized, rate_limit, service_unavailable, internal)
  - `status_code`: HTTP status code
- **Cardinality**: Medium (~80 combinations, bounded by operations and error types)
- **Usage**: Track error rates by type, identify patterns in failures
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
| `status` | 3 | success, error, timeout |
| `status_code` | ~15 | 200, 201, 400, 401, 403, 404, 429, 500, 503, etc. |
| `operation` | ~15 | select_mc, atomic_assign, update_heartbeat, etc. |
| `rejection_reason` | 5 | at_capacity, draining, unhealthy, rpc_failed, none |
| `error_type` | ~6 | not_found, forbidden, unauthorized, rate_limit, etc. |

**Total Estimated Cardinality**: ~400 time series (well within Prometheus limits)

---

## References

- **ADR-0011**: Observability standards and metric naming conventions
- **ADR-0010**: GC-MC integration and SLO requirements
- **Implementation**: `crates/global-controller/src/observability/metrics.rs`
- **Dashboard**: `infra/grafana/dashboards/gc-service.json` (TODO)
