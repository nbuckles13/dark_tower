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

### `mc_message_latency_seconds`
- **Type**: Histogram
- **Description**: Signaling message processing latency
- **Labels**:
  - `message_type`: Protobuf message type (e.g., `join_request`, `leave_request`, `layout_update`)
- **Buckets**: [0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000]
- **SLO Target**: p99 < 100ms for signaling messages
- **Cardinality**: Low (~20 message types)
- **Usage**: Monitor message processing latency, identify slow message types

### `mc_messages_dropped_total`
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
  - `type`: Heartbeat type (`fast`, `comprehensive`)
- **Cardinality**: Low (2 statuses x 2 types = 4 series)
- **Usage**: Monitor GC registration health, detect connectivity issues

### `mc_gc_heartbeat_latency_seconds`
- **Type**: Histogram
- **Description**: GC heartbeat round-trip latency
- **Labels**:
  - `type`: Heartbeat type (`fast`, `comprehensive`)
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
- **Type**: Counter
- **Description**: Fenced-out events (split-brain recovery)
- **Labels**:
  - `reason`: Fencing reason (`stale_generation`, `concurrent_write`)
- **Cardinality**: Low (2-3 reasons)
- **Usage**: Detect split-brain scenarios. Should be rare in normal operation. Investigate if rate > 0.1/min.

---

## Recovery Metrics

### `mc_recovery_duration_seconds`
- **Type**: Histogram
- **Description**: Session recovery duration after reconnection with binding token
- **Labels**: None
- **Buckets**: [0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000, 10.000]
- **SLO Target**: p99 < 500ms
- **Cardinality**: 1 (no labels)
- **Usage**: Monitor session recovery performance. Includes Redis state fetch, session rehydration, and actor re-creation.

---

## Prometheus Query Examples

### Active Meetings
```promql
sum(mc_meetings_active)
```

### Message Processing Latency p99
```promql
histogram_quantile(0.99,
  sum(rate(mc_message_latency_seconds_bucket[5m])) by (le)
)
```

### Message Drop Rate
```promql
100 * sum(rate(mc_messages_dropped_total[5m])) /
(sum(rate(mc_messages_dropped_total[5m])) + sum(rate(mc_message_latency_seconds_count[5m])))
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

---

## SLO Definitions

### Message Processing Latency
- **SLI**: p99 message processing duration
- **Threshold**: < 100ms
- **Window**: 30 days
- **Objective**: 99% of messages under threshold

### Redis Latency
- **SLI**: p99 Redis operation duration
- **Threshold**: < 10ms
- **Window**: 30 days
- **Objective**: 99.9% of operations under threshold

### Session Recovery
- **SLI**: p99 session recovery duration
- **Threshold**: < 500ms
- **Window**: 30 days
- **Objective**: 99% of recoveries under threshold

---

## Cardinality Management

All MC service metrics follow strict cardinality bounds per ADR-0011:

| Label | Bound | Values |
|-------|-------|--------|
| `actor_type` | 3 | `controller`, `meeting`, `connection` |
| `message_type` | ~20 | Bounded by protobuf message types |
| `operation` | ~10 | Bounded by Redis commands |
| `reason` | 2-3 | `stale_generation`, `concurrent_write` |
| `status` | 2 | `success`, `error` |
| `type` | 2 | `fast`, `comprehensive` |

**Total Estimated Cardinality**: ~50 time series (well within Prometheus limits)

---

## References

- **ADR-0011**: Observability standards and metric naming conventions
- **ADR-0023**: Meeting Controller design (Section 11: Observability)
- **Implementation**: `crates/mc-service/src/observability/metrics.rs`
- **Dashboard**: `infra/grafana/dashboards/mc-overview.json`
