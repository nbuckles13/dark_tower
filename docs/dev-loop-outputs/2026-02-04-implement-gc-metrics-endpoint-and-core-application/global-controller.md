# Global Controller Specialist Checkpoint

**Date**: 2026-02-04
**Task**: Implement GC Metrics Endpoint and Core Application Metrics
**Status**: Complete

---

## Patterns Discovered

### 1. Metrics Module Pattern
Following AC service pattern, created a dedicated `observability/metrics.rs` module with:
- Metric recording functions that accept labels and durations
- Path normalization to prevent cardinality explosion
- Status code categorization (success/error/timeout)
- `#[allow(dead_code)]` for metrics defined for future use

### 2. Prometheus Handle Sharing
The Prometheus recorder can only be installed once per process. For test utilities:
- Used `OnceLock<PrometheusHandle>` to share handle across tests
- Fallback to `build_recorder().handle()` when recorder already installed

### 3. Privacy-by-Default Instrumentation
Updated handlers with `#[instrument(skip_all)]` pattern:
- Named spans following `gc.<subsystem>.<operation>` convention
- Only SAFE fields in span attributes (method, endpoint, status)
- Empty status field to be filled later when needed

### 4. Histogram Bucket Alignment
Buckets aligned with ADR-0010/ADR-0011 SLO targets:
- HTTP: [0.005, ..., 0.200, ..., 2.0] for p95 < 200ms
- MC assignment: [0.005, ..., 0.020, ..., 0.5] for p95 < 20ms
- DB queries: [0.001, ..., 0.050, ..., 1.0] for p99 < 50ms

---

## Gotchas Encountered

### 1. Module Resolution in main.rs vs lib.rs
The binary (main.rs) has its own module tree separate from lib.rs. Had to add:
```rust
mod observability;  // In main.rs
```
Not just relying on lib.rs exports.

### 2. Integration Test Dependencies
Tests in `tests/` directory are external and need explicit dependencies on metrics-exporter-prometheus even though it's in regular dependencies. Added to dev-dependencies as well.

### 3. Test Server Metrics Handle
Each test server needs a metrics handle, but we can only install one recorder per process. Solution:
- Global `OnceLock` to lazily initialize and share the handle
- Fallback chain for when recorder is already installed

### 4. init_metrics_recorder Signature Change
Updating `build_routes()` to require `PrometheusHandle` required updating:
- main.rs initialization
- gc-test-utils server harness
- All integration tests (auth_tests.rs, meeting_tests.rs)

---

## Key Decisions

### 1. Deferred Metric Wiring
Defined all required metrics per ADR-0011 but marked many as `#[allow(dead_code)]`:
- HTTP metrics: wired via middleware (active)
- MC assignment, DB, gRPC, MH metrics: defined but not yet instrumented

Rationale: Provides the complete metrics framework without requiring extensive refactoring of services/repositories in this PR.

### 2. HTTP Metrics Middleware Position
Placed HTTP metrics middleware as outermost layer:
```rust
.layer(middleware::from_fn(http_metrics_middleware))
```
This captures ALL responses including framework-level errors (415, 404, 405).

### 3. Endpoint Normalization Strategy
Parameterized dynamic segments to bound cardinality:
- `/api/v1/meetings/{code}` - meeting code
- `/api/v1/meetings/{id}/settings` - meeting UUID
- `/other` - catch-all for unknown paths

### 4. Metrics Documentation
Created comprehensive `docs/observability/metrics/gc.md` with:
- All metric definitions
- Labels and cardinality estimates
- SLO alignment
- PromQL query examples

---

## Current Status

### Completed:
- [x] Prometheus metrics registry initialization in main.rs
- [x] `/metrics` endpoint added to routes
- [x] Core HTTP metrics (gc_http_requests_total, gc_http_request_duration_seconds)
- [x] HTTP metrics middleware capturing all responses
- [x] All required metrics defined (MC assignment, DB, token refresh, gRPC, MH)
- [x] Privacy-by-default `#[instrument(skip_all)]` on handlers
- [x] Metrics catalog documentation (gc.md)
- [x] SLO-aligned histogram buckets
- [x] gc-test-utils updated for metrics handle
- [x] Integration tests updated

### Verification Results:
- Layer 1 (cargo check): PASSED
- Layer 2 (cargo fmt): PASSED
- Layer 3 (guards): PASSED (9/9)
- Layer 4 (unit tests): PASSED (12/12 observability tests)
- Layer 5 (all tests): Unit tests pass, DB integration tests skipped (no DATABASE_URL)
- Layer 6 (clippy): PASSED
- Layer 7 (semantic guards): PASSED (10/10)

### Future Work:
- Wire metrics into repositories (DB query timing)
- Wire metrics into services (MC assignment, MH selection)
- Wire metrics into TokenManager (common crate)
- Add W3C Trace Context propagation (requires opentelemetry dependencies)

---

## Files Created
- `crates/global-controller/src/observability/mod.rs`
- `crates/global-controller/src/observability/metrics.rs`
- `crates/global-controller/src/handlers/metrics.rs`
- `crates/global-controller/src/middleware/http_metrics.rs`
- `docs/observability/metrics/gc.md`

## Files Modified
- `crates/global-controller/src/main.rs`
- `crates/global-controller/src/lib.rs`
- `crates/global-controller/src/routes/mod.rs`
- `crates/global-controller/src/handlers/mod.rs`
- `crates/global-controller/src/handlers/meetings.rs`
- `crates/global-controller/src/handlers/me.rs`
- `crates/global-controller/src/middleware/mod.rs`
- `crates/global-controller/Cargo.toml`
- `crates/gc-test-utils/src/server_harness.rs`
- `crates/gc-test-utils/Cargo.toml`
- `crates/global-controller/tests/auth_tests.rs`
- `crates/global-controller/tests/meeting_tests.rs`
