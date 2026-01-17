# Security Specialist Gotchas

Security pitfalls, edge cases, and warnings discovered in the Dark Tower codebase.

---

## Gotcha: Bcrypt Library vs OWASP Requirements
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Bcrypt crate accepts cost 4-31, but OWASP 2024 requires minimum 10. Library validation is insufficient for compliance. Always enforce security-aware bounds in application code.

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

## Gotcha: Test Coverage Hides Timing Issues
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/token_service.rs`

Coverage instrumentation adds overhead that masks timing differences. Timing-sensitive tests must be `#[cfg_attr(coverage, ignore)]`. Manual verification required for timing-critical code.

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

Some JWT libraries trust public keys embedded in the token's `jwk` header parameter, allowing attackers to sign tokens with their own key. Always validate against keys from a trusted JWKS endpoint only, NEVER from the token header. Test by embedding a fake `jwk` in the header and verifying signature validation still uses the server's JWKS.

---

## Gotcha: SSRF via JWT jku Header
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

The `jku` (JWK Set URL) header tells the validator where to fetch public keys. If the validator follows this URL, attackers can: (1) exfiltrate internal data via SSRF, (2) serve their own keys to forge tokens. Never fetch keys from URLs specified in token headers. Test vectors should include: external URLs, internal services, localhost, cloud metadata endpoints (169.254.169.254, metadata.google.internal).

---

## Gotcha: Rate Limit Testing May Not Trigger
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/10_auth_smoke.rs`

Rate limit tests that send N requests expecting a 429 may not trigger if: (1) rate limits are per-IP and test runs through different IPs, (2) rate limit thresholds are very high, (3) rate limiting is per-client-id and test varies credentials. Consider checking metrics endpoints for rate limit counters as alternative validation.

---

## Gotcha: Typed Deserialization May Miss JWKS Leakage
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

When testing JWKS endpoints for private key leakage, DON'T rely on typed deserialization alone. A struct without a `d` field will silently ignore `d` in the JSON. Use raw JSON (`serde_json::Value`) to check if forbidden fields exist. Pattern: `jwks_value.get("keys")[i].get("d").is_none()` catches fields that typed structs would skip.

---
