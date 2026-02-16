# Security Review - MC GC Heartbeat Metric Instrumentation

**Reviewer**: Security Specialist
**Date**: 2026-02-10
**Task**: Add MC GC heartbeat metric instrumentation

## Files Reviewed

1. `crates/meeting-controller/src/grpc/gc_client.rs` - Added metric recording to heartbeat methods
2. `infra/grafana/dashboards/mc-overview.json` - Fixed metric name from `mc_gc_heartbeat_total` to `mc_gc_heartbeats_total`

## Security Analysis

### 1. PII/Sensitive Data Leakage in Metrics or Logs

**Status**: PASS

The implementation correctly avoids leaking sensitive data:

- **Metrics labels are bounded and safe**: The `record_gc_heartbeat()` function uses only `status` and `type` labels with bounded values ("success"/"error" and "fast"/"comprehensive")
- **No PII in metrics**: The heartbeat metrics record only operational data (status, type, latency duration) - no user IDs, tokens, meeting IDs, or participant identifiers
- **Existing logging is secure**: The `debug!` and `warn!` logging statements only log operational context (timestamps, error messages) without exposing tokens or sensitive configuration
- **Token handling unchanged**: The existing `add_auth()` method that handles JWT tokens was not modified

### 2. Timing Attacks or Information Disclosure

**Status**: PASS

- **Latency metrics are appropriate**: Recording heartbeat latency is a standard observability practice and does not expose sensitive timing information
- **Error metrics are generic**: The "error" status label does not differentiate between authentication failures, network errors, or other failure modes in a way that could be exploited
- **No new attack surface**: The metric instrumentation is purely observational and adds no new code paths that could be exploited

### 3. Resource Exhaustion Vectors

**Status**: PASS

- **Cardinality is bounded**: Per ADR-0011, the metrics have bounded cardinality:
  - `mc_gc_heartbeats_total`: 4 combinations (2 statuses x 2 types)
  - `mc_gc_heartbeat_latency_seconds`: 2 combinations (2 types)
- **No unbounded string labels**: The status and type values are hardcoded strings, preventing cardinality explosion
- **Histogram buckets are standard**: The latency histogram uses default Prometheus buckets

### 4. Authentication/Authorization Bypasses

**Status**: PASS

- **No changes to auth flow**: The metric recording is added after the gRPC call completes, not affecting the authentication flow
- **Token handling preserved**: The `add_auth()` method remains unchanged
- **No bypass vectors**: The implementation does not introduce any code paths that skip authentication

### 5. Dashboard Configuration Security

**Status**: PASS

- **Metric name fix is correct**: The change from `mc_gc_heartbeat_total` to `mc_gc_heartbeats_total` matches the metric definition in `observability/metrics.rs`
- **No injection vectors**: The Prometheus query in the dashboard uses a well-formed PromQL expression with no user input
- **Standard Grafana patterns**: The dashboard follows standard Grafana configuration patterns

## Detailed Code Review

### gc_client.rs Changes

```rust
// Start timer for latency measurement (ADR-0011)
let start = Instant::now();

match client.fast_heartbeat(grpc_request).await {
    Ok(response) => {
        let duration = start.elapsed();
        // Record success metrics per ADR-0011: both counter and histogram
        record_gc_heartbeat("success", "fast");
        record_gc_heartbeat_latency("fast", duration);
        // ... existing handling
    }
    Err(e) => {
        let duration = start.elapsed();
        // Record error metrics per ADR-0011: both counter and histogram
        record_gc_heartbeat("error", "fast");
        record_gc_heartbeat_latency("fast", duration);
        // ... existing error handling
    }
}
```

**Security observations**:
1. `Instant::now()` is a monotonic clock and safe for timing
2. Metric recording happens after the RPC completes, so it cannot affect the outcome
3. Error paths record metrics before the existing error logging, which is appropriate
4. The same pattern is correctly applied to `comprehensive_heartbeat()`

### mc-overview.json Changes

```json
"expr": "sum by(status) (rate(mc_gc_heartbeats_total[5m]))"
```

**Security observations**:
1. The metric name now matches the actual metric definition
2. The PromQL query is well-formed and uses standard Prometheus patterns
3. No user-controllable input in the query

## Verdict

**APPROVED**

The implementation is secure. The metric instrumentation follows established patterns from ADR-0011, uses bounded cardinality labels, and does not introduce any security vulnerabilities. No sensitive data is leaked through metrics or logs.

## Findings Summary

| Severity | Count | Details |
|----------|-------|---------|
| BLOCKER  | 0     | -       |
| CRITICAL | 0     | -       |
| MAJOR    | 0     | -       |
| MINOR    | 0     | -       |

**Total findings**: 0
