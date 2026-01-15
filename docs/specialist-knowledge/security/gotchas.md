# Security Specialist Gotchas

Security pitfalls, edge cases, and warnings discovered in the Dark Tower codebase.

---

## Gotcha: Bcrypt Library vs OWASP Requirements
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Bcrypt crate accepts cost 4-31, but OWASP 2024 requires minimum 10. Library validation is insufficient for compliance. Always enforce security-aware bounds in application code.

---

## Gotcha: Timing Attacks in Config-Time vs Request-Time
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Security parameters loaded at config time (startup) are safe from timing attacks. Parameters that vary per-request introduce timing side channels. Bcrypt cost is config-time, so no timing leak between requests.

---

## Gotcha: Dummy Hash Must Match Production Cost
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/token_service.rs`

Timing-safe authentication uses dummy hash for non-existent users. Dummy hash MUST use same cost factor as production. If default cost changes, regenerate dummy or timing attack possible.

---

## Gotcha: Clock Skew Creates Pre-Authentication Window
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

JWT `iat` validation with clock skew allows tokens up to N seconds in the future. 300s skew = 5 minute pre-generation window. Necessary for distributed systems but enables token pre-computation attacks.

---

## Gotcha: Error Messages Enable Enumeration
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/token_service.rs`

Different error messages for "user not found" vs "wrong password" enable account enumeration. Always return identical generic errors. Check all error paths return same message AND same HTTP status.

---

## Gotcha: DoS via Expensive Operations
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Bcrypt and signature verification are intentionally expensive. Without input limits, attackers can DoS by submitting large payloads. Always size-check before crypto operations.

---

## Gotcha: Test Coverage Hides Timing Issues
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/token_service.rs`

Coverage instrumentation adds overhead that masks timing differences. Timing-sensitive tests must be `#[cfg_attr(coverage, ignore)]`. Manual verification required for timing-critical code.

---

## Gotcha: Default Secrets in Test Configs
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Test configs may default to insecure values (zero keys, low costs). Ensure production deployment requires explicit secure configuration. Consider failing startup if secrets are default/zero.

---

## Gotcha: Crypto Functions MUST Use #[instrument(skip_all)]
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

All functions handling secrets (keys, tokens, passwords) MUST use `#[instrument(skip_all)]` to prevent accidental logging via tracing spans. **This is a mandatory review check for any code touching crypto.** Also: types holding crypto material need custom Debug impl with redaction, or use `secrecy` crate wrappers.

---

## Gotcha: SecretBox Doesn't Derive Clone
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

`SecretBox<T>` from `secrecy` crate doesn't implement `Clone` via derive. If your struct contains `SecretBox` and needs Clone, you must implement it manually with `SecretBox::new(Box::new(self.field.expose_secret().clone()))`. Forgetting this causes compile errors, but the fix pattern must maintain secret protection.

---

## Gotcha: Serde Serialize Bypasses SecretString Protection
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/models/mod.rs`

If you derive `Serialize` on a struct with `SecretString`, the default serialization will NOT expose the secret (it serializes the wrapper). For API responses that MUST return secrets (registration, rotation), implement custom `Serialize` with explicit `.expose_secret()`. Document this as intentional - it's the one place secrets should be exposed.

---

## Gotcha: grep for .expose_secret() During Reviews
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/`

Every call to `.expose_secret()` is a potential leak point. During security reviews, grep for all `.expose_secret()` calls and verify each is: (1) necessary for crypto operations, (2) intentional API exposure, or (3) test code. Any other usage is suspicious. This is the primary benefit of SecretBox - it makes secret access auditable.

---

## Gotcha: CVE-2018-0114 - Embedded JWK in JWT Header
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

Some JWT libraries trust public keys embedded in the token's `jwk` header parameter, allowing attackers to sign tokens with their own key. Always validate against keys from a trusted JWKS endpoint only, NEVER from the token header. Test by embedding a fake `jwk` in the header and verifying signature validation still uses the server's JWKS. Most modern libraries are safe, but this must be verified.

---

## Gotcha: SSRF via JWT jku Header
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

The `jku` (JWK Set URL) header tells the validator where to fetch public keys. If the validator follows this URL, attackers can: (1) exfiltrate internal data via SSRF, (2) serve their own keys to forge tokens. Never fetch keys from URLs specified in token headers. Test vectors should include: external URLs, internal services, localhost, cloud metadata endpoints (169.254.169.254, metadata.google.internal).

---

## Gotcha: Rate Limit Testing May Not Trigger
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/10_auth_smoke.rs`

Rate limit tests that send N requests expecting a 429 may not trigger if: (1) rate limits are per-IP and test runs through different IPs, (2) rate limit thresholds are very high, (3) rate limiting is per-client-id and test varies credentials. Consider checking metrics endpoints for rate limit counters as alternative validation. Log warnings rather than failing tests when rate limits don't trigger - different environments have different configurations.

---

## Gotcha: Clock Skew in Time-Based JWT Validation Tests
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

When testing `iat` (issued at) claims, account for clock skew between test runner and token issuer. Use a tolerance window (e.g., 300 seconds) when asserting timestamps. Record time before AND after token issuance to create a valid window. Don't assume clocks are synchronized - distributed systems rarely have perfect time agreement.

---

## Gotcha: Typed Deserialization May Miss JWKS Leakage
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

When testing JWKS endpoints for private key leakage, DON'T rely on typed deserialization alone. A struct without a `d` field will silently ignore `d` in the JSON. Use raw JSON (`serde_json::Value`) to check if forbidden fields exist. Pattern: `jwks_value.get("keys")[i].get("d").is_none()` catches fields that typed structs would skip.

---

## Gotcha: Application-Level Timeout Without Statement Timeout
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/main.rs`

Setting a request timeout in the application (e.g., `tower_http::TimeoutLayer`) is insufficient if the database connection lacks a statement timeout. Attackers can still hang the database connection, exhausting the connection pool. ALWAYS set `statement_timeout` at the PostgreSQL connection level. This creates two independent timeouts: (1) DB-level statement timeout prevents hung queries, (2) Application-level request timeout prevents hung handlers. Both are necessary.

---

## Gotcha: JWK Algorithm Mismatch Bypasses Token Validation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

While JWT token validation checks `alg: EdDSA` in the token header, the JWKS JWK object may have inconsistent `kty` or `alg` fields. If a JWK with `kty: RSA` and `alg: RS256` is returned by the JWKS endpoint, and the validator doesn't check JWK fields, an attacker could potentially trick the validator into using the wrong key for the wrong algorithm (though signature verification would still fail). The defense is double-layered: (1) Token must have `alg: EdDSA`, (2) JWK must have `kty: OKP` and `alg: EdDSA`. Check BOTH during verification, not just one. A misconfigured JWKS endpoint serving wrong key types could bypass token-level checks if JWK fields aren't validated.

---

## Gotcha: JWKS HTTP Responses Lack Size Limits
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwks.rs`

JWKS endpoints can be exploited to consume memory by returning extremely large responses (gigabytes of data). If JWKS client doesn't limit response size, an attacker can OOM the application. Pattern: Implement response size cap (e.g., 1MB) in HTTP client or JWKS parsing. Test with oversized responses. This is documented as Phase 3+ hardening but not implemented in Phase 2 - flag as minor tech debt.

---
