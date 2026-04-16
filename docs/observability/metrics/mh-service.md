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
  - `event`: Notification event type (`connected`, `disconnected`)
  - `status`: Delivery outcome (`success`, `error`)
- **Cardinality**: Low (2 events x 2 statuses = 4 series)
- **Usage**: Monitor MH→MC notification delivery health, detect MC connectivity issues
- **Dashboard**: MH Overview - MC Notification Delivery

**PromQL example** - notification failure rate:
```promql
sum(rate(mh_mc_notifications_total{status="error"}[5m])) /
sum(rate(mh_mc_notifications_total[5m]))
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
