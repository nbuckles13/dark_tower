# DRY Reviewer - GC Assignment Cleanup

**Task**: GC Assignment Cleanup - connecting the end_assignment and cleanup_old_assignments functions to handlers and background jobs based on ADR-0010

**Reviewed Files**:
- `crates/global-controller/src/tasks/assignment_cleanup.rs` (NEW)
- `crates/global-controller/src/config.rs` (modified)

---

## Analysis

### 1. Background Task Pattern: `assignment_cleanup.rs` vs `health_checker.rs`

**Comparison**:
| Aspect | health_checker.rs | assignment_cleanup.rs |
|--------|-------------------|----------------------|
| Loop structure | `tokio::select!` with interval + cancel token | Same pattern |
| Config approach | Inline constants, params passed directly | Dedicated config struct with `from_env()` |
| Error handling | Log and continue | Same pattern |
| Logging | Structured logging with targets | Same pattern |
| Tests | Unit + integration tests | Same pattern |

**Verdict**: The patterns are similar but appropriately distinct:
- `health_checker` has a simpler interface (single threshold parameter)
- `assignment_cleanup` has multiple configurable parameters (interval, inactivity hours, retention days), justifying a dedicated config struct
- This is **not duplication** - it's the same architectural pattern applied to different domains

**Status**: ACCEPTABLE - Similar patterns for similar problems, not extractable to common without over-engineering.

### 2. Config Patterns: GC config.rs vs AC config.rs

**Comparison**:
| Aspect | AC Config | GC Config |
|--------|-----------|-----------|
| Structure | `from_env()` / `from_vars()` pattern | Same pattern |
| Debug impl | Custom Debug with `[REDACTED]` | Same pattern |
| Error type | `ConfigError` enum with `thiserror` | Same pattern |
| JWT clock skew constants | `DEFAULT_JWT_CLOCK_SKEW_SECONDS`, `MAX_JWT_CLOCK_SKEW_SECONDS` | Same constants |
| Validation | Detailed validation with error messages | Same approach |

**Duplicated Constants**:
```rust
// In crates/ac-service/src/config.rs:
pub const DEFAULT_JWT_CLOCK_SKEW_SECONDS: i64 = 300;
pub const MAX_JWT_CLOCK_SKEW_SECONDS: i64 = 600;

// In crates/global-controller/src/config.rs:
pub const DEFAULT_JWT_CLOCK_SKEW_SECONDS: i64 = 300;
pub const MAX_JWT_CLOCK_SKEW_SECONDS: i64 = 600;
```

**Assessment**: JWT clock skew constants are duplicated. However:
- These are security-related constants that could diverge if services have different requirements
- Consolidation could be done in `crates/common/src/config.rs` but is not mandatory
- This is a **TECH_DEBT** item, not a BLOCKER

### 3. Common Crate Usage

**Checked**: `crates/common/` contents:
- `config.rs` - Has `DatabaseConfig`, `RedisConfig`, `ObservabilityConfig` (not used by either service currently)
- `secret.rs` - Has `SecretBox` (used by AC, not needed by GC assignment cleanup)
- `error.rs` - Common error types
- `types.rs` - Common types

**Findings**:
- No background task utilities exist in `common/` that should have been used
- The `CancellationToken` pattern is standard Tokio, not something we should wrap
- No existing `from_env()` helper that was bypassed

**Status**: NO BLOCKER - No code exists in `common/` that was ignored

### 4. AssignmentCleanupConfig `from_env()` Pattern

The new `AssignmentCleanupConfig::from_env()` pattern:
```rust
pub fn from_env() -> Self {
    let check_interval_seconds = std::env::var("GC_CLEANUP_INTERVAL_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CHECK_INTERVAL_SECONDS);
    // ...
}
```

This pattern is slightly different from the main Config pattern:
- Returns `Self` not `Result<Self, ConfigError>` (silently falls back to defaults)
- Uses `unwrap_or` instead of explicit validation

**Assessment**: This is intentional - cleanup config has reasonable defaults and should not fail startup. This is consistent with operational patterns where cleanup tasks degrade gracefully.

---

## Findings Summary

### BLOCKER: 0
No code exists in `crates/common/` that should have been used but wasn't.

### TECH_DEBT: 1

**TD-001: Duplicated JWT Clock Skew Constants**
- **Location**: `crates/ac-service/src/config.rs` and `crates/global-controller/src/config.rs`
- **Description**: `DEFAULT_JWT_CLOCK_SKEW_SECONDS` and `MAX_JWT_CLOCK_SKEW_SECONDS` are defined in both files with identical values
- **Recommendation**: Consider moving to `crates/common/src/config.rs` when either service next touches JWT config
- **Priority**: Low (constants are unlikely to drift, both derive from NIST SP 800-63B)

---

## Verdict (Iteration 1)

```
verdict: APPROVED
finding_count:
  blocker: 0
  tech_debt: 1
summary: No blocking duplication issues. The background task pattern in assignment_cleanup.rs appropriately mirrors health_checker.rs for a similar domain problem. One tech debt item identified: JWT clock skew constants are duplicated between AC and GC configs, recommend consolidation in common when convenient.
```

---

# Iteration 2 Review

**Date**: 2026-01-23
**Fixes Applied**:
1. Added batch_size parameter to cleanup queries
2. Added validate_meeting_id() function
3. Added from_env() tests
4. Added meeting_id validation tests

## New Analysis

### 1. validate_meeting_id() vs validate_controller_id() Duplication

**Location**: `crates/global-controller/src/grpc/mc_service.rs`

`validate_meeting_id()` (lines 78-100) and `validate_controller_id()` (lines 54-76) are **nearly identical**:

- Both check for empty string
- Both check max length (255 chars)
- Both validate characters: alphanumeric, hyphen, underscore
- Only difference: error message field name ("meeting_id" vs "controller_id")

**Severity**: **TECH_DEBT** (not BLOCKER)

**Rationale**: While this is clear duplication, the functions:
- Are tightly coupled to gRPC `Status` error type
- Provide context-specific error messages
- Are co-located in the same file
- May diverge in the future (meeting IDs might have different rules)

**Recommendation**: Consider refactoring to a generic `validate_identifier()` function:
```rust
fn validate_identifier(id: &str, field_name: &str, max_len: usize) -> Result<(), Status>
```

This should be addressed in a future cleanup pass, not blocking this merge.

### 2. batch_size Pattern

**Location**: `crates/global-controller/src/repositories/meeting_assignments.rs`

The `batch_size` parameter was added to `end_stale_assignments()` and `cleanup_old_assignments()`.

**Analysis**:
- This is the ONLY place in the codebase using this pattern
- No existing batch_size utility in `crates/common/`
- The implementation is straightforward (`Option<i64>` with default)
- Pattern is localized and doesn't warrant a shared abstraction yet

**Severity**: N/A (no duplication found)

### 3. from_env() Tests

**Location**: `crates/global-controller/src/tasks/assignment_cleanup.rs`

The new `from_env()` tests follow similar patterns to AC config tests.

**Analysis**:
- Test patterns are similar but appropriately so
- ENV_MUTEX pattern for test isolation is good
- No shared test utilities would be appropriate here

**Severity**: N/A (patterns are similar but not identical, different complexity levels)

### 4. Check Against crates/common/

**Checked**:
- `crates/common/src/types.rs` - Contains ID types (MeetingId, ParticipantId, etc.) but uses UUID-based types, not string validation
- No shared validation utilities exist in common

**Finding**: No duplication with `crates/common/`

## Iteration 2 Findings Summary

| Finding | File | Severity | Description |
|---------|------|----------|-------------|
| TD-001 (from iter 1) | config.rs (AC/GC) | TECH_DEBT | JWT clock skew constants duplicated |
| TD-002 (new) | mc_service.rs | TECH_DEBT | validate_meeting_id() duplicates validate_controller_id() logic |

## Iteration 2 Verdict

```
verdict: APPROVED
finding_count:
  blocker: 0
  tech_debt: 2
summary: Two TECH_DEBT findings, no BLOCKERs. New finding: validate_meeting_id() duplicates validate_controller_id() logic in mc_service.rs - both functions have identical validation logic (empty check, length check, character validation) with only error message differences. This is acceptable duplication for now as functions may diverge and are co-located. Previous tech debt (JWT constants) remains. Merge is not blocked.
```
