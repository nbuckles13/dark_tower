# Test Specialist Review: TokenManager

**Date**: 2026-02-02
**Files Reviewed**: `crates/common/src/token_manager.rs`, `crates/common/Cargo.toml`
**Test Count**: 15 tests

---

## Test Coverage Observations

### What's Covered Well

1. **Configuration API** - Defaults, builder pattern, debug redaction
2. **Happy path token acquisition** - `spawn_token_manager` success flow
3. **Token receiver semantics** - Clone, debug redaction, multiple reads
4. **Retry on server errors** - 500 errors trigger retry
5. **Automatic refresh** - Refresh before expiry works
6. **Error type Display/Clone** - All variants tested for Display
7. **Task lifecycle** - Abort handle stops background task

### Coverage by Code Path

| Path | Coverage | Test(s) |
|------|----------|---------|
| `spawn_token_manager` success | COVERED | `test_spawn_token_manager_success` |
| Token refresh loop | COVERED | `test_token_refresh_after_expiry` |
| Retry on 500 | COVERED | `test_retry_on_500_error` |
| 401/400 rejection | NOT COVERED | - |
| Invalid JSON response | NOT COVERED | - |
| Channel closed error | NOT COVERED | - |
| Backoff timing | NOT COVERED | - |

### Test Quality Assessment

- **Determinism**: PARTIAL - Uses real time in refresh test (flaky risk)
- **Isolation**: GOOD - Each test has own mock server
- **Assertions**: GOOD - Clear messages, specific checks
- **wiremock usage**: EXCELLENT - Proper HTTP mocking

---

## Findings

### MAJOR (3)

**1. Missing test for AuthenticationRejected (401/400)** - `token_manager.rs:442-451`
- **Risk**: Invalid credentials path untested - production could return wrong error type
- **Required**: Mock 401 response, verify `TokenError::AuthenticationRejected`

**2. Missing test for InvalidResponse (malformed JSON)** - `token_manager.rs:425-428`
- **Risk**: JSON parsing error path could break silently
- **Required**: Mock invalid JSON, verify `TokenError::InvalidResponse`

**3. Missing test for missing OAuth fields** - `token_manager.rs:218-228`
- **Risk**: Response with missing `access_token` or `expires_in` untested
- **Required**: Mock response missing required fields, verify error

### MINOR (3)

**4. Missing explicit backoff timing verification** - `token_manager.rs:361-362`
- **Risk**: Backoff logic could regress without test failure (e.g., no delay)
- **Required**: Use `tokio::time::pause()` to verify exponential delays

**5. Missing test for zero expires_in** - `token_manager.rs:431-433`
- **Risk**: `expires_in: 0` could cause tight refresh loop
- **Required**: Test behavior when AC returns immediate expiry

**6. Missing test for ChannelClosed from changed()** - `token_manager.rs:198-203`
- **Risk**: Error path in public API untested
- **Required**: Abort handle, call `changed()`, verify `ChannelClosed` error

### TECH_DEBT (3)

**7. Time-based tests use real time** - `test_token_refresh_after_expiry`
- Uses 3-second real sleep, making test slow and potentially flaky
- Consider refactoring with `#[tokio::test(start_paused = true)]`

**8. No test for HTTP timeout error path**
- `http_timeout` configured but timeout behavior not tested
- Would require wiremock delay or custom mock

**9. No explicit concurrent receiver test**
- Implementation is thread-safe but no stress test validates this
- Consider adding test with multiple tasks calling `token()` during refresh

---

## Verdict

**VERDICT: REQUEST_CHANGES**

**Rationale**: 3 MAJOR and 3 MINOR findings require fixes before approval. The error paths for authentication rejection (401/400), invalid JSON responses, and missing OAuth fields are all explicitly handled in the implementation but have zero test coverage. These are not edge cases - they represent real failure modes that services will encounter in production.

### Required Actions

1. Add test for 401/400 responses returning `AuthenticationRejected`
2. Add test for malformed JSON returning `InvalidResponse`
3. Add test for missing required fields in OAuth response
4. Add test verifying exponential backoff timing (use `time::pause`)
5. Add test for `expires_in: 0` behavior
6. Add test for `changed()` returning `ChannelClosed` after abort

### Finding Summary

| Severity | Count |
|----------|-------|
| BLOCKER | 0 |
| CRITICAL | 0 |
| MAJOR | 3 |
| MINOR | 3 |
| TECH_DEBT | 3 |

---

## Appendix: Missing Test Cases

### Test 1: Authentication Rejected
```rust
#[tokio::test]
async fn test_authentication_rejected_on_401() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/service/token"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&mock_server)
        .await;

    let config = test_config(&mock_server.uri());
    // Need timeout since infinite retry - or test acquire_token directly
    // ...
}
```

### Test 2: Invalid JSON Response
```rust
#[tokio::test]
async fn test_invalid_json_response() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/service/token"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&mock_server)
        .await;
    // ...
}
```

### Test 3: Missing Required Fields
```rust
#[tokio::test]
async fn test_missing_access_token_field() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/service/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token_type": "Bearer",
            "expires_in": 3600
            // missing access_token
        })))
        .mount(&mock_server)
        .await;
    // ...
}
```

---

**Reviewer**: Test Specialist
**Principle References**: `errors.md`, `logging.md`, `crypto.md`
