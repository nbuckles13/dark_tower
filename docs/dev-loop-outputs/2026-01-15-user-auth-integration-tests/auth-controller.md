# Checkpoint: auth-controller Specialist

**Task**: Implement integration tests for user authentication flows
**Specialist**: auth-controller
**Status**: COMPLETED
**Date**: 2026-01-16

---

## Patterns Discovered

### 1. Host Header Testing Pattern

For subdomain-based multi-tenant testing, the `host_header` method pattern works well:

```rust
pub fn host_header(&self, subdomain: &str) -> String {
    format!("{}.localhost:{}", subdomain, self.addr.port())
}
```

This allows tests to set the Host header correctly for org extraction middleware.

### 2. User Creation with Role Assignment

Creating test users should include default role assignment in the same function:

```rust
pub async fn create_test_user(...) -> Result<Uuid, anyhow::Error> {
    // Create user
    let user = sqlx::query_as("INSERT INTO users ...").fetch_one(pool).await?;

    // Add default role
    sqlx::query("INSERT INTO user_roles (user_id, role) VALUES ($1, 'user')")
        .bind(user.0)
        .execute(pool)
        .await?;

    Ok(user.0)
}
```

### 3. JWT Claim Verification in Tests

Decode JWT payload directly for claim verification:

```rust
let parts: Vec<&str> = token.split('.').collect();
let payload_bytes = base64::Engine::decode(
    &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    parts[1],
)?;
let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)?;
assert!(payload.get("org_id").is_some());
```

---

## Gotchas Encountered

### 1. Error Code Mapping

The user auth endpoints use `AcError::InvalidToken` for validation errors (email format, password length, display name), which returns HTTP 401 rather than 400. This is the existing pattern - don't fight it.

### 2. Response Body Consumption

When calling `.status()` after `.text().await`, the response body is already consumed. Don't try to parse it again:

```rust
// WRONG - body already consumed
let status = response.status();
let body: Value = response.json().await?;  // Error!

// RIGHT - use response once
let response = client.post(...).send().await?;
assert_eq!(response.status(), StatusCode::OK);
// OR
let body: Value = response.json().await?;  // Choose one
```

### 3. Rate Limiting Depends on Auth Events

Registration rate limiting counts `user_login` events (which are created on auto-login after registration). This means the rate limit behavior is tied to the auth_events table, not a separate counter.

### 4. sqlx::test Requires DATABASE_URL

Tests using `#[sqlx::test]` need the DATABASE_URL environment variable. Use `./scripts/test.sh` which sets this up automatically.

---

## Key Decisions

1. **Separate inactive user helper**: Created `create_inactive_test_user` rather than having a parameter on `create_test_user` to keep the common case simple.

2. **Test assertions use 401**: Validation errors return 401 (InvalidToken) not 400, matching the existing error handling pattern in the auth handler.

3. **client() method**: Added a simple `client()` method returning `reqwest::Client::new()` for convenience.

---

## Integration Points

### With Org Extraction Middleware

Tests interact with the org extraction middleware (`require_org_context`) via the Host header. The middleware extracts the subdomain and looks up the org_id.

### With Token Service

Tests verify token claims by decoding the JWT payload. The `issue_user_token` function in token_service creates the UserClaims structure.

### With User Repository

Tests use the users table directly via `create_test_user`. The password is hashed with bcrypt using `crypto::hash_client_secret`.

---

## Files Changed

- `crates/ac-test-utils/src/server_harness.rs` - Added 5 methods
- `crates/ac-service/tests/integration/user_auth_tests.rs` - New file (22 tests)
- `crates/ac-service/tests/integration_tests.rs` - Added module registration
