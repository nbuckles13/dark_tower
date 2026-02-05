# DRY Review: GC Metrics Implementation

**Reviewer**: DRY Reviewer Specialist
**Date**: 2026-02-04
**Task**: GC metrics endpoint and core application instrumentation

---

## Summary

The GC metrics implementation introduces **significant code duplication** with the existing AC-service implementation. The duplication is primarily in **HTTP metrics middleware** and **path normalization logic**, both of which are nearly identical between AC and GC. No BLOCKING issues found (nothing exists in common crate that was ignored), but substantial TECH_DEBT exists that warrants follow-up extraction to the common crate.

---

## Findings

### BLOCKING Duplications

**None**

The common crate (`crates/common/src/`) does not currently contain any metrics or observability utilities. The GC implementation correctly imports what IS available in common (jwt utilities, config, types, etc.) and implements service-specific metrics locally. This is the expected pattern when common utilities don't yet exist.

---

### TECH_DEBT Duplications

#### 1. HTTP Metrics Middleware - 95% Identical

| Service | Location | Lines |
|---------|----------|-------|
| GC (New) | `crates/global-controller/src/middleware/http_metrics.rs` | 1-41 |
| AC (Existing) | `crates/ac-service/src/middleware/http_metrics.rs` | 1-41 |

**Issue**: The `http_metrics_middleware` function is nearly identical in both services:

```rust
// Both services have this exact same middleware:
pub async fn http_metrics_middleware(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let response = next.run(request).await;
    let duration = start.elapsed();
    let status_code = response.status().as_u16();
    record_http_request(&method, &path, status_code, duration);
    response
}
```

**Similarity**: ~95% (only import path differs)

**Recommendation**: Extract generic HTTP metrics middleware to `common::middleware::http_metrics` with a configurable metric recorder callback or trait. Each service can then provide its own `record_http_request` implementation while sharing the timing logic.

---

#### 2. Path/Endpoint Normalization Pattern - 80% Similar Algorithm

| Service | Location | Pattern |
|---------|----------|---------|
| GC (New) | `crates/global-controller/src/observability/metrics.rs:77-122` | `normalize_endpoint()` + `normalize_dynamic_endpoint()` |
| AC (Existing) | `crates/ac-service/src/observability/metrics.rs:235-289` | `normalize_path()` + `normalize_dynamic_path()` |

**Issue**: Both services implement the same algorithm for path normalization to prevent cardinality explosion:
1. Check known static paths, return as-is
2. For dynamic paths, split by `/` and replace IDs with placeholders
3. Return `/other` for unknown paths

The logic is identical, only the path patterns differ (GC handles `/api/v1/meetings/{code}`, AC handles `/api/v1/admin/clients/{id}`).

**Similarity**: ~80% (same algorithm, different path patterns)

**Recommendation**: Create a `PathNormalizer` struct in `common::observability` that:
- Takes a list of static paths to preserve
- Takes regex or glob patterns for dynamic path normalization
- Provides `normalize(&self, path: &str) -> String`

---

#### 3. UUID Detection Function - 100% Identical Algorithm (AC only)

| Service | Location |
|---------|----------|
| AC (Existing) | `crates/ac-service/src/observability/metrics.rs:295-325` |

**Issue**: AC has a well-tested `is_uuid()` function for path normalization. GC doesn't need it currently (uses simple path length checks), but if GC adds UUID-based paths in the future, this should be shared.

**Similarity**: N/A (GC doesn't have it yet)

**Recommendation**: When extracting path normalization to common, include `is_uuid()` as a utility function.

---

#### 4. record_http_request Function Signature - 90% Similar

| Service | Location | Signature |
|---------|----------|-----------|
| GC (New) | `crates/global-controller/src/observability/metrics.rs:43` | `record_http_request(method, endpoint, status_code, duration)` |
| AC (Existing) | `crates/ac-service/src/observability/metrics.rs:212` | `record_http_request(method, path, status_code, duration)` |

**Issue**: Same function signature, same purpose, different metric names (`gc_` vs `ac_` prefix).

**Similarity**: ~90% (same signature, different metric prefix)

**Recommendation**: Create a trait `HttpMetricsRecorder` in common:
```rust
pub trait HttpMetricsRecorder {
    fn record_http_request(&self, method: &str, path: &str, status_code: u16, duration: Duration);
}
```
Each service implements with its own metric prefix.

---

#### 5. record_db_query Function - 70% Similar Pattern

| Service | Location | Signature |
|---------|----------|-----------|
| GC (New) | `crates/global-controller/src/observability/metrics.rs:168` | `record_db_query(operation, status, duration)` |
| AC (Existing) | `crates/ac-service/src/observability/metrics.rs:109` | `record_db_query(operation, table, status, duration)` |

**Issue**: Similar purpose but slightly different signatures (AC includes `table` label, GC doesn't).

**Similarity**: ~70% (similar purpose, different label sets)

**Recommendation**: Consider a trait-based approach or a builder pattern for DB metrics that allows optional labels.

---

#### 6. record_error Function - 75% Similar Pattern

| Service | Location | Signature |
|---------|----------|-----------|
| GC (New) | `crates/global-controller/src/observability/metrics.rs:229` | `record_error(operation, error_type, status_code)` |
| AC (Existing) | `crates/ac-service/src/observability/metrics.rs:170` | `record_error(operation, error_category, status_code)` |

**Issue**: Same pattern, different label name (`error_type` vs `error_category`).

**Similarity**: ~75% (same pattern, minor naming differences)

**Recommendation**: Standardize on a common error metric recording interface.

---

#### 7. Status Code Categorization - GC-Only Pattern

| Service | Location | Function |
|---------|----------|----------|
| GC (New) | `crates/global-controller/src/observability/metrics.rs:66-72` | `categorize_status_code()` |

**Issue**: GC introduces `categorize_status_code()` which maps HTTP status to `success/error/timeout`. AC doesn't have this but could benefit from it for SLO tracking.

**Similarity**: N/A (new utility)

**Recommendation**: Consider adding to common for consistency across services.

---

## Non-Duplications (Correctly Unique)

The following patterns are **correctly service-specific** and should NOT be extracted:

1. **Metric names with service prefix** (`gc_`, `ac_`, `mc_`): Each service needs its own prefix
2. **Service-specific metric types**: Token issuance (AC), MC assignment (GC), Actor mailbox (MC)
3. **Path patterns**: Each service has unique API endpoints
4. **SLO thresholds**: Different services have different performance targets

---

## Verdict

- [x] APPROVED (no BLOCKING findings)
- [ ] NOT APPROVED (BLOCKING findings exist)

**Rationale**: All duplications are TECH_DEBT (code exists in other services, not in common). The GC implementation correctly implements its own metrics since no shared utilities exist in common yet. This is the expected pattern before extraction.

---

## Summary Table

| Severity | Count | Blocking? |
|----------|-------|-----------|
| BLOCKING | 0 | Yes |
| TECH_DEBT | 7 | No |

---

## Recommended Follow-up Tasks

1. **TD-OBS-001**: Extract HTTP metrics middleware to `common::middleware::http_metrics`
2. **TD-OBS-002**: Create `PathNormalizer` utility in `common::observability`
3. **TD-OBS-003**: Define `HttpMetricsRecorder` trait in `common::observability`
4. **TD-OBS-004**: Standardize error metric recording interface
5. **TD-OBS-005**: Add `categorize_status_code()` utility to common

**Priority**: Medium (affects maintainability, not functionality)
**Effort**: ~2-3 dev days for full extraction

---

## Files Reviewed

### New GC Code
- `crates/global-controller/src/observability/metrics.rs` (478 lines)
- `crates/global-controller/src/handlers/metrics.rs` (41 lines)
- `crates/global-controller/src/middleware/http_metrics.rs` (115 lines)
- `crates/global-controller/src/handlers/meetings.rs` (instrumentation)
- `crates/global-controller/src/handlers/me.rs` (instrumentation)

### Comparison Targets
- `crates/common/src/` (no observability module - correctly)
- `crates/ac-service/src/observability/metrics.rs` (656 lines)
- `crates/ac-service/src/middleware/http_metrics.rs` (115 lines)
- `crates/meeting-controller/src/actors/metrics.rs` (588 lines)
