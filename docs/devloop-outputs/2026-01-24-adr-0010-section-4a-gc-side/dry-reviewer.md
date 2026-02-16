# DRY Reviewer Checkpoint - ADR-0010 Section 4a GC-side

**Reviewer**: DRY Reviewer
**Date**: 2026-01-24
**Task**: MH registry, GC->MC AssignMeeting RPC with retry

## Review Status

| Review | Date | Verdict |
|--------|------|---------|
| Initial | 2026-01-24 | APPROVED (0 BLOCKER, 4 TECH_DEBT) |
| Re-Review | 2026-01-24 | APPROVED (0 BLOCKER, 4 TECH_DEBT) |

---

## Files Reviewed

### New Files
- `crates/global-controller/src/grpc/mh_service.rs` (541 lines)
- `crates/global-controller/src/repositories/media_handlers.rs` (426 lines)
- `crates/global-controller/src/services/mc_client.rs` (451 lines)
- `crates/global-controller/src/services/mh_selection.rs` (317 lines)
- `crates/global-controller/src/services/mc_assignment.rs` (408 lines)
- `crates/global-controller/src/tasks/mh_health_checker.rs` (325 lines)

### Compared Against
- `crates/global-controller/src/repositories/meeting_controllers.rs` (MC registry)
- `crates/global-controller/src/repositories/meeting_assignments.rs` (MC assignment)
- `crates/global-controller/src/tasks/health_checker.rs` (MC health checker)
- `crates/global-controller/src/grpc/mc_service.rs` (MC gRPC handlers)

---

## Re-Review Findings

### No New BLOCKER Duplication Introduced

After re-reviewing the codebase, I confirm:

1. **No new copy-paste duplication** - The implementation correctly follows established patterns
2. **Appropriate code reuse** - `weighted_random_select` in `meeting_assignments.rs` is exported and reused by `mc_assignment.rs` via `pub use`
3. **HealthStatus reuse** - Correctly imported from `meeting_controllers.rs`, not duplicated

### Previous TECH_DEBT Items Remain Appropriate

All 4 TECH_DEBT items from the initial review remain valid and have NOT been upgraded to BLOCKER:

---

## Detailed Findings (Unchanged from Initial)

### TECH_DEBT-1: Weighted Random Selection Function Duplication

**Location**:
- `mh_selection.rs` lines 148-195 (function `weighted_random_select`)
- `meeting_assignments.rs` lines 484-531 (function `weighted_random_select`)

**Description**:
Nearly identical weighted random selection functions exist for MH and MC selection. Both:
- Take a slice of candidates with `load_ratio: f64`
- Use CSPRNG from ring for random selection
- Implement identical weight calculation: `1.0 - load_ratio.min(0.99)`
- Have identical fallback logic for CSPRNG failures

**Why not BLOCKER**:
The functions operate on different types (`MhCandidate` vs `McCandidate`). While the algorithm is identical, creating a generic abstraction would require:
1. A trait for "load-balanced candidates"
2. Type-erasure or generics across the repository layer

This is acceptable duplication per ADR-0019: similar patterns on different domain objects are TECH_DEBT, not BLOCKER.

**Recommended Future Action**:
Consider creating a `LoadBalancedCandidate` trait in the common crate:
```rust
pub trait LoadBalancedCandidate {
    fn load_ratio(&self) -> f64;
}
```
Then extract `weighted_random_select<T: LoadBalancedCandidate>` to common.

---

### TECH_DEBT-2: Health Checker Task Pattern Duplication

**Location**:
- `tasks/mh_health_checker.rs` (95 lines of main task code)
- `tasks/health_checker.rs` (92 lines of main task code)

**Description**:
Both health checker tasks follow an identical pattern:
1. Log startup with staleness/interval config
2. Create tokio interval timer
3. `tokio::select!` loop between interval tick and cancel token
4. On tick: call repository's `mark_stale_*_unhealthy` method
5. Log on shutdown

The difference is only the repository method called and log target names.

**Why not BLOCKER**:
- Different repositories with different table schemas
- Different log targets for operational clarity
- Total duplication is ~90 lines, not 50+ lines of exact copy-paste
- Pattern is simple and unlikely to diverge

**Recommended Future Action**:
Consider a generic health checker builder:
```rust
pub async fn start_generic_health_checker<F, Fut>(
    name: &str,
    staleness_threshold: u64,
    check_fn: F,
    cancel_token: CancellationToken,
) where
    F: Fn(i64) -> Fut,
    Fut: Future<Output = Result<u64, GcError>>,
```

---

### TECH_DEBT-3: Repository SQL Pattern Similarity

**Location**:
- `repositories/media_handlers.rs` (UPSERT, update_load_report, mark_stale_unhealthy, get_handler)
- `repositories/meeting_controllers.rs` (UPSERT, update_heartbeat, mark_stale_unhealthy, get_controller)

**Description**:
Both repositories follow identical patterns for:
- UPSERT registration (INSERT ... ON CONFLICT DO UPDATE)
- Load report/heartbeat updates (UPDATE with timestamp refresh)
- Staleness marking (UPDATE WHERE last_heartbeat < threshold)
- Single-record fetch (SELECT by ID)

SQL queries differ in column names and table names but structure is identical.

**Why not BLOCKER**:
- Different tables with different schemas (MH has bandwidth metrics, MC has meeting counts)
- SQL queries are compile-time checked by sqlx
- Abstracting SQL queries is generally not recommended (loses type safety)
- This is expected "structural" duplication, not "semantic" duplication

**Recommended Future Action**:
Document this as an intentional pattern in ARCHITECTURE.md. Consider sqlx macros or code generation only if a third registry type is added.

---

### TECH_DEBT-4: gRPC Service Validation Pattern Similarity

**Location**:
- `grpc/mh_service.rs` lines 54-124 (validation helper methods)
- `grpc/mc_service.rs` lines 59-163 (validation helper methods)

**Description**:
Both services now have similar validation helper methods. MH service has:
- `validate_handler_id`
- `validate_region`
- `validate_endpoint`

MC service has:
- `validate_controller_id`
- `validate_region`
- `validate_endpoint`
- `validate_capacity`

**Why not BLOCKER**:
- Validation logic differs slightly (MH has different field names, MC has capacity validation)
- Different character validation rules for different contexts
- Inline validation is still readable and maintainable
- Not exact copy-paste (semantically similar but different parameters)

**Recommended Future Action**:
Consider extracting common validation patterns to a shared module:
- `validate_id(id, field_name, max_len)`
- `validate_endpoint(endpoint, field_name)`
- `validate_region(region)`

---

## HealthStatus Reuse Analysis (Positive Finding)

**Good Pattern Observed**: The `HealthStatus` enum is defined once in `meeting_controllers.rs` and reused by `media_handlers.rs` via the mod.rs re-export:
```rust
pub use meeting_controllers::{HealthStatus, MeetingControllersRepository};
```

This is the correct approach and prevents duplication of the health status enum and its conversion methods.

---

## weighted_random_select Reuse (Positive Finding)

**Good Pattern Observed**: The `weighted_random_select` function is exported from `meeting_assignments.rs`:
```rust
pub fn weighted_random_select(candidates: &[McCandidate]) -> Option<&McCandidate>
```

And the `mc_assignment.rs` service correctly imports and uses it:
```rust
use crate::repositories::{weighted_random_select, McAssignment, MeetingAssignmentsRepository};
```

This prevents duplication for MC selection. The MH version in `mh_selection.rs` is a separate implementation for the `MhCandidate` type (hence TECH_DEBT-1).

---

## Verdict

**APPROVED**

No BLOCKER findings. All identified duplication falls into the TECH_DEBT category per ADR-0019 rules:
- Similar patterns across different domain objects (MC vs MH)
- Structural similarity in SQL/repository layer (expected)
- Total duplication per finding is under 50 lines of exact copy-paste

The implementation correctly reuses `HealthStatus` enum and follows established patterns.

---

## Tech Debt Items for Future Tracking

1. **TD-DRY-001**: Extract generic `weighted_random_select` to common crate with `LoadBalancedCandidate` trait
2. **TD-DRY-002**: Create generic health checker task builder
3. **TD-DRY-003**: Document registry SQL patterns in ARCHITECTURE.md
4. **TD-DRY-004**: Consider extracting common gRPC validation helpers

---

*Re-reviewed by DRY Reviewer - 2026-01-24*
