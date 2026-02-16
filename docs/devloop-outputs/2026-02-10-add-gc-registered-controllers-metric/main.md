# Dev-Loop Output: Add GC Registered Controllers Metric

**Date**: 2026-02-10
**Start Time**: 21:51
**Task**: Add GC registered controllers metric to expose count of registered Meeting Controllers. GC should expose a gauge metric like gc_registered_controllers{controller_type="meeting",status="active"} to show how many MCs are currently registered. This will enable operators to monitor MC fleet size and detect registration issues. The metric should be incremented when MCs register and decremented when they become inactive or are removed.
**Branch**: `feature/mc-heartbeat-metrics`
**Duration**: ~45m

---

## Loop State (Internal)

<!-- This section is maintained by dev-loop skills for state recovery. -->

| Field | Value |
|-------|-------|
| Implementing Agent | `a0cff05` |
| Implementing Specialist | `global-controller` |
| Current Step | `complete` |
| Iteration | `1` |
| Security Reviewer | `a3b8829` |
| Test Reviewer | `ae72c19` |
| Code Reviewer | `a773b72` |
| DRY Reviewer | `a2c598e` |

---

## Task Overview

### Objective

Add a gauge metric to GC to expose the count of registered Meeting Controllers by type and status.

### Detailed Requirements

**Problem**: The GC dashboard currently has no way to see how many Meeting Controllers are registered. Operators cannot easily monitor MC fleet size or detect registration issues.

**Metric to Add**:
- Name: `gc_registered_controllers`
- Type: Gauge
- Labels:
  - `controller_type`: "meeting" (future: "media" for Media Handlers)
  - `status`: "active", "draining", or other status values
- Example: `gc_registered_controllers{controller_type="meeting",status="active"} 5`

**Implementation Requirements**:

1. **Define metric in `observability/metrics.rs`**:
   - Add a gauge metric function like `set_registered_controllers(controller_type: &str, status: &str, count: u64)`
   - Follow existing pattern (bounded labels per ADR-0011)
   - Document cardinality: controller_type (2 values max) × status (3-4 values) = 6-8 combinations

2. **Wire metric to controller registration/deregistration**:
   - Location: `services/mc_assignment.rs` or `repositories/meeting_controllers.rs`
   - Increment when MC registers (after successful database insert)
   - Decrement when MC becomes inactive or is removed
   - Update on status changes (active ↔ draining)

3. **Query database for initial counts**:
   - On GC startup, query database for current controller counts by type and status
   - Set gauge to correct values before accepting traffic
   - Prevents metric starting at 0 after GC restart when MCs are already registered

**Files to Modify**:
1. `crates/global-controller/src/observability/metrics.rs` - Add metric definition
2. `crates/global-controller/src/services/mc_assignment.rs` or `repositories/meeting_controllers.rs` - Wire metric updates
3. Possibly `crates/global-controller/src/main.rs` - Initialize metric on startup

**Acceptance Criteria**:
- [ ] Metric `gc_registered_controllers` is defined with bounded labels
- [ ] Metric is incremented when MC registers successfully
- [ ] Metric is decremented when MC is removed or becomes inactive
- [ ] Metric is initialized correctly on GC startup (query DB for current counts)
- [ ] Metric follows ADR-0011 cardinality limits
- [ ] All tests pass
- [ ] Dashboard panel can be added later to visualize this metric

### Scope
- **Service(s)**: Global Controller
- **Schema**: No schema changes (read-only query on existing tables)
- **Cross-cutting**: Observability (metric definition)

### Debate Decision
N/A - Straightforward metric addition, no architectural decision needed

---

## Matched Principles

The following principle categories were matched:
- `docs/principles/observability.md` (matched on "metric")
- `docs/principles/logging.md` (matched on "metric")

---

## Pre-Work

TBD

---

## Implementation Summary

### Overview

Added a gauge metric `gc_registered_controllers` to expose the count of registered Meeting Controllers by type and status. The metric follows ADR-0011 cardinality guidelines with bounded labels.

### Changes Made

1. **Metric Definition** (`crates/global-controller/src/observability/metrics.rs`):
   - Added `set_registered_controllers(controller_type, status, count)` function using `gauge!` macro
   - Added `CONTROLLER_STATUSES` constant for all valid status values
   - Added `update_registered_controller_gauges()` helper that sets all 5 status values (including zeros)
   - Added comprehensive tests for the new functions

2. **Database Query** (`crates/global-controller/src/repositories/meeting_controllers.rs`):
   - Added `get_controller_counts_by_status()` function that returns a `Vec<(HealthStatus, i64)>`
   - Uses GROUP BY query to count controllers per status
   - Records DB query metrics per ADR-0011

3. **gRPC Service Integration** (`crates/global-controller/src/grpc/mc_service.rs`):
   - Added `refresh_controller_metrics()` method to McService
   - Calls metric refresh after:
     - MC registration (register_mc)
     - Fast heartbeat (fast_heartbeat)
     - Comprehensive heartbeat (comprehensive_heartbeat)

4. **Health Checker Integration** (`crates/global-controller/src/tasks/health_checker.rs`):
   - Added local `refresh_controller_metrics()` function
   - Calls metric refresh after marking stale controllers unhealthy

5. **Startup Initialization** (`crates/global-controller/src/main.rs`):
   - Added `init_registered_controllers_metric()` function
   - Queries database for current counts on startup
   - Logs total controller count at startup

### Metric Specification

- **Name**: `gc_registered_controllers`
- **Type**: Gauge
- **Labels**:
  - `controller_type`: "meeting" (future: "media")
  - `status`: "pending", "healthy", "degraded", "unhealthy", "draining"
- **Cardinality**: 10 combinations (2 types x 5 statuses) - well within ADR-0011 limit
- **Example**: `gc_registered_controllers{controller_type="meeting",status="healthy"} 5`

### Files Modified

| File | Changes |
|------|---------|
| `crates/global-controller/src/observability/metrics.rs` | Added gauge metric function, helper, constant, and tests |
| `crates/global-controller/src/repositories/meeting_controllers.rs` | Added count query function and row struct |
| `crates/global-controller/src/grpc/mc_service.rs` | Added metric refresh method and calls |
| `crates/global-controller/src/tasks/health_checker.rs` | Added metric refresh function and call |
| `crates/global-controller/src/main.rs` | Added startup initialization |

---

## Verification Results

### Dev-Loop 7-Layer Verification

| Layer | Check | Result |
|-------|-------|--------|
| 1 | `cargo check --workspace` | PASS |
| 2 | `cargo fmt --all --check` | PASS |
| 3 | `./scripts/guards/run-guards.sh` | PASS (9/9 guards) |
| 4 | `./scripts/test.sh --workspace --lib` | PASS (all unit tests) |
| 5 | `./scripts/test.sh --workspace` | PASS (all tests) |
| 6 | `cargo clippy --workspace -- -D warnings` | PASS |
| 7 | `./scripts/guards/run-guards.sh --semantic` | PASS (10/10 guards) |

### Test Coverage

- Added 3 new tests in `observability/metrics.rs`:
  - `test_set_registered_controllers()` - Tests all 10 label combinations
  - `test_update_registered_controller_gauges()` - Tests helper with partial and full counts
  - `test_controller_statuses_constant()` - Verifies constant contains all 5 statuses

### Acceptance Criteria Verification

- [x] Metric `gc_registered_controllers` is defined with bounded labels
- [x] Metric is updated when MC registers successfully
- [x] Metric is updated when MC status changes (via heartbeat)
- [x] Metric is initialized correctly on GC startup (query DB for current counts)
- [x] Metric follows ADR-0011 cardinality limits (10 combinations << 1000 limit)
- [x] All tests pass
- [x] Dashboard panel can be added later to visualize this metric

---

## Code Review Results

### Security Specialist
**Verdict**: APPROVED ✓
**Agent ID**: a3b8829
**Findings**: 0 total (0 blocker, 0 critical, 0 major, 0 minor, 0 tech debt)

**Summary**: The implementation follows security best practices. Metrics expose only aggregate counts with bounded cardinality (10 label combinations). SQL queries are parameterized with no user-controlled input. Error handling prevents information disclosure. All gRPC endpoints require JWT authentication.

**Key Observations**:
- Bounded labels: 2 controller types × 5 statuses = 10 combinations (within ADR-0011 limits)
- No PII exposure (only aggregate counts)
- SQL injection safe (parameterized queries via sqlx)
- Graceful error handling (no information leakage)

### Test Specialist
**Verdict**: APPROVED ✓
**Agent ID**: ae72c19
**Findings**: 2 total (0 blocker, 0 critical, 0 major, 0 minor, 2 tech debt)

**Summary**: Implementation has adequate test coverage. Unit tests cover all new metric functions (set_registered_controllers, update_registered_controller_gauges, CONTROLLER_STATUSES constant). Integration tests in health_checker.rs exercise the full refresh flow.

**Tech Debt** (non-blocking):
1. Repository query `get_controller_counts_by_status()` lacks dedicated integration test (exercised indirectly)
2. Metric gauge values not directly verified (would require metrics-util test recorder)

**Key Observations**:
- 3 new unit tests covering all 10 cardinality combinations
- Integration tests exercise metric refresh in health_checker
- Error paths log warnings and continue (appropriate for metrics)

### Code Quality Reviewer
**Verdict**: APPROVED ✓
**Agent ID**: a773b72
**Findings**: 1 total (0 blocker, 0 critical, 0 major, 0 minor, 1 tech debt)

**Summary**: The implementation is well-structured with proper error handling per ADR-0002, safe parameterized SQL queries, and comprehensive test coverage. The only finding is a TECH_DEBT item for code duplication - the refresh_controller_metrics logic is implemented three times.

**Tech Debt** (non-blocking):
- `refresh_controller_metrics()` logic duplicated in 3 locations (main.rs, health_checker.rs, mc_service.rs) - should be consolidated

**Key Observations**:
- ADR-0002 compliant (no panics, no unwrap/expect)
- SQL safety (parameterized queries)
- Error resilience (metric failures don't fail operations)
- Good documentation (cardinality bounds clearly documented)

### DRY Reviewer
**Verdict**: APPROVED ✓
**Agent ID**: a2c598e
**Findings**: 1 total (0 blocker, 0 critical, 0 major, 0 minor, 1 tech debt)

**Summary**: No cross-service duplication found. Service-specific metrics follow ADR-0011 with service prefixes. Internal GC duplication of refresh_controller_metrics() logic noted as tech debt (TD-GC-002).

**Tech Debt** (non-blocking):
- TD-GC-002: Internal duplication of refresh logic in 3 locations (can be addressed in future cleanup)

**Key Observations**:
- No code exists in `common` that should have been imported
- Service-specific metrics are intentional per ADR-0011
- HealthStatus enums differ between services (proto vs DB enum)

---

## Reflection

### Lessons Learned

#### From Global Controller Specialist

**Changes**: Added 1, Updated 0, Pruned 0

Added one new pattern from this implementation. The pattern documents how to initialize gauge metrics from database state on service startup, preventing metrics from showing incorrect zero values after restarts when the actual system state is non-zero. This pattern is reusable across any service that tracks state in a database and exposes it via gauge metrics.

**Knowledge file created**:
- `docs/specialist-knowledge/global-controller/patterns.md` (entry: Gauge Metric Initialization from Database State)

#### From Security Review

**Changes**: Added 0, Updated 0, Pruned 0

No changes needed. The review validated that existing knowledge was sufficient - the implementation correctly followed documented patterns for bounded cardinality labels, PII prevention in metrics, and SQL injection prevention. All patterns applied (metric label bounding, parameterized queries, graceful error handling) were already documented in patterns.md.

#### From Test Review

**Changes**: Updated 1, Added 0, Pruned 0

Updated the "Observability Wiring Tests" pattern with a new example from GC. The pattern already documented the principle (simple metric wiring verified via behavior tests + wrapper module tests), but this implementation provides a concrete GC-specific example (gauge initialization + refresh on state changes). The update adds the gauge metric pattern to complement the existing counter/histogram examples.

**Knowledge file updated**:
- `docs/specialist-knowledge/test/patterns.md` (updated: Observability Wiring Tests pattern with GC example)

#### From Code Review

**Changes**: Added 0, Updated 0, Pruned 0

No changes needed. The implementation is a routine application of existing patterns with no new reusable insights to add. The tech debt finding (refresh_controller_metrics duplicated 3x) is standard DRY detection already covered in existing knowledge. The implementation correctly followed documented patterns for metrics cardinality, error handling, and SQL safety.

#### From DRY Review

**Changes**: Added 0, Updated 0, Pruned 0

No changes needed. The implementation is a routine application of existing patterns. The service-prefixed metrics are correctly identified as architectural (not duplication) per existing knowledge. The internal duplication of refresh logic (TD-GC-002) was correctly classified as non-blocking tech debt using existing guidance. The knowledge base provided complete guidance for this review.

---

### Summary

**Total Knowledge Changes**: 1 added, 1 updated, 0 pruned

One specialist (global-controller) discovered a new pattern worth documenting, and one specialist (test) enhanced an existing pattern with a concrete example. Three specialists (security, code-reviewer, DRY) confirmed their existing knowledge was sufficient. This is the expected outcome: as specialists mature, routine implementations increasingly rely on established patterns without requiring updates. The new pattern focuses on a reusable gauge metric initialization approach that will benefit future observability work.

---

## Completion Summary

**Status**: Complete

Successfully added `gc_registered_controllers` gauge metric to Global Controller to expose the count of registered Meeting Controllers by type and status. This enables operators to monitor MC fleet size and detect registration issues.

**Key Achievements**:
- ✅ Defined gauge metric with bounded labels (10 cardinality combinations)
- ✅ Implemented database query to count controllers by status
- ✅ Wired metric refresh into registration, heartbeats, and health checks
- ✅ Initialized metric on GC startup from database state
- ✅ Added comprehensive unit test coverage (3 new tests)
- ✅ All 7 verification layers passed
- ✅ All 4 code reviewers approved (2 non-blocking tech debt items)

**Metric Specification**:
```
gc_registered_controllers{controller_type="meeting",status="healthy"} 5
gc_registered_controllers{controller_type="meeting",status="unhealthy"} 1
gc_registered_controllers{controller_type="meeting",status="pending"} 0
gc_registered_controllers{controller_type="meeting",status="degraded"} 0
gc_registered_controllers{controller_type="meeting",status="draining"} 0
```

**Tech Debt Tracked** (non-blocking):
- TD-GC-002: `refresh_controller_metrics()` duplicated in 3 locations
- Repository query `get_controller_counts_by_status()` lacks dedicated integration test
- Metric gauge values not directly verified in tests

**Impact**: Operators can now monitor MC registration health via Prometheus/Grafana. The metric can be used to create alerts for MC fleet sizing issues and to track MC availability trends over time.

**Next Steps**: Add dashboard panel to visualize this metric in `infra/grafana/dashboards/gc-overview.json`
