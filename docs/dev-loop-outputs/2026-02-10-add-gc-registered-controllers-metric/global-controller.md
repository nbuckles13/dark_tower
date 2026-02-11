# Global Controller Specialist Checkpoint

**Date**: 2026-02-10
**Task**: Add gc_registered_controllers gauge metric

---

## Patterns Discovered

1. **Gauge Metrics Pattern**: The codebase uses the `metrics` crate with `gauge!` macro for gauge metrics. Unlike counters, gauges are set to absolute values (not incremented). This required using `.set(count as f64)` instead of `.increment()`.

2. **Status as Database Enum**: The `HealthStatus` enum in `repositories/meeting_controllers.rs` maps to the database health_status column. It provides `as_db_str()` and `from_db_str()` for conversion. When querying counts, I needed to convert the enum back to its string form for metric labels.

3. **Metrics Refresh After DB Operations**: The pattern for updating metrics after database changes is to query the current state and set the gauge values, rather than trying to track deltas. This ensures accuracy even if multiple GC instances are running.

4. **Centralized Metric Updates**: Created a helper function `update_registered_controller_gauges()` that takes counts and sets all 5 status values (including zeros for missing statuses). This ensures Prometheus always sees all time series.

5. **Startup Initialization Pattern**: The metric is initialized in `main.rs` after the database pool is created but before accepting traffic. This ensures the metric reflects actual state even after GC restart.

---

## Gotchas Encountered

1. **Impl Block Placement**: When adding a new function to an impl block, I accidentally placed it after the closing brace. The Rust compiler error was helpful but required careful reading of the context.

2. **Import Ordering**: cargo fmt reorders imports alphabetically, which required reordering the `repositories::MeetingControllersRepository` import in main.rs.

3. **HealthStatus Conversion**: The `HealthStatus::from_db_str()` method exists but returns the enum, not a string. For metrics, I needed to use `status.as_db_str()` to get the string representation for metric labels.

4. **Cardinality Awareness**: Per ADR-0011, I documented the cardinality of the metric explicitly (2 controller types x 5 statuses = 10 combinations), which is well within the 1000 limit per metric.

5. **Background Task Integration**: The health_checker task needed the metric refresh after marking stale controllers unhealthy. I had to import the metrics module and create a local helper function that does the same conversion as McService.

---

## Key Decisions

1. **Refresh on Every DB Change**: Rather than trying to track increments/decrements, I chose to re-query the database for current counts after each relevant operation. This is simpler and more robust, with minimal overhead (one lightweight GROUP BY query).

2. **Set All Statuses Including Zeros**: The `update_registered_controller_gauges()` helper always sets all 5 status values, defaulting missing ones to 0. This ensures consistent time series in Prometheus.

3. **Initialize on Startup**: The metric is populated from database state during GC startup, not just from heartbeats. This ensures correct values after GC restart.

4. **Metric in Health Checker**: Added metric refresh to the health checker background task since it can change controller statuses (mark stale as unhealthy).

5. **Warning on Initialization Failure**: If the startup query fails, we log a warning but don't fail startup. The metric will be populated correctly on the first heartbeat.

---

## Current Status

**COMPLETE** - All verification layers pass:
- Layer 1 (cargo check): PASS
- Layer 2 (cargo fmt): PASS
- Layer 3 (guards): PASS
- Layer 4 (unit tests): PASS
- Layer 5 (all tests): PASS
- Layer 6 (clippy): PASS
- Layer 7 (semantic guards): PASS

---

## Files Modified

1. **crates/global-controller/src/observability/metrics.rs**
   - Added `set_registered_controllers()` gauge function
   - Added `CONTROLLER_STATUSES` constant
   - Added `update_registered_controller_gauges()` helper
   - Added tests for the new functions

2. **crates/global-controller/src/repositories/meeting_controllers.rs**
   - Added `get_controller_counts_by_status()` query function
   - Added `ControllerCountRow` struct for query result

3. **crates/global-controller/src/grpc/mc_service.rs**
   - Added `refresh_controller_metrics()` method
   - Called metric refresh after registration
   - Called metric refresh after fast_heartbeat
   - Called metric refresh after comprehensive_heartbeat

4. **crates/global-controller/src/tasks/health_checker.rs**
   - Added `refresh_controller_metrics()` local function
   - Called metric refresh after marking stale controllers unhealthy

5. **crates/global-controller/src/main.rs**
   - Added `init_registered_controllers_metric()` function
   - Called metric initialization on startup after database connection

---

## Metric Details

- **Name**: `gc_registered_controllers`
- **Type**: Gauge
- **Labels**:
  - `controller_type`: "meeting" (or "media" for future MH support)
  - `status`: "pending", "healthy", "degraded", "unhealthy", "draining"
- **Cardinality**: 10 combinations (2 types x 5 statuses)
- **Example**: `gc_registered_controllers{controller_type="meeting",status="active"} 5`

---

## Reflection

### Knowledge Gained

Added 1 new pattern to `docs/specialist-knowledge/global-controller/patterns.md`: **Gauge Metric Initialization from Database State**. This pattern addresses the challenge of ensuring gauge metrics accurately reflect system state after a service restart, rather than starting at zero. The implementation demonstrated the value of querying the source of truth (database) during startup and setting all possible label combinations to create consistent Prometheus time series.

### Implementation Smoothness

The implementation proceeded without major obstacles. Existing patterns (Repository Layer for DB Query Metrics, Instant::now() for Operation Duration Metrics) provided clear guidance. The main design decision was whether to track deltas (increment/decrement) or query current state - I chose the latter for robustness across multiple GC instances.

### Code Review Findings

All 4 reviewers approved with 0 blocking findings. 4 tech debt items were noted, primarily around duplication of the metric refresh logic across 3 locations (mc_service.rs and health_checker.rs). This is acceptable for now but should be consolidated in a future refactor, possibly as a shared metric refresh service or trait.
