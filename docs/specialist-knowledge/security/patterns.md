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

## Pattern: JWKS Private Key Field Validation
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

When testing JWKS endpoints for private key leakage, check for ALL private key fields that could be present: `d` (private key for RSA/EC/OKP), `p`, `q`, `dp`, `dq`, `qi` (RSA CRT parameters). Use raw JSON parsing rather than typed deserialization to catch any field that shouldn't be there. This validates CWE-321 (cryptographic key exposure).

---

## Pattern: JWT Header Injection Test Suite
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

When testing JWT header security, validate three attack surfaces: (1) `kid` injection - test path traversal (`../../etc/passwd`), SQL injection (`'; DROP TABLE--`), XSS, null bytes, header injection; (2) `jwk` embedding (CVE-2018-0114) - verify service ignores embedded public keys; (3) `jku` SSRF - test external URLs, internal URLs, file:// protocol, cloud metadata endpoints. All tests pass because JWT signatures cover the header - tampering invalidates signature.

---

## Pattern: Security Test via Signature Integrity
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

JWT header injection attacks are implicitly prevented by signature validation. When testing header injection (kid, jwk, jku), modifying the header invalidates the signature. This means testing "header injection rejection" is really testing "signature validation works correctly." Document this in test comments to avoid confusion - the protection mechanism is cryptographic, not input validation.

---

## Pattern: Subprocess Command Array for Shell Injection Prevention
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

When invoking external commands (kubectl, etc.) from Rust, use `Command::new("cmd").args([...])` with explicit argument arrays, NOT shell string concatenation. This prevents shell metacharacter injection. Even if namespace/input values are controlled, this pattern is defense-in-depth. Example: `Command::new("kubectl").args(["get", "pod", &name, &format!("--namespace={}", ns)])` - the namespace cannot break out of its argument position.

---

## Pattern: Query Timeout via Connection URL Parameters
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/main.rs`

Prevent hung queries and DoS attacks by setting database statement_timeout at connection time, not per-query. Pattern: append `?options=-c%20statement_timeout%3D{seconds}` to the PostgreSQL connection URL. This ensures ALL queries timeout after N seconds, preventing resource exhaustion. Combine with application-level request timeout (e.g., 30s via `tower_http::TimeoutLayer`) for defense-in-depth. Set timeout low enough (e.g., 5 seconds) to catch expensive operations, high enough for legitimate slow queries. Timeout value should be logged at startup for observability.

---
