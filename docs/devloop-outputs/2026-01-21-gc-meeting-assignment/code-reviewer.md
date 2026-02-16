# Code Reviewer Checkpoint: GC Meeting Assignment

**Date**: 2026-01-21
**Reviewer**: Code Reviewer Specialist
**Task**: GC should assign users to MCs via load balancing per design in ADR-0010

---

## Review Summary

**Verdict**: APPROVED

The implementation follows proper Rust idioms, maintains correct layering (handler -> service -> repository), and adheres to ADR-0002 (no-panic policy). Code quality is high with good documentation and error handling throughout.

---

## Files Reviewed

### New Files

| File | LOC | Rating |
|------|-----|--------|
| `migrations/20260121000001_meeting_assignments.sql` | 38 | Excellent |
| `crates/global-controller/src/repositories/meeting_assignments.rs` | 629 | Good |
| `crates/global-controller/src/services/mc_assignment.rs` | 192 | Excellent |
| `crates/global-controller/tests/meeting_assignment_tests.rs` | 470 | Good |

### Modified Files

| File | Changes | Rating |
|------|---------|--------|
| `crates/global-controller/src/config.rs` | gc_id added | Good |
| `crates/global-controller/src/models/mod.rs` | McAssignmentInfo added | Excellent |
| `crates/global-controller/src/handlers/meetings.rs` | MC assignment integration | Good |
| `crates/global-controller/src/repositories/mod.rs` | Module exports | Good |
| `crates/global-controller/src/services/mod.rs` | Module exports | Good |
| `crates/global-controller/tests/meeting_tests.rs` | MC registration helper | Good |

---

## Findings

### BLOCKER: 0
No blocking issues found.

### CRITICAL: 0
No critical issues found.

### MAJOR: 0
No major issues found.

### MINOR: 2

#### MINOR-1: Use of `#[allow(dead_code)]` instead of `#[expect(dead_code)]`

**Location**: `crates/global-controller/src/repositories/meeting_assignments.rs:26-27`, `316-317`, `373-374`

**Issue**: ADR-0002 recommends using `#[expect(..., reason = "...")]` instead of `#[allow(...)]` for lint suppression. This enables compiler warnings when the lint no longer applies.

**Current**:
```rust
#[allow(dead_code)] // Used in tests
#[derive(Debug, Clone)]
pub struct MeetingAssignment {
```

**Recommendation**:
```rust
#[expect(dead_code, reason = "Used in tests and future assignment queries")]
#[derive(Debug, Clone)]
pub struct MeetingAssignment {
```

**Impact**: Minor - affects future maintainability when dead code becomes live

---

#### MINOR-2: Duplicate logging in repository and service for assignment

**Location**:
- `crates/global-controller/src/repositories/meeting_assignments.rs:233-239`
- `crates/global-controller/src/services/mc_assignment.rs:120-126`

**Issue**: Both repository and service log "Meeting assigned to MC" with similar information, creating redundant log entries.

**Recommendation**: Keep logging only at the service layer (business logic layer) to avoid duplicate log entries. Repository should log only database-specific operations.

**Impact**: Minor - log noise, no functional impact

---

### TECH_DEBT: 3

#### TD-1: `unwrap_or` in config.rs for gc_id generation

**Location**: `crates/global-controller/src/config.rs:204`

**Issue**: Uses `unwrap_or` with a magic string fallback:
```rust
let short_suffix = uuid_suffix.get(..8).unwrap_or("00000000");
```

**Rationale**: This is acceptable because `Uuid::new_v4().to_string()` always produces a string >= 8 chars. The fallback is purely defensive. However, this pattern could be cleaner.

**Recommendation**: Document why this fallback exists or refactor to use safer indexing.

---

#### TD-2: Service token handling in create_ac_client

**Location**: `crates/global-controller/src/handlers/meetings.rs:532`

**Issue**:
```rust
let service_token = std::env::var("GC_SERVICE_TOKEN").unwrap_or_default();
```

This reads from env var on every request and falls back to empty string if not set.

**Rationale**: Already noted in main.md as tech debt. Should use a service token refresh mechanism.

**Recommendation**: Move to config or token refresh service in future phase.

---

#### TD-3: MeetingRow.join_token_secret should use SecretString

**Location**: `crates/global-controller/src/models/mod.rs:91`

**Issue**: `join_token_secret: String` stores a secret in a plain String that may appear in debug output or logs.

**Rationale**: Already noted in main.md. Should be converted to SecretString pattern per SecretBox refactor.

---

---

## ADR Compliance

### ADR-0002 (No-Panic Policy)

| Check | Status | Notes |
|-------|--------|-------|
| No `unwrap()` in production code | PASS | Only in `#[cfg(test)]` blocks |
| No `expect()` in production code | PASS | Only in `#[cfg(test)]` blocks |
| No `panic!()` | PASS | None found |
| No index operations `[idx]` | PASS | Uses `.get()` with `.ok_or()` |
| Uses `?` operator properly | PASS | Throughout repository and service |
| Error types use `thiserror` | PASS | `GcError` defined with thiserror |
| Lint suppressions use `#[expect]` | MINOR | Uses `#[allow]` in a few places |

**Verdict**: Compliant with ADR-0002 (minor improvement opportunity with `#[expect]`)

---

## Code Organization

### Layering (Handler -> Service -> Repository)

| Layer | Responsibility | Implementation |
|-------|----------------|----------------|
| Handler (`meetings.rs`) | HTTP concerns, request validation | Correctly delegates to service |
| Service (`mc_assignment.rs`) | Business logic orchestration | Correctly calls repository |
| Repository (`meeting_assignments.rs`) | Database operations | Proper sqlx queries |

**Verdict**: Proper separation of concerns maintained.

---

## Rust Idioms

### Error Handling

- Uses `Result<T, GcError>` throughout
- Maps errors at appropriate boundaries
- Uses `.ok_or()` / `.ok_or_else()` for Option to Result conversion
- Proper `?` operator usage for error propagation

### Iterators and Collections

- Uses `.iter()` and `.map()` appropriately
- Avoids manual indexing
- Uses `into_iter()` when consuming ownership

### Ownership

- Borrows parameters where appropriate (`&str`, `&PgPool`)
- Clones only when necessary (returning owned data)
- Uses references in loop iterations

---

## Documentation Quality

### Public API Documentation

| Item | Documented | Quality |
|------|------------|---------|
| `MeetingAssignmentsRepository` | Yes | Good |
| `McAssignmentService` | Yes | Excellent |
| `weighted_random_select` | Yes | Good |
| Public structs | Yes | Good |

### Module-Level Documentation

- Repository module has security notes
- Service module has architecture notes
- Both explain purpose and usage

---

## Performance Considerations

### SQL Query Efficiency

| Query | Index Usage | Notes |
|-------|-------------|-------|
| `get_healthy_assignment` | Uses `idx_assignments_by_region` | Proper |
| `get_candidate_mcs` | Uses existing MC indexes | Proper |
| `atomic_assign` | Uses PK and partial indexes | Optimized CTE |
| `end_assignment` | Uses `idx_assignments_by_region` | Proper |
| `cleanup_old_assignments` | Uses `idx_assignments_ended_at` | Proper |

### Algorithm Efficiency

- `weighted_random_select`: O(n) for n candidates - acceptable given CANDIDATE_COUNT=5
- CSPRNG usage is appropriate for security-sensitive randomness

---

## Test Quality

### Coverage

- Unit tests for `weighted_random_select` covering edge cases
- Integration tests for all repository functions
- Integration tests for service functions
- Tests for race condition handling

### Test Organization

- Tests are in appropriate locations (unit in module, integration in tests/)
- Uses `#[sqlx::test]` for database tests
- Helper functions extract common setup

---

## Final Summary

### Strengths

1. Clean separation of concerns across layers
2. Proper error handling with no panics in production code
3. Atomic database operations with race condition handling
4. Good documentation throughout
5. Comprehensive test coverage
6. Proper use of CSPRNG for security-sensitive randomness

### Areas for Improvement (Non-Blocking)

1. Replace `#[allow(dead_code)]` with `#[expect(dead_code, reason = "...")]`
2. Reduce duplicate logging between repository and service
3. Address tech debt items in future iterations

---

## Verdict

**APPROVED**

The implementation is well-structured, follows ADR-0002, and maintains high code quality. The minor issues identified are non-blocking and can be addressed in future iterations.

---

| Category | Count |
|----------|-------|
| BLOCKER | 0 |
| CRITICAL | 0 |
| MAJOR | 0 |
| MINOR | 2 |
| TECH_DEBT | 3 |
