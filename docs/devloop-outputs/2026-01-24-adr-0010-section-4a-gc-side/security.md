# Security Review: ADR-0010 Section 4a GC-side Implementation

**Reviewer**: Security Specialist
**Date**: 2026-01-24
**Task**: MH registry, GC->MC AssignMeeting RPC with retry
**Status**: RE-REVIEW AFTER FIXES

## Files Reviewed

### New Files
1. `crates/global-controller/src/grpc/mh_service.rs` - MH registration gRPC handlers
2. `crates/global-controller/src/repositories/media_handlers.rs` - MH database operations
3. `crates/global-controller/src/services/mc_client.rs` - MC gRPC client
4. `crates/global-controller/src/services/mh_selection.rs` - MH selection logic
5. `crates/global-controller/src/tasks/mh_health_checker.rs` - Background health checker
6. `migrations/20260124000001_mh_registry.sql` - Database migration

### Modified Files
1. `crates/global-controller/src/services/mc_assignment.rs` - Assignment with retry
2. `proto/internal.proto` - Protocol definitions

---

## Previous Findings - Now Resolved

### MINOR-001: Handler ID Validation - FIXED

**File**: `crates/global-controller/src/grpc/mh_service.rs`
**Lines**: 50-75

**Fix Verification**:
- Added `validate_handler_id()` function (lines 50-75)
- Enforces 255 character maximum via `MAX_HANDLER_ID_LENGTH` constant (line 31)
- Validates characters are alphanumeric, hyphens, or underscores (lines 66-73)
- Called in both `register_mh` (line 146) and `send_load_report` (line 203)
- Unit tests added (lines 274-313) covering: valid, empty, too long, boundary case (255 chars), invalid characters

**Assessment**: Properly fixed. Implementation matches existing `mc_service.rs` pattern.

---

### MINOR-002: Endpoint URL Validation - FIXED

**File**: `crates/global-controller/src/grpc/mh_service.rs`
**Lines**: 96-124

**Fix Verification**:
- Added `validate_endpoint()` function (lines 96-124)
- Enforces 255 character maximum via `MAX_ENDPOINT_LENGTH` constant (line 37)
- Validates scheme is `http://`, `https://`, or `grpc://` (lines 114-117)
- Called for both endpoints in `register_mh` (lines 148-149)
- Unit tests added (lines 346-384) covering: valid endpoints (all schemes), empty, invalid scheme, too long, boundary case

**Assessment**: Properly fixed. Added `grpc://` scheme support which is appropriate for gRPC endpoints.

---

### MINOR-003: SecretString for Service Token - FIXED

**File**: `crates/global-controller/src/services/mc_client.rs`
**Lines**: 19, 77-78, 87, 179-181

**Fix Verification**:
- Imports `common::secret::{ExposeSecret, SecretString}` (line 19)
- `service_token` field changed to `SecretString` (line 78)
- Constructor accepts `SecretString` (line 87)
- Uses `expose_secret()` only at point of use when setting auth header (line 181)
- Test updated to use `SecretString::from()` (line 448)

**Assessment**: Properly fixed. Token is now protected by the `secrecy` crate which provides:
- Safe `Debug` implementation (outputs `[REDACTED]`)
- Memory zeroization on drop
- Compile-time enforcement via type system

---

## Re-Review Findings

### No New Issues Found

The fixes were implemented correctly without introducing new security concerns:

1. **Error messages remain generic**: "handler_id is too long", "contains invalid characters" - no internal details leaked
2. **Validation constants are appropriate**: 255 chars aligns with database column sizes and common limits
3. **SecretString scope is minimal**: `expose_secret()` only called at the exact point needed (auth header)
4. **`#[expect(...)]` annotations** on validation functions properly justify the large `Status` return type
5. **Test coverage is comprehensive**: All validation paths have unit tests including boundary cases

---

## Previously Verified (No Changes)

### INFO-001: CSPRNG usage is correct
- `ring::rand::SystemRandom` used for weighted random selection in `mh_selection.rs`

### INFO-002: SQL injection protection confirmed
- All queries use parameterized statements via sqlx

### INFO-003: Error handling is appropriate
- Internal errors logged; clients receive generic messages

### INFO-004: Authentication via gRPC layer
- Module documents JWT auth requirement; integration pending

### INFO-005: Database migration is well-structured
- CHECK constraints, appropriate indexes

---

## Summary

| Severity | Previous Count | Current Count |
|----------|----------------|---------------|
| BLOCKER  | 0              | 0             |
| CRITICAL | 0              | 0             |
| MAJOR    | 0              | 0             |
| MINOR    | 3              | 0             |

**Verdict**: APPROVED

All three previous findings have been properly addressed:
1. Handler ID validation now enforces length limits and character restrictions
2. Endpoint URL validation now enforces scheme and length limits
3. Service token is now wrapped in `SecretString` for leak protection

The implementation is consistent with existing patterns in `mc_service.rs` and follows project security standards.

---

## Checklist

- [x] Input validation reviewed - All inputs properly validated
- [x] SQL injection prevention verified - Parameterized queries confirmed
- [x] Secret handling reviewed - SecretString used for tokens
- [x] Error messages checked for information leakage - Generic messages only
- [x] Logging safety reviewed - skip_all on handlers, no secret exposure
- [x] Rate limiting considerations - Handled by gRPC auth layer
- [x] Authentication requirements documented - Module docs specify JWT requirement
- [x] Previous findings resolved - All 3 MINOR issues fixed
- [x] No regressions introduced - Fixes are clean, no new issues
