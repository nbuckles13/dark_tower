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

## Pattern: JWKS Private Key Field Validation
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

When testing JWKS endpoints for private key leakage, check for ALL private key fields that could be present: `d` (private key for RSA/EC/OKP), `p`, `q`, `dp`, `dq`, `qi` (RSA CRT parameters). Use raw JSON parsing rather than typed deserialization to catch any field that shouldn't be there. This validates CWE-321 (cryptographic key exposure).

---

## Pattern: Query Timeout via Connection URL Parameters
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/main.rs`

Prevent hung queries and DoS attacks by setting database statement_timeout at connection time, not per-query. Pattern: append `?options=-c%20statement_timeout%3D{seconds}` to the PostgreSQL connection URL. This ensures ALL queries timeout after N seconds, preventing resource exhaustion. Combine with application-level request timeout for defense-in-depth. Set timeout low enough (e.g., 5 seconds) to catch expensive operations, high enough for legitimate slow queries.

---

## Pattern: JWK Field Validation as Defense-in-Depth
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

JWT validation includes algorithm pinning (token must have `alg: EdDSA`), but defense-in-depth also requires validating JWK fields: (1) `kty` (key type) must be `"OKP"` (Octet Key Pair) for Ed25519 keys, (2) `alg` field in JWK, if present, must be `"EdDSA"`. This prevents accepting keys from wrong cryptosystems. Pattern: Validate JWK fields at start of token verification before any crypto operations.

---

## Pattern: Error Body Sanitization for Credential Protection
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`

HTTP error responses can contain credentials (JWTs in error messages, Bearer tokens in auth headers). Sanitize error bodies at capture time using regex pattern matching:
1. JWT pattern: `eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+` -> `[JWT_REDACTED]`
2. Bearer pattern: `Bearer\s+eyJ[A-Za-z0-9_-]+` -> `[BEARER_REDACTED]`
3. Truncate long bodies (>256 chars) to limit info disclosure

This provides defense-in-depth beyond custom Debug implementations, catching credentials in assertion output, Display formatting, and log messages.

---

## Pattern: External Resource Registration Validation
**Added**: 2026-01-24
**Related files**: `crates/global-controller/src/services/media_handler_registry.rs`

When services register external resources (handlers, endpoints, callback URLs), validate both identifier format AND URL security:

1. **Identifier format validation**: Use allowlist regex patterns (e.g., `^[a-zA-Z0-9_-]+$` for handler IDs). Reject inputs with path traversal, null bytes, or injection characters. Short max lengths (64-128 chars) prevent DoS via long identifiers.

2. **Endpoint URL validation**: Require HTTPS scheme (reject HTTP, FTP, file://). Validate URL parsability. Consider allowlisting domains/IP ranges for internal services. Reject localhost/127.0.0.1 in production to prevent SSRF to internal services.

This pattern applies to: Media Handler registration, webhook callbacks, federation endpoints, any user-supplied URLs stored for later use.

---
