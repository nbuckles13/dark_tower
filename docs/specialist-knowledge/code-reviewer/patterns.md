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
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

Validate security-critical values at multiple layers: config parsing time AND at point of use. Even if config validation ensures valid ranges, crypto functions should independently verify inputs. Prevents bugs if validation is bypassed or config is constructed programmatically.

---

## Pattern: OWASP/NIST Reference Documentation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Document security-critical constants with references to authoritative sources (OWASP, NIST). Example: bcrypt cost factor 12 references OWASP password storage cheat sheet. This provides audit trail and justification for security decisions.

---

## Pattern: SecretBox Custom Debug Implementation
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

When a struct contains `SecretBox<T>` fields, implement custom `Debug` using `f.debug_struct()` with `&"[REDACTED]"` for sensitive fields. This prevents accidental logging:
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
Document why Clone is needed in the struct doc comment.

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

## Pattern: Debugging-Friendly Assertion Messages
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`, `crates/env-tests/tests/40_resilience.rs`

Assertion messages should guide debugging, not just state what failed. Include: (1) what was expected, (2) what was received, (3) likely causes, (4) remediation steps. Example:
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

---

## Pattern: Test Isolation with #[serial] for Shared Resources
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`

Use `#[serial]` from `serial_test` crate when tests share mutable state (Kubernetes cluster, database, network resources). Prevents race conditions and flaky tests:
```rust
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_same_namespace_connectivity() { ... }
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
**Related files**: `crates/gc-test-utils/src/server_harness.rs`, `crates/ac-test-utils/src/server_harness.rs`

For integration testing, create a test harness that spawns a real service instance:
1. Use `Arc<AppState>` to wrap shared state (DB pool, config)
2. Spawn the HTTP server in background via `tokio::spawn()` and hold the `JoinHandle`
3. Return a wrapper struct (e.g., `TestGcServer`) that provides accessor methods (`url()`, `pool()`, `config()`)
4. Implement `Drop` to explicitly `abort()` the background task for immediate cleanup
5. Support test-specific config via `from_vars()` with sensible defaults

Benefits: Tests the full integration with real networking, real database pool, real middleware.

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

## Pattern: Repository Organization (repositories/ Directory)
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/repositories/`

When a service manages multiple domain entities (users, organizations, clients, etc.), organize repositories as separate files in `src/repositories/` directory, each scoped to one entity. Re-export all via `src/repositories/mod.rs` for convenience. Each file should contain only that entity's queries and error types. This improves discoverability and reduces file complexity.

---

## Pattern: Middleware for Organization Context Extraction
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/middleware/org_extraction.rs`

Use middleware to extract organization context from requests (subdomain, token claims, etc.) and attach to typed extensions. Handlers receive context via Axum's `Extension` extractor without manual extraction. Middleware should return error responses for invalid context, not panics. This centralizes org extraction logic across all handlers.

---

## Pattern: Integration Test Organization with Section Comments
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

Organize integration tests with clear section separators and category headers:
```rust
// ============================================================================
// Registration Tests (11 tests)
// ============================================================================

/// Test that valid registration returns user_id and access_token.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_happy_path(pool: PgPool) -> Result<(), anyhow::Error> { ... }
```
Benefits: Easy navigation in long test files, clear test count per category. Works well for files with 20+ tests.
