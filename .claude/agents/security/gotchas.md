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
