# Code Reviewer - Patterns

Reusable code quality patterns observed in Dark Tower codebase.

---

## Pattern: Configuration Value Pattern (Constants + Field + Parsing + Tests)
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

When adding configurable security parameters (e.g., bcrypt cost, JWT clock skew), follow the four-part pattern: (1) define constants with DEFAULT/MIN/MAX bounds, (2) add config struct field with serde defaults, (3) implement parsing with range validation, (4) add comprehensive tests. This ensures consistency and makes security boundaries explicit.

---

## Pattern: Defense-in-Depth Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto.rs`

Validate security-critical values at multiple layers: config parsing time AND at point of use. Even if config validation ensures valid ranges, crypto functions should independently verify inputs. Prevents bugs if validation is bypassed or config is constructed programmatically.

---

## Pattern: OWASP/NIST Reference Documentation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Document security-critical constants with references to authoritative sources (OWASP, NIST). Example: bcrypt cost factor 12 references OWASP password storage cheat sheet. This provides audit trail and justification for security decisions.

---

## Pattern: No Panic Production Code (ADR-0002)
**Added**: 2026-01-11
**Related files**: `docs/decisions/adr-0002-no-panic-policy.md`

All production code uses `Result<T, E>` for fallible operations. The `.unwrap()`, `.expect()`, and `panic!()` are only allowed in: test code, truly unreachable invariants with proof comments, and development tools. Grep for these patterns during review.

---

## Pattern: SecretBox Custom Debug Implementation
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

When a struct contains `SecretBox<T>` fields, implement custom `Debug` using `f.debug_struct()` with `&"[REDACTED]"` for sensitive fields. This is idiomatic Rust and prevents accidental logging:
```rust
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"[REDACTED]")
            .field("bind_address", &self.bind_address)
            .field("master_key", &"[REDACTED]")
            .finish()
    }
}
```
Document which fields are redacted and why in the doc comment above the impl.

---

## Pattern: SecretBox Custom Clone Implementation
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

`SecretBox<T>` intentionally does not implement `Clone` to prevent accidental secret duplication. When cloning is required, implement manually:
```rust
impl Clone for Config {
    fn clone(&self) -> Self {
        Self {
            master_key: SecretBox::new(Box::new(self.master_key.expose_secret().clone())),
            // ... other fields
        }
    }
}
```
Document why Clone is needed in the struct doc comment (e.g., "Clone is manually implemented since SecretBox requires explicit cloning").

---

## Pattern: SecretString Serialize for One-Time Exposure
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/models/mod.rs`, `crates/ac-service/src/handlers/admin_handler.rs`

For API responses that must expose a secret exactly once (e.g., client_secret at registration), implement custom `Serialize` that calls `.expose_secret()`:
```rust
impl Serialize for CreateClientResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("CreateClientResponse", 3)?;
        state.serialize_field("client_id", &self.client_id)?;
        state.serialize_field("client_secret", self.client_secret.expose_secret())?;
        state.end()
    }
}
```
CRITICAL: Add doc comment stating "This is intentional: the [response type] is the ONLY time the plaintext [secret] is shown to the user."

---

## Pattern: Manual Trait Impl Threshold
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

Manual trait implementations (Debug, Clone, Serialize) are acceptable for up to ~5 similar types. Beyond that, consider a derive macro. Current examples: `RegisterServiceResponse`, `CreateClientResponse`, `RotateSecretResponse` all follow the same pattern - acceptable as 3 types. If pattern proliferates, create `#[derive(SecretSerialize)]` or similar.

---

## Pattern: Debugging-Friendly Assertion Messages
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`, `crates/env-tests/tests/40_resilience.rs`

Assertion messages should guide debugging, not just state what failed. Include: (1) what was expected, (2) what was received, (3) likely causes, (4) remediation steps. Example from NetworkPolicy test:
```rust
assert!(
    can_reach,
    "Canary pod in dark-tower namespace should be able to reach AC service at {}. \
     If this fails, check: 1) AC service is running, 2) Service DNS resolves, \
     3) No NetworkPolicy blocking same-namespace traffic.",
    target_url
);
```

---

## Pattern: CVE/CWE Reference Documentation in Security Tests
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

Document security tests with specific CVE/CWE references in doc comments. This provides audit trail and helps future reviewers understand the threat being mitigated. Example:
```rust
/// Test that tokens with embedded 'jwk' header are rejected (CVE-2018-0114).
/// An attacker should not be able to embed their own key in the token header.
#[tokio::test]
async fn test_jwk_header_injection_rejected() { ... }
```
Common references: CVE-2018-0114 (JWT embedded key), CWE-321 (hardcoded crypto key), CWE-89 (SQL injection).

---

## Pattern: Test Isolation with #[serial] for Shared Resources
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`

Use `#[serial]` from `serial_test` crate when tests share mutable state (Kubernetes cluster, database, network resources). Prevents race conditions and flaky tests. Apply to all tests in a file that touch the same resource:
```rust
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_same_namespace_connectivity() { ... }

#[tokio::test]
#[serial]
async fn test_network_policy_blocks_cross_namespace() { ... }
```

---

## Pattern: Feature-Gated Test Organization
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/*.rs`

Organize tests by feature flag based on infrastructure requirements:
- `#![cfg(feature = "smoke")]` - Basic tests requiring minimal infrastructure
- `#![cfg(feature = "flows")]` - Full integration tests requiring deployed services
- `#![cfg(feature = "resilience")]` - Chaos/resilience tests requiring cluster access

Each test file declares exactly one feature gate at the module level. This prevents accidental execution in `cargo test --workspace` while enabling selective test runs in CI.

---

## Pattern: Test Harness with Real Server Instance (Arc + JoinHandle)
**Added**: 2026-01-14
**Related files**: `crates/gc-test-utils/src/server_harness.rs`

For integration testing, create a test harness that spawns a real service instance:
1. Use `Arc<AppState>` to wrap shared state (DB pool, config)
2. Spawn the HTTP server in background via `tokio::spawn()` and hold the `JoinHandle`
3. Return a wrapper struct (e.g., `TestGcServer`) that provides accessor methods (`url()`, `pool()`, `config()`)
4. Implement `Drop` to explicitly `abort()` the background task for immediate cleanup
5. Support test-specific config via `from_vars()` with sensible defaults
6. Document in examples and comprehensive self-test that verifies spawning works

Benefits: Tests the full integration with real networking, real database pool, real middleware (tracing, timeouts).

---

## Pattern: Module-Level Architecture Documentation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/lib.rs`

Document the overall service architecture in the library crate's module doc comment:
1. State what the service does (meeting management, API gateway, etc.)
2. Explain the responsibility breakdown (config, handlers, models, routes)
3. Show the data flow diagram (routes -> handlers -> services -> repositories)
4. List all modules with brief descriptions
5. This becomes the single source of truth for how the crate is organized

This helps new contributors quickly understand the codebase structure without hunting through files.

---

## Pattern: Health Check That Reports Status Without Erroring
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/handlers/health.rs`

Health checks for Kubernetes readiness/liveness probes should:
1. Always return HTTP 200 (no error status)
2. Include a `status` field ("healthy" or "unhealthy") in response body
3. Include probe-specific sub-statuses (e.g., `database` field)
4. Ping critical dependencies (database, cache) but don't fail the request if they're down
5. Use `is_ok()` for probe calls, not `?` operator
6. Let K8s interpret the response body to make routing decisions

This allows K8s to see unhealthy services and stop routing traffic, but doesn't cause the probe to timeout.

---

## Pattern: Repository Organization for Multiple Domain Entities
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/repository/users.rs`, `crates/ac-service/src/repository/organizations.rs`

When a service manages multiple domain entities (users, organizations, clients, etc.), organize repositories as separate files in `src/repository/` directory, each scoped to one entity. Re-export all via `src/repository/mod.rs` for convenience. Each file should contain only that entity's queries and error types. This improves discoverability, reduces file complexity, and makes dependency relationships clear. Pattern is already established in AC service and extends well to GC.

---

## Pattern: Custom Debug Implementation for Sensitive Request/Response Types
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/models/users.rs`, `crates/ac-service/src/handlers/auth_handler.rs`

Request and response types containing sensitive data (passwords, tokens, claims) should implement custom `Debug` to redact sensitive fields. Unlike Config structs, these typically don't contain `SecretBox` (which auto-redacts), so manual Debug is essential:
```rust
impl fmt::Debug for RegisterUserRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisterUserRequest")
            .field("email", &self.email)
            .field("password", &"[REDACTED]")
            .finish()
    }
}
```
This prevents accidental credential leaks in logs when requests are formatted for debugging.

---

## Pattern: Middleware for Request Context Injection
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/middleware/org_context.rs`

Use middleware to extract and attach request context (organization ID, user claims, request ID) to typed extensions. This allows handlers to receive context via Axum's `Extension` extractor without manual extraction in each handler. Pattern:
1. Define a middleware function that parses tokens/headers and extracts context
2. Attach context to `request.extensions_mut()`
3. Handlers receive context via `Extension(OrgContext)` extractor
4. Middleware should return error responses for invalid context, not panics
5. Document what context is attached and when in the middleware module doc comment

This centralizes context extraction logic and makes handler signatures clearer about their dependencies.

---

## Pattern: ServiceError Wrapping for Handler Results
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/handlers/`, `crates/ac-service/src/error.rs`

Handlers that call service layer functions should use `?` operator with service layer error types (e.g., `UserServiceError`), not create new domain errors. The service layer defines domain-specific error variants (UserNotFound, InvalidPassword, etc.), and handlers map these to HTTP responses at the boundary. This preserves error context and makes error handling policy centralized. Pattern:
```rust
pub async fn get_user(
    Extension(OrgContext { user_id, .. }): Extension<OrgContext>,
    Extension(pool): Extension<PgPool>,
) -> Result<Json<UserResponse>, UserServiceError> {
    let user = user_service::get_user(user_id, &pool).await?;
    Ok(Json(UserResponse::from(user)))
}
```
The service layer returns `UserServiceError` which implements `IntoResponse` to map to HTTP status codes.

---

## Pattern: Integration Test Organization with Section Comments
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

Organize integration tests with clear section separators and category headers when testing related flows:
```rust
// ============================================================================
// Registration Tests (11 tests)
// ============================================================================

/// Test that valid registration returns user_id and access_token.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_happy_path(pool: PgPool) -> Result<(), anyhow::Error> { ... }
```
Benefits: (1) Easy navigation in long test files, (2) Clear test count per category, (3) Module-level doc comments explain test organization. This pattern works well for files with 20+ tests.

---

## Pattern: Subdomain-Based Host Header Testing
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`, `crates/ac-test-utils/src/server_harness.rs`

For multi-tenant systems using subdomain-based org extraction, provide a test helper that constructs Host headers:
```rust
impl TestAuthServer {
    pub fn host_header(&self, subdomain: &str) -> String {
        format!("{}.localhost:{}", subdomain, self.addr().port())
    }
}
```
Tests then use: `.header("Host", server.host_header("acme"))`. This centralizes host header construction and makes tests resilient to port changes. Also test edge cases: IP addresses (rejected), uppercase (rejected), unknown subdomains (404).

---

## Pattern: Underscore Prefix for Intentionally Unused Variables
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

When test setup creates data that isn't directly used but is necessary for the test scenario (e.g., creating an org for subdomain extraction), use underscore prefix to silence compiler warnings:
```rust
let _org_id = server.create_test_org("acme", "Acme Corp").await?;
```
This is idiomatic Rust and signals intentional non-use. Consider adding a brief comment when the reason isn't obvious. Better than `#[allow(unused)]` because it's explicit at the binding site.

---

## Pattern: Consistent Error Code Assertions in API Tests
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

When testing error responses, assert both HTTP status code AND the application error code:
```rust
assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
let body: serde_json::Value = response.json().await?;
assert_eq!(
    body["error"]["code"].as_str(),
    Some("INVALID_CREDENTIALS"),
    "Error code should be INVALID_CREDENTIALS"
);
```
This catches cases where the status is correct but the error body is wrong. Also verify error messages contain expected keywords using `.contains()` assertions.
