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
rate(mh_gc_registration_total{status="error"}[5m])
  / rate(mh_gc_registration_total[5m])
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

## Incoming gRPC Metrics

### `mh_grpc_requests_total`
- **Type**: Counter
- **Description**: Total incoming gRPC requests from MC
- **Labels**:
  - `method`: RPC method (`register`, `route_media`, `stream_telemetry`)
  - `status`: Outcome (`success`, `error`)
- **Cardinality**: Low (6 = 3 methods x 2 statuses)
- **Usage**: Monitor MC→MH traffic volume and error rates

**PromQL example** - error rate by method:
```promql
rate(mh_grpc_requests_total{status="error"}[5m])
```

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
