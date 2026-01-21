# Global Controller - MC Registration gRPC Infrastructure

**Date**: 2026-01-20
**Task**: Implement MC registration gRPC infrastructure for Global Controller
**Phase**: 2 (gRPC service and repository implementation)

## Files Created

1. **`crates/global-controller/src/repositories/mod.rs`**
   - Module declaration for repository layer
   - Re-exports `HealthStatus` and `MeetingControllersRepository`

2. **`crates/global-controller/src/repositories/meeting_controllers.rs`**
   - `HealthStatus` enum: pending, healthy, degraded, unhealthy, draining
   - `MeetingController` struct for database row representation
   - `MeetingControllersRepository` with:
     - `register_mc()` - UPSERT for MC registration
     - `update_heartbeat()` - Update heartbeat, capacity, and health status
     - `mark_stale_controllers_unhealthy()` - Batch update for stale controllers
     - `get_controller()` - Get by ID (prepared for future use)
   - Uses runtime `sqlx::query()` to avoid compile-time database requirement

3. **`crates/global-controller/src/grpc/mod.rs`**
   - Module declarations for gRPC services
   - Re-exports `McService` and `GrpcAuthInterceptor`

4. **`crates/global-controller/src/grpc/mc_service.rs`**
   - Implements `GlobalControllerService` trait from tonic
   - `register_mc()` - Validate and register MC, return heartbeat intervals
   - `fast_heartbeat()` - Lightweight heartbeat with capacity update
   - `comprehensive_heartbeat()` - Full heartbeat with metrics
   - Input validation helpers for controller_id, region, endpoint, capacity
   - Default intervals: fast=10s, comprehensive=30s

5. **`crates/global-controller/src/grpc/auth_layer.rs`**
   - `GrpcAuthInterceptor` - Synchronous interceptor (alternative API)
   - `async_auth::GrpcAuthLayer` - Tower layer for async JWT validation
   - `async_auth::GrpcAuthService` - Tower service with async validation
   - `ValidatedClaims` wrapper for request extensions
   - Token size limit: 8KB (security requirement)

6. **`crates/global-controller/src/tasks/mod.rs`**
   - Module declaration for background tasks
   - Re-exports `start_health_checker`

7. **`crates/global-controller/src/tasks/health_checker.rs`**
   - Background task running every 5 seconds
   - Marks stale controllers (>staleness threshold) as unhealthy
   - Graceful shutdown via CancellationToken
   - Continues running on database errors (resilient)

## Files Modified

1. **`crates/global-controller/src/lib.rs`**
   - Added `pub mod grpc;`
   - Added `pub mod repositories;`
   - Added `pub mod tasks;`

2. **`crates/global-controller/src/config.rs`**
   - Added `grpc_bind_address: String` (default: "0.0.0.0:50051")
   - Added `mc_staleness_threshold_seconds: u64` (default: 30)
   - Added env var parsing: `GC_GRPC_BIND_ADDRESS`, `MC_STALENESS_THRESHOLD_SECONDS`
   - Updated tests for new config fields

3. **`crates/global-controller/src/main.rs`**
   - Start gRPC server alongside HTTP server using `tokio::select!`
   - Apply `GrpcAuthLayer` for async JWT validation on gRPC
   - Spawn health checker background task
   - Coordinate graceful shutdown via `CancellationToken`

4. **`crates/global-controller/Cargo.toml`**
   - Added `tokio-util = { workspace = true }` for CancellationToken

5. **`crates/proto-gen/src/lib.rs`**
   - Added clippy allows for generated code: `default_trait_access`, `too_many_lines`

## Key Implementation Decisions

### 1. Runtime SQL Queries
Used `sqlx::query()` with `.bind()` instead of `sqlx::query!()` macro to avoid requiring DATABASE_URL at compile time. This allows building without a running database while maintaining SQL injection safety through parameterized queries.

### 2. Dual Authentication Approaches
Provided two authentication options:
- **Synchronous Interceptor** (`GrpcAuthInterceptor`): Extracts token, stores for later validation
- **Async Tower Layer** (`GrpcAuthLayer`): Full async JWT validation before service call

The async layer is used in main.rs for production, while the interceptor is available as an alternative.

### 3. Health Status Enum
Created explicit `HealthStatus` enum with conversions:
- `from_proto(i32)` - Convert from proto enum values
- `as_db_str()` - Convert to database string
- `from_db_str(&str)` - Parse from database

### 4. Resilient Health Checker
The health checker task:
- Runs every 5 seconds (configurable)
- Logs errors but continues running (database may recover)
- Respects cancellation token for graceful shutdown

## Gotchas Discovered

1. **tonic::Status is large** - Clippy warns about `result_large_err` for functions returning `Result<(), Status>`. Added `#[allow(clippy::result_large_err)]` since Status is the standard gRPC error type.

2. **Generated proto code** - Proto-gen generates code with clippy warnings. Added crate-level allows for `default_trait_access` and `too_many_lines`.

3. **Borrow checker with interceptor** - Had to copy token string before releasing borrow in the interceptor's call method.

## Test Coverage

**New Tests Added**: 40 tests

- `repositories/meeting_controllers.rs`: 4 unit tests
  - `test_health_status_from_proto`
  - `test_health_status_as_db_str`
  - `test_health_status_from_db_str`
  - `test_health_status_roundtrip`

- `tests/mc_repository_tests.rs`: 9 integration tests (NEW - added by Test specialist)
  - `test_register_mc_creates_new_record` - Verify INSERT creates record
  - `test_register_mc_upsert_updates_existing` - Verify ON CONFLICT updates
  - `test_update_heartbeat_success` - Verify heartbeat updates capacity/timestamp
  - `test_update_heartbeat_returns_false_for_missing` - Verify false for unknown ID
  - `test_mark_stale_controllers_unhealthy_marks_stale` - Verify stale marked
  - `test_mark_stale_controllers_unhealthy_skips_draining` - Verify draining preserved
  - `test_get_controller_returns_record` - Verify retrieval
  - `test_get_controller_returns_none_for_missing` - Verify None for unknown
  - `test_multiple_heartbeats_update_correctly` - Multiple heartbeat updates work

- `grpc/mc_service.rs`: 23 tests (14 original + 9 boundary tests)
  - `test_validate_controller_id_*` (5 tests)
  - `test_validate_region_*` (2 tests)
  - `test_validate_endpoint_*` (3 tests)
  - `test_validate_capacity_*` (3 tests)
  - `test_heartbeat_intervals`
  - **Boundary tests (NEW - added by Test specialist)**:
    - `test_validate_controller_id_at_255_chars` - Should pass at limit
    - `test_validate_controller_id_at_256_chars` - Should fail over limit
    - `test_validate_region_at_50_chars` - Should pass at limit
    - `test_validate_region_at_51_chars` - Should fail over limit
    - `test_validate_endpoint_at_255_chars` - Should pass at limit
    - `test_validate_endpoint_at_256_chars` - Should fail over limit
    - `test_validate_controller_id_at_1_char` - Minimum valid
    - `test_validate_region_at_1_char` - Minimum valid
    - `test_validate_endpoint_minimum_valid` - Minimum valid URL

- `grpc/auth_layer.rs`: 2 tests
  - `test_pending_token_validation_debug`
  - `test_max_token_size`

- `tasks/health_checker.rs`: 2 tests
  - `test_default_check_interval`
  - `test_cancellation_token_stops_task`

**Total global-controller tests**: 227 (all passing with DATABASE_URL set)

## Verification Results

```
cargo check --workspace: PASSED
cargo fmt --all: PASSED
cargo clippy --workspace --lib --bins -- -D warnings: PASSED
cargo test -p global-controller --lib: 166 passed, 0 failed
cargo test -p global-controller (full): 227 passed, 0 failed
```

## Code Review Iterations

### Iteration 1 - Test Specialist Findings (2026-01-20)

**Blocking findings addressed:**

1. **CRITICAL: No database integration tests** - FIXED
   - Added `tests/mc_repository_tests.rs` with 9 integration tests using `#[sqlx::test]`
   - Tests cover all repository functions: register_mc, update_heartbeat, mark_stale, get_controller
   - Tests verify UPSERT behavior, staleness marking, draining preservation

2. **CRITICAL: Missing boundary tests for validation** - FIXED
   - Added 9 boundary tests to `grpc/mc_service.rs`
   - Tests validate exact limit behavior for controller_id (255), region (50), endpoint (255)
   - Tests verify both passing at limit and failing over limit

3. **HIGH: Health checker cancellation test** - Already adequate
   - Existing test verifies token stops the task loop

---RESULT---
STATUS: SUCCESS
SUMMARY: Implemented MC registration gRPC infrastructure including repository layer, gRPC service with auth layer, health checker background task, configuration updates, and comprehensive test coverage (integration + boundary tests)
FILES_CREATED: tests/mc_repository_tests.rs (9 integration tests)
FILES_MODIFIED: repositories/mod.rs, repositories/meeting_controllers.rs, grpc/mod.rs, grpc/mc_service.rs (+ 9 boundary tests), grpc/auth_layer.rs, tasks/mod.rs, tasks/health_checker.rs, lib.rs, config.rs, main.rs, Cargo.toml, proto-gen/src/lib.rs
TESTS_ADDED: 40 (22 original + 18 from code review)
VERIFICATION: PASSED (cargo check, fmt, clippy, tests)
ERROR: none
---END---
