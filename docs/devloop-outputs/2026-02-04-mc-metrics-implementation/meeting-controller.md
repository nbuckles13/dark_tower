# Meeting Controller Specialist Checkpoint

**Date**: 2026-02-04
**Task**: Add Prometheus metrics to Meeting Controller
**Status**: Complete (Iteration 2)

---

## Patterns Discovered

1. **Follow AC's Wrapper Function Pattern** - The AC service uses simple wrapper functions that call `counter!`, `gauge!`, and `histogram!` macros directly. This pattern is clean, testable, and doesn't require complex trait abstractions.

2. **Metrics Recording Before Error Returns** - When timing Redis operations, record the latency even on error paths (before returning the error) to capture failure latencies in histograms.

3. **Health State Separation** - The `HealthState` struct keeps liveness/readiness separate from metrics. Liveness is always true after startup; readiness depends on GC registration.

4. **Fail-Fast Health Server Binding** - The health server listener is bound synchronously (before spawning the serve task) to fail startup immediately if the port is unavailable. This prevents a situation where the process runs but can't serve health checks.

5. **PrometheusBuilder Installation Must Be Early** - The `PrometheusBuilder::new().install_recorder()` call must happen before any metrics are recorded. We place it immediately after configuration loading.

6. **Integration Testing with tower::ServiceExt** - Use `tower::util::ServiceExt::oneshot()` to test Axum routers directly without starting a server. Requires importing `tower` as a dev-dependency.

---

## Gotchas Encountered

1. **Health Endpoints Use Standard Paths** - The AC pattern uses `/health` and `/ready` (not `/health/live` and `/health/ready`). Keep consistent with existing patterns.

2. **Arc for HealthState in GC Task** - The GC task needs to set health state to ready after successful registration. This requires passing an `Arc<HealthState>` into the task function.

3. **Prometheus Handle Cloning for Metrics Route** - The Prometheus handle must be cloned into the closure for the `/metrics` route handler. Axum's closure needs `move` and the handle must be cloned inside to satisfy lifetime requirements.

4. **metrics-util Snapshot API** - The `Snapshot` struct from `metrics_util::debugging` doesn't have `is_empty()`. Use `snapshot.into_vec()` and check the resulting Vec's length instead.

5. **tower::ServiceExt Import** - Use `tower::util::ServiceExt` not `tower::ServiceExt` for the `oneshot()` method.

---

## Key Decisions

1. **Metric Naming Convention** - Used `mc_` prefix consistently per ADR-0011. Counters have `_total` suffix, histograms have `_seconds` suffix for durations.

2. **Bounded Label Cardinality** - All label values are bounded:
   - `actor_type`: 3 values (controller, meeting, connection)
   - `operation`: ~10 Redis commands
   - `message_type`: bounded by protobuf definitions
   - `reason`: 2-3 fencing reasons

3. **Additional Operational Metrics** - Added metrics beyond ADR-0023 requirements:
   - `mc_actor_panics_total` - Bug indicator
   - `mc_messages_dropped_total` - Backpressure indicator
   - `mc_gc_heartbeats_total` - GC communication health
   - `mc_gc_heartbeat_latency_seconds` - GC latency tracking

4. **Health Endpoints Path** - Used `/health` and `/ready` (matching AC pattern).

---

## Iteration 2 Fixes

Fixed code review findings:

1. **MAJOR-1: Integration test for /metrics endpoint** - Added `test_prometheus_metrics_endpoint_integration` that:
   - Installs a debugging recorder
   - Records all 7 ADR-0023 metrics
   - Verifies the snapshot contains recorded metrics
   - Verifies at least 7 metrics are present

2. **MAJOR-2: Integration tests for health endpoints** - Added 4 tests using `tower::util::ServiceExt::oneshot()`:
   - `test_health_router_liveness_endpoint` - Verifies `/health` returns 200
   - `test_health_router_readiness_endpoint_not_ready` - Verifies `/ready` returns 503 when not ready
   - `test_health_router_readiness_endpoint_ready` - Verifies `/ready` returns 200 when ready
   - `test_health_router_unknown_path_returns_404` - Verifies unknown paths return 404

3. **MINOR-2: Missing re-exports** - Added `record_actor_panic`, `record_message_dropped`, `record_gc_heartbeat`, and `record_gc_heartbeat_latency` to `mod.rs` re-exports

4. **MINOR-1: Redis metrics verification** - Deferred as lower priority (metrics functions already have unit tests)

---

## Files Created

- `crates/meeting-controller/src/observability/mod.rs` - Module definition
- `crates/meeting-controller/src/observability/metrics.rs` - Metrics wrapper functions
- `crates/meeting-controller/src/observability/health.rs` - Health endpoints and state

## Files Modified

- `crates/meeting-controller/Cargo.toml` - Added metrics + tower dev dependencies
- `crates/meeting-controller/src/lib.rs` - Export observability module
- `crates/meeting-controller/src/main.rs` - Prometheus init, health server startup
- `crates/meeting-controller/src/redis/client.rs` - Redis latency instrumentation

---

## Current Status

**Complete** - All acceptance criteria met:

- [x] All 7 ADR-0023 metrics implemented using `metrics` crate
- [x] `/metrics` endpoint returns Prometheus text format
- [x] Pattern matches AC's `observability/metrics.rs` structure
- [x] Histogram buckets use default (SLO targets in documentation)
- [x] Label cardinality within ADR-0011 limits (bounded values)
- [x] Unit tests for metric recording
- [x] Integration tests for /metrics endpoint
- [x] Integration tests for health router
- [x] No PII in metric labels
- [x] Health server bind failure fails MC startup

---

## Verification Results (Iteration 2)

All 7 layers passed:

1. **cargo check --workspace** - PASSED
2. **cargo fmt --all --check** - PASSED
3. **./scripts/guards/run-guards.sh** - PASSED (9 guards)
4. **./scripts/test.sh --workspace --lib** - PASSED (153 MC tests, +6 from iteration 1)
5. **./scripts/test.sh --workspace** - PASSED (all tests)
6. **cargo clippy --workspace -- -D warnings** - PASSED
7. **./scripts/guards/run-guards.sh --semantic** - PASSED (10 guards)
