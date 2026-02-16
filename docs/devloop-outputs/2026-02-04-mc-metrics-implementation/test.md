# Test Specialist Code Review

**Date**: 2026-02-05
**Reviewer**: Test Specialist
**Implementation**: MC Metrics (ADR-0023 Section 11)

---

## Summary

The implementation includes reasonable unit test coverage for the metrics and health modules, following the same pattern established in the AC service. Tests exercise all metric recording functions and health state transitions. However, there are gaps in HTTP endpoint integration testing and some edge case coverage that should be addressed.

---

## Files Reviewed

| File | Test Coverage | Assessment |
|------|---------------|------------|
| `observability/metrics.rs` | 13 unit tests | Good coverage of all metric functions |
| `observability/health.rs` | 6 unit tests | Covers state management and handlers |
| `redis/client.rs` | Existing tests unchanged | Metrics instrumentation not directly tested |
| `main.rs` | No unit tests | Startup integration not testable in unit tests |

---

## Findings

### MAJOR-1: No Integration Test for /metrics Endpoint

**Location**: Missing in `tests/` directory

**Issue**: The `/metrics` endpoint is not tested via integration tests. The main.md dev-loop output explicitly lists this as tech debt: "Integration Tests for /metrics Endpoint - Add tests that verify the Prometheus text format output includes all expected metrics."

**Impact**: Cannot verify that:
- Prometheus text format is correct
- All 7 required metrics are exposed
- Metric labels match expected cardinality bounds

**Recommendation**: Add an integration test that starts a minimal server and verifies the `/metrics` endpoint returns Prometheus format containing expected metric names like `mc_connections_active`, `mc_meetings_active`, etc.

---

### MAJOR-2: No Integration Tests for Health Endpoints

**Location**: `observability/health.rs`

**Issue**: While unit tests cover `liveness_handler` and `readiness_handler` directly, there are no integration tests verifying the actual HTTP router behavior via `health_router()`. The router setup (routes, state wiring) is untested.

**Impact**: Route configuration bugs (e.g., wrong path, missing state) would not be caught.

**Recommendation**: Add an integration test using `axum::test::TestClient` to verify:
- `GET /health` returns 200
- `GET /ready` returns 503 when not ready, 200 when ready

---

### MINOR-1: Redis Metrics Recording Not Verified in Tests

**Location**: `redis/client.rs` tests

**Issue**: The Redis client tests only test serialization and key formatting. They don't verify that `record_redis_latency()` and `record_fenced_out()` are called during Redis operations (though this would require a mock recorder).

**Impact**: Could silently break metrics instrumentation without test failure.

**Recommendation**: Consider adding one test with `metrics_util::debugging::DebuggingRecorder` to verify metrics are recorded during Redis operations. This is lower priority since:
1. The metrics functions themselves are tested
2. Metrics calls are simple and unlikely to break

---

### MINOR-2: Missing Test for record_actor_panic and record_message_dropped Re-exports

**Location**: `observability/mod.rs`

**Issue**: The module re-exports `record_fenced_out`, `record_message_latency`, etc., but `record_actor_panic` and `record_message_dropped` are not re-exported despite being defined in metrics.rs. If future code relies on re-exports, this could cause issues.

**Impact**: Inconsistent API surface. Tests exist for the functions but not the re-export pattern.

**Recommendation**: Either:
1. Add these to the re-exports in mod.rs, OR
2. Document why they are intentionally not re-exported (perhaps they are MC-internal only)

---

### OBSERVATION: Test Pattern Quality

**Positive Observations**:

1. **No-op Recorder Pattern** - Tests correctly document that `metrics` crate records to a no-op if no recorder is installed. This is the correct approach for unit testing metrics recording functions.

2. **Cardinality Test** - `test_cardinality_bounds()` explicitly tests that only bounded label values are used, addressing ADR-0011 requirements.

3. **Thread Safety Test** - `test_health_state_thread_safety()` verifies atomic operations work correctly across threads.

4. **Async Handler Tests** - `#[tokio::test]` tests properly exercise async handlers.

5. **Edge Case Coverage** - Tests cover zero values, typical values, and boundary conditions for metrics.

---

## ADR-0011 Testing Requirements Check

Per ADR-0011 Section 9 - Testing Requirements:

| Requirement | Status | Notes |
|-------------|--------|-------|
| Metrics unit tests - All metrics recorded | PARTIAL | Functions tested, but not recording verified |
| PII leakage tests - All log paths | N/A | No logging added in this change |
| Trace propagation - Service boundaries | N/A | No tracing changes in this scope |
| Dashboard availability - All dashboards load | N/A | Dashboards not in scope |
| Chaos metrics verification - Critical paths | MISSING | No chaos/failure scenario tests |

---

## Verdict

**REQUEST_CHANGES**

The implementation has solid unit tests for the core metrics and health functionality, but lacks integration tests for the HTTP endpoints. Given that `/metrics` is the primary interface for observability (Prometheus scraping) and `/health` + `/ready` are critical for Kubernetes deployments, integration tests should be added before merge.

---

## Finding Count

| Severity | Count | Details |
|----------|-------|---------|
| BLOCKER | 0 | - |
| CRITICAL | 0 | - |
| MAJOR | 2 | Missing integration tests for /metrics and health endpoints |
| MINOR | 2 | Redis metrics verification, inconsistent re-exports |

---

## Suggested Test Additions

### Integration Test for Health Endpoints

```rust
// tests/health_endpoints.rs
#[tokio::test]
async fn test_health_endpoint_returns_ok() {
    let state = Arc::new(HealthState::new());
    let app = health_router(Arc::clone(&state));

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ready_endpoint_reflects_state() {
    let state = Arc::new(HealthState::new());
    let app = health_router(Arc::clone(&state));

    // Initially not ready
    let response = app.clone()
        .oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    // After set_ready
    state.set_ready();
    let response = app
        .oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
```

### Integration Test for Metrics Endpoint

```rust
// tests/metrics_endpoint.rs
#[tokio::test]
async fn test_metrics_endpoint_returns_prometheus_format() {
    // Initialize test recorder
    let handle = PrometheusBuilder::new().install_recorder().unwrap();

    // Record some metrics
    set_connections_active(5);
    set_meetings_active(3);

    let output = handle.render();

    // Verify expected metrics are present
    assert!(output.contains("mc_connections_active"));
    assert!(output.contains("mc_meetings_active"));
}
```

---

## Return Format

```
verdict: REQUEST_CHANGES
finding_count:
  blocker: 0
  critical: 0
  major: 2
  minor: 2
summary: Unit tests adequately cover metric recording functions and health state management. Missing integration tests for /metrics and /health HTTP endpoints, which are critical for Prometheus scraping and Kubernetes deployments. Two minor issues around Redis metrics verification and inconsistent module re-exports.
```
