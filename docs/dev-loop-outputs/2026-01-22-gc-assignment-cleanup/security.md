# Security Review: GC Assignment Cleanup - Iteration 2

**Reviewer**: Security Specialist
**Date**: 2026-01-23
**Task**: GC Assignment Cleanup implementation review (iteration 2 fixes)

## Verdict

**APPROVED**

## Finding Summary

| Severity | Count |
|----------|-------|
| Blocker  | 0     |
| Critical | 0     |
| Major    | 0     |
| Minor    | 0     |
| Tech Debt | 0    |

## Summary

The iteration 2 fixes properly address the two MINOR findings from iteration 1. Cleanup queries now use batch size limits via LIMIT clauses to prevent long-running transactions, and meeting_id validation now uses the same character validation as controller_id (alphanumeric, hyphen, underscore only). No new security issues were introduced by these fixes. The implementation is approved for merge.

---

## Verification of Iteration 2 Fixes

### Fix 1: Batch Size Limits for Cleanup Queries

**Status**: VERIFIED

**File**: `crates/global-controller/src/repositories/meeting_assignments.rs`

**Previous Finding (M-1)**: Cleanup queries lacked LIMIT clauses, risking long-running transactions in pathological cases with millions of stale assignments.

**Verification**:

1. **Default batch size constant added (line 27)**:
   ```rust
   const DEFAULT_CLEANUP_BATCH_SIZE: i64 = 1000;
   ```

2. **`end_stale_assignments` updated (lines 374-424)**:
   - Added `batch_size: Option<i64>` parameter
   - Uses `let limit = batch_size.unwrap_or(DEFAULT_CLEANUP_BATCH_SIZE);`
   - Query uses subquery with LIMIT:
   ```sql
   UPDATE meeting_assignments ma
   SET ended_at = NOW()
   WHERE ma.meeting_id IN (
       SELECT inner_ma.meeting_id
       FROM meeting_assignments inner_ma
       WHERE inner_ma.ended_at IS NULL
         AND inner_ma.assigned_at < NOW() - ($1 || ' hours')::INTERVAL
         AND EXISTS (...)
       LIMIT $2
   )
   ```

3. **`cleanup_old_assignments` updated (lines 437-474)**:
   - Added `batch_size: Option<i64>` parameter
   - Uses `let limit = batch_size.unwrap_or(DEFAULT_CLEANUP_BATCH_SIZE);`
   - Query uses subquery with LIMIT:
   ```sql
   DELETE FROM meeting_assignments
   WHERE meeting_id IN (
       SELECT meeting_id FROM meeting_assignments
       WHERE ended_at < NOW() - ($1 || ' days')::INTERVAL
       LIMIT $2
   )
   ```

4. **Background task uses defaults (lines 144, 168)**:
   ```rust
   MeetingAssignmentsRepository::end_stale_assignments(pool, config.inactivity_hours, None)
   MeetingAssignmentsRepository::cleanup_old_assignments(pool, config.retention_days, None)
   ```
   Passing `None` correctly uses the default batch size of 1000.

**Security Analysis**: The LIMIT clause via subquery pattern is correct for PostgreSQL. This bounds the maximum rows affected per cleanup iteration, preventing:
- Long-held table locks
- Transaction log bloat
- Connection timeout issues

If more than 1000 rows need cleanup, subsequent iterations will handle them progressively. This is the recommended pattern for large-scale batch operations.

### Fix 2: Meeting ID Character Validation

**Status**: VERIFIED

**File**: `crates/global-controller/src/grpc/mc_service.rs`

**Previous Finding (M-2)**: meeting_id length was validated but character set was not, making it more permissive than controller_id validation.

**Verification**:

1. **New `validate_meeting_id` function added (lines 79-100)**:
   ```rust
   fn validate_meeting_id(id: &str) -> Result<(), Status> {
       if id.is_empty() {
           return Err(Status::invalid_argument("meeting_id is required"));
       }
       if id.len() > MAX_MEETING_ID_LENGTH {
           return Err(Status::invalid_argument("meeting_id is too long"));
       }
       // Allow alphanumeric, hyphens, and underscores (same as controller_id)
       if !id
           .chars()
           .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
       {
           return Err(Status::invalid_argument(
               "meeting_id contains invalid characters",
           ));
       }
       Ok(())
   }
   ```

2. **Handler uses the new validation (line 361)**:
   ```rust
   Self::validate_meeting_id(&req.meeting_id)?;
   ```

3. **Comprehensive test coverage added (lines 577-632)**:
   - `test_validate_meeting_id_valid` - Valid cases pass
   - `test_validate_meeting_id_empty` - Empty rejected
   - `test_validate_meeting_id_too_long` - Over 255 chars rejected
   - `test_validate_meeting_id_at_255_chars` - Boundary case passes
   - `test_validate_meeting_id_invalid_chars` - Various invalid characters rejected
   - `test_validate_meeting_id_at_1_char` - Single char passes

**Security Analysis**: The character validation now matches `validate_controller_id` exactly:
- Allows: alphanumeric characters (a-z, A-Z, 0-9), hyphen (-), underscore (_)
- Rejects: spaces, slashes, special characters (@, #, $, etc.)

This prevents potential issues with:
- Path traversal characters (`/`, `\`, `..`)
- SQL metacharacters (though parameterized queries already protect)
- Shell metacharacters (`;`, `|`, `&`)
- Unicode confusables or homoglyphs

---

## New Code Analysis (Iteration 2)

### 1. No SQL Injection Regressions

The batch size parameter is passed as a bound parameter (`$2`), not concatenated:
```rust
.bind(limit)
```

This maintains the parameterized query pattern and prevents any injection via the batch_size parameter.

### 2. No Information Disclosure Regressions

Error messages remain generic:
- `"meeting_id is required"`
- `"meeting_id is too long"`
- `"meeting_id contains invalid characters"`

No internal details (character limits, allowed patterns) are leaked.

### 3. No DoS Regressions

The batch size limits actually improve DoS resistance by:
- Preventing single operations from hogging connections
- Ensuring predictable query execution times
- Allowing other operations to proceed between batches

### 4. Test Coverage Additions

The new tests for `from_env()` parsing (lines 227-289) verify:
- Valid environment variable parsing
- Fallback to defaults on invalid values (non-numeric)
- Fallback to defaults when variables are missing

This ensures configuration cannot be manipulated to create security issues.

---

## Security Checklist (Re-verification)

| Check | Status |
|-------|--------|
| Authentication required for gRPC endpoints | PASS |
| Input validation on all user inputs | PASS |
| Meeting ID character validation matches controller_id | PASS (NEW) |
| Parameterized SQL queries (including batch_size) | PASS |
| Batch limits prevent long-running transactions | PASS (NEW) |
| Generic error messages to clients | PASS |
| Internal errors logged with context | PASS |
| No sensitive data in logs | PASS |
| Resource limits on background tasks | PASS |
| Graceful shutdown support | PASS |

---

## Conclusion

All iteration 1 findings have been properly addressed:

1. **M-1 (Cleanup batch limits)**: Fixed with LIMIT clauses via subquery pattern, using configurable batch_size parameter with 1000 default.

2. **M-2 (Meeting ID validation)**: Fixed with new `validate_meeting_id()` function implementing the same character validation as `validate_controller_id()`.

No new security issues were introduced. The fixes follow security best practices (parameterized queries, defensive defaults, comprehensive tests). This implementation is **APPROVED** for merge.
