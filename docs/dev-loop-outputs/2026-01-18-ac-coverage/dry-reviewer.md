# DRY Reviewer Checkpoint

**Date**: 2026-01-18
**Task**: Review integration tests for cross-service duplication
**Verdict**: APPROVED

---

## Review Summary

The new integration tests (`internal_token_tests.rs`) are service-specific test code within `ac-service`. No cross-service duplication concerns exist - this is test code exercising AC-specific endpoints.

---

## Cross-Service Duplication Analysis

### Files Reviewed

1. `crates/ac-service/tests/integration_tests.rs` - Module declaration only
2. `crates/ac-service/tests/integration/internal_token_tests.rs` - New test file

### Patterns Searched

| Pattern | Result |
|---------|--------|
| `test_uuid()` helper | AC-specific, other services have similar but different helpers |
| JWT payload decoding | Similar pattern in `user_auth_tests.rs` within same service |
| `TestAuthServer` usage | AC-test-utils pattern, service-specific by design |
| Request payload builders | Similar pattern in other test files within AC only |

---

## Findings

### BLOCKING Issues

**None** - No code exists in `common` that was ignored.

### TECH_DEBT Issues

**None** - The patterns found are within-service duplication, not cross-service.

---

## Detailed Analysis

### 1. `test_uuid(n: u128)` Helper (line 20-22)

```rust
fn test_uuid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}
```

**Assessment**: This is a trivial 3-line helper that exists in multiple test files within AC. It does NOT exist in other services (GC, MC, MH) because they have different test patterns. This is NOT cross-service duplication.

**Verdict**: ACCEPTABLE - Trivial helper, extraction cost > benefit

### 2. JWT Payload Decoding (lines 1068-1071, 1144-1147)

```rust
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
let payload_json = URL_SAFE_NO_PAD.decode(payload_b64)?;
let claims: serde_json::Value = serde_json::from_slice(&payload_json)?;
```

**Assessment**: This pattern is used within `internal_token_tests.rs` twice and also appears in `user_auth_tests.rs`. However:
- This is within-service duplication (same crate)
- This is test code, not production code
- AC-test-utils could extract this, but that's Test specialist's call

**Verdict**: ACCEPTABLE - Within-service test code, not cross-service

### 3. Request Payload Builders (lines 25-56)

```rust
fn meeting_token_request(...) -> serde_json::Value { ... }
fn guest_token_request(...) -> serde_json::Value { ... }
```

**Assessment**: These builders are specific to AC's internal token endpoints. GC and other services have different endpoints with different payloads.

**Verdict**: ACCEPTABLE - Service-specific patterns

---

## Check Against Common Crate

Verified that `crates/common/src/` does NOT contain:
- Test utilities (correct - test code stays in test crates)
- JWT decoding helpers (only secret types in `common::secret`)
- UUID generators (correct - `common::types` has domain IDs, not test helpers)

No code in `common` was ignored.

---

## Check Against Tech Debt Registry

Reviewed `docs/specialist-knowledge/dry-reviewer/integration.md`:

| TD-ID | Pattern | Applies? |
|-------|---------|----------|
| TD-1 | JWT Validation Duplication (AC vs GC) | No - this is test code, not production JWT validation |
| TD-2 | EdDSA Key Handling | No - tests use TestAuthServer, not raw key handling |

---

## DRY Review Summary

| Severity | Count | Blocking? |
|----------|-------|-----------|
| BLOCKING | 0 | N/A |
| TECH_DEBT | 0 | N/A |

---

## Recommendation

**APPROVED** - No cross-service duplication detected. The test code is appropriately scoped to the AC service.

---

## Status

Review complete. Verdict: **APPROVED**

---

## Reflection Summary

### What I Learned

This was a straightforward review - the test code is service-specific with no cross-service duplication. The `test_uuid()` helper and JWT decode logic are within-service patterns, not cross-service duplication that would warrant extraction.

### Knowledge Updates Made

**No changes** - The review confirmed that within-service test code duplication (like JWT decoding) is the Test specialist's domain, not DRY reviewer's. Cross-service analysis found nothing to flag.

### Curation Check

Verified tech debt registry (TD-1, TD-2) - both entries remain accurate and relevant. No new cross-service duplication patterns discovered.
