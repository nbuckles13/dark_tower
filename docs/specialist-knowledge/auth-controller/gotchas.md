# Auth Controller Gotchas

Mistakes to avoid and edge cases discovered in the Auth Controller codebase.

---

## Gotcha: Bcrypt Cost Validated at Multiple Layers
**Added**: 2026-01-11
**Updated**: 2026-01-30
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/config.rs`

Bcrypt library accepts cost 4-31, but OWASP requires minimum 10. Config enforces 10-14, AND `hash_client_secret()` re-validates as defense-in-depth. If called directly with invalid cost, it returns an error with context: `"Invalid bcrypt cost: N (must be 10-14)"`.

---

## Gotcha: Clock Skew Creates Pre-generation Window
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

JWT `iat` validation uses clock skew. 300s skew means tokens 5 min in future are accepted. Necessary for distributed systems but creates pre-generation window. NIST recommends 5 min.

---

## Gotcha: Timing Attack Requires Matching Dummy Hash
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/token_service.rs`

When client_id not found, bcrypt runs against dummy hash. Dummy MUST use same cost factor as production or timing differs. Current: `$2b$12$...`. If default cost changes, regenerate dummy.

---

## Gotcha: Hash Secret Defaults for Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

AC_HASH_SECRET defaults to 32 zero bytes. Intentional for tests, MUST set in production. Config does not error on missing - silently uses default.

---

## Gotcha: Bcrypt Cost Affects Registration Latency
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/registration_service.rs`

Higher cost = slower registration/rotation. Cost 12 ~200ms, cost 14 ~800ms. Tests with repeated bcrypt ops can be slow.

---

## Gotcha: Error Messages Must Be Identical
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/token_service.rs`

Invalid client_id and invalid password return identical `AcError::InvalidCredentials`. Prevents enumeration. Never add specific messages like "client not found".

---

## Gotcha: Timing Tests Skipped Under Coverage
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/token_service.rs`

`#[cfg_attr(coverage, ignore)]` skips timing tests during coverage (instrumentation overhead). Manual verification needed for timing-sensitive changes.

---

## Gotcha: JWT Size Check Before Parsing
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

`verify_jwt()` checks 4KB limit BEFORE base64/signature ops. Defense against DoS. Do not move check after parsing. Limit is generous (typical JWT 200-500 bytes).

---

## Gotcha: JTI Required for Revocable Tokens
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/handlers/internal_tokens.rs`

Meeting and guest tokens need unique `jti` (JWT ID) claims for tracking and revocation. Always include jti for tokens that may need revocation. Service tokens may omit jti if revocation not needed.

---

## Gotcha: Claims Extension Type Must Match Exactly
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/middleware/auth.rs`, `crates/ac-service/src/handlers/internal_tokens.rs`

When using `Extension<T>` in handlers, middleware must insert the exact same type:
```rust
// Middleware:
req.extensions_mut().insert(claims);  // crypto::Claims

// Handler:
Extension(claims): Extension<crypto::Claims>  // Must match
```
No trait objects or generics - type must match exactly or extraction fails silently.

---

## Gotcha: Subdomain Extraction Edge Cases
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/middleware/org_extraction.rs`

Host header parsing has edge cases: IP addresses (no subdomain), ports (strip before parsing), single-part hostnames (localhost), and hosts with many parts (a.b.c.example.com). Middleware must handle: stripping port, checking for IP addresses, and extracting first segment only when at least 3 parts exist. Tests should cover all edge cases.

---

## Gotcha: split_whitespace() Scope Extraction Behavior
**Added**: 2026-01-18
**Related files**: `crates/ac-service/src/handlers/internal_tokens.rs`

The handler uses `claims.scope.split_whitespace().collect()` to extract scopes from the JWT claim. This means:
- Empty string scope (`""`) results in empty Vec (not Vec with empty string)
- Whitespace-only scopes (`"   "`) also result in empty Vec
- Multiple spaces between scopes are handled correctly

Tests should verify empty scope rejection, not just "wrong scope" rejection. An empty token scope should fail authorization even if the handler has a default behavior.

---

## Gotcha: Multi-line Instrument Attributes and Guard False Positives
**Added**: 2026-01-28
**Related files**: `crates/ac-service/src/handlers/internal_tokens.rs`, `crates/ac-service/src/handlers/auth_handler.rs`

The `instrument-skip-all` guard uses grep to detect `#[instrument` without `skip_all` on the same line. When attributes span multiple lines, the guard sees `#[instrument(` on line N without `skip_all` (which appears on line N+2) and flags it as a violation. These are false positives. The code is correct if `skip_all` appears anywhere in the attribute. Manually verify flagged functions before making changes.

---

## Gotcha: Test Assertions for Dynamic Error Messages
**Added**: 2026-01-30
**Related files**: `crates/ac-service/src/crypto/mod.rs`

When error messages include underlying error context (e.g., `format!("Operation failed: {}", e)`), test assertions must use `starts_with()` not exact matching. The underlying library's error text may vary across versions:
```rust
// WRONG: Breaks when library error text changes
assert!(matches!(err, AcError::Crypto(msg) if msg == "Decryption failed"));

// CORRECT: Matches our prefix, ignores library suffix
assert!(matches!(err, AcError::Crypto(msg) if msg.starts_with("Decryption operation failed:")));
```
This pattern ensures tests verify our error handling logic without being fragile to dependency changes.

---

## Gotcha: client_id Considered Sensitive in Tracing
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

The semantic guard flags `client_id` in `#[instrument(fields(...))]` as a credential leak. While less sensitive than client_secret, client_id may reveal service identity in logs. Use `#[instrument(skip_all)]` without client_id in fields. Log client_id at trace level inside the function if needed for debugging.

---

## Gotcha: Clippy Rejects Assertions on Constants
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

`assert!(CONSTANT > 0)` triggers clippy `assertions_on_constants` because the assertion is evaluated at compile-time and optimized away. Use `assert_eq!(CONSTANT, expected_value, "descriptive message")` instead to verify constant values in tests without triggering the warning.

---

## Gotcha: SecretBox Requires Manual Clone Implementation
**Added**: 2026-02-10
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

Structs containing `SecretBox` fields cannot `#[derive(Clone)]` because SecretBox deliberately doesn't implement Clone automatically. Implement Clone manually by exposing, cloning, and re-wrapping: `SecretBox::new(Box::new(self.field.expose_secret().clone()))`. Same applies to custom Debug implementations for redaction.

---

## Gotcha: HMAC Hash Prefix Distinguishes Algorithm
**Added**: 2026-02-10
**Related files**: `crates/ac-service/src/observability/mod.rs`

Correlation hashes use `h:` prefix (e.g., `h:a1b2c3d4`) to distinguish HMAC-SHA256 from legacy SHA-256 hashes. The prefix prevents confusion when analyzing logs that may contain both hash types. Always include prefix when formatting hash output: `format!("h:{}", hex::encode(prefix))`.

---

## Gotcha: Metrics Middleware Must Be Outermost Layer
**Added**: 2026-02-10
**Related files**: `crates/ac-service/src/middleware/http_metrics.rs`, `crates/ac-service/src/routes/mod.rs`

HTTP metrics middleware must be applied as the outermost layer (last in code, first in execution) to capture framework-level errors (404, 405, 415) that never reach inner middleware or handlers. Apply via `.layer(middleware::from_fn(http_metrics_middleware))` AFTER all route definitions and BEFORE any application middleware.

---

## Gotcha: Watch Receiver Borrow Blocks Sender
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

`watch::Receiver::borrow()` holds a lock that blocks the sender from updating. Always clone immediately: `self.0.borrow().clone()`. Create a wrapper type (e.g., `TokenReceiver`) that enforces this pattern in its `token()` method, preventing callers from accidentally holding the borrow.

---
