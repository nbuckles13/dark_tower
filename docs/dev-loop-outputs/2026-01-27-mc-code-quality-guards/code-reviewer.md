# Code Reviewer Checkpoint

**Date**: 2026-01-27
**Reviewer**: Code Reviewer Specialist
**Task**: Fix code quality issues in meeting-controller (error hiding, instrument skip-all, actor blocking)

## Files Reviewed

1. `/home/nathan/code/dark_tower/crates/meeting-controller/src/errors.rs`
2. `/home/nathan/code/dark_tower/crates/meeting-controller/src/actors/connection.rs`
3. `/home/nathan/code/dark_tower/crates/meeting-controller/src/actors/controller.rs`
4. `/home/nathan/code/dark_tower/crates/meeting-controller/src/actors/meeting.rs`
5. `/home/nathan/code/dark_tower/crates/meeting-controller/src/grpc/gc_client.rs`
6. `/home/nathan/code/dark_tower/crates/meeting-controller/src/grpc/mc_service.rs`
7. `/home/nathan/code/dark_tower/crates/meeting-controller/src/redis/client.rs`

## Summary of Changes Reviewed

### 1. McError::Internal Changed from Unit to String Variant

**Assessment**: APPROVED

The change from `McError::Internal` (unit variant) to `McError::Internal(String)` correctly preserves error context while maintaining the client_message() abstraction to hide internal details from clients.

**Observed Pattern**:
- All internal errors now include descriptive context: `McError::Internal(format!("channel send failed: {e}"))`
- The `client_message()` method still returns generic "An internal error occurred" for Internal errors
- Error code mapping (6 = INTERNAL_ERROR) is correct
- Tests updated appropriately

### 2. Instrument Attributes Changed to skip_all with fields

**Assessment**: APPROVED

All `#[instrument]` attributes have been updated to use the `skip_all` pattern with explicit fields:

```rust
#[instrument(skip_all, name = "mc.actor.controller", fields(mc_id = %self.mc_id))]
#[instrument(skip_all, name = "mc.actor.meeting", fields(meeting_id = %self.meeting_id))]
#[instrument(skip_all, fields(mc_id = %self.mc_id, region = %self.config.region))]
```

This pattern:
- Prevents accidental logging of sensitive data by skipping all arguments
- Explicitly captures only the fields needed for tracing
- Follows best practices for observability without data leakage

### 3. Background Cleanup Task in remove_meeting()

**Assessment**: APPROVED

The `remove_meeting()` method in `controller.rs` now spawns a background task for cleanup:

```rust
tokio::spawn(async move {
    match tokio::time::timeout(Duration::from_secs(5), managed.task_handle).await {
        Ok(Ok(())) => { /* logged */ }
        Ok(Err(e)) => { /* logged with warning */ }
        Err(_) => { /* timeout warning */ }
    }
});
```

This correctly:
- Avoids blocking the message loop (critical for actor responsiveness)
- Handles all three cases (clean exit, panic, timeout)
- Logs appropriately at each level
- Uses owned values (`meeting_id_owned`, `mc_id`) to avoid lifetime issues

## Findings

### TECH_DEBT-001: Vec<u8> for master_secret

**Severity**: TECH_DEBT
**Location**: `controller.rs:198`, `meeting.rs:351`
**Description**: The `master_secret` is stored as `Vec<u8>` and cloned during meeting actor creation. Consider using `SecretBox<Vec<u8>>` for consistent sensitive data handling as per the project's SecretBox pattern.
**Impact**: Low - the secret is only held in memory and not logged, but doesn't follow the project's established pattern for secrets.
**Resolution**: Document for future Phase 6 security hardening.

### TECH_DEBT-002: Display name generation could be improved

**Severity**: TECH_DEBT
**Location**: `meeting.rs:587`
**Description**: `format!("Participant {}", self.participants.len() + 1)` generates display names but this doesn't account for participants leaving and rejoining, which could lead to confusing display names.
**Impact**: Low - UX concern only, not a functional issue.
**Resolution**: Consider using participant_id suffix or user-provided display name in Phase 6g.

## Compliance Checks

### ADR-0002 (No-Panic Policy)

**Status**: COMPLIANT

- All production code uses proper error handling with `Result<T, E>`
- No `unwrap()`, `expect()`, or `panic!()` in production code paths
- Test modules correctly annotated with `#[allow(clippy::unwrap_used, clippy::expect_used)]`
- Collection access uses safe patterns (`.get()`, `.iter()`, etc.)
- Error types properly defined using `thiserror`

### Error Handling Patterns

**Status**: COMPLIANT

- All channel send operations use `.map_err(|e| McError::Internal(format!(...)))`
- All oneshot receive operations use `.map_err(|e| McError::Internal(format!(...)))`
- Error messages are descriptive and consistent
- Client-facing error messages hide internal details via `client_message()`

### Tracing/Instrumentation

**Status**: COMPLIANT

- All public async methods have `#[instrument]` attributes
- All use `skip_all` to prevent accidental data logging
- Meaningful span names with `name = "mc.actor.xxx"` pattern
- Appropriate fields captured for debugging

## Verdict

**APPROVED**

The implementation correctly addresses all three code quality issues:
1. Error context preservation with `McError::Internal(String)`
2. Safe instrumentation with `skip_all` and explicit fields
3. Non-blocking cleanup with spawned background task

The code is idiomatic Rust, follows ADR-0002, and maintains good code organization. The two TECH_DEBT items are minor and appropriate for future phases.

## Metrics

| Category | Count |
|----------|-------|
| BLOCKER | 0 |
| CRITICAL | 0 |
| MAJOR | 0 |
| MINOR | 0 |
| TECH_DEBT | 2 |

---

## Iteration 2: SecretBox Migration Review

**Date**: 2026-01-28
**Focus**: SecretBox<Vec<u8>> migration of master_secret

### Files Re-examined

1. `crates/meeting-controller/src/actors/session.rs` - Uses master_secret
2. `crates/meeting-controller/src/actors/meeting.rs` - Stores master_secret in struct
3. `crates/meeting-controller/src/actors/controller.rs` - Creates and passes master_secret

### Detailed Analysis

#### 1. session.rs - SessionBindingManager Usage

**Lines 159**: `salt.extract(self.master_secret.expose_secret())`
- **Assessment**: IDIOMATIC ✓
- The `expose_secret()` returns `&Vec<u8>`, which is the reference needed for HKDF
- No unnecessary clone here - HKDF's `extract()` takes a slice reference
- Correct pattern: read-only access via reference, zeroized on drop

**Line 46**: `master_secret.expose_secret().len() >= 32`
- **Assessment**: IDIOMATIC ✓
- Safe validation using reference without copy
- Panic is appropriate per ADR-0002 (unreachable invariant check at constructor)

#### 2. meeting.rs - MeetingActor Storage and Passing

**Line 29**: `master_secret: SecretBox<Vec<u8>>`
- **Assessment**: IDIOMATIC ✓
- Proper field storage type for sensitive cryptographic material
- Will be zeroized automatically on struct drop

**Line 353**: `binding_manager: SessionBindingManager::new(master_secret)`
- **Assessment**: IDIOMATIC ✓
- Passes ownership to SessionBindingManager constructor
- The SessionBindingManager takes ownership and zeroizes on drop
- No unnecessary cloning at this point

#### 3. controller.rs - Key Issue: Create Meeting Pattern

**Line 364**: `SecretBox::new(Box::new(self.master_secret.expose_secret().clone()))`

**Assessment**: REQUEST_CHANGES (MAJOR-004)

**Issues Identified**:

1. **Unnecessary Clone**: This pattern clones the entire `Vec<u8>` for EACH meeting created
   - Causes allocation and copy overhead proportional to secret size (32+ bytes)
   - All meetings share the same master secret - multiple copies is wasteful

2. **Idiomatic Concern**: The pattern `expose_secret().clone()` is not the cleanest approach
   - When you need to pass ownership to a new SecretBox, consider alternatives:
   - Could accept `SecretBox<Arc<Vec<u8>>>` to share via Arc instead of cloning
   - Or keep a reference and derive per-meeting keys (not copy the master secret itself)

3. **Security Consideration**: Each clone increases the number of memory regions containing the secret
   - The master secret should ideally exist in ONE memory location
   - Per-meeting secrets SHOULD be derived via HKDF, not full copies of master

**Current Pattern**:
```rust
let meeting_secret = SecretBox::new(Box::new(self.master_secret.expose_secret().clone()));
```

This creates:
- 1 copy in controller's master_secret
- N copies (one per meeting) in MeetingActor instances
- Total exposure surface = N+1 locations with the secret

**Better Pattern** (for iteration 3 consideration):
- Could pass `&SecretBox<Vec<u8>>` to MeetingActor instead of copying
- MeetingActor would use it via reference for HKDF derivation
- Only meeting-specific keys would be stored (already derived via HKDF)
- This would reduce copies and improve security posture

**Recommendation**: Keep as MAJOR finding for Phase 6d refactor to eliminate redundant cloning by refactoring SessionBindingManager to accept a reference to master_secret instead of ownership.

#### 4. ADR-0002 Compliance Recheck

**Status**: COMPLIANT ✓

All `expect()` and `assert!()` calls are justified:
- `session.rs:46` - Constructor invariant check (unreachable if violated)
- `session.rs:162, 166` - HKDF/fill operations with documented unreachable conditions
- All are in appropriate contexts per ADR-0002 Section 3

#### 5. Idiomatic Rust Patterns

**Overall Assessment**: MOSTLY_IDIOMATIC with one optimization opportunity

**What's Good**:
- Consistent use of `SecretBox<Vec<u8>>` throughout
- Proper ownership transfer semantics
- No unsafe code
- Automatic zeroization on drop
- Test helpers correctly create secrets

**What Could Improve**:
- The `expose_secret().clone()` pattern in controller.rs creates unnecessary copies
- Consider whether SessionBindingManager truly needs OWNERSHIP of master_secret, or just a reference for derivation

### Findings Summary

| ID | Severity | Location | Issue | Recommendation |
|------|----------|----------|-------|-----------------|
| MAJOR-004 | MAJOR | controller.rs:364 | Unnecessary clone of master_secret per meeting | Refactor to reduce copies - either via reference or Arc wrapping |
| IDIOMATIC-001 | MINOR | controller.rs:364 | Pattern `expose_secret().clone()` could be cleaner | Document pattern or consider alternative ownership strategy |

### Compliance Checks

| Aspect | Status | Notes |
|--------|--------|-------|
| SecretBox Usage | APPROVED | Correct type for sensitive material, zeroization working |
| Panic Policy (ADR-0002) | COMPLIANT | All panics are justified invariant checks |
| Reference vs Copy | MOSTLY_COMPLIANT | One unnecessary copy in create_meeting() |
| Test Patterns | APPROVED | Test helpers create secrets correctly |

### Verdict

**REQUEST_CHANGES**

The SecretBox migration is largely well-executed with consistent patterns throughout the three files. However, **MAJOR-004** must be addressed:

The `create_meeting()` method creates unnecessary clones of the master secret for each meeting. This is both a performance concern (allocations) and a security concern (increases copies of sensitive material in memory).

**Required Fix**: Refactor either:
1. To pass `&SecretBox<Vec<u8>>` reference to SessionBindingManager, OR
2. To wrap in Arc to share without copying

**Code Quality**: Apart from MAJOR-004, the code is idiomatic Rust with proper use of SecretBox, no unwraps in production paths, and correct zeroization behavior.

### Metrics

| Category | Count |
|----------|-------|
| BLOCKER | 0 |
| CRITICAL | 0 |
| MAJOR | 1 (MAJOR-004: unnecessary clones) |
| MINOR | 1 (IDIOMATIC-001: pattern clarity) |
| TECH_DEBT | 0 |

---

*Code Reviewer Specialist - Iteration 2*
*2026-01-28*
