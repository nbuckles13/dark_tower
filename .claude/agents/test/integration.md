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
