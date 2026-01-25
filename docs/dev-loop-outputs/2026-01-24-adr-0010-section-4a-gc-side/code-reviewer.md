# Code Quality Review: ADR-0010 Section 4a GC-side

**Date**: 2026-01-24
**Reviewer**: Code Quality Reviewer
**Task**: MH registry, GC->MC AssignMeeting RPC with retry

## Files Reviewed

1. `crates/global-controller/src/grpc/mh_service.rs`
2. `crates/global-controller/src/repositories/media_handlers.rs`
3. `crates/global-controller/src/services/mc_client.rs`
4. `crates/global-controller/src/services/mh_selection.rs`
5. `crates/global-controller/src/services/mc_assignment.rs`
6. `crates/global-controller/src/tasks/mh_health_checker.rs`
7. `proto/internal.proto`
8. `migrations/20260124000001_mh_registry.sql`

## Re-Review (After Fixes)

### Previous Finding: MINOR-001 - Timestamp fallback

**Status**: RESOLVED

**Previous Code**:
```rust
let timestamp = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0);
```

**Fixed Code** (lines 252-253):
```rust
// Use current timestamp from chrono (consistent with rest of codebase)
let timestamp = Utc::now().timestamp() as u64;
```

**Verification**: The fix is correct. Using `chrono::Utc::now().timestamp()` is:
- More idiomatic and consistent with the rest of the codebase
- Does not require a fallback since `chrono` always returns valid timestamps
- Properly imported at the top of the file (`use chrono::Utc;`)

### TECH_DEBT-001: Duplicated `weighted_random_select` implementation

**Status**: Documented (not blocking)

**Location**:
- `crates/global-controller/src/services/mh_selection.rs:148-195`
- `crates/global-controller/src/repositories/meeting_assignments.rs:484-531`

**Issue**: The `weighted_random_select` function is implemented twice with nearly identical logic - once for `MhCandidate` and once for `McCandidate`. This violates DRY principles.

**Recommendation**: Consider creating a generic trait for "weighted selectable" items or a common utility function. To be addressed in future refactoring.

**Severity**: TECH_DEBT

---

### TECH_DEBT-002: Magic numbers for health status proto conversion

**Status**: Documented (not blocking)

**Location**: `crates/global-controller/src/grpc/mh_service.rs:206-213`

**Code**:
```rust
let health_status = match req.health {
    0 => HealthStatus::Pending,
    1 => HealthStatus::Healthy,
    2 => HealthStatus::Degraded,
    3 => HealthStatus::Unhealthy,
    4 => HealthStatus::Draining,
    _ => HealthStatus::Pending,
};
```

**Issue**: The existing `HealthStatus::from_proto(i32)` method in `meeting_controllers.rs` handles this conversion. Reusing that method would be more consistent and maintainable.

**Recommendation**: Replace with `HealthStatus::from_proto(req.health)` and standardize the default value behavior.

**Severity**: TECH_DEBT

---

## Verification Checklist

- [x] No `.unwrap()` or `.expect()` in production code
- [x] Proper error handling with `?` and `Result`
- [x] Safe collection access (`.get()` not `[]`)
- [x] Consistent patterns with existing codebase
- [x] Clear code organization
- [x] Appropriate comments where needed
- [x] Tests use `#[allow(clippy::unwrap_used, clippy::expect_used)]`

## Positive Observations

1. **Excellent error handling**: All fallible operations use `Result` with proper error propagation via `?` operator.

2. **Consistent with existing patterns**: The repository pattern, service layer organization, and gRPC service implementations follow the established codebase conventions.

3. **Good documentation**: Module-level docs, function docs, and inline comments explain the security considerations and design decisions.

4. **Safe collection access**: Uses `.get(i)` in weighted random selection instead of indexing.

5. **Proper CSPRNG usage**: Uses `ring::rand::SystemRandom` for weighted random selection, consistent with security requirements.

6. **Comprehensive test coverage**: Integration tests use `#[sqlx::test]` with proper migration paths.

7. **Graceful shutdown support**: The MH health checker task properly handles cancellation tokens for clean shutdown.

8. **Proto field numbering**: New proto messages follow sequential field numbering without reusing numbers.

9. **SQL injection prevention**: All queries use parameterized statements via sqlx.

10. **Clear separation of concerns**: Repository, service, and gRPC layers have well-defined responsibilities.

## Summary

The implementation is well-structured and follows the project's established patterns. The MINOR finding from the previous review (timestamp fallback with `.unwrap_or(0)`) has been properly fixed using `chrono::Utc::now().timestamp()`, which is consistent with the codebase. Two tech debt items remain documented for future refactoring but do not block the implementation.

## Verdict

**APPROVED**

The MINOR finding has been resolved. The two TECH_DEBT items are documented and do not block the implementation. The code meets quality standards.

---

## Review History

| Date | Action | Findings |
|------|--------|----------|
| 2026-01-24 | Initial Review | 1 MINOR, 2 TECH_DEBT |
| 2026-01-24 | Re-Review | MINOR fixed, 2 TECH_DEBT documented |
