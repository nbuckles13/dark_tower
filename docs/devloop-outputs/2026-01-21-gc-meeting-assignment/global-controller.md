# Global Controller Specialist Checkpoint

**Task**: GC MC Assignment via Load Balancing (ADR-0010)
**Date**: 2026-01-21
**Status**: Completed

## Patterns Discovered

### 1. INSERT ON CONFLICT DO UPDATE for Atomic Assignment
Using PostgreSQL's `INSERT ... ON CONFLICT DO UPDATE WHERE` for atomic assignment operations:
- Single statement handles both insert (new assignment) and update (replace unhealthy)
- The `WHERE EXISTS` clause conditionally allows update only if existing MC is unhealthy
- Avoids CTE snapshot issues where NOT EXISTS sees pre-deletion state

```sql
INSERT INTO meeting_assignments (...) VALUES (...)
ON CONFLICT (meeting_id, region) DO UPDATE
SET meeting_controller_id = EXCLUDED.meeting_controller_id, ...
WHERE EXISTS (
    SELECT 1 FROM meeting_controllers mc
    WHERE mc.controller_id = meeting_assignments.meeting_controller_id
      AND mc.health_status != 'healthy'
)
RETURNING meeting_controller_id
```

### 2. Weighted Random Selection with CSPRNG
Used `ring::rand::SystemRandom` for cryptographically secure random selection:
- Convert random bytes to f64 in range [0, 1)
- Weight inversely proportional to load ratio (1.0 - load_ratio)
- Prevents thundering herd while preferring less-loaded instances

### 3. Heartbeat Staleness as Health Indicator
Combined health_status column with `last_heartbeat_at` timestamp for robust health checks:
- Query includes time-based staleness check: `last_heartbeat_at > NOW() - INTERVAL`
- Default 30-second threshold configured in constants
- Allows detection of controllers that stopped heartbeating without explicitly updating status

## Gotchas Encountered

### 1. Legacy `endpoint` Column is NOT NULL
The `meeting_controllers` table has a legacy `endpoint` column that is NOT NULL. The newer `grpc_endpoint` and `webtransport_endpoint` were added but don't replace it. Test helper functions must populate all required columns.

### 2. Test Isolation Requires MC Registration
Tests that exercise meeting join now require a healthy MC to be registered first. Without this, the service returns 503 (ServiceUnavailable). Added `register_healthy_mc_for_region()` helper for tests.

### 3. Semantic Guards Flag Pre-existing Issues
The semantic credential-leak guard flagged issues in `MeetingRow.join_token_secret` and service token handling - these are pre-existing technical debt, not introduced by this work.

## Key Decisions

### 1. Region-scoped Assignments
Assignments are scoped by (meeting_id, region) composite key. A meeting can have different assignments in different regions for geographic distribution.

### 2. Soft Deletes for Audit Trail
Assignments use `ended_at` timestamp instead of hard delete. Allows tracking assignment history and debugging race conditions.

### 3. McAssignmentInfo in JoinMeetingResponse
Added `mc_assignment` field to join response containing:
- `mc_id` - Controller identifier
- `grpc_endpoint` - For service communication
- `webtransport_endpoint` (optional) - For client connections

### 4. Dead Code Annotations
Functions for `end_assignment`, `cleanup_old_assignments`, and `get_assignment` are marked `#[allow(dead_code)]` as they're used in tests but not yet in production handlers. Will be connected when meeting end flow is implemented.

## Iteration 2 Fixes (2026-01-21)

### Findings Addressed

| # | Severity | Finding | Resolution |
|---|----------|---------|------------|
| 1 | MAJOR | Add concurrent race condition test | Added `test_concurrent_assignment_race_condition` - spawns 10 concurrent tasks, verifies atomic CTE handles race |
| 2 | MAJOR | Add MC health transition test | Added `test_mc_health_transition_creates_new_assignment` - verifies unhealthy MC gets replaced |
| 3 | MINOR | Use `#[expect]` instead of `#[allow]` | Kept `#[allow(dead_code)]` with comments - `#[expect]` causes warnings when tests compile functions |
| 4 | MINOR | Remove duplicate logging | Removed repo-layer logging at line 233-239, kept only service-layer logging |

### Key Implementation Change

**Problem Discovered**: The original CTE approach using `WITH deleted AS (...), inserted AS (...)` didn't work reliably due to PostgreSQL CTE snapshot semantics. Both CTEs see the same snapshot, but the `inserted` CTE's `NOT EXISTS` sees the pre-deletion state, leading to `ON CONFLICT DO NOTHING` silently failing.

**Solution**: Changed to `INSERT ... ON CONFLICT DO UPDATE WHERE EXISTS (unhealthy MC check)`:
- Single atomic operation that either inserts or updates
- Correctly handles the case where existing MC becomes unhealthy
- Returns the new assignment when successful
- Falls back to `get_current_assignment` on conflict (existing healthy assignment)

### Tests Added

1. **`test_concurrent_assignment_race_condition`**: Spawns 10 concurrent tasks attempting to assign the same meeting. Verifies:
   - All concurrent calls succeed
   - All return the same MC (the winner)
   - Only one active assignment exists in database

2. **`test_mc_health_transition_creates_new_assignment`**: Tests ADR-0010 health transition flow:
   - First assignment to healthy MC
   - MC marked unhealthy
   - Second assignment goes to different healthy MC
   - Verifies exactly one active assignment exists

## Current Status

All verification layers passed:
- Layer 1: cargo check - PASS
- Layer 2: cargo fmt - PASS
- Layer 3: Guards - PASS (8/8)
- Layer 4: Unit tests - PASS (201 tests)
- Layer 5: All tests - PASS (18 meeting assignment tests + full workspace)
- Layer 6: Clippy - PASS

Implementation is complete with all code review findings addressed.
