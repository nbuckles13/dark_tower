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
