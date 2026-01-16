# Auth Controller Gotchas

Mistakes to avoid and edge cases discovered in the Auth Controller codebase.

---

## Gotcha: Bcrypt Cost vs Library Minimum
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Bcrypt library accepts cost 4-31, but OWASP requires minimum 10. Config enforces 10-14, but crypto function does not re-validate. Always pass config-validated cost.

---

## Gotcha: Clock Skew Creates Pre-generation Window
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

JWT `iat` validation uses clock skew. 300s skew means tokens 5 min in future are accepted. Necessary for distributed systems but creates pre-generation window. NIST recommends 5 min.

---

## Gotcha: #[cfg(test)] Imports in Production Files
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Files have `#[cfg(test)] use crate::config::{DEFAULT_BCRYPT_COST, ...}`. Intentional - tests need constants that production receives via Config. Do not remove.

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

## Gotcha: TLS Validation Skipped in Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

`validate_tls_config()` skips in test builds (tracing issues). TLS warnings only in production. Don't rely on tests for sslmode validation.

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

Meeting and guest tokens need unique `jti` (JWT ID) claims for tracking and revocation:
```rust
jti: uuid::Uuid::new_v4().to_string(),
```
Always include jti for tokens that may need revocation. Service tokens may omit jti if revocation not needed.

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

## Gotcha: Signing Function Not Reusable Across Claim Types
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/handlers/internal_tokens.rs`

Existing `crypto::sign_jwt()` expects `crypto::Claims`. Cannot reuse for different claim types. Created local signing functions for meeting and guest tokens.

**Future**: Generic signing function would need `impl Serialize` to handle different claim types (see TD-1 tech debt).
