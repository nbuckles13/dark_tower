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

## Pattern: Service Layer Receives Config Values
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/registration_service.rs`

Service functions receive config values as parameters, not Config struct:
```rust
pub async fn register_service(pool, service_type, region, bcrypt_cost) -> Result<...>
```
Handlers extract from AppState.config and pass down.

---

## Pattern: Boundary Tests for Config Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Config tests cover: default value, custom valid, min boundary, max boundary, below min (rejected), above max (rejected), zero/negative, non-numeric, float, empty string, all valid range (loop), constants relationship (MIN <= DEFAULT <= MAX).

---

## Pattern: AppState for Handler Dependencies
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/handlers/auth_handler.rs`

Handlers use Axum State extractor with Arc<AppState> containing pool and config. Access as `state.config.bcrypt_cost`.

---

## Pattern: Test Helper for Config Construction
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

Tests use `test_config()` helper with minimal valid Config from HashMap. Provides zero master key and localhost DATABASE_URL.

---

## Pattern: JWT Claims Extension for Multiple Token Types
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/handlers/internal_tokens.rs`

When different token types need different claims, create dedicated claim structs rather than overloading a single Claims type:
```rust
struct MeetingTokenClaims {
    sub: String,
    token_type: String,  // Discriminator field
    meeting_id: String,
    // meeting-specific fields
}

struct GuestTokenClaims {
    sub: String,
    token_type: String,
    display_name: String,
    waiting_room: bool,
    // guest-specific fields
}
```
Type-safe approach prevents mixing incompatible fields.

---

## Pattern: Scope Validation at Handler Level
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/handlers/internal_tokens.rs`

Validate scopes in handler rather than middleware for endpoint-specific authorization:
```rust
let token_scopes: Vec<&str> = claims.scope.split_whitespace().collect();
if !token_scopes.contains(&REQUIRED_SCOPE) {
    return Err(AcError::InsufficientScope { ... });
}
```
Separates authentication (middleware) from authorization (handler). More flexible than per-scope middleware.

---

## Pattern: Middleware for Claims Injection
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/middleware/auth.rs`

Authentication middleware validates token and injects claims without checking specific scopes:
```rust
pub async fn require_service_auth(...) -> Result<impl IntoResponse, AcError> {
    // Validate token
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}
```
Handler extracts claims via `Extension<crypto::Claims>`. Decouples authentication from authorization.

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

## Pattern: UserClaims with Custom Debug for Sensitive Field Redaction
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/crypto/mod.rs`

User token claims contain sensitive fields (email, roles). Implement custom `Debug` trait to redact sensitive data in logs while preserving debuggability for non-sensitive fields:
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

## Pattern: Repository Functions for Domain Entity Lookups
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/repositories/users.rs`, `crates/ac-service/src/repositories/organizations.rs`

Create dedicated repository modules for each domain entity with focused query functions: `get_user_by_email()`, `get_organization_by_subdomain()`. Keeps database access isolated from business logic. Each function handles one query with compile-time sqlx verification. Handlers and services compose these primitives rather than embedding SQL.

---

## Pattern: Auto-Login on Registration
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/handlers/auth_handler.rs`, `crates/ac-service/src/services/user_service.rs`

Issue JWT token immediately after successful user registration in a single transaction. Eliminates need for separate login call, improves UX, and reduces round trips. Registration handler creates user, then calls token issuance with the newly created user record. Response includes both user info and access token.
