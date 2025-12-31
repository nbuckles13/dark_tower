# Principle: Error Handling

## Core Philosophy

**Production code MUST NEVER panic.** All errors must be recoverable, loggable, and return proper responses to clients. This ensures service availability, observability, and graceful degradation.

---

## DO

### Error Propagation
- **Use `Result<T, E>` for all fallible operations** - Function signatures must reflect failure modes
- **Use `?` operator for error propagation** - Propagate errors up the call stack cleanly
- **Convert `Option` to `Result` with `.ok_or()` or `.ok_or_else()`** - Make missing values explicit errors
- **Define custom error types per crate** - Use `thiserror` for library errors (e.g., `AcError`, `GcError`)
- **Map errors at API boundaries with `.map_err()`** - Convert internal errors to public error types
- **Use `.ok_or_else()` with lazy construction** - Avoid expensive error construction unless needed

### Collection Safety
- **Use `.get(idx)` instead of `[idx]` for slices/vectors** - Returns `Option` instead of panicking on out-of-bounds
- **Use `.get(&key)` instead of `[key]` for maps** - Returns `Option<&V>` instead of panicking on missing keys
- **Use `.first()` instead of `[0]` for accessing first element** - Safe access with `Option`
- **Chain with `.ok_or()` for required values** - Convert `Option` to `Result` with descriptive error

### Error Types
- **Implement `std::error::Error` trait** - Use `thiserror` derive macro for automatic implementation
- **Include context in error variants** - Use struct variants with named fields for rich error information
- **Log internal details, return generic messages** - Don't leak sensitive information to clients
- **Map to appropriate HTTP status codes** - 400-level for client errors, 500-level for server errors
- **Use `#[from]` attribute for error conversion** - Automatic conversion from source errors

### Test Code Exceptions
- **Tests CAN use `.unwrap()` for known-good test data** - Tests should fail fast on setup errors
- **Mark test code with `#[cfg(test)]`** - Clearly separate test code from production code
- **Prefer `Result<(), E>` return types even in tests** - Provides better error messages on failure

---

## DON'T

### Prohibited Functions (Production Code)
- **NEVER use `.unwrap()`** - Panics on `None` or `Err`, crashes the service
- **NEVER use `.expect("message")`** - Still panics, even with a message
- **NEVER use `panic!("message")`** - Directly crashes the service
- **NEVER use `unreachable!()`** - Use compile-time exhaustiveness checking instead
- **NEVER use `.unwrap_or_else(|| panic!())`** - Defeats the purpose of `unwrap_or_else`
- **NEVER use index operators on collections** - `vec[idx]`, `map[key]` panic on invalid access

### Error Handling Anti-Patterns
- **DON'T ignore errors silently** - Always handle or propagate errors
- **DON'T use `String` as error type** - Lacks structure and context, hard to handle programmatically
- **DON'T expose internal error details to clients** - Leak implementation details and potential security info
- **DON'T use `#[allow(...)]` for lint suppressions** - Use `#[expect(..., reason = "...")]` instead
- **DON'T suppress lints without justification** - Every `#[expect]` must have a `reason` attribute

---

## Examples

### ❌ BAD: Panic-prone Code

```rust
// NEVER: Can panic if parse fails
let token_value = format!("Bearer {}", token).parse().unwrap();

// NEVER: Can panic if no clusters configured
let first_cluster = config.clusters[0];

// NEVER: Can panic if missing kid header
let kid = header.kid.unwrap();

// NEVER: Can panic on out of bounds
let participant = participants[participant_id];

// NEVER: String concatenation for errors
return Err("Database connection failed".to_string());
```

### ✅ GOOD: Robust Error Handling

```rust
// ✅ Propagate parse error with context
let token_value = format!("Bearer {}", token)
    .parse()
    .map_err(|e| AcError::InvalidToken(format!("Invalid format: {}", e)))?;

// ✅ Handle missing value explicitly
let first_cluster = config.clusters
    .first()
    .ok_or(ConfigError::NoClusters)?;

// ✅ Convert Option to Result with descriptive error
let kid = header.kid
    .ok_or(AcError::InvalidToken("Missing kid in JWT header".to_string()))?;

// ✅ Safe collection access with context
let participant = participants
    .get(&participant_id)
    .ok_or(MeetingError::ParticipantNotFound { participant_id })?;

// ✅ Structured error type with thiserror
return Err(AcError::Database("Connection pool exhausted".to_string()));
```

### Custom Error Type Definition

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AcError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Cryptographic error: {0}")]
    Crypto(String),

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Insufficient scope: required {required}, provided {provided:?}")]
    InsufficientScope {
        required: String,
        provided: Vec<String>,
    },

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,
}

impl AcError {
    /// Map to HTTP status code for client responses
    pub fn status_code(&self) -> u16 {
        match self {
            AcError::Database(_) | AcError::Crypto(_) => 500,
            AcError::InvalidCredentials | AcError::InvalidToken(_) => 401,
            AcError::InsufficientScope { .. } => 403,
            AcError::RateLimitExceeded => 429,
        }
    }
}
```

### Error Mapping at API Boundaries

```rust
// ✅ Map internal errors to public API errors
async fn handle_auth_request(req: AuthRequest) -> Result<AuthResponse, AcError> {
    // Database error -> generic error (don't leak internal details)
    let credentials = repositories::get_credentials(&req.client_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Database query failed");
            AcError::Database("Failed to retrieve credentials".to_string())
        })?;

    // Crypto error -> generic error (don't leak crypto details)
    let verified = crypto::verify_signature(&req.signature)
        .map_err(|e| {
            tracing::error!(error = %e, "Signature verification failed");
            AcError::Crypto("Signature verification failed".to_string())
        })?;

    // Business logic error -> specific client error
    if !verified {
        return Err(AcError::InvalidCredentials);
    }

    Ok(AuthResponse::success())
}
```

### Suppressing Lints (Rare Cases)

```rust
// ✅ Use #[expect] with reason for intentional lint suppressions
#[expect(clippy::too_many_arguments, reason = "Represents all table columns")]
pub async fn create_signing_key(
    pool: &PgPool,
    key_id: &str,
    public_key: &str,
    private_key_encrypted: &str,
    algorithm: &str,
    created_at: i64,
    expires_at: i64,
    is_active: bool,
) -> Result<SigningKey, AcError> {
    // Implementation
}

// ❌ NEVER use #[allow] - doesn't warn if lint no longer applies
#[allow(clippy::too_many_arguments)]
pub async fn create_signing_key(...) -> Result<SigningKey, AcError> {
    // Implementation
}
```

---

## Guards

### Clippy Lints (Cargo.toml)

Enable these lints in workspace `Cargo.toml` to enforce error handling policy:

```toml
[workspace.lints.clippy]
unwrap_used = "deny"           # Forbid .unwrap()
expect_used = "deny"           # Forbid .expect()
panic = "deny"                 # Forbid panic!()
indexing_slicing = "warn"      # Warn on vec[idx], map[key]
```

### Code Review Checklist

When reviewing code, verify:
- [ ] No `.unwrap()` or `.expect()` in production code
- [ ] No `panic!()` or `unreachable!()`
- [ ] Collection access uses `.get()` not `[idx]`
- [ ] Errors have structured types (not `String`)
- [ ] Error messages include context
- [ ] Internal errors logged, generic messages returned to clients
- [ ] Lint suppressions use `#[expect(..., reason = "...")]` not `#[allow(...)]`
- [ ] Each `#[expect]` has a `reason` attribute explaining why
- [ ] Function signatures use `Result<T, E>` for fallible operations

---

## ADR References

- **ADR-0002: No Panic Policy** - Complete rationale, alternatives considered, implementation guide
