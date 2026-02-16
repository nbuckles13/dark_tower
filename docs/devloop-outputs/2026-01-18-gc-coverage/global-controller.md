# Global Controller Specialist Checkpoint: GC Test Coverage

**Date**: 2026-01-18
**Specialist**: global-controller
**Task**: Improve test coverage for GC service files

---

## Implementation Summary

### Files Modified

1. **`crates/global-controller/src/services/ac_client.rs`**
   - Added 26 new tests covering:
     - `AcClient::new()` success path
     - `request_meeting_token()` - success, network error, 5xx, 403, 400, 401, 418 (unexpected), invalid JSON response
     - `request_guest_token()` - success, network error, 5xx, 403, 400
     - Serialization/deserialization for `ParticipantType`, `MeetingRole`
     - Clone and Debug traits for all request/response types

2. **`crates/global-controller/src/auth/jwks.rs`**
   - Added 18 new tests covering:
     - `get_key()` - cache hit, cache miss, key not found in valid cache, key not found after refresh
     - `refresh_cache()` - network error, non-success status (500, 404), invalid JSON
     - `force_refresh()` success path
     - `clear_cache()` test utility
     - Multiple keys in JWKS
     - Cache expiration triggers refresh
     - Clone and Debug traits

3. **`crates/global-controller/src/auth/jwt.rs`**
   - Added 15 new tests covering:
     - `verify_token()` - rejects non-OKP key type, rejects non-EdDSA algorithm, rejects missing x field, rejects invalid base64 public key
     - `JwtValidator::new()` creation
     - `extract_kid()` edge cases - empty parts, numeric kid, null kid, empty string kid, special characters
     - Token size boundary tests

4. **`crates/gc-test-utils/src/server_harness.rs`**
   - Added 4 new tests covering:
     - `addr()` getter method
     - `config()` getter method
     - Server cleanup on drop
     - Multiple servers with different ports

### Test Summary

| File | Tests Added | Key Coverage Areas |
|------|-------------|-------------------|
| `ac_client.rs` | 26 | HTTP response handling, error mapping, serialization |
| `jwks.rs` | 18 | Cache management, HTTP errors, key lookup |
| `jwt.rs` | 15 | JWK validation, kid extraction edge cases |
| `server_harness.rs` | 4 | Getter methods, Drop implementation |

**Total Tests Added**: 63

---

## Verification Results

All 7 verification layers passed:

| Layer | Command | Status |
|-------|---------|--------|
| 1 | `cargo check --workspace` | PASSED |
| 2 | `cargo fmt --all --check` | PASSED |
| 3 | `./scripts/guards/run-guards.sh` | PASSED (7/7 guards) |
| 4 | `./scripts/test.sh --workspace --lib` | PASSED (136 GC tests) |
| 5 | `./scripts/test.sh --workspace` | PASSED |
| 6 | `cargo clippy --workspace -- -D warnings` | PASSED |
| 7 | Semantic guards (new .rs files) | N/A (no new files) |

---

## Test Patterns Applied

### Using wiremock for HTTP mocking
```rust
#[tokio::test]
async fn test_request_meeting_token_success() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/auth/internal/meeting-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .mount(&mock_server)
        .await;

    let client = AcClient::new(mock_server.uri(), "token".to_string()).unwrap();
    let result = client.request_meeting_token(&request).await;
    assert!(result.is_ok());
}
```

### Testing error paths with unreachable servers
```rust
#[tokio::test]
async fn test_network_error() {
    // Port 1 is privileged and will fail to connect
    let client = AcClient::new("http://127.0.0.1:1".to_string(), "token".to_string()).unwrap();
    let result = client.request_meeting_token(&request).await;
    match result.unwrap_err() {
        GcError::ServiceUnavailable(msg) => assert!(msg.contains("unavailable")),
        e => panic!("Expected ServiceUnavailable, got {:?}", e),
    }
}
```

### Testing JWK validation branches
```rust
#[test]
fn test_verify_token_rejects_non_okp_key_type() {
    let jwk = Jwk {
        kty: "RSA".to_string(), // Wrong key type - should reject
        // ...
    };
    let result = verify_token(&token, &jwk);
    assert!(matches!(result.unwrap_err(), GcError::InvalidToken(_)));
}
```

---

## Notes

- All tests use fixed UUIDs via `Uuid::from_u128(N)` for reproducibility
- Tests follow naming convention `test_<function>_<scenario>_<expected_result>`
- wiremock is used for all HTTP mocking (already a dev dependency)
- Tests include edge cases identified from code review:
  - Empty/malformed JWT headers
  - Non-string kid values in JWT headers
  - Cache expiration behavior
  - Multiple HTTP status codes (200, 400, 401, 403, 404, 418, 500, 502)
