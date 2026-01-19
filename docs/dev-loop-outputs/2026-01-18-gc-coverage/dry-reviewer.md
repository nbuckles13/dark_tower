# DRY Reviewer Review

**Date**: 2026-01-18
**Reviewer**: DRY Reviewer Specialist
**Files Reviewed**: ac_client.rs, jwks.rs, jwt.rs, server_harness.rs (test modules)

---

## Review Summary

| Aspect | Assessment |
|--------|------------|
| **Verdict** | APPROVED |
| **Cross-Service Duplication** | None identified |
| **Blockers** | 0 |
| **Tech Debt Items** | 0 (new) |

---

## Duplication Analysis

### 1. HTTP Client Test Patterns

**Location**: `ac_client.rs`, `jwks.rs`
**Pattern**: wiremock-based HTTP testing

Both files use similar wiremock patterns for HTTP mocking. This is NOT duplication because:
- Each file tests different endpoints with different response structures
- The pattern is a test utility pattern, not business logic
- Both follow the same wiremock idioms (which is good consistency)

**Classification**: ACCEPTABLE - Standard test pattern usage

### 2. JWT/JWKS Test Utilities

**Location**: `jwt.rs`, `jwks.rs`
**Pattern**: Base64 encoding, JSON header construction

Both files construct test JWTs/JWKs using similar base64 encoding:
```rust
let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key"}"#;
let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
```

**Assessment**: This is test setup code, not extractable business logic. Each test needs specific header/payload combinations.

**Classification**: ACCEPTABLE - Test-specific data construction

### 3. Cross-Service Comparison: AC vs GC Test Patterns

**Comparison**: ac-service tests vs global-controller tests

Checked for patterns that might duplicate AC test utilities:
- `ac-test-utils` contains `TestAcServer` harness
- `gc-test-utils` contains `TestGcServer` harness

These are structurally similar but:
- Each spawns a different service (AC vs GC)
- Each has service-specific configuration
- Extracting to common would require complex generics with marginal benefit

**Classification**: ACCEPTABLE - Parallel evolution (TD-2 pattern from registry)

### 4. Error Handling Test Patterns

**Location**: All test files
**Pattern**: `assert!(result.is_err())` + `match result.unwrap_err()`

This pattern repeats across test files:
```rust
let result = client.method().await;
assert!(result.is_err());
match result.unwrap_err() {
    Error::Variant(msg) => { ... }
    e => panic!("Expected ..., got {:?}", e),
}
```

**Assessment**: This is idiomatic Rust error testing. Extracting to a helper would reduce clarity without significant benefit.

**Classification**: ACCEPTABLE - Idiomatic pattern

---

## Registry Check

Checked existing tech debt registry (`docs/specialist-knowledge/dry-reviewer/integration.md`):

| TD-ID | Pattern | Status | Relevance |
|-------|---------|--------|-----------|
| TD-2 | TestServer harness similarity | Documented | GC tests follow same pattern (acceptable) |

No new tech debt items identified.

---

## Cross-Service Code Check

Searched for code that exists in `common` crate but wasn't used:
- No common test utilities that apply to these tests
- No shared HTTP client patterns in common (each service has specific needs)

**Result**: No BLOCKER duplication found

---

## Summary

| Duplication Type | Count | Classification |
|------------------|-------|----------------|
| BLOCKER | 0 | - |
| TECH_DEBT (new) | 0 | - |
| TECH_DEBT (existing) | 1 | TD-2: TestServer parallel evolution |
| ACCEPTABLE | 4 | Standard test patterns |

---

## Conclusion

The test code does not introduce new cross-service duplication:
- HTTP mocking patterns are standard test idioms
- Test harnesses follow documented parallel evolution pattern
- No business logic duplication
- No common utilities bypassed

**APPROVED** - No blocking duplication found.
