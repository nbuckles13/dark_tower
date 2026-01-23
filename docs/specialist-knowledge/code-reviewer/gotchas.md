# Code Reviewer - Gotchas

Common code smells and anti-patterns to watch for in Dark Tower codebase.

---

## Gotcha: Single-Layer Security Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

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

## Gotcha: Health Check HTTP 200 vs Error Status
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/handlers/health.rs`

Common mistake: returning an error status (500) when a health check probe fails. This causes the HTTP request to fail and the probe to timeout, which is worse than returning 200 with `"unhealthy"` status. K8s expects to parse the response body, so:
- BAD: `.map_err(|_| GcError::DatabaseUnavailable)` - probe fails
- GOOD: `let db_healthy = sqlx::query().await.is_ok()` then return 200 with status field

The probe should always succeed HTTP-wise; the body tells K8s the actual health state.

---

## Gotcha: Confusing Service Layer vs Repository Layer Errors
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/services/`, `crates/ac-service/src/repositories/`

Repository layer errors are internal implementation details. Service layer should wrap these in domain-specific errors that handlers understand. Don't leak repository errors through service layer - always map. Pattern: repository might return `DatabaseError::UniqueViolation`, service wraps in `UserServiceError::EmailAlreadyExists`.

---

## Gotcha: Token Parsing in Middleware vs Handler
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/middleware/org_extraction.rs`

Don't implement token parsing logic in both middleware and handlers. Parse once in middleware, attach structured claims to extensions, handlers use pre-parsed data. Duplicate parsing is error-prone and wastes cycles.

---

## Gotcha: Weak OR Assertion Logic in Rate Limiting Tests
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

Rate limiting tests that assert `hit_rate_limit || success_count <= N` can pass even if rate limiting is broken. The OR condition allows either branch to satisfy the assertion. Better: assert that rate limit was actually hit (`assert!(hit_rate_limit, ...)`), or loop until confirmed. Weak assertions mask bugs.

---

## Gotcha: Duplicated JWT Decoding Logic in Tests
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

When tests verify JWT claims, the base64 decode + JSON parse pattern gets duplicated. Extract to a helper function in the test harness (e.g., `decode_jwt_payload(token: &str) -> Result<serde_json::Value, anyhow::Error>`). Reduces duplication and makes JWT format changes easier to maintain.

---

## Gotcha: Improvements in New Code That Should Be Backported
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`, `crates/env-tests/src/fixtures/auth_client.rs`

When reviewing new code that follows an existing pattern but adds improvements, flag the improvement for backporting. Example: `GcClient` added `sanitize_error_body()` which `AuthClient` lacks. Don't block the review, but document as a suggestion or add to TODO.md. Pattern: "GcClient adds [feature] which AuthClient does not have - consider backporting."

---

## Gotcha: #[expect(dead_code)] vs #[allow(dead_code)]
**Added**: 2026-01-22
**Related files**: `crates/global-controller/src/`

Use `#[allow(dead_code)]` with a comment explaining future use, not `#[expect(dead_code)]`. The `#[expect]` attribute causes "unfulfilled lint expectation" warnings when the code is actually used (e.g., by tests or future features). This creates noise in CI and can mask real warnings.

```rust
// BAD: Causes warning when tests use this code
#[expect(dead_code)]
fn future_feature() { ... }

// GOOD: Silent suppression with documentation
#[allow(dead_code)] // Used by MC registration (Phase 6)
fn future_feature() { ... }
```

This is particularly common when scaffolding code for future phases or writing repository methods ahead of their service-layer callers.

---

## Gotcha: Duplicate Logging Between Repository and Service Layers
**Added**: 2026-01-22
**Related files**: `crates/global-controller/src/repositories/`, `crates/global-controller/src/services/`

Logging the same operation at both repository and service layers creates duplicate log entries that clutter observability and make debugging harder. Choose ONE layer for logging:

- **Repository layer**: Log database-specific details (query timing, row counts)
- **Service layer**: Log business operations (user actions, workflow steps)

Typically prefer service layer logging because it captures business context. Repository layer should only log if there's database-specific diagnostic value not available at service layer.

```rust
// BAD: Both layers log the same operation
// In repository:
tracing::info!("Assigning meeting {} to MC {}", meeting_id, mc_id);
// In service:
tracing::info!("Assigned meeting {} to MC {}", meeting_id, mc_id);

// GOOD: Service layer only (has business context)
// In repository: (no logging)
// In service:
tracing::info!(meeting_id = %meeting_id, mc_id = %mc_id, "Meeting assigned to MC");
```
