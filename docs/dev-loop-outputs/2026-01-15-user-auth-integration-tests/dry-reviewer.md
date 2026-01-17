# DRY Reviewer Checkpoint

**Date**: 2026-01-15
**Task**: Integration tests for user auth flows (registration, login, org extraction)
**Verdict**: APPROVED_WITH_NOTES

---

## Duplication Analysis

### 1. JWT Token Decoding Pattern

**Finding**: The test file contains **3 instances** of identical JWT decoding logic (lines 100-106, 155-159, 576-580, 902-905):

```rust
// Pattern repeated in multiple tests
let parts: Vec<&str> = token.split('.').collect();
let payload_bytes =
    base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, parts[1])?;
let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)?;
```

**Locations**:
- Line 100-106: `test_register_token_has_user_claims`
- Line 155-159: `test_register_assigns_default_user_role`
- Line 576-580: `test_login_token_has_user_claims`
- Line 902-905: `test_org_extraction_valid_subdomain`

**Current Status**: Duplicated inline JWT decoding logic.

**Available Utility**: The `ac-test-utils` crate already has a `TokenAssertions` trait in `/crates/ac-test-utils/src/assertions.rs` that provides:
- `assert_valid_jwt()` - Validates JWT structure and decodes header
- `assert_has_scope()` - Extracts and validates JWT claims
- `assert_for_subject()` - Validates subject claim

**However**: The `TokenAssertions` trait does **NOT** provide a utility to directly extract the raw payload for custom assertions. The trait only validates properties.

### 2. Test Server Helpers in server_harness.rs

**Finding**: New methods added to `TestAuthServer`:
- `create_test_org()` - Creates organization in DB
- `create_test_user()` - Creates user with default "user" role
- `create_inactive_test_user()` - Creates inactive user
- `host_header()` - Formats Host header with subdomain

**Status**: These are all appropriate additions to the test harness and don't duplicate existing utilities.

### 3. Existing ac-test-utils Utilities (Complete Inventory)

Available in `ac-test-utils` crate:
- `TestAuthServer` - Integration test server harness (line 19-464 in server_harness.rs)
- `TestTokenBuilder` - Builder for creating test JWT claims (token_builders.rs)
- `TokenAssertions` trait - Assertions for token validation (assertions.rs)
- `crypto_fixtures` - Deterministic test crypto (test_master_key, etc.)
- `test_ids` - Fixed UUIDs/constants for reproducible tests
- `rotation_time` - Time utilities for key rotation tests

**None of these provide JWT payload extraction.**

---

## Findings

### 🔴 BLOCKING (code exists in common but not used)

**None**. The JWT decoding pattern is not available in `common` or `ac-test-utils`. No blocked findings.

### 📋 TECH_DEBT (candidate for future extraction)

#### Issue 1: JWT Payload Extraction Utility
**Severity**: TECH_DEBT (not BLOCKING)
**Pattern**: JWT payload decoding is repeated 4 times in user_auth_tests.rs

**Code locations**:
- `crates/ac-service/tests/integration/user_auth_tests.rs` lines 100-106, 155-159, 576-580, 902-905

**Recommendation**: Extract a reusable `decode_jwt_payload()` utility in `ac-test-utils/src/assertions.rs` that returns `serde_json::Value`. This would enable tests to extract and validate specific claims without assertion.

**Future extraction candidate**:
```rust
// In ac-test-utils/src/assertions.rs
pub fn decode_jwt_payload(token: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid JWT format".into());
    }
    let payload_bytes = URL_SAFE_NO_PAD.decode(parts[1])?;
    let payload = serde_json::from_slice(&payload_bytes)?;
    Ok(payload)
}
```

**Impact if extracted**: Would reduce 4 inline implementations to 4 function calls (~16 lines saved).

#### Issue 2: Host Header Construction
**Severity**: TECH_DEBT (not BLOCKING)
**Pattern**: `server.host_header("subdomain")` is well-centralized in `TestAuthServer` and used consistently (lines 39, 85, 140, 182, etc.)

**Status**: ✅ Already following DRY principle. No extraction needed.

#### Issue 3: Test Organization/User Creation
**Severity**: TECH_DEBT (not BLOCKING)
**Pattern**: `create_test_org()` and `create_test_user()` calls are consistent and centralized

**Status**: ✅ Already following DRY principle. Utilities are properly encapsulated in server harness.

---

## Cross-Service Duplication Check

**Question**: Are similar patterns used in other services (GC, MC, MH, ac-service tests)?

**Finding**: Only `ac-service` tests exist currently. No cross-service duplication identified.

**Future consideration**: If GC/MC/MH integration tests are added, they should import `ac-test-utils` for JWT decoding utilities to avoid duplication.

---

## Recommendations

### 1. Add JWT Payload Extraction Helper (Future, Non-Blocking)

**Propose**: Add to `ac-test-utils/src/assertions.rs`:

```rust
/// Decode JWT payload to extract claims programmatically
///
/// Returns the parsed payload for custom assertions.
/// Complement to TokenAssertions trait for cases where you need to
/// extract and inspect specific claims rather than assert properties.
pub fn decode_jwt_payload(token: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid JWT format: expected 3 parts".into());
    }
    let payload_bytes = URL_SAFE_NO_PAD.decode(parts[1])?;
    Ok(serde_json::from_slice(&payload_bytes)?)
}
```

**When to implement**: Phase 5 (GC) when you add GC integration tests and discover similar JWT payload extraction needs.

### 2. Current Code Review Status

**For this PR**: ✅ APPROVED TO MERGE

The repeated JWT decoding is a minor TECH_DEBT item:
- Not a blocking issue (code exists, just not reused)
- Isolated to a single test file
- Documented here for future extraction
- No cross-service impact (only ac-service tests exist)

**Document as**: ADR-0019 Tech Debt item for future resolution

---

## Code Review Notes

### JWT Decoding Usage

The repeated pattern follows this structure consistently:
1. Split token into 3 parts
2. Decode base64 part[1] (payload)
3. Deserialize to JSON value
4. Extract specific claims for assertion

**Quality**: Code is correct and follows the pattern already used in `assertions.rs` (lines 82-88, 102-106, etc.)

**Consistency**: All 4 instances use identical error handling (Result type, unwrap pattern in tests)

### Test Server Harness Additions

✅ **Well-designed additions**:
- `create_test_org()` - Clean subdomain-based org creation
- `create_test_user()` - Automatic default role assignment
- `create_inactive_test_user()` - Proper handling of is_active flag
- `host_header()` - DRY principle for header construction

✅ **Proper encapsulation**: All DB operations use `sqlx::query_as` with parameterized queries (SQL injection safe)

---

## Status

**Review complete. Verdict: APPROVED_WITH_NOTES**

✅ No blocking DRY violations
📋 Minor TECH_DEBT documented: JWT payload extraction utility (non-blocking, future extraction candidate)
✅ Test utilities are well-organized and reusable
✅ Server harness additions are appropriate and don't duplicate existing code

**Recommended action**: Approve merge. Track JWT payload extraction as future tech debt (ADR-0019).
