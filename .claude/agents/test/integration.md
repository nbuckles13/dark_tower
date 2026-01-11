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
