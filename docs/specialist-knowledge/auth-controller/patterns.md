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
