# Meeting Controller Metrics Catalog

**Service**: Meeting Controller (mc-service)
**Implementation**: `crates/mc-service/src/observability/metrics.rs`
**Job Label**: `mc-service-local` (local development), `mc-service` (production)

All MC service metrics follow ADR-0011 naming conventions with the `mc_` prefix.

---

## Connection & Meeting Metrics (Gauges)

### `mc_connections_active`
- **Type**: Gauge
- **Description**: Number of active WebTransport connections
- **Labels**: None
- **Usage**: Track connection count for capacity planning
- **Example**:
  ```promql
  mc_connections_active{job="mc-service"}
  ```

### `mc_meetings_active`
- **Type**: Gauge
- **Description**: Number of active meetings
- **Labels**: None
- **Usage**: Track meeting count for capacity planning
- **Example**:
  ```promql
  mc_meetings_active{job="mc-service"}
  ```

---

## Actor Metrics

### `mc_actor_mailbox_depth`
- **Type**: Gauge
- **Description**: Mailbox depth for each actor type
- **Labels**:
  - `actor_type`: Actor type (controller, meeting, connection)
- **Cardinality**: 3 (bounded by ActorType enum)
- **Usage**: Backpressure monitoring; high values indicate message processing lag
- **Example**:
  ```promql
  mc_actor_mailbox_depth{job="mc-service", actor_type="meeting"}
  ```

### `mc_actor_panics_total`
- **Type**: Counter
- **Description**: Actor panic events by type
- **Labels**:
  - `actor_type`: Actor type (controller, meeting, connection)
- **Cardinality**: 3
- **Alert**: Any non-zero value indicates a bug
- **Example**:
  ```promql
  rate(mc_actor_panics_total{job="mc-service"}[5m])
  ```

### `mc_messages_dropped_total`
- **Type**: Counter
- **Description**: Messages dropped due to backpressure
- **Labels**:
  - `actor_type`: Actor type (controller, meeting, connection)
- **Cardinality**: 3
- **Usage**: Non-zero values indicate system overload
- **Example**:
  ```promql
  sum(rate(mc_messages_dropped_total{job="mc-service"}[5m])) by (actor_type)
  ```

---

## Latency Metrics (Histograms)

### `mc_message_latency_seconds`
- **Type**: Histogram
- **Description**: Signaling message processing latency
- **Labels**:
  - `message_type`: Protobuf message type (~20 bounded values)
- **Buckets**: [0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000]
- **SLO Target**: p99 < 100ms
- **Cardinality**: ~20
- **Example**:
  ```promql
  histogram_quantile(0.99,
    sum(rate(mc_message_latency_seconds_bucket{job="mc-service"}[5m])) by (le)
  ) * 1000
  ```

### `mc_redis_latency_seconds`
- **Type**: Histogram
- **Description**: Redis operation latency
- **Labels**:
  - `operation`: Redis command type (get, set, del, incr, hset, hget, eval, zadd, zrange)
- **Buckets**: [0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000]
- **SLO Target**: p99 < 10ms
- **Cardinality**: ~10
- **Example**:
  ```promql
  histogram_quantile(0.99,
    sum(rate(mc_redis_latency_seconds_bucket{job="mc-service"}[5m])) by (le, operation)
  ) * 1000
  ```

### `mc_recovery_duration_seconds`
- **Type**: Histogram
- **Description**: Session recovery duration after reconnection with binding token
- **Labels**: None
- **Buckets**: [0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000, 10.000]
- **SLO Target**: p99 < 500ms
- **Cardinality**: 1
- **Example**:
  ```promql
  histogram_quantile(0.99,
    sum(rate(mc_recovery_duration_seconds_bucket{job="mc-service"}[5m])) by (le)
  ) * 1000
  ```

---

## GC Heartbeat Metrics

### `mc_gc_heartbeats_total`
- **Type**: Counter
- **Description**: GC heartbeat attempts by status and type
- **Labels**:
  - `status`: Heartbeat outcome (success, error)
  - `type`: Heartbeat type (fast, comprehensive)
- **Cardinality**: 4 (2 statuses x 2 types)
- **Usage**: Track heartbeat success rate
- **Example**:
  ```promql
  rate(mc_gc_heartbeats_total{job="mc-service", status="error"}[5m])
  ```

### `mc_gc_heartbeat_latency_seconds`
- **Type**: Histogram
- **Description**: GC heartbeat round-trip latency
- **Labels**:
  - `type`: Heartbeat type (fast, comprehensive)
- **Buckets**: [0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000]
- **SLO Target**: p99 < 100ms
- **Cardinality**: 2
- **Example**:
  ```promql
  histogram_quantile(0.99,
    sum(rate(mc_gc_heartbeat_latency_seconds_bucket{job="mc-service"}[5m])) by (le, type)
  ) * 1000
  ```

---

## Fencing Metrics

### `mc_fenced_out_total`
- **Type**: Counter
- **Description**: Fenced-out events (split-brain recovery)
- **Labels**:
  - `reason`: Fencing reason (stale_generation, concurrent_write)
- **Cardinality**: 2-3
- **Alert**: Rate > 0.1/min warrants investigation
- **Example**:
  ```promql
  sum(rate(mc_fenced_out_total{job="mc-service"}[5m])) by (reason)
  ```

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

### Message Processing Rate
```promql
sum(rate(mc_message_latency_seconds_count{job="mc-service"}[5m]))
```

### Message Processing p99 Latency
```promql
histogram_quantile(0.99,
  sum(rate(mc_message_latency_seconds_bucket{job="mc-service"}[5m])) by (le)
) * 1000
```

### Redis p99 Latency
```promql
histogram_quantile(0.99,
  sum(rate(mc_redis_latency_seconds_bucket{job="mc-service"}[5m])) by (le)
) * 1000
```

### Token Refresh Success Rate
```promql
sum(rate(mc_token_refresh_total{job="mc-service", status="success"}[5m])) /
sum(rate(mc_token_refresh_total{job="mc-service"}[5m]))
```

### Token Refresh p99 Latency
```promql
histogram_quantile(0.99,
  sum(rate(mc_token_refresh_duration_seconds_bucket{job="mc-service"}[5m])) by (le)
)
```

### Error Rate by Operation
```promql
sum(rate(mc_errors_total{job="mc-service"}[5m])) by (operation, error_type)
```

### GC Heartbeat Success Rate
```promql
sum(rate(mc_gc_heartbeats_total{job="mc-service", status="success"}[5m])) /
sum(rate(mc_gc_heartbeats_total{job="mc-service"}[5m]))
```

---

## SLO Definitions

### Message Processing Latency
- **SLI**: p99 message processing duration
- **Threshold**: < 100ms
- **Window**: 30 days
- **Objective**: 99% of messages under threshold

### Redis Operation Latency
- **SLI**: p99 Redis operation duration
- **Threshold**: < 10ms
- **Window**: 30 days
- **Objective**: 99.5% of operations under threshold

### Session Recovery Latency
- **SLI**: p99 session recovery duration
- **Threshold**: < 500ms
- **Window**: 30 days
- **Objective**: 99% of recoveries under threshold

### GC Heartbeat Latency
- **SLI**: p99 heartbeat round-trip duration
- **Threshold**: < 100ms
- **Window**: 30 days
- **Objective**: 99.5% of heartbeats under threshold

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
| `actor_type` | 3 | controller, meeting, connection |
| `message_type` | ~20 | Bounded by protobuf message types |
| `operation` (redis) | ~10 | get, set, del, incr, hset, hget, eval, zadd, zrange |
| `operation` (error) | ~5 | token_refresh, gc_heartbeat, redis_session, meeting_join, session_binding |
| `status` | 2-3 | success, error (or success, error, timeout) |
| `type` (heartbeat) | 2 | fast, comprehensive |
| `reason` (fencing) | 2-3 | stale_generation, concurrent_write |
| `error_type` | 18 | Bounded by McError enum variants |
| `status_code` | 6 | Signaling codes: 2, 3, 4, 5, 6, 7 |

**Total Estimated Cardinality**: ~200 time series (well within Prometheus limits)

---

## References

- **ADR-0011**: Observability standards and metric naming conventions
- **ADR-0010**: GC-MC integration and SLO requirements
- **ADR-0023**: MC architecture and operational metrics
- **Implementation**: `crates/mc-service/src/observability/metrics.rs`
- **Dashboard**: `infra/grafana/dashboards/mc-overview.json`
- **SLO Dashboard**: `infra/grafana/dashboards/mc-slos.json`
