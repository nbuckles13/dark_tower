# DRY Reviewer Checkpoint: MC-GC Integration Env Tests

**Date**: 2026-01-31
**Task**: Implement env-tests for MC-GC integration (ADR-0010 Phase 4a)
**Files Reviewed**:
- `crates/env-tests/tests/22_mc_gc_integration.rs` (new test file)
- `crates/env-tests/src/fixtures/gc_client.rs` (modified - added McAssignment struct)

**Compared Against**:
- `crates/env-tests/tests/21_cross_service_flows.rs` (existing test file)
- `crates/env-tests/src/fixtures/auth_client.rs` (existing fixture)

---

## Review Summary

The implementation follows established patterns well. The new test file (`22_mc_gc_integration.rs`) shares similar structure with the existing cross-service flows test (`21_cross_service_flows.rs`), which is expected and acceptable for test files.

## Findings

### TECH_DEBT: Similar test setup pattern (Non-blocking)

**Location**: `22_mc_gc_integration.rs:27-32` and `21_cross_service_flows.rs:25-30`

**Pattern**: Both files have an identical `cluster()` helper function:
```rust
async fn cluster() -> ClusterConnection {
    ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running")
}
```

**Why TECH_DEBT, not BLOCKER**:
- This is a 5-line helper function
- Tests benefit from being self-contained
- Extracting to a shared module would add complexity without significant benefit
- The pattern is idiomatic for test setup

**Recommendation**: Could be extracted to `ClusterConnection::test()` or a test prelude in the future, but low priority.

---

### TECH_DEBT: Similar token acquisition pattern (Non-blocking)

**Location**: Multiple tests in both files

**Pattern**: Token request construction is repeated:
```rust
let token_request =
    TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");
```

**Why TECH_DEBT, not BLOCKER**:
- Each test should explicitly show what credentials it uses
- Copy-paste here aids readability and debugging
- Not complex logic, just data construction

**Recommendation**: Could create `TokenRequest::test_default()` in the future.

---

### TECH_DEBT: Response validation patterns (Non-blocking)

**Location**: Multiple tests in `22_mc_gc_integration.rs`

**Pattern**: Response validation assertions (checking token format, endpoint URLs) are repeated across tests.

**Why TECH_DEBT, not BLOCKER**:
- Each test validates slightly different aspects
- Test code benefits from explicit assertions
- Extracting validators would reduce test readability

**Recommendation**: If more tests need similar validation, consider a `JoinMeetingResponse::validate()` helper method on the response struct itself.

---

## Positive Observations

1. **Consistent fixture patterns**: `GcClient` follows the same pattern as `AuthClient` (base_url, http_client, error type)

2. **Good error handling**: `GcClientError` follows the same pattern as `AuthClientError`

3. **Proper Debug implementations**: Both files correctly implement `Debug` with `[REDACTED]` for sensitive fields

4. **Sanitization logic**: The `sanitize_error_body` function in `gc_client.rs` is well-implemented and not duplicated elsewhere

5. **McAssignment struct**: Well-designed, follows existing patterns, properly documents optional fields

---

## Verdict

**APPROVED**

All findings are TECH_DEBT severity, which per ADR-0019 does not block approval. The duplication found is:
- Expected for test files (self-contained tests are a feature)
- Minor (5-10 lines at most)
- Not complex logic requiring extraction

The implementation follows established codebase patterns consistently.

---

## Metrics

| Severity | Count |
|----------|-------|
| BLOCKER  | 0     |
| CRITICAL | 0     |
| MAJOR    | 0     |
| MINOR    | 0     |
| TECH_DEBT| 3     |

