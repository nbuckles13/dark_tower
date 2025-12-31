# Principle: Logging Safety and Observability

**Status**: Active
**Category**: Security, Observability
**Guards**:
- `scripts/guards/simple/no-secrets-in-logs.sh`
- `scripts/guards/semantic/credential-leak.sh`

## Summary

Logging must balance observability with security. Sensitive data (passwords, tokens, secrets, PII) must never appear in logs, traces, or error messages. All logging follows a privacy-by-default model with explicit opt-in for safe fields.

---

## DO

1. **Use privacy-by-default instrumentation**
   - Start with `#[instrument(skip_all)]` on all handlers and critical functions
   - Explicitly allow-list only SAFE fields in the `fields()` clause
   - Include correlation IDs (`trace_id`, `request_id`) for debugging

2. **Skip sensitive parameters in tracing**
   - Always use `skip(password, secret, token, key, credential)` in `#[instrument]`
   - Skip entire parameter lists with `skip_all` if any contain secrets
   - Example: `#[instrument(skip(password))]` or `#[instrument(skip_all, fields(username))]`

3. **Use SecretString for sensitive values**
   - Wrap all credentials in `common::secret::SecretString` type
   - This auto-redacts in Debug output: `Secret([REDACTED])`
   - Access with `.expose_secret()` only when needed
   - Example: `pub password: SecretString` in request structs

4. **Log metadata, not secrets**
   - Log token expiration times, scope counts, not token values
   - Log client IDs, user IDs (if not PII), not credentials
   - Log operation status, error types, not error details with secrets

5. **Use structured logging with safe fields**
   - Log only enumerated values: `grant_type`, `status`, `operation`
   - Use correlation IDs: `trace_id`, `span_id`, `request_id`
   - Log timing metrics: `duration_ms`, `timestamp`

6. **Apply logging tier policies**
   - **DEBUG**: Full payloads allowed (DEV ONLY, never in staging/prod)
   - **INFO**: SAFE fields only, no user-identifiable information
   - **WARN**: Error classification, hashed identifiers only
   - **ERROR**: Sanitized error messages, correlation IDs only

7. **Use three-level visibility for UNSAFE fields**
   - **Masked** (default): Output as `****` for presence indication
   - **Hashed**: Output as `h:a1b2c3d4` when correlation needed (use sparingly)
   - **Plaintext**: Only in DEBUG level, dev environments only

8. **Include span names following conventions**
   - Format: `{service}.{subsystem}.{operation}`
   - Examples: `ac.token.issue`, `gc.http.request`, `mc.session.join`

9. **Log errors generically at API boundaries**
   - Return sanitized error messages to clients
   - Log detailed errors internally with correlation IDs
   - Never leak internal implementation details in errors

10. **Use explicit redaction for debug output**
    - When logging structs, implement custom Debug that redacts secrets
    - Or log only safe fields individually
    - Never use `{:?}` on structs containing secrets unless they use SecretString

---

## DON'T

1. **Never log raw credentials**
   - ❌ `info!("Password: {}", password)`
   - ❌ `debug!("Token: {}", token)`
   - ❌ `error!("Auth failed for secret: {}", secret)`

2. **Never use #[instrument] without skip on functions with secrets**
   - ❌ `#[instrument]` on `fn authenticate(username: &str, password: &str)`
   - ✅ `#[instrument(skip(password))]` or `#[instrument(skip_all)]`

3. **Never call expose_secret() in log statements**
   - ❌ `info!("Secret: {}", secret.expose_secret())`
   - ✅ `info!("Secret configured: {}", secret_length)`

4. **Never log PII (Personally Identifiable Information)**
   - ❌ Full email addresses, phone numbers, real names
   - ❌ IP addresses in plaintext (use hashed or masked)
   - ❌ User agents (fingerprinting risk)
   - ❌ Geolocation coordinates

5. **Never use Debug formatting on structs with secrets**
   - ❌ `debug!("Request: {:?}", request)` when request contains `client_secret`
   - ✅ Use SecretString for secret fields or implement custom Debug

6. **Never include secrets in error messages**
   - ❌ `return Err(anyhow!("Invalid token: {}", token))`
   - ✅ `return Err(anyhow!("Invalid token format"))`

7. **Never log request/response bodies containing credentials**
   - ❌ `debug!("Request body: {}", body)` for auth endpoints
   - ✅ `debug!("Request received for grant_type={}", grant_type)`

8. **Never use high-cardinality values as span attributes**
   - ❌ Full UUIDs, unbounded meeting IDs
   - ✅ Hashed IDs with `h:` prefix or indexed values (1-1000)

9. **Never assume a field is safe**
   - Any field that could identify a user, session, or contain secrets is UNSAFE
   - When in doubt, mask it or use hashed visibility

10. **Never skip security review for new logging**
    - All new log fields must be reviewed for PII/secret leakage
    - Security specialist must approve any UNSAFE field visibility changes

---

## Examples

### ✅ CORRECT: Privacy-by-default instrumentation

```rust
use tracing::instrument;

/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
/// Only safe fields (grant_type, status) are recorded.
#[instrument(
    name = "ac.token.issue_user",
    skip_all,
    fields(grant_type = "password", status)
)]
pub async fn handle_user_token(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UserTokenRequest>,
) -> Result<Json<TokenResponse>, AcError> {
    // Access secret only when needed, never log it
    let password = payload.password.expose_secret();
    let result = issue_token(&payload.username, password).await;

    // Log safe metadata only
    tracing::info!(
        username = %payload.username,
        status = "success",
        "Token issued"
    );

    result
}
```

### ✅ CORRECT: Using SecretString

```rust
use common::secret::{ExposeSecret, SecretString};

#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    pub client_id: String,
    pub client_secret: Option<SecretString>,  // Auto-redacts in Debug
}

// Debug output: TokenRequest { client_id: "my-service", client_secret: Some(Secret([REDACTED])) }
```

### ✅ CORRECT: Skipping sensitive parameters

```rust
#[instrument(skip(password))]
fn authenticate(username: &str, password: &str) -> Result<User> {
    // username is logged, password is skipped
}

#[instrument(skip_all, fields(client_id = %request.client_id))]
fn validate_client(request: &ClientRequest) -> Result<()> {
    // Only client_id is logged, all parameters skipped
}
```

### ✅ CORRECT: Logging metadata, not secrets

```rust
// Log token metadata
info!(
    expires_at = %token.exp,
    scope_count = token.scopes.len(),
    "Token issued"
);

// Log operation status
info!(
    client_id = %client_id,
    status = "success",
    duration_ms = elapsed.as_millis(),
    "Client authenticated"
);
```

### ✅ CORRECT: Safe error handling

```rust
// Generic error to client
let error_msg = "Invalid token format";

// Detailed logging with correlation ID
tracing::error!(
    trace_id = %trace_id,
    error_type = "jwt_parse_error",
    "Token validation failed"
);

Err(AcError::Unauthorized(error_msg.to_string()))
```

### ❌ WRONG: Missing skip in #[instrument]

```rust
// VIOLATION: password will be logged in trace
#[instrument]
fn authenticate(username: &str, password: &str) -> Result<User> { ... }
```

### ❌ WRONG: Direct logging of secrets

```rust
// VIOLATION: password logged directly
info!("User {} authenticated with password {}", user, password);

// VIOLATION: token in debug output
debug!("Token issued: {}", token);

// VIOLATION: secret in error
error!("Auth failed with secret: {}", client_secret);
```

### ❌ WRONG: Debug formatting structs with secrets

```rust
struct TokenRequest {
    client_id: String,
    client_secret: String,  // NOT using SecretString!
}

// VIOLATION: Debug will print client_secret
debug!("Request: {:?}", request);
```

### ❌ WRONG: Exposing secrets in logs

```rust
// VIOLATION: expose_secret() in log statement
info!("Secret: {}", secret.expose_secret());

// VIOLATION: named field with secret
tracing::info!(password = %pwd, "Setting password");
```

---

## Field Classification

### SAFE Fields (always log in plaintext)

- System identifiers: `service`, `region`, `environment`
- Correlation IDs: `trace_id`, `span_id`, `request_id`
- Operation metadata: `method`, `status_code`, `error_type`, `operation`
- Timing: `duration_ms`, `timestamp`
- Enums/bounded values: `grant_type`, `codec`, `media_type`

### UNSAFE Fields (require visibility selection)

**Credentials & Secrets:**
- `password`, `secret`, `api_key`, `bearer_token`, `private_key`, `master_key`
- `jwt` (full token), `session_cookie`, `refresh_token`, `access_token`

**PII (Personally Identifiable Information):**
- `email`, `phone_number`, `display_name`, `full_name`
- `ip_address`, `user_agent` (fingerprinting risk)
- `geolocation` (exact coordinates)

**Session/Request Data:**
- `meeting_id`, `participant_id` (may correlate to PII)
- `request_body`, `response_body` (may contain secrets/PII)
- `error_message` (may leak internal details)

**When in doubt**: Treat as UNSAFE and mask it.

---

## Three-Level Visibility Model

For UNSAFE fields, choose appropriate visibility:

| Level | Output | Cost | Use Case |
|-------|--------|------|----------|
| **Masked** | `****` | Zero | Default for presence indication |
| **Hashed** | `h:a1b2c3d4` | ~1μs | When correlation needed |
| **Plaintext** | Full value | Zero | DEBUG level only, dev only |

### When to use each level

1. **Masked (default)**: Most UNSAFE fields
   - Examples: `ip_address`, `email`, `user_agent`
   - You just need to know the field was present

2. **Hashed**: Correlation across log entries needed
   - Examples: `meeting_id` (track meeting lifecycle), `client_id` (track client requests)
   - Use sparingly due to CPU cost and cardinality

3. **Plaintext**: Development debugging only
   - Never in staging/prod environments
   - Only at DEBUG log level

---

## Type-System Constraints

The `secrecy` crate provides compile-time safety:

```rust
use common::secret::{ExposeSecret, SecretString};

// Secrets are wrapped in SecretString
pub struct Credentials {
    pub username: String,          // Safe to log
    pub password: SecretString,    // Auto-redacts in Debug
}

// Debug output: Credentials { username: "alice", password: Secret([REDACTED]) }

// Explicit exposure required to access
fn use_password(creds: &Credentials) {
    let pwd: &str = creds.password.expose_secret();
    validate(pwd);
}
```

**Benefits:**
- Compile-time prevention of accidental logging
- Explicit `.expose_secret()` makes secret access visible in code
- Memory zeroization on drop
- Serde integration for JSON deserialization

---

## Guards

### Simple Guard: `scripts/guards/simple/no-secrets-in-logs.sh`

Detects:
- `#[instrument]` without `skip(...)` on functions with secret parameters
- Log macros containing secret variable patterns
- Named tracing fields with secret names

### Semantic Guard: `scripts/guards/semantic/credential-leak.sh`

Analyzes:
- Complex control flow where secrets might leak indirectly
- Struct definitions that might contain secrets and be logged
- Error handling paths that might include secret data

---

## ADR References

- [ADR-0011: Observability Framework](../decisions/adr-0011-observability-framework.md) - Privacy-by-default logging, PII protection, span requirements
- [ADR-0002: No-Panic Error Handling](../decisions/adr-0002-no-panic-policy.md) - Error handling patterns that prevent leaking secrets in panics

---

## Testing

All logging code must include tests for:

1. **PII leakage tests**: Verify secrets never appear in logs
2. **Instrumentation coverage**: All handlers have proper `#[instrument]` attributes
3. **SecretString usage**: All credential fields use SecretString wrapper
4. **Error message sanitization**: Generic errors at API boundaries

Example test:

```rust
#[tokio::test]
async fn test_no_password_in_logs() {
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::sink())
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        let request = UserTokenRequest {
            username: "alice".to_string(),
            password: SecretString::new("secret123".to_string()),
        };

        // Call handler
        let result = handle_user_token(state, Json(request)).await;

        // Verify logs don't contain password
        // (use test subscriber that captures logs)
        assert!(!logs.contains("secret123"));
    });
}
```

---

## Summary Checklist

Before merging any PR with logging changes:

- [ ] All `#[instrument]` attributes use `skip_all` or `skip(secret_params)`
- [ ] All credential fields use `SecretString` wrapper
- [ ] No `.expose_secret()` calls in log statements
- [ ] Only SAFE fields logged at INFO/WARN/ERROR levels
- [ ] UNSAFE fields use appropriate visibility (masked by default)
- [ ] Error messages are generic, no internal details leaked
- [ ] PII leakage tests added for new logging code
- [ ] Security specialist reviewed UNSAFE field visibility

**Remember**: When in doubt, don't log it. Correlation IDs enable debugging without exposing sensitive data.
