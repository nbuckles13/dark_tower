# Auth Controller Patterns

Reusable patterns discovered and established in the Auth Controller codebase.

---

## Pattern: Configurable Security Parameters via Environment
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Security parameters (JWT clock skew, bcrypt cost) follow consistent pattern:
1. Constants for DEFAULT, MIN, MAX with docs
2. Parse from env var with validation
3. Reject outside safe range with descriptive error
4. Warn (accept) values below recommended default

---

## Pattern: Config Testability via from_vars()
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Config has `from_env()` for production and `from_vars(&HashMap)` for tests. All parsing in `from_vars()`. Tests inject specific values without env manipulation.

---

## Pattern: Crypto Functions Accept Config Parameters
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Crypto functions receive config explicitly: `hash_client_secret(secret, cost)`, `verify_jwt(token, key, clock_skew)`. No global state. Enables testing with different configs.

---

## Pattern: Timing Attack Prevention via Dummy Hash Verification
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/token_service.rs`

When client_id not found, bcrypt runs against a pre-generated dummy hash. This ensures constant-time behavior regardless of whether the client exists. The dummy hash MUST use the same cost factor as production hashes. Always pair this with identical error messages for found/not-found cases.

---

## Pattern: TTL Capping (Defense in Depth)
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/handlers/internal_tokens.rs`

Always cap TTL at endpoint level regardless of client request:
```rust
const MAX_TOKEN_TTL_SECONDS: u32 = 900;
let ttl = payload.ttl_seconds.min(MAX_TOKEN_TTL_SECONDS);
```
Defense in depth - even if validation bypassed, tokens remain short-lived.

---

## Pattern: Custom Debug for Sensitive Field Redaction
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Claims containing sensitive fields (email, roles) implement custom `Debug` trait to redact sensitive data in logs while preserving debuggability for non-sensitive fields:
```rust
impl fmt::Debug for UserClaims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserClaims")
            .field("sub", &self.sub)
            .field("email", &"[REDACTED]")
            .finish()
    }
}
```
Prevents accidental exposure of PII in error logs and debug output.

---

## Pattern: Subdomain-Based Organization Extraction Middleware
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/middleware/org_extraction.rs`

Extract organization context from request Host header subdomain before handler execution. Middleware parses subdomain, looks up organization in database, and injects `OrgContext` via `Extension`. Handlers receive validated organization without repeated lookup logic. Pattern enables multi-tenant routing without path-based organization IDs.

---

## Pattern: Host Header Subdomain Testing
**Added**: 2026-01-15
**Related files**: `crates/ac-test-utils/src/server_harness.rs`

Test subdomain extraction by setting Host header in HTTP requests: `.header("Host", server.host_header("acme"))` returns `acme.localhost:PORT`. The port must be included for test server binding. Test cases should cover valid subdomains, IP addresses (rejected), uppercase (rejected), and unknown subdomains (404).

---

## Pattern: Scope Validation Test Pattern (Multiple Attack Vectors)
**Added**: 2026-01-18
**Related files**: `crates/ac-service/tests/integration/internal_token_tests.rs`

When testing scope-based authorization, cover multiple attack vectors beyond happy path:
1. **Exact match succeeds** - Required scope present works
2. **Prefix attack** - `internal:meeting` should NOT match `internal:meeting-token`
3. **Suffix attack** - `internal:meeting-token-extra` should NOT match `internal:meeting-token`
4. **Case sensitivity** - `INTERNAL:MEETING-TOKEN` should NOT match `internal:meeting-token`
5. **Empty scope** - Token with empty scope claim should be rejected
6. **Multiple scopes** - Token with multiple scopes including required one should succeed

This pattern prevents subtle authorization bypass where similar-looking scopes are accepted.

---

## Pattern: Error Context in Returned Error (Not Logged Separately)
**Added**: 2026-01-28
**Updated**: 2026-01-30
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/handlers/internal_tokens.rs`

Include error context directly in the returned error type, NOT via separate logging. The `IntoResponse` implementation sanitizes errors at the API boundary - clients get generic messages while full context is preserved for server-side error chains:
```rust
// CORRECT: Context in returned error
.map_err(|e| AcError::Crypto(format!("Keypair generation failed: {}", e)))

// WRONG: Logging separately then returning generic
.map_err(|e| {
    tracing::error!(error = %e, "Keypair generation failed");  // Redundant!
    AcError::Crypto("Key generation failed".to_string())  // Context lost
})
```
Crypto library errors (ring, bcrypt, jsonwebtoken) are safe to include - they don't expose sensitive material. Exception: For `InvalidCredentials`, use `|_|` to prevent information leakage about authentication failures.

---

## Pattern: Function-Based Async Spawning (JoinHandle + Receiver)
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

For background tasks that produce continuously updated values, return `(JoinHandle<()>, Receiver)` tuple. The function blocks until initialization succeeds, guaranteeing the receiver contains a valid value on return. This pattern avoids Arc wrappers since the spawned task owns all data directly. Caller controls lifecycle via `handle.abort()` or dropping the handle.

---

## Pattern: Empty Sentinel for Watch Channel Initialization
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

When using `tokio::sync::watch` for async value broadcasting, initialize with an "empty" sentinel value (e.g., empty string for tokens). The spawning function waits for `changed()` before returning, ensuring callers receive a valid value. Include a defensive check after `changed()` to verify the value is no longer the sentinel.

---

## Pattern: Constructor Variants for Security Enforcement
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

Provide both `new()` (permissive for development) and `new_secure()` (validates HTTPS, returns Result) constructors. Document security warnings on the permissive variant. This allows easy local development while enforcing production security through explicit constructor choice.

---

## Pattern: HTTP Metrics Middleware for Comprehensive Observability
**Added**: 2026-02-10
**Related files**: `crates/ac-service/src/middleware/http_metrics.rs`

Wrap all routes with HTTP metrics middleware as the outermost layer to capture ALL responses including framework-level errors (415, 400, 404, 405) that never reach handlers. Middleware records method, path, status code, and duration. Apply via `.layer(middleware::from_fn(http_metrics_middleware))` AFTER defining routes but BEFORE application-specific middleware.

---

## Pattern: HMAC-SHA256 Correlation Hashing for PII-Safe Logging
**Added**: 2026-02-10
**Related files**: `crates/ac-service/src/observability/mod.rs`

Use HMAC-SHA256 with per-service secret key (not plain SHA-256) for correlation hashing of PII fields like `client_id`. Hash with `hash_for_correlation(value, secret)`, which truncates to first 4 bytes (8 hex chars) and prefixes with `h:` to distinguish from legacy hashes. This enables log correlation without storing plaintext PII while preventing rainbow table attacks.

---

## Pattern: SecretBox for Non-String Sensitive Data
**Added**: 2026-02-10
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

Use `SecretBox<Vec<u8>>` for binary sensitive data (keys, encrypted material) and `SecretString` for text secrets (passwords, tokens). Both types provide Debug redaction and require explicit `.expose_secret()` to access values. Custom Clone implementation needed: `SecretBox::new(Box::new(self.field.expose_secret().clone()))`. EncryptedKey struct wraps encrypted_data in SecretBox despite being ciphertext to prevent accidental exposure of encrypted key material.

---

## Pattern: Error Category Mapping for Bounded Cardinality Metrics
**Added**: 2026-02-10
**Related files**: `crates/ac-service/src/observability/mod.rs`

Map domain-specific errors to 4 bounded categories (Authentication, Authorization, Cryptographic, Internal) via `impl From<&AcError> for ErrorCategory`. This prevents cardinality explosion in Prometheus labels while preserving useful error classification. Use `ErrorCategory::from(&err).as_str()` for metric labels.

---

## Pattern: Metrics Recording in Handler Body (Not Middleware)
**Added**: 2026-02-10
**Related files**: `crates/ac-service/src/handlers/internal_tokens.rs`, `crates/ac-service/src/handlers/auth_handler.rs`

Record domain-specific metrics (token_issuance, error categories) inside handler functions, not middleware. Use `Instant::now()` at handler start, record metrics before returning. Update span status via `tracing::Span::current().record("status", ...)` to correlate traces with metrics. Generic HTTP metrics go in middleware; domain metrics go in handlers.

---

## Pattern: Path Normalization for Cardinality Control
**Added**: 2026-02-10
**Related files**: `crates/ac-service/src/observability/metrics.rs`

HTTP metrics normalize paths by replacing UUIDs and numeric IDs with placeholders (e.g., `/clients/{id}` instead of `/clients/550e8400-...`) to prevent cardinality explosion in Prometheus labels. Use `normalize_path()` function that applies regex substitution. Without normalization, each unique ID creates a new time series.

---
