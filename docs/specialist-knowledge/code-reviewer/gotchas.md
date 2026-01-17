# Code Reviewer - Gotchas

Common code smells and anti-patterns to watch for in Dark Tower codebase.

---

## Gotcha: Single-Layer Security Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto.rs`

If security parameters are only validated at config parse time, bugs or programmatic construction can bypass checks. Always validate at point of use too. Example: bcrypt cost should be checked both when loading config AND when hashing passwords.

---

## Gotcha: Magic Numbers Without Constants
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Security-critical numeric values (cost factors, timeouts, limits) should be defined as named constants with documentation, not inline literals. Bad: `if cost < 4`. Good: `if cost < BCRYPT_COST_MIN` with constant documenting why 4 is minimum.

---

## Gotcha: Missing Range Tests for Config Values
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Config validation tests should cover: below minimum (rejected), at minimum (accepted), default value (accepted), at maximum (accepted), above maximum (rejected). Missing boundary tests allow edge case bugs to slip through.

---

## Gotcha: Inconsistent Pattern Between Similar Features
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

When adding features similar to existing ones (e.g., bcrypt cost like JWT clock skew), verify exact pattern match: same constant naming, same validation approach, same test coverage style. Inconsistency creates maintenance burden and hides bugs.

---

## Gotcha: String Concatenation in SQL Queries
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/repository/`

Never use format!() or string concatenation for SQL. Always use sqlx compile-time checked queries with parameterized values. This is enforced by project convention and prevents SQL injection by design.

---

## Gotcha: Deriving Debug on Structs with SecretBox Fields
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

Do NOT use `#[derive(Debug)]` on structs containing `SecretBox<T>` or `SecretString`. While `SecretBox` itself redacts in Debug output, the struct's derived Debug may expose other sensitive context (like database URLs with credentials). Always implement Debug manually to control exactly what's shown. Look for structs with secret fields that derive Debug - they need manual impl.

---

## Gotcha: Missing Documentation on Custom Serialize for Secrets
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

When implementing custom `Serialize` that exposes a `SecretString` via `.expose_secret()`, always add a doc comment explaining this is intentional. Without documentation, future reviewers may flag it as a security bug. Pattern: `/// Custom Serialize that exposes client_secret for API response. This is intentional: [reason].`

---

## Gotcha: Forgetting Clone Impl When Using SecretBox
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`

`SecretBox<T>` does not derive `Clone` by design. If your struct needs Clone and contains SecretBox fields, you must implement Clone manually. Compiler error will catch this, but watch for workarounds like removing Clone requirement entirely - sometimes Clone is actually needed (e.g., Config shared across threads via Arc).

---

## Gotcha: Inconsistent Redaction Placeholder Strings
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/`

Use consistent `"[REDACTED]"` string across all Debug implementations. Inconsistent placeholders (e.g., `"***"`, `"<hidden>"`, `"[SECRET]"`) make log analysis harder and suggest incomplete refactoring. Grep for redaction patterns to verify consistency.

---

## Gotcha: Unused Struct Fields in Test Deserialization
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

When defining structs for deserializing test data (e.g., JWT claims), only include fields that are actually used in assertions. Unused fields like `aud: Option<Vec<String>>` add complexity without value. If the field might be used later, add a comment explaining why it's present. Otherwise, remove it.

---

## Gotcha: panic!() in Test Metrics Fallback
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/10_auth_smoke.rs`

Avoid `panic!()` in test fallback paths where infrastructure might be temporarily unavailable. Example: Metrics endpoint returning 500 should trigger a warning, not a test panic. Use `eprintln!()` for warnings or `assert!()` with helpful message. Reserve `panic!()` for true invariant violations.

---

## Gotcha: Magic Numbers for Timeouts in Test Infrastructure
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Timeouts and durations in test infrastructure should be named constants:
- Pod sleep duration: `3600` -> `const POD_SLEEP_SECONDS: u32 = 3600;`
- Wait timeout: `30` -> `const POD_READY_TIMEOUT_SECONDS: u32 = 30;`
- wget timeout: `"5"` -> `const WGET_TIMEOUT_SECONDS: &str = "5";`

This makes tuning easier and documents expected behavior.

---

## Gotcha: Synchronous Subprocess in Async Context
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Using `std::process::Command` (blocking) inside `async fn` works for test code but blocks the runtime thread. Document this limitation. For production code or long-running operations, use `tokio::process::Command`. Current CanaryPod implementation is acceptable because:
1. It's test infrastructure only
2. kubectl calls are short-lived
3. Tests run sequentially with `#[serial]`

---

## Gotcha: Confusing AppState Clone with Config Clone
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/routes/mod.rs`

Both `AppState` and `Config` need to implement `Clone` for Axum's `State` extractor. When testing that they're Clone, it's easy to accidentally test the same trait twice. Make sure trait bound tests are distinct:
```rust
#[test]
fn test_app_state_is_clone() {
    fn assert_clone<T: Clone>() {}
    assert_clone::<AppState>();  // Tests AppState specifically
}

#[test]
fn test_config_is_clone() {
    fn assert_clone<T: Clone>() {}
    assert_clone::<Config>();  // Tests Config specifically
}
```
These look similar but are essential distinct tests - AppState contains Config, but if Config loses Clone, AppState will fail to compile.

---

## Gotcha: Health Check HTTP 200 vs Error Status
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/handlers/health.rs`

Common mistake: returning an error status (500) when a health check probe fails. This causes the HTTP request to fail and the probe to timeout, which is worse than returning 200 with `"unhealthy"` status. K8s expects to parse the response body, so:
- BAD: `.map_err(|_| GcError::DatabaseUnavailable)` - probe fails
- GOOD: `let db_healthy = sqlx::query().await.is_ok()` then return 200 with status field

The probe should always succeed HTTP-wise; the body tells K8s the actual health state.

---

## Gotcha: AppState Clone Holding Arc References
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/routes/mod.rs`, `crates/gc-test-utils/src/server_harness.rs`

`AppState` can cheaply clone because it holds an `Arc<AppState>` when passed to Axum. However, individual fields (PgPool, Config) must individually support Clone. If a future field doesn't clone (e.g., `tokio::sync::Mutex` without Arc), the struct-level Clone becomes problematic. Always verify all fields are Clone before deriving it.

---

## Gotcha: UserClaims Struct Visibility and Debug Implementation
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/models/users.rs`

`UserClaims` is a private struct used internally for token validation. It should NOT derive `Debug` automatically - implement manually with redacted fields to prevent accidental logging of sensitive claims. If this struct becomes public in the future, ensure custom Debug is in place. Don't assume struct privacy eliminates the need for Debug redaction.

---

## Gotcha: Missing Validation in Middleware Before Injecting Context
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/middleware/org_context.rs`

Middleware that extracts and injects context (like `OrgContext`) must validate the data before attaching to extensions. If a handler calls `.unwrap()` or `.expect()` on an `Option<T>` extracted from extensions, middleware failure becomes a panic. Always ensure middleware either validates completely or returns error responses. Audit middleware for fallible operations that don't return errors properly.

---

## Gotcha: Confusing Service Layer vs Repository Layer Errors
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/service/`, `crates/ac-service/src/repository/`

Repository layer errors (e.g., `UserRepositoryError`) are internal implementation details. Service layer should wrap these in domain-specific errors (e.g., `UserServiceError`) that handlers understand. Don't leak repository errors through service layer - always map. Pattern: repository might return `DatabaseError::UniqueViolation`, service wraps in `UserServiceError::EmailAlreadyExists`.

---

## Gotcha: Handler Returning Wrong Error Type
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/handlers/auth_handler.rs`

Handlers should return the service layer error type directly, not wrap it again in another handler-specific error. The service error should implement `IntoResponse` to map to HTTP status codes at the boundary. Bad pattern: `handler -> HandlerError -> ServiceError`. Good pattern: `handler -> ServiceError` (which implements IntoResponse).

---

## Gotcha: Token Parsing in Middleware vs Handler
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/middleware/org_context.rs`

Don't implement token parsing logic in both middleware and handlers. Parse once in middleware, attach structured claims to extensions, handlers use pre-parsed data. If token validation happens in middleware, handlers should not re-validate - trust the middleware's extraction or return error. Duplicate parsing is error-prone and wastes cycles.

---

## Gotcha: Organization ID Type Safety
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/models/users.rs`, `crates/ac-service/src/middleware/org_context.rs`

When extracting organization ID from tokens or claims, verify the type matches expectations. If tokens embed `org_id` as a string UUID, make sure the type system enforces this (e.g., `newtype` wrapper like `OrganizationId(uuid::Uuid)` rather than raw `String`). This prevents accidental mixing of different ID namespaces.

---

## Gotcha: Duplicated JWT Decoding Logic in Tests
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

When tests verify JWT claims, the base64 decode + JSON parse pattern gets duplicated:
```rust
let parts: Vec<&str> = token.split('.').collect();
let payload_bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, parts[1])?;
let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)?;
```
This pattern appeared 8+ times in user_auth_tests.rs. Extract to a helper function in the test harness (e.g., `TestAuthServer::decode_jwt_payload(token: &str) -> Result<serde_json::Value, anyhow::Error>`). Reduces duplication and makes JWT format changes easier to maintain.

---

## Gotcha: Weak OR Assertion Logic in Rate Limiting Tests
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

Rate limiting tests that assert `hit_rate_limit || success_count <= N` can pass even if rate limiting is broken. The OR condition allows either branch to satisfy the assertion. Pattern seen:
```rust
assert!(hit_rate_limit || success_count <= 6, "Should hit rate limit...");
```
This passes if `success_count == 5` even without hitting rate limit. Better: assert that rate limit was actually hit (`assert!(hit_rate_limit, ...)`), or loop until confirmed. Weak assertions mask bugs.

---

## Gotcha: Implementation Details in Test Assertion Comments
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

Avoid exposing internal error mapping in test comments:
```rust
// BAD: Exposes internal error type
assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "Invalid email should return 401 (using InvalidToken error)");

// GOOD: Focus on observable behavior
assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "Invalid email should return 401 Unauthorized");
```
Comments like "using InvalidToken error" leak implementation details that tests shouldn't care about. Tests verify behavior, not internal error variants. If the internal mapping changes, these comments become misleading.
