# Principle: No Secrets in Logs

**Status**: Active
**Guard**: `scripts/guards/simple/no-secrets-in-logs.sh`
**Semantic Guard**: `scripts/guards/semantic/credential-leak.sh`

## Summary

Sensitive data (passwords, tokens, secrets, private keys) must never appear in logs, traces, or error messages. This prevents credential leaks through log aggregation, error reporting, and debugging output.

## What Counts as a Secret

| Category | Examples | Variable Patterns |
|----------|----------|-------------------|
| Passwords | User passwords, API passwords | `password`, `passwd`, `pwd` |
| Tokens | JWTs, access tokens, refresh tokens | `token`, `access_token`, `refresh_token`, `bearer` |
| Secrets | Client secrets, API secrets | `secret`, `client_secret`, `api_secret` |
| Keys | Private keys, API keys, encryption keys | `key`, `private_key`, `api_key`, `master_key` |
| Credentials | Combined auth data | `credential`, `cred`, `auth` |

## Violation Patterns

### 1. Missing `skip` in `#[instrument]`

```rust
// VIOLATION: password parameter will be logged
#[instrument]
fn authenticate(username: &str, password: &str) -> Result<User> { ... }

// CORRECT: skip sensitive parameters
#[instrument(skip(password))]
fn authenticate(username: &str, password: &str) -> Result<User> { ... }
```

### 2. Direct logging of secrets

```rust
// VIOLATION: password logged directly
info!("User {} authenticated with password {}", user, password);

// VIOLATION: token in debug output
debug!("Token issued: {}", token);

// CORRECT: log only non-sensitive data
info!("User {} authenticated successfully", user);
debug!("Token issued for client_id={}", client_id);
```

### 3. Debug formatting structs containing secrets

```rust
struct TokenRequest {
    client_id: String,
    client_secret: String,  // Secret field!
}

// VIOLATION: Debug will print client_secret
debug!("Request: {:?}", request);

// CORRECT: Implement custom Debug that redacts secrets
// Or log only safe fields:
debug!("Request from client_id={}", request.client_id);
```

### 4. Named field logging with secrets

```rust
// VIOLATION: secret as named field
tracing::info!(password = %pwd, "Setting password");
tracing::info!(token = ?jwt, "Issued token");

// CORRECT: use redacted placeholders
tracing::info!(password = "[REDACTED]", "Setting password");
tracing::info!("Token issued for user_id={}", user_id);
```

### 5. Error messages containing secrets

```rust
// VIOLATION: secret in error context
return Err(anyhow!("Invalid token: {}", token));

// CORRECT: generic error without secret
return Err(anyhow!("Invalid token format"));
```

## Acceptable Patterns

### Logging identifiers (not secrets)

```rust
// OK: client_id is an identifier, not a secret
info!("Token requested by client_id={}", client_id);

// OK: user_id is not sensitive
debug!("User {} logged in", user_id);
```

### Logging metadata about secrets

```rust
// OK: logging token metadata, not the token itself
info!("Token expires at {}", token.exp);
info!("Token has {} scopes", token.scopes.len());
```

### Using redaction helpers

```rust
// OK: explicit redaction
info!("Password hash updated: {}", redact(&password_hash));
```

## Type-System Constraints (Implemented)

**Status**: Active - Using `secrecy` crate

The type system prevents violations at compile time using the `secrecy` crate. This is
the **first line of defense** in the layered approach:

```
Constraints (compile-time) → Simple Guards → Semantic Guards → Learning
```

### Using SecretString

Import from the `common` crate:

```rust
use common::secret::{ExposeSecret, SecretString};

// Define request structs with SecretString for sensitive fields
#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    pub client_id: String,
    pub client_secret: Option<SecretString>,  // Automatically redacted in Debug
}

// The Debug implementation will show:
// TokenRequest { client_id: "my-service", client_secret: Some(Secret([REDACTED])) }
```

### Key Benefits

1. **Compile-time safety**: Any struct containing `SecretString` automatically has safe Debug
2. **Explicit access**: Must call `.expose_secret()` to get the inner value
3. **Memory zeroization**: Secrets are wiped from memory when dropped
4. **Serde integration**: Works seamlessly with JSON deserialization

### Accessing Secret Values

```rust
// To use the secret value, explicitly expose it:
let password: &str = payload.password.expose_secret();
validate_password(password)?;

// For Option<SecretString>:
let secret = payload.client_secret
    .as_ref()
    .map(|s| s.expose_secret().to_string());
```

### Applied In

- `ac-service/handlers/auth_handler.rs`:
  - `UserTokenRequest.password: SecretString`
  - `ServiceTokenRequest.client_secret: Option<SecretString>`

### Module Location

- `common/src/secret.rs` - Re-exports from `secrecy` crate with Dark Tower documentation

This makes accidental logging impossible because `SecretString` always displays as `[REDACTED]`.

## Guard Detection

The simple guard (`no-secrets-in-logs.sh`) detects:
- `#[instrument]` without `skip(...)` for functions with secret parameters
- Log macros (`info!`, `debug!`, `warn!`, `error!`, `trace!`) containing secret variable patterns
- Named tracing fields with secret names

The semantic guard (`credential-leak.sh`) analyzes:
- Complex control flow where secrets might leak indirectly
- Struct definitions that might contain secrets and be logged
- Error handling paths that might include secret data

## References

- ADR-0011: Observability Framework (PII protection)
- OWASP Logging Cheat Sheet
- CWE-532: Insertion of Sensitive Information into Log File
