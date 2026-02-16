# Test Specialist Review Checkpoint

**Reviewer**: Test Specialist
**Date**: 2026-01-27
**Task**: Fix code quality issues in meeting-controller (error hiding, instrument skip-all, actor blocking)

## Summary of Changes Reviewed

1. **McError::Internal variant change**: Changed from `Internal` (unit variant) to `Internal(String)` to preserve error context
2. **#[instrument] changes**: Changed all `#[instrument(skip(...))]` to `#[instrument(skip_all, fields(...))]`
3. **remove_meeting() non-blocking cleanup**: Changed to spawn background task instead of blocking on task_handle.await

## Test Coverage Analysis

### 1. McError::Internal(String) Change

**Current Test Coverage**: ADEQUATE

The errors.rs test module already tests the `McError::Internal` variant:

```rust
// Line 169 in errors.rs tests:
assert_eq!(McError::Internal("test".to_string()).error_code(), 6);
```

The `Internal(String)` variant is now tested for:
- Error code mapping (returns 6 for INTERNAL_ERROR)
- Client message hiding (returns "An internal error occurred")
- Display formatting is inherited from thiserror

The actual usage of `McError::Internal` with error context strings is tested implicitly through the actor handle methods when channels fail. These are edge cases that would require actor cancellation to trigger, which are partially covered by existing tests (e.g., `test_controller_cancellation_token`).

**Verdict**: No additional tests needed for the error type itself.

### 2. #[instrument(skip_all, fields(...))] Changes

**Current Test Coverage**: NOT APPLICABLE

These are purely tracing/observability changes that do not affect behavior. The `#[instrument]` attribute controls span creation and field logging. Changing from `skip(self, receiver, ...)` to `skip_all` with explicit `fields(...)` is a code style/observability improvement that:
- Does not change function behavior
- Does not change return values
- Does not affect error handling

**Verdict**: No tests needed for instrumentation changes.

### 3. remove_meeting() Background Spawn Change

**Current Test Coverage**: ADEQUATE

The test `test_controller_handle_remove_meeting` (lines 691-710) verifies:
1. Meeting can be created
2. Meeting can be removed (returns Ok)
3. After removal, meeting is not found

The behavior change (spawning background task vs blocking) does not change the observable API behavior:
- The method still returns `Ok(())` on successful removal initiation
- The method still returns `Err(MeetingNotFound)` if meeting doesn't exist
- The meeting is immediately removed from the `meetings` HashMap (observable via get_meeting)

The background cleanup is an internal implementation detail - the caller doesn't wait for the meeting actor task to complete, they just wait for the removal to be acknowledged. The test correctly verifies this behavior.

**Note**: The background spawn creates a detached task for cleanup timeout handling. This is a fire-and-forget pattern where:
- On success: logs debug message
- On panic: logs warning
- On timeout: logs warning

These logging outcomes are not testable without log capture infrastructure, but the functional behavior (meeting removed from tracking) is tested.

**Verdict**: Existing tests adequately cover the observable behavior.

## Findings

| Severity | ID | Description | Status |
|----------|-----|-------------|--------|
| TECH_DEBT | TD-001 | No explicit test for channel failure paths (McError::Internal with context strings) | Document for future |
| TECH_DEBT | TD-002 | Background spawn cleanup outcomes (panic/timeout) are only observable via logs | Document for future |

### TD-001: Channel Failure Path Tests

The `McError::Internal` error messages like "channel send failed: {e}" are produced when the actor's channel is closed. While the error type is tested, the specific error path requires either:
- Actor cancellation during an in-flight request
- Dropped receiver while sender is active

These are edge cases that could be tested with more sophisticated test harnesses but are not critical for this refactoring task.

### TD-002: Background Cleanup Observability

The spawned cleanup task in `remove_meeting()` has three outcomes (clean exit, panic, timeout) that are only observable via tracing logs. Future work could:
- Add metrics counters for cleanup outcomes
- Create integration tests with log capture

## Conclusion

This is a refactoring task that:
1. Improves error context preservation (no behavior change, type change only)
2. Improves tracing instrumentation (no behavior change)
3. Improves responsiveness by non-blocking cleanup (same API contract, different internal timing)

All observable behaviors are adequately tested by existing test suites. The changes do not introduce new testable behavior that lacks coverage.

## Verdict

**APPROVED**

The existing tests adequately cover the changed behavior. The `#[instrument]` changes have no testable behavior. The `McError::Internal(String)` change is tested. The `remove_meeting()` background spawn maintains the same API contract and is tested for its observable effects.

---

## Iteration 2: SecretBox Migration Review (2026-01-28)

**Review Task**: Test coverage for `master_secret` migration from `Vec<u8>` to `SecretBox<Vec<u8>>`.

**Files Reviewed**:
- `/home/nathan/code/dark_tower/crates/meeting-controller/src/actors/session.rs` (366 lines)
- `/home/nathan/code/dark_tower/crates/meeting-controller/src/actors/meeting.rs` (1763 lines)
- `/home/nathan/code/dark_tower/crates/meeting-controller/src/actors/controller.rs` (790 lines)

### Key Findings

**This is a type-level refactoring that preserves all semantics.** All test helpers and test cases have been correctly updated to use `SecretBox<Vec<u8>>`.

#### Session.rs Tests (10 cases)

Test helper correctly wraps secret:
```rust
fn test_manager() -> SessionBindingManager {
    SessionBindingManager::new(SecretBox::new(Box::new(vec![0u8; 32])))
}
```

Coverage includes:
- ✅ Token generation returns valid hex (32 bytes HMAC-SHA256)
- ✅ Token validation success path
- ✅ Token validation failures (wrong correlation/participant/nonce/meeting_id - all HMAC verified)
- ✅ Invalid hex and wrong length handling
- ✅ Different secrets produce different tokens
- ✅ **Master secret minimum size requirement (32 bytes) - Tests SecretBox type directly**

The `derive_meeting_key()` method correctly calls `self.master_secret.expose_secret()` on line 159 to extract bytes for HKDF. Tests indirectly verify this through token validation.

#### Meeting.rs Tests (11 cases)

Test helper correctly wraps secret:
```rust
fn test_secret() -> SecretBox<Vec<u8>> {
    SecretBox::new(Box::new(vec![0u8; 32]))
}
```

Coverage includes:
- ✅ Actor spawn with SecretBox secret
- ✅ Join generates binding tokens (via SessionBindingManager)
- ✅ Duplicate join rejected
- ✅ Reconnect with valid token succeeds
- ✅ Reconnect with invalid token rejected
- ✅ Token rotation after reconnect (new correlation_id and binding_token generated)
- ✅ Grace period expiration removes participant
- ✅ Reconnect within grace period preserves participant

#### Controller.rs Tests (7 cases)

Test helper correctly wraps secret:
```rust
fn test_secret() -> SecretBox<Vec<u8>> {
    SecretBox::new(Box::new(vec![0u8; 32]))
}
```

**Critical integration pattern verified (line 364)**:
```rust
let meeting_secret = SecretBox::new(Box::new(self.master_secret.expose_secret().clone()));
```

This demonstrates proper SecretBox usage:
1. `expose_secret()` safely exposes bytes (temporary)
2. `.clone()` creates copy for new meeting
3. `SecretBox::new()` wraps copy with zeroization guarantee
4. Original master_secret remains wrapped and secure

Coverage includes:
- ✅ Controller creation with SecretBox secret
- ✅ Meeting creation (implicitly tests secret cloning)
- ✅ Meeting removal
- ✅ Cancellation token propagation

### Analysis Conclusion

**This refactoring is purely type-level security hardening.**

The migration from `Vec<u8>` to `SecretBox<Vec<u8>>`:
- Does NOT change runtime behavior
- Does NOT change algorithm logic
- Does NOT require new test cases (tests already verify behavior)
- Only CHANGES secure memory guarantees at the type system level

**Existing tests adequately cover**:
1. ✅ SecretBox wrapping (directly tested in session.rs)
2. ✅ expose_secret() usage (tested indirectly through token validation)
3. ✅ Secret cloning/duplication pattern (tested through controller -> meeting flow)
4. ✅ Token generation and validation (verified through all 28 test cases)

### Semantic Preservation Verification

| Aspect | Before | After | Test Coverage |
|--------|--------|-------|-----------------|
| Secret storage | `Vec<u8>` | `SecretBox<Vec<u8>>` | ✅ Size tests (32-byte minimum) |
| Token generation | HMAC-SHA256 | HMAC-SHA256 (same) | ✅ 10 session.rs tests |
| Token validation | Constant-time | Constant-time (same) | ✅ 10 session.rs tests |
| HKDF derivation | From Vec<u8> | From SecretBox via expose_secret() | ✅ 11 meeting.rs tests |
| Controller -> Meeting | Direct clone | SecretBox::new clone | ✅ 7 controller.rs tests |

## Verdict

**APPROVED**

Finding count:
- **blocker**: 0
- **critical**: 0
- **major**: 0
- **minor**: 0
- **tech_debt**: 0

Summary: SecretBox migration is a type-level security hardening that preserves all runtime semantics. All test helpers correctly use `SecretBox<Vec<u8>>`. Existing tests cover token generation, validation, grace periods, and reconnection. No additional test cases needed; this is semantic preservation with stronger guarantees.

---

*Generated by Test Specialist during dev-loop-review phase (Iteration 2)*
