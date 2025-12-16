# AC Service Metrics Catalog

**Service**: Authentication Controller (ac-service)
**Implementation**: `/home/nathan/code/dark_tower/crates/ac-service/src/observability/metrics.rs`
**Job Label**: `ac-service-local` (local development), `ac-service` (production)

All AC service metrics follow ADR-0011 naming conventions with the `ac_` prefix.

---

## Token Issuance Metrics

### `ac_token_issuance_total`
- **Type**: Counter
- **Description**: Total number of token issuance attempts
- **Labels**:
  - `grant_type`: OAuth 2.0 grant type (`client_credentials`, `authorization_code`, etc.)
  - `status`: Outcome of the attempt (`success`, `error`)
- **Cardinality**: Low (4 grant types × 2 statuses = 8 series)
- **Usage**: Track token issuance rate and success/failure ratio
- **Related Metric**: `ac_token_issuance_duration_seconds`

### `ac_token_issuance_duration_seconds`
- **Type**: Histogram
- **Description**: Duration of token issuance operations
- **Labels**:
  - `grant_type`: OAuth 2.0 grant type
  - `status`: Outcome of the attempt (`success`, `error`)
- **Buckets**: Default Prometheus buckets
- **SLO Target**: p99 < 350ms
- **Cardinality**: Low (4 grant types × 2 statuses = 8 series)
- **Usage**: Monitor token issuance latency, calculate p50/p95/p99

### `ac_token_validations_total`
- **Type**: Counter
- **Description**: Total number of token validation attempts
- **Labels**:
  - `status`: Validation result (`success`, `error`)
  - `error_category`: Category of validation error (`authentication`, `authorization`, `cryptographic`, `internal`, `none`)
- **Cardinality**: Low (2 statuses × 5 categories = 10 series)
- **Status**: Defined but not currently used (future)
- **Usage**: Track validation rate and error types

---

## JWKS Metrics

### `ac_jwks_requests_total`
- **Type**: Counter
- **Description**: Total number of JWKS endpoint requests
- **Labels**:
  - `cache_status`: Cache behavior (`hit`, `miss`, `bypass`)
- **Cardinality**: Low (3 cache statuses = 3 series)
- **Usage**: Monitor JWKS cache effectiveness, detect excessive cache misses

---

## Key Management Metrics

### `ac_key_rotation_total`
- **Type**: Counter
- **Description**: Total number of key rotation attempts
- **Labels**:
  - `status`: Rotation outcome (`success`, `error`)
- **Cardinality**: Low (2 statuses = 2 series)
- **Usage**: Track key rotation frequency and failures

### `ac_signing_key_age_days`
- **Type**: Gauge
- **Description**: Age of the current signing key in days
- **Labels**: None
- **Status**: Defined but not currently exported
- **Usage**: Alert when key age exceeds threshold (e.g., 90 days)

### `ac_active_signing_keys`
- **Type**: Gauge
- **Description**: Number of active signing keys in the keystore
- **Labels**: None
- **Status**: Defined but not currently exported
- **Usage**: Ensure key rotation doesn't leave orphaned keys

### `ac_key_rotation_last_success_timestamp`
- **Type**: Gauge
- **Description**: Unix timestamp of the last successful key rotation
- **Labels**: None
- **Status**: Defined but not currently exported
- **Usage**: Alert when time since last rotation exceeds threshold

---

## Rate Limiting Metrics

### `ac_rate_limit_decisions_total`
- **Type**: Counter
- **Description**: Total number of rate limiting decisions
- **Labels**:
  - `action`: Rate limit decision (`allowed`, `rejected`)
- **Cardinality**: Low (2 actions = 2 series)
- **Status**: Defined but not currently exported
- **Usage**: Monitor rate limiting effectiveness, detect abuse patterns

---

## Database Metrics

### `ac_db_queries_total`
- **Type**: Counter
- **Description**: Total number of database queries executed
- **Labels**:
  - `operation`: SQL operation type (`select`, `insert`, `update`, `delete`)
  - `table`: Database table name (`service_credentials`, `signing_keys`, etc.)
  - `status`: Query outcome (`success`, `error`)
- **Cardinality**: Low (4 operations × ~5 tables × 2 statuses = ~40 series)
- **Status**: Defined but not currently exported
- **Usage**: Track database query rates and failures by table and operation

### `ac_db_query_duration_seconds`
- **Type**: Histogram
- **Description**: Duration of database queries
- **Labels**:
  - `operation`: SQL operation type
  - `table`: Database table name
- **Buckets**: Default Prometheus buckets
- **Cardinality**: Low (4 operations × ~5 tables = ~20 series)
- **Status**: Defined but not currently exported
- **Usage**: Monitor database query latency, identify slow queries

---

## Cryptographic Metrics

### `ac_bcrypt_duration_seconds`
- **Type**: Histogram
- **Description**: Duration of bcrypt operations (hash, verify)
- **Labels**:
  - `operation`: Bcrypt operation type (`hash`, `verify`)
- **Buckets**: Coarse buckets (50ms minimum) to prevent timing side-channel attacks
- **Cardinality**: Low (2 operations = 2 series)
- **Status**: Defined but not currently exported
- **Usage**: Monitor bcrypt performance, ensure cost factor remains appropriate
- **Security Note**: Buckets are intentionally coarse per Security specialist guidance

---

## Audit Metrics

### `ac_audit_log_failures_total`
- **Type**: Counter
- **Description**: Total number of audit log write failures (compliance-critical)
- **Labels**:
  - `event_type`: Type of audit event that failed to log (`token_issued`, `key_rotation`, etc.)
  - `reason`: Reason for failure (`db_write_failed`, `encryption_failed`, etc.)
- **Cardinality**: Medium (bounded by event types and failure reasons)
- **Status**: Defined but not currently exported
- **Alert Threshold**: ANY non-zero value should trigger oncall page
- **Usage**: Detect audit log failures that could impact compliance

---

## Error Metrics

### `ac_errors_total`
- **Type**: Counter
- **Description**: Total number of errors by category
- **Labels**:
  - `operation`: Operation that failed (`token_issuance`, `key_rotation`, `db_query`, etc.)
  - `error_category`: Error classification (`authentication`, `authorization`, `cryptographic`, `internal`)
  - `status_code`: HTTP status code (e.g., `401`, `403`, `500`)
- **Cardinality**: Medium (bounded by operations, 4 categories, and common status codes)
- **Usage**: Track error rates by type, identify patterns in failures

---

## Prometheus Query Examples

### Request Rate (Total)
```promql
sum(rate(ac_token_issuance_total{job="ac-service-local"}[5m])) +
sum(rate(ac_jwks_requests_total{job="ac-service-local"}[5m]))
```

### Error Rate
```promql
sum(rate(ac_errors_total{job="ac-service-local"}[5m])) /
(sum(rate(ac_token_issuance_total{job="ac-service-local"}[5m])) +
 sum(rate(ac_jwks_requests_total{job="ac-service-local"}[5m])))
```

### Token Issuance p95 Latency
```promql
histogram_quantile(0.95,
  sum(rate(ac_token_issuance_duration_seconds_bucket{job="ac-service-local"}[5m])) by (le)
) * 1000
```

### Token Issuance Rate by Grant Type
```promql
sum(rate(ac_token_issuance_total{job="ac-service-local"}[5m])) by (grant_type)
```

### JWKS Cache Hit Rate
```promql
sum(rate(ac_jwks_requests_total{job="ac-service-local", cache_status="hit"}[5m])) /
sum(rate(ac_jwks_requests_total{job="ac-service-local"}[5m]))
```

---

## Dashboard Panels

The AC service Grafana dashboard (`/home/nathan/code/dark_tower/infra/grafana/dashboards/ac-service.json`) includes:

1. **Request Rate**: Combined token issuance + JWKS requests
2. **Error Rate**: Errors / Total requests
3. **p95 Latency**: Token issuance latency at 95th percentile
4. **Tokens Issued (1h)**: Total tokens issued in the last hour
5. **Request Rate by Endpoint**: Token issuance (by grant type) and JWKS (by cache status)
6. **Request Rate by Status Code**: Token issuance by success/error status
7. **Request Latency Percentiles**: p50, p95, p99 latency over time
8. **Token Issuance Rate by Grant Type**: Breakdown by OAuth 2.0 grant type
9. **Authentication Attempts**: Success vs failure rates

---

## SLO Definitions

### Token Issuance Latency
- **SLI**: p99 token issuance duration
- **Threshold**: < 350ms
- **Window**: 30 days
- **Objective**: 99% of requests under threshold

### Token Validation Availability
- **SLI**: Successful validations / Total validations
- **Threshold**: 99.99%
- **Window**: 30 days
- **Note**: Currently not enforced (validation metrics not yet exported)

---

## Cardinality Management

All AC service metrics follow strict cardinality bounds per ADR-0011:

| Label | Bound | Values |
|-------|-------|--------|
| `grant_type` | 4 max | `client_credentials`, `authorization_code`, `refresh_token`, `password` |
| `status` | 2 | `success`, `error` |
| `error_category` | 4 | `authentication`, `authorization`, `cryptographic`, `internal` |
| `operation` | Bounded by code | `select`, `insert`, `update`, `delete`, etc. |
| `table` | Bounded by schema | ~5 tables (`service_credentials`, `signing_keys`, etc.) |
| `cache_status` | 3 | `hit`, `miss`, `bypass` |
| `action` | 2 | `allowed`, `rejected` |

**Total Estimated Cardinality**: ~100 time series (well within Prometheus limits)

---

## References

- **ADR-0011**: Observability standards and metric naming conventions
- **Implementation**: `/home/nathan/code/dark_tower/crates/ac-service/src/observability/metrics.rs`
- **Dashboard**: `/home/nathan/code/dark_tower/infra/grafana/dashboards/ac-service.json`
- **PRR-0001**: Post-release review identifying dashboard metric name mismatches
