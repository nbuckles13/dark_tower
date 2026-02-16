# Specialist Checkpoint: global-controller

**Date**: 2026-01-22
**Task**: GC Assignment Cleanup - connecting end_assignment and cleanup_old_assignments

---

## Patterns Discovered

### 1. Background Task with CancellationToken Pattern
Following the existing `health_checker.rs` pattern:
- Use `tokio::time::interval` for periodic execution
- Use `tokio::select!` to race work vs cancellation
- Log task start/stop with target-qualified log messages
- Return unit type (task runs forever until cancelled)

### 2. Configuration from Environment Pattern
Environment variables with defaults:
- Parse with `std::env::var().ok().and_then(|s| s.parse().ok()).unwrap_or(default)`
- Provide `from_env()` constructor for configuration structs
- Log configuration on startup for debugging

### 3. Soft-Delete Before Hard-Delete Pattern
For cleanup operations:
1. Soft-delete: Set `ended_at = NOW()` for stale items
2. Hard-delete: `DELETE WHERE ended_at < NOW() - INTERVAL`
3. Only soft-delete items with unhealthy MCs to avoid incorrectly ending active meetings

### 4. gRPC Method Extension Pattern
Adding new methods to existing gRPC service:
1. Update `.proto` file with new message types and service method
2. Rebuild proto-gen crate to regenerate bindings
3. Add method implementation to service impl block
4. Import new message types

---

## Gotchas Encountered

### 1. Proto Regeneration Required
After modifying `internal.proto`, must run `cargo build -p proto-gen` to regenerate Rust bindings before the new types are available.

### 2. Stale Assignment Detection Logic
Initially considered ending all old assignments, but realized this would incorrectly end active meetings. The correct approach:
- Only end assignments where the MC is unhealthy (not responding to heartbeats)
- This ensures active meetings with healthy MCs are not incorrectly terminated

### 3. PostgreSQL Interval Casting
Use `($1 || ' hours')::INTERVAL` pattern for dynamic interval values with string binding. This is consistent with existing code in the repository.

---

## Key Decisions

### 1. Cleanup Task as Separate Background Task
Rationale: Following the existing pattern of health_checker as a separate background task. This allows independent configuration and lifecycle management.

### 2. Configurable Parameters via Environment
- `GC_CLEANUP_INTERVAL_SECONDS` - default 3600 (1 hour)
- `GC_INACTIVITY_HOURS` - default 1 hour before soft-delete
- `GC_RETENTION_DAYS` - default 7 days before hard-delete

### 3. Only End Stale Assignments with Unhealthy MCs
Rationale: A meeting assigned 2 hours ago with a healthy MC is likely still active. Only end assignments when both:
- Assigned long ago (exceeds inactivity threshold)
- MC is unhealthy (not sending heartbeats)

### 4. gRPC Endpoint for Meeting End Notification
Created `NotifyMeetingEnded` RPC method for MCs to notify GC when meeting ends normally. This allows proper soft-delete for audit trail.

---

## Current Status

**Status**: Iteration 2 fixes applied, verification passed

**Files Created**:
- `crates/global-controller/src/tasks/assignment_cleanup.rs`

**Files Modified**:
- `proto/internal.proto` - Added NotifyMeetingEnded message types and RPC method
- `crates/global-controller/src/grpc/mc_service.rs` - Implemented notify_meeting_ended
- `crates/global-controller/src/tasks/mod.rs` - Export new task module
- `crates/global-controller/src/main.rs` - Wire up cleanup task
- `crates/global-controller/src/services/mc_assignment.rs` - Remove dead_code annotation
- `crates/global-controller/src/repositories/meeting_assignments.rs` - Add end_stale_assignments, remove dead_code annotations

**Verification Results**:
- Layer 1 (cargo check): PASSED
- Layer 2 (cargo fmt): PASSED
- Layer 3 (guards): PASSED (8/8)
- Layer 4 (unit tests): PASSED (211 tests)
- Layer 5 (all tests): PASSED
- Layer 6 (clippy): PASSED
- Layer 7 (semantic guards): PASSED

---

## Test Coverage

New tests added in `assignment_cleanup.rs`:
1. `test_default_config` - Verify default configuration values
2. `test_default_check_interval` - Verify default interval constant
3. `test_default_inactivity_hours` - Verify default inactivity constant
4. `test_default_retention_days` - Verify default retention constant
5. `test_cancellation_token_stops_task` - Unit test for graceful shutdown
6. `test_assignment_cleanup_starts_and_stops` - Integration test for task lifecycle
7. `test_assignment_cleanup_ends_stale_assignments` - Integration test for soft-delete
8. `test_assignment_cleanup_preserves_healthy_assignments` - Integration test for healthy MCs
9. `test_assignment_cleanup_hard_deletes_old_assignments` - Integration test for hard-delete
10. `test_assignment_cleanup_preserves_recent_ended_assignments` - Integration test for retention

All tests use `#[sqlx::test(migrations = "../../migrations")]` for database integration tests.
