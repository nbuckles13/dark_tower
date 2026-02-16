# DRY Reviewer - Code Review

**Task**: Add MC GC heartbeat metric instrumentation
**Date**: 2026-02-10
**Reviewer**: DRY Specialist

---

## Files Reviewed

1. `crates/meeting-controller/src/grpc/gc_client.rs` - Added metric recording to heartbeat methods
2. `infra/grafana/dashboards/mc-overview.json` - Fixed metric name from `mc_gc_heartbeat_total` to `mc_gc_heartbeats_total`

---

## Duplication Analysis

### 1. Metric Recording Pattern in gc_client.rs

**Pattern**: Using `Instant::now()` for timing, recording counter and histogram on success/error paths

```rust
// Start timer for latency measurement (ADR-0011)
let start = Instant::now();

match client.fast_heartbeat(grpc_request).await {
    Ok(response) => {
        let duration = start.elapsed();
        record_gc_heartbeat("success", "fast");
        record_gc_heartbeat_latency("fast", duration);
        // ...
    }
    Err(e) => {
        let duration = start.elapsed();
        record_gc_heartbeat("error", "fast");
        record_gc_heartbeat_latency("fast", duration);
        // ...
    }
}
```

**Cross-service search**:

| Crate | Similar Pattern? | Location |
|-------|-----------------|----------|
| `crates/common/` | No metrics | No metrics infrastructure in common crate |
| `crates/global-controller/` | Similar pattern | `repositories/meeting_controllers.rs` uses `Instant::now()` + `metrics::record_db_query()` |
| `crates/ac-service/` | Similar pattern | `observability/metrics.rs` has similar counter+histogram functions |

**Assessment**: This is a **service-specific** metric pattern. Each service (AC, GC, MC) has its own metrics module with service-prefixed metrics (`ac_`, `gc_`, `mc_`). The pattern is intentionally duplicated because:
1. Each service has different metric names and labels
2. ADR-0011 mandates service-specific prefixes
3. No shared metric utilities exist in `common` crate

**Severity**: Not a finding - this is an intentional architectural pattern.

### 2. Latency Recording Pattern

**Pattern**: Recording both counter and histogram for the same operation

```rust
record_gc_heartbeat(status, type);          // counter
record_gc_heartbeat_latency(type, duration); // histogram
```

**Cross-service search**:

| Service | Counter + Histogram Pair | Notes |
|---------|-------------------------|-------|
| AC | `record_token_issuance()` | Counter + Histogram in one function |
| GC | `record_http_request()` | Counter + Histogram in one function |
| GC | `record_mc_assignment()` | Counter + Histogram in one function |
| MC | `record_gc_heartbeat()` + `record_gc_heartbeat_latency()` | Separate functions |

**Assessment**: MC uses separate functions while AC and GC combine them. This is a **minor stylistic inconsistency** but not a blocker:
- MC pattern allows recording only counter OR only histogram if needed
- Both approaches are valid per ADR-0011

**Severity**: TECH_DEBT - Could consider unifying pattern but not blocking

### 3. Dashboard Metric Name

**Pattern**: Grafana dashboard uses `mc_gc_heartbeats_total` (plural)

**Cross-service search**: Checked MC metrics.rs - confirms the metric is registered as `mc_gc_heartbeats_total` (plural). The fix aligns dashboard with code.

**Assessment**: Not duplication - this is a bug fix aligning the dashboard with the actual metric name.

---

## Findings Summary

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| TD-MC-001 | TECH_DEBT | Counter+Histogram pattern differs from AC/GC (separate vs combined functions) | `observability/metrics.rs` |

### No BLOCKER Findings

The implementation does not duplicate code that exists in `common/` crate. Each service properly maintains its own metrics module per ADR-0011 architectural decision.

---

## Tech Debt Documentation

### TD-MC-001: Metric Recording Pattern Inconsistency

**Issue**: MC uses separate functions `record_gc_heartbeat()` and `record_gc_heartbeat_latency()` while AC and GC combine counter+histogram recording in single functions.

**Impact**: Low - both patterns work correctly, just stylistic inconsistency.

**Future Action**: Consider unifying the pattern across services when establishing shared metric patterns (if ever moved to common crate).

---

## Verdict

```
verdict: APPROVED
finding_count:
  blocker: 0
  critical: 0
  major: 0
  minor: 0
  tech_debt: 1
checkpoint_exists: true
summary: No cross-service duplication found. MC metrics follow ADR-0011 with service-specific prefixes. Minor tech debt noted for stylistic inconsistency in counter+histogram recording pattern compared to AC/GC, but not blocking as both approaches are valid.
```
