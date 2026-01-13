# Security Specialist Patterns

Security review patterns and best practices for the Dark Tower codebase.

---

## Pattern: Defense-in-Depth Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Security-critical functions should re-validate parameters even when callers are trusted. Example: `hash_client_secret()` checks bcrypt cost is within safe range despite config validation. Prevents misconfiguration if function called from unexpected paths.

---

## Pattern: Configurable Security with Safe Bounds
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Security parameters should be configurable but bounded. Pattern: MIN (security floor), DEFAULT (recommended), MAX (safety ceiling). Reject values outside range at startup. Warn on values below default but above MIN.

---

## Pattern: Constant-Time Error Responses
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/token_service.rs`

Authentication failures must return identical errors regardless of failure reason. Pattern: Run same crypto operations (dummy hash) even on non-existent users. Return generic "invalid credentials" for all failure paths.

---

## Pattern: Size-Before-Parse Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Check input sizes BEFORE expensive operations (base64 decode, signature verify, JSON parse). Prevents DoS via oversized inputs. Example: JWT 4KB limit checked before any parsing.

---

## Pattern: Log Security Events, Not Secrets
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Log security-relevant events (cost warnings, validation failures) but never secrets. Pattern: `warn!("bcrypt cost {} below recommended", cost)` logs fact, not the password being hashed.

---

## Pattern: Cryptographic Agility via Config
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Design crypto to accept algorithm/strength parameters from config. Enables future upgrades (bcrypt cost increase, algorithm migration) without code changes. Document recommended values per current standards (OWASP, NIST).

---

## Pattern: Security Review Checklist
**Added**: 2026-01-11
**Related files**: `.claude/agents/security.md`

When reviewing security code, check: (1) Timing attack vectors, (2) Error message information leakage, (3) Input validation at boundaries, (4) Crypto parameter bounds, (5) Key/secret handling, (6) Logging sanitization, (7) `#[instrument(skip_all)]` on crypto functions, (8) Custom Debug on secret-holding types.

---

## Pattern: Tracing-Safe Crypto Functions
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

All functions handling secrets MUST use `#[instrument(skip_all)]` to prevent tracing from capturing sensitive parameters in spans. Types holding crypto material need manual Debug impl with `[REDACTED]` fields, or use `secrecy::Secret<T>` wrapper. This is a MANDATORY check when reviewing any crypto-adjacent code.

---

## Pattern: SecretBox/SecretString for Compile-Time Secret Safety
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/models/mod.rs`

Use `SecretBox<T>` (binary data) and `SecretString` (text) from `secrecy` crate for all secrets. Benefits: (1) Debug auto-redacts as `[REDACTED]`, (2) `.expose_secret()` makes access explicit and grep-able, (3) Zeroization on drop. Use `SecretBox<Vec<u8>>` for keys, `SecretString` for passwords/tokens. Types with derived Debug that contain secrets automatically get safe logging.

---

## Pattern: Intentional Secret Exposure via Custom Serialize
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/models/mod.rs`, `crates/ac-service/src/handlers/admin_handler.rs`

For "one-time reveal" API responses (registration, secret rotation), implement custom `Serialize` that calls `.expose_secret()`. This is the ONLY place secrets should be exposed. Pattern: (1) Custom Debug that redacts, (2) Custom Serialize that exposes for API response, (3) Document as intentional in comments. Example: `RegisterServiceResponse`, `RotateSecretResponse`.

---

## Pattern: Custom Clone for SecretBox Types
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

`SecretBox<T>` doesn't derive Clone. For types containing SecretBox, implement Clone manually: `SecretBox::new(Box::new(self.field.expose_secret().clone()))`. This maintains secret protection on cloned values. Essential for structs like `Config` that may be cloned across threads.

---
