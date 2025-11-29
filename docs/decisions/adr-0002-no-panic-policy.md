# ADR-0002: No-Panic Error Handling Policy

**Status**: Accepted

**Date**: 2025-01-22

**Deciders**: All Specialists

---

## Context

Rust provides several ways to handle errors and unexpected conditions:
- `Result<T, E>` for recoverable errors
- `Option<T>` for optional values
- `unwrap()` / `expect()` / `panic!()` for unrecoverable errors

**Problems with panic in production**:
- **Process crashes**: Panic terminates the thread (or process if not caught)
- **Poor observability**: Stack traces in logs, but service is down
- **Cascading failures**: One panic can bring down entire service
- **No recovery**: Client gets connection reset, can't retry gracefully

**Requirements**:
- Production services must never crash unexpectedly
- All errors must be observable and loggable
- Clients should receive proper error responses
- Failed requests shouldn't affect other requests

## Decision

**We prohibit the use of panic-inducing functions in production code.**

### Prohibited Functions

**Never use in production code**:
- ❌ `unwrap()`
- ❌ `expect()`
- ❌ `panic!()`
- ❌ `unreachable!()`
- ❌ Index operations that can panic: `vec[idx]`, `map[key]`
- ❌ `unwrap_or_else(|| panic!())`

### Required Patterns

**Always use**:
- ✅ `?` operator with proper error types
- ✅ `if let Some(x) = option { ... }`
- ✅ `match result { Ok(x) => ..., Err(e) => ... }`
- ✅ `.ok_or()` or `.ok_or_else()` to convert `Option` to `Result`
- ✅ `.get(idx)` instead of `[idx]` for collections
- ✅ Custom error types with `thiserror` or `anyhow`

### Code Examples

#### Bad (Can Panic)

```rust
// ❌ Can panic if parse fails
let auth_value = format!("Bearer {}", token).parse().unwrap();

// ❌ Can panic if no clusters
let first_cluster = config.clusters[0];

// ❌ Can panic if missing kid
let kid = header.kid.unwrap();

// ❌ Can panic on out of bounds
let participant = participants[participant_id];
```

#### Good (Error Handling)

```rust
// ✅ Propagate error
let auth_value = format!("Bearer {}", token)
    .parse()
    .map_err(|e| GrpcError::InvalidMetadata {
        field: "authorization".to_string(),
        source: e
    })?;

// ✅ Handle missing value
let first_cluster = config.clusters
    .first()
    .ok_or(ConfigError::NoClusters)?;

// ✅ Convert Option to Result
let kid = header.kid.ok_or(ValidationError::MissingKid)?;

// ✅ Safe collection access
let participant = participants
    .get(&participant_id)
    .ok_or(MeetingError::ParticipantNotFound { participant_id })?;
```

### Error Types

**Use `thiserror` for library errors**:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Missing kid in JWT header")]
    MissingKid,

    #[error("Unknown key ID: {kid}")]
    UnknownKid { kid: String },

    #[error("Token expired at {exp}")]
    TokenExpired { exp: u64 },

    #[error("Invalid signature")]
    InvalidSignature(#[from] SignatureError),
}
```

**Use `anyhow` for application errors** (optional):
```rust
use anyhow::{Context, Result};

async fn handle_request() -> Result<Response> {
    let config = load_config()
        .context("Failed to load configuration")?;

    let token = validate_token(&config)
        .context("Token validation failed")?;

    Ok(Response::success())
}
```

### Exception: Tests

**Acceptable in tests only**:
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_token_validation() {
        let token = create_test_token();
        let claims = validate_token(&token).unwrap();  // ✅ OK in tests
        assert_eq!(claims.sub, "test_user");
    }
}
```

**Rationale**: Tests should fail fast on unexpected conditions.

**Preferred even in tests**:
```rust
#[test]
fn test_token_validation() -> Result<(), Box<dyn std::error::Error>> {
    let token = create_test_token();
    let claims = validate_token(&token)?;  // ✅ Better - gives clear error
    assert_eq!(claims.sub, "test_user");
    Ok(())
}
```

## Consequences

### Positive

- ✅ **No crashes**: Services stay up despite errors
- ✅ **Better observability**: Errors logged with context, not just stack traces
- ✅ **Graceful degradation**: Failed requests return error responses, others succeed
- ✅ **Client-friendly**: Clients receive HTTP 500 or gRPC error, can retry
- ✅ **Easier debugging**: Error types carry context (what failed, why)
- ✅ **Compiler-enforced**: `?` operator requires Result return type

### Negative

- ❌ **More verbose**: Error handling adds lines of code
- ❌ **Error type complexity**: Need to define error enums
- ❌ **Conversion overhead**: Mapping between error types

### Neutral

- Error handling is explicit (visible in function signatures)
- Encourages thinking about failure modes

## Alternatives Considered

### Alternative 1: Allow `expect()` with Good Messages

**Approach**: Allow `expect("descriptive message")` for "impossible" cases

**Pros**:
- Less verbose than full error handling
- Panic message provides some context

**Cons**:
- Still crashes the service
- "Impossible" cases happen in production
- No recovery mechanism

**Why not chosen**: Crashes are unacceptable in production services

### Alternative 2: Global Panic Handler

**Approach**: Install panic handler that logs and continues

**Pros**:
- Service doesn't crash
- Can log panic details

**Cons**:
- Thread that panicked is still dead
- Undefined state after panic
- Can't send proper error response
- Hard to test panic handling

**Why not chosen**: Better to prevent panics than handle them

### Alternative 3: Panic Only on Programmer Errors

**Approach**: Use `assert!` for invariants, `Result` for runtime errors

**Pros**:
- Catches bugs early in development
- Distinguishes programmer errors from user errors

**Cons**:
- Asserts still crash in production
- Hard to distinguish "programmer error" from "runtime error"

**Why not chosen**: Production crashes are never acceptable

## Implementation Notes

### Linting

Enable Clippy lints in `Cargo.toml`:
```toml
[workspace.lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
indexing_slicing = "warn"
```

This makes violations compilation errors.

### Common Patterns

**Converting Option to Result**:
```rust
let value = option.ok_or(Error::NotFound)?;
let value = option.ok_or_else(|| Error::expensive_construction())?;
```

**Handling Multiple Errors**:
```rust
async fn complex_operation() -> Result<Output, MyError> {
    let a = operation_a().await?;  // Propagate error
    let b = operation_b().await.map_err(MyError::OperationB)?;  // Convert error
    let c = operation_c().await.context("Operation C failed")?;  // Add context
    Ok(Output { a, b, c })
}
```

**Safe Collection Access**:
```rust
// Instead of: vec[idx]
let item = vec.get(idx).ok_or(Error::IndexOutOfBounds { idx, len: vec.len() })?;

// Instead of: map[key]
let value = map.get(&key).ok_or(Error::KeyNotFound { key })?;
```

**Fallible Parsing**:
```rust
// Instead of: s.parse().unwrap()
let value: i32 = s.parse().map_err(|e| Error::ParseInt { input: s, source: e })?;
```

### Migration Strategy

For existing code with `unwrap()`:
1. Run `cargo clippy` to find violations
2. Replace each `unwrap()` with proper error handling
3. Add necessary error types
4. Update function signatures to return `Result`
5. Propagate errors with `?` operator

### Suppressing Lints with `#[expect]`

**When lints must be suppressed** (rare cases), use `#[expect(...)]` instead of `#[allow(...)]`:

```rust
// ✅ GOOD: Use #[expect] with explanation
#[expect(clippy::too_many_arguments, reason = "Represents all table columns")]
pub async fn create_signing_key(
    pool: &PgPool,
    key_id: &str,
    public_key: &str,
    // ... 6 more parameters
) -> Result<SigningKey, AcError> { }

// ❌ BAD: Don't use #[allow]
#[allow(clippy::too_many_arguments)]
pub async fn create_signing_key(...) { }
```

**Why `#[expect]` is better**:
- ✅ **Warns when lint no longer applies**: If the code changes and the lint doesn't trigger, you get a warning to remove the `#[expect]`
- ✅ **More explicit**: Shows this is an intentional design decision, not forgotten code
- ✅ **Encourages review**: Forces periodic evaluation of whether the suppression is still needed
- ✅ **Self-documenting**: The `reason` attribute explains why the lint is suppressed

**Valid reasons to suppress lints**:
1. **`too_many_arguments`**: Database repository functions representing table columns
2. **`cast_possible_truncation`**: When the cast is guaranteed safe by protocol design (e.g., payload length bounded by protocol)
3. **`dead_code`**: Phase-specific code that will be used in future phases (document with comment)

**Example with dead_code**:
```rust
#[expect(dead_code, reason = "Will be used in Phase 4 for JWKS endpoint")]
pub async fn get_all_active_keys(pool: &PgPool) -> Result<Vec<SigningKey>, AcError> {
    // Implementation for future JWKS endpoint
}
```

### Code Review Checklist

When reviewing code, verify:
- [ ] No `unwrap()` or `expect()` in production code
- [ ] No `panic!()` or `unreachable!()`
- [ ] Collection access uses `.get()` not `[idx]`
- [ ] Errors have descriptive types (not just `String`)
- [ ] Error messages include context
- [ ] Lint suppressions use `#[expect(...)]` not `#[allow(...)]`
- [ ] Each `#[expect]` has a `reason` attribute explaining why

## References

- Rust Book - Error Handling: https://doc.rust-lang.org/book/ch09-00-error-handling.html
- `thiserror` crate: https://docs.rs/thiserror/
- `anyhow` crate: https://docs.rs/anyhow/
- Clippy lints: https://rust-lang.github.io/rust-clippy/
- Related: ADR-0001 (Actor Pattern ensures errors don't crash other actors)
