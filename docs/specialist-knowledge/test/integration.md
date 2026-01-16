# Test Specialist - Integration Notes

Notes on test requirements for other specialists.

---

## For Security Specialist: Bcrypt Cost Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/config.rs`

When reviewing bcrypt or password hashing changes:
- Verify defense-in-depth validation exists (both config AND function level)
- Check cross-cost verification tests exist for migration scenarios
- Ensure cost factor is extracted from hash and asserted (not just "verify works")
- Test that hash format matches expected algorithm version (2b for bcrypt)

---

## For Database Specialist: Config Schema Changes
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

When adding configurable parameters:
- Request boundary tests (min, max, default)
- Request invalid input tests (wrong type, out of range, empty)
- Request constant assertion tests if adding new MIN/MAX/DEFAULT constants
- Consider if config value needs database storage (e.g., per-tenant settings)

---

## For Auth Controller Specialist: Handler Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

When adding new handlers:
- Include integration tests with `#[sqlx::test(migrations = "../../migrations")]`
- Test config propagation (e.g., bcrypt_cost flows from config to crypto layer)
- Test error paths return correct AcError variants
- Verify audit logs are emitted on both success and failure

---

## For Code Reviewer: Test Coverage Checklist
**Added**: 2026-01-11
**Related files**: All test files

When reviewing new tests, verify:
1. Boundary values tested (not just happy path)
2. Error messages checked for useful content
3. Security-critical constants have assertion tests
4. Integration tests verify end-to-end config propagation
5. Cross-version/migration scenarios covered where applicable

---

## For Operations Specialist: Performance Test Notes
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Bcrypt cost affects authentication latency:
- Cost 10: ~50ms
- Cost 12 (default): ~200ms
- Cost 14 (max): ~800ms

Include load tests that verify authentication latency SLOs with configured cost. Alert if latency spikes during cost increase rollout.

---

## Outstanding Test Gaps
**Added**: 2026-01-11

1. Warning log tests for low bcrypt_cost config (needs tracing-test)
2. Warning log tests for low clock_skew config (needs tracing-test)
3. TLS config warning tests (cfg(test) bypass prevents testing)
4. Performance regression tests for bcrypt at different costs

---

## For Security Specialist: SecretBox/SecretString Refactors
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/config.rs`, `crates/ac-service/src/models/mod.rs`

When reviewing SecretBox/SecretString refactors, verify tests cover:
1. **Debug redaction**: Test that `format!("{:?}", struct_with_secret)` contains `[REDACTED]` and NOT the actual value
2. **expose_secret() usage**: Tests must call `.expose_secret()` to access values - compiler enforces this
3. **Custom Clone impls**: If struct has `SecretBox` field, verify Clone test exists (SecretBox requires explicit handling)
4. **Custom Serialize impls**: If intentionally exposing secret in API response (e.g., one-time credential display), test and document this

---

## For All Specialists: Integration Test Module Inclusion
**Added**: 2026-01-12
**Related files**: `crates/*/tests/integration/mod.rs`

When adding new integration test files:
1. Create the test file (e.g., `clock_skew_tests.rs`)
2. **MUST add `mod clock_skew_tests;` to `mod.rs`** - without this, the file is never compiled!
3. Run `cargo test --package <crate> -- <test_name>` to verify tests execute
4. Check test count in output matches expected number of tests

Failure mode: Test file exists, looks correct, but 0 tests run. Silent failure.

---

## For Infrastructure Specialist: env-tests Cluster Requirements
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/*.rs`, `crates/env-tests/tests/*.rs`

env-tests require running cluster infrastructure:
- Kind cluster with AC service deployed
- Port-forwards active: AC (8082), Prometheus (9090), Grafana (3000), Loki (3100)
- kubectl in PATH and configured for cluster

CanaryPod tests additionally require:
- RBAC: pods.create/delete permissions in test namespaces
- Network connectivity between namespaces (for positive tests)
- NetworkPolicy deployed (for negative tests to validate blocking)

---

## For Security Specialist: JWT Security Test Coverage
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

Current JWT security test coverage in env-tests:
- **JWKS exposure**: Private key fields (d, p, q, dp, dq, qi) checked
- **Algorithm confusion**: Wrong algorithm rejected
- **Token tampering**: Payload modification detected
- **Header injection**: kid, jwk (CVE-2018-0114), jku validated
- **Time claims**: iat currentness, exp > iat, lifetime ~3600s

Missing (documented for future work):
- Expired token rejection (requires time manipulation or waiting)
- Token size limits (>8KB rejection)
- Audience validation edge cases

---

## For Global Controller Specialist: HTTP Endpoint Tests
**Added**: 2026-01-14
**Related files**: `crates/global-controller/tests/health_tests.rs`, `crates/gc-test-utils/src/server_harness.rs`

When testing GC HTTP endpoints:
1. Use `TestGcServer::spawn(pool)` for real HTTP server testing
2. Use `#[sqlx::test(migrations = "../../migrations")]` on each test for database isolation
3. Test response status codes AND response bodies (JSON structure)
4. Verify response headers (Content-Type, authentication headers, etc.)
5. Consider both happy path and error responses

Example test structure:
```rust
#[sqlx::test(migrations = "../../migrations")]
async fn test_endpoint(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestGcServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let response = client.get(&format!("{}/v1/health", server.url())).send().await?;

    // Check status
    assert_eq!(response.status(), 200);

    // Check content type
    assert!(response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.contains("application/json")));

    // Check body structure
    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["status"], "healthy");

    Ok(())
}
```

Database operations within tests:
- Use `server.pool()` to run assertions against test database
- No separate setup needed - TestGcServer uses the pool from sqlx::test

---

## For Code Reviewer: Error Type Coverage Checklist (GC)
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/errors.rs`

When reviewing GC error types, verify tests cover:
1. **Error Display**: Each error variant displays with appropriate message
2. **Status Code Mapping**: Each variant maps to correct HTTP status code
3. **IntoResponse Behavior**: Converts to correct status + JSON body
4. **Special Headers**: 401 responses include WWW-Authenticate header
5. **Message Sanitization**: Internal errors don't leak sensitive details to client

Example test pattern:
```rust
#[tokio::test]
async fn test_error_into_response() {
    let error = GcError::NotFound("Resource".to_string());
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = read_body_json(response.into_body()).await;
    assert_eq!(body["error"]["code"], "NOT_FOUND");
    assert_eq!(body["error"]["message"], "Resource");
}
```

All GC error conversions should be sync (not async) - use plain `#[test]` for Display/status_code, `#[tokio::test]` only for IntoResponse (due to body reading).

---

## For Security Specialist: JWT Validation Test Requirements
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`, `crates/global-controller/tests/auth_tests.rs`

When reviewing JWT validation code, verify test coverage includes:
1. **Token size limits**: Exact boundary tests (at limit, 1 byte over, far over)
2. **Algorithm validation**: alg:none, HS256, missing alg, plus correct EdDSA
3. **JWK structure validation**: kty check (must be "OKP"), alg check (must be "EdDSA"), required fields present
4. **iat (issued-at) validation**: With clock skew tolerance from config
5. **Time claim sanity**: exp > iat, lifetime reasonable (~3600s)
6. **Header injection**: kid, jwk, jku attacks
7. **JWKS caching**: Cache TTL respected, updates on new kid
8. **Error messages**: Generic "invalid or expired" - no info leak

Security-critical tests should be marked with descriptive names like `test_algorithm_confusion_attack_alg_hs256_rejected` so the vulnerability being tested is obvious.

---

## For Global Controller Specialist: Auth Middleware Integration
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/middleware/auth.rs`

When adding authentication middleware to new routes:
1. Understand the middleware layers: Extract Bearer token → Fetch JWK from cache → Validate signature → Extract claims
2. Each layer has different error modes:
   - No Authorization header = 401 (Unauthorized)
   - Invalid Bearer format = 401 (Unauthorized)
   - JWK not found = 500 (Internal) - JWKS endpoint unavailable
   - Invalid signature = 401 (Unauthorized) - attacker tampering
   - Invalid claims = 401 (Unauthorized) - expired, wrong aud, etc.
3. Test error paths don't expose internal details (use generic messages)
4. Test that claims are correctly extracted and available to handlers
5. Protected routes should use `middleware::from_fn_with_state(require_auth)` pattern

The middleware pattern with `from_fn_with_state` allows accessing shared state (config, JWKS client) while validating requests.

---

## For Implementation Teams: Test Coverage Requirements by Feature Type
**Added**: 2026-01-15
**Related files**: Code review process, test specialist role

Different feature types have different test coverage expectations:

**Security-critical features** (auth, crypto, validation):
- Minimum 90% code coverage (target 95%)
- All attack vectors tested explicitly (not just happy path)
- Boundary cases tested (off-by-one errors in size/count checks)
- Integration tests verify end-to-end flows
- Example: JWT validation requires token size limits, algorithm confusion, claim validation tests

**Core business logic** (user management, meetings, payments):
- Minimum 85% code coverage
- Happy path and error paths tested
- Database integration tested with real transactions
- Example: User registration requires happy path, validation failures, database persistence tests

**Infrastructure/utilities** (logging, config, metrics):
- Minimum 80% code coverage
- Config boundary values tested
- Error handling paths tested

**Code review standard**: "Missing integration tests for critical paths = BLOCKER". If a feature touches the database or calls external services, integration tests are required before approval.

---

## For Authentication/Authorization Work: User Provisioning Test Patterns
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/auth_tests.rs`, user management features

When implementing user provisioning features (registration, login, token issuance, claims management), ensure test coverage includes:

**Happy path tests** (1-2 tests):
- Register user → issue token → extract claims → verify expected fields

**Validation tests** (5-8 tests):
- Invalid username (too short, too long, invalid chars)
- Invalid email format
- Invalid password (too weak, empty, too long)
- Each validation should have its own test

**Rate limiting tests** (2-4 tests):
- N registrations within rate limit window succeeds
- (N+1)th registration within window fails with 429
- Window expiry allows next registration to succeed
- Per-IP or per-email rate limiting (depending on design)

**Timing attack prevention** (1 test):
- Registration duration constant regardless of validation failure
- Measures elapsed time for valid vs invalid username, ensures within tolerance

**Claims structure tests** (3-5 tests):
- All required fields present (sub, iat, exp, scopes)
- Optional fields handled correctly (service_type for service tokens, custom claims)
- Scopes serialized as JSON array
- Scope parsing/validation works (has_scope method or similar)

**Integration tests** (2-4 tests):
- Full registration flow with database persistence
- Login with credentials → correct token returned
- Token verification → correct claims extracted
- Multiple users → each gets correct claims

**Example test structure**:
```
tests/auth_tests.rs
├── Happy path
│   └── test_register_and_issue_token
├── Validation
│   ├── test_register_rejects_short_username
│   ├── test_register_rejects_long_username
│   ├── test_register_rejects_invalid_email
│   ├── test_register_rejects_weak_password
│   └── test_register_validates_all_fields_independently
├── Rate limiting
│   ├── test_register_rate_limiting_per_email
│   └── test_register_rate_limiting_per_ip
├── Timing
│   └── test_register_timing_constant_for_all_failures
├── Claims
│   ├── test_user_claims_structure_complete
│   ├── test_user_claims_scopes_parsed_correctly
│   └── test_user_claims_missing_required_field_rejected
└── Integration
    ├── test_register_persists_to_database
    ├── test_login_with_credentials
    └── test_token_claims_match_user_record
```

This structure ensures comprehensive coverage and can be adapted for service tokens, admin tokens, or other token types.
