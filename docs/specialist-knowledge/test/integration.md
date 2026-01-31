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
**Added**: 2026-01-12, **Updated**: 2026-01-28
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/config.rs`, `crates/ac-service/src/models/mod.rs`, `crates/meeting-controller/src/actors/`

When reviewing SecretBox/SecretString refactors, verify tests cover:
1. **Debug redaction**: Test that `format!("{:?}", struct_with_secret)` contains `[REDACTED]` and NOT the actual value
2. **expose_secret() usage**: Tests must call `.expose_secret()` to access values - compiler enforces this
3. **Custom Clone impls**: If struct has `SecretBox` field, verify Clone test exists (SecretBox requires explicit handling)
4. **Custom Serialize impls**: If intentionally exposing secret in API response (e.g., one-time credential display), test and document this

**2026-01-28 update**: Phase 6c showed that type-level refactors (Vec<u8> → SecretBox<Vec<u8>>) are primarily compiler-verified. Test helper updates are mechanical (wrapping at construction, exposing at usage). No new test cases required - existing tests remain valid after type updates. This is a feature of SecretBox: transparent wrapping that preserves semantics while adding security properties. Verify only that:
- Compiler checks pass (all type mismatches resolved)
- All existing tests still execute (same count before/after)
- No new >1 second access patterns introduced (could block async contexts)

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

Database operations within tests:
- Use `server.pool()` to run assertions against test database
- No separate setup needed - TestGcServer uses the pool from sqlx::test

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
1. Understand the middleware layers: Extract Bearer token -> Fetch JWK from cache -> Validate signature -> Extract claims
2. Each layer has different error modes:
   - No Authorization header = 401 (Unauthorized)
   - Invalid Bearer format = 401 (Unauthorized)
   - JWK not found = 500 (Internal) - JWKS endpoint unavailable
   - Invalid signature = 401 (Unauthorized) - attacker tampering
   - Invalid claims = 401 (Unauthorized) - expired, wrong aud, etc.
3. Test error paths don't expose internal details (use generic messages)
4. Test that claims are correctly extracted and available to handlers
5. Protected routes should use `middleware::from_fn_with_state(require_auth)` pattern

---

## For Implementation Teams: Test Coverage Requirements by Feature Type
**Added**: 2026-01-15

Different feature types have different test coverage expectations:

**Security-critical features** (auth, crypto, validation):
- Minimum 90% code coverage (target 95%)
- All attack vectors tested explicitly (not just happy path)
- Boundary cases tested (off-by-one errors in size/count checks)
- Integration tests verify end-to-end flows

**Core business logic** (user management, meetings, payments):
- Minimum 85% code coverage
- Happy path and error paths tested
- Database integration tested with real transactions

**Infrastructure/utilities** (logging, config, metrics):
- Minimum 80% code coverage
- Config boundary values tested
- Error handling paths tested

**Code review standard**: "Missing integration tests for critical paths = BLOCKER". If a feature touches the database or calls external services, integration tests are required before approval.

---

## For Security Specialist: Error Body Sanitization in Test Clients
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`

When reviewing test client fixtures, verify error handling includes body sanitization. `GcClient` implements `sanitize_error_body()` which removes JWT patterns and Bearer tokens from error messages before storage. `AuthClient` does NOT have this yet - consider backporting. Sanitization catches credential leaks that custom Debug alone misses, especially in assertion output and error Display formatting.

---

## For Code Reviewer Specialist: Deferred Test Gaps
**Added**: 2026-01-23

When reviewing test coverage, some gaps are intentionally deferred rather than fixed:
- **sqlx error paths**: Functions using sqlx cannot easily test database error paths (no mock layer)
- **External service failures**: Testing behavior when AC/MC/MH returns errors requires test infrastructure
- **Race conditions**: Some timing-dependent tests are too flaky to include

When flagging test gaps during review:
1. Check if the gap is already documented (search for "Deferred:" in PR description)
2. If undocumented, ask implementer if it's intentional deferral or oversight
3. For legitimate deferrals, ensure they're tracked (tech debt file or comment)

Acceptable documentation formats:
- `// TODO: Error path test deferred - requires sqlx mocking (ADR-XXXX)`
- PR description: "Deferred: error path tests for run_cleanup() - requires sqlx mocking"
- Entry in `.claude/TODO.md` under test gaps section

---

## For Service Specialists: State Machine Transition Tests
**Added**: 2026-01-21, **Updated**: 2026-01-24
**Related files**: `crates/global-controller/tests/meeting_assignment_tests.rs`

When services track entity state (e.g., MC health, meeting status), test coverage must include state TRANSITIONS, not just happy-path states:

1. **Initial state behavior**: What happens when no state exists yet?
2. **State change handling**: When entity transitions (healthy → unhealthy), does the system respond correctly?
3. **Concurrent state access**: Multiple callers accessing during transition?
4. **Boundary states**: Degraded/transitional states between extremes (added 2026-01-24)

Example: MC assignment tests must cover:
- Assignment to healthy MC (happy path)
- Behavior when assigned MC becomes unhealthy (transition)
- Reassignment to different healthy MC after unhealthy transition
- **Behavior with Degraded health status** (boundary state - not healthy, not unhealthy)

The transition test pattern:
```rust
// 1. Create initial state (assignment to MC-1)
let first = assign_meeting(&pool, meeting_id).await?;

// 2. Trigger state transition (MC-1 becomes unhealthy)
update_mc_health(&pool, &first.mc_id, HealthStatus::Unhealthy).await?;

// 3. Verify system behavior after transition (reassigned to MC-2)
let second = assign_meeting(&pool, meeting_id).await?;
assert_ne!(second.mc_id, first.mc_id, "Should assign to different MC");

// 4. Verify database state consistency
let count = count_active_assignments(&pool, meeting_id).await?;
assert_eq!(count, 1, "Should have exactly one active assignment");
```

Without transition tests, bugs in failover logic remain hidden until production incidents.

---

## For All Specialists: Cross-Service Test Client Consistency
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`, `crates/env-tests/src/fixtures/auth_client.rs`

env-tests now has two service client fixtures: `AuthClient` (AC) and `GcClient` (GC). When adding new service clients (MC, MH), follow the established pattern:
1. Error enum with `HttpError`, `RequestFailed`, `JsonError` variants
2. Custom Debug on types with sensitive fields (tokens, captcha, subject IDs)
3. `sanitize_error_body()` for error response handling
4. `health_check()` method for availability detection
5. `raw_*` methods returning Response for error path testing

The `GcClient` pattern is more complete than `AuthClient` - use it as the reference.

---

## For Meeting Controller Specialist: Mock Infrastructure Testing
**Added**: 2026-01-25, **Updated**: 2026-01-25
**Related files**: `crates/mc-test-utils/src/mock_redis.rs`, `crates/meeting-controller/tests/session_actor_tests.rs`

When implementing MC features that depend on Redis (session binding, fencing tokens, nonce validation):

1. Use `MockRedis` for unit tests - no async complexity, immediate feedback
2. Builder pattern for test setup: `.with_session().with_fencing_generation()`
3. Test atomic operations explicitly:
   - Fencing validation: current generation, higher (ok), lower (rejected)
   - Nonce consumption: first use (ok), second use (NonceReused error)
   - Fenced writes: validate generation before write

**Phase 6b additions**:
- Actor lifecycle tests: spawn, shutdown, cancellation
- Session binding token validation: test EACH bound field independently (session_id, correlation_id, nonce)
- Time-based tests: use `#[tokio::test(start_paused = true)]` for grace period testing
- Host authorization: verify host-only operations reject non-host participants

**Phase 6c additions** (GC Integration):
- Lua script behavioral tests: 11 tests covering fencing logic (not just structural "script runs")
- Capacity/draining tests: 8 tests for atomic capacity checks and draining state
- Auth interceptor edge cases: empty header, malformed Bearer, case sensitivity
- gRPC retry/backoff: mixed success/failure sequences (not just all-succeed or all-fail)
- Error code exhaustive testing: every McError variant mapped to protocol codes
- **MockGcServer pattern**: 9 integration tests using configurable mock gRPC server (see `crates/meeting-controller/tests/gc_integration.rs`)
- **Heartbeat task testing**: 4 tests using `#[tokio::test(start_paused = true)]` with `tokio::time::advance()` for deterministic interval testing

**Phase 6c iteration 3-4 (Re-registration recovery)**:
- **MockBehavior enum**: Added 4 states (Accept, Reject, NotFound, NotFoundThenAccept) for modeling GC response patterns
- **Re-registration flow tests**: 2 tests covering recovery from lost GC state (attempt_reregistration success, full NOT_FOUND → re-register → heartbeat flow)
- **NOT_FOUND detection tests**: 2 tests verifying both fast and comprehensive heartbeats detect NOT_FOUND and return McError::NotRegistered
- **snapshot() method test**: Unit test for ControllerMetrics::snapshot() (caught gap where integration tests exercised method but didn't verify API)
- **McError::NotRegistered client_message test**: Explicit verification that NotRegistered doesn't leak internal details

Test count: **143 tests** in meeting-controller (126 unit + 13 integration + 4 heartbeat, up from 138 in round 2)

Review learnings:
- Round 1-2: Initial implementation with good unit test coverage but missing integration tests for MC-GC flow
- Round 3: Identified 4 test gaps (CRITICAL/MAJOR/MINOR) after iteration 3 added re-registration code
- Round 4: All gaps resolved via MockBehavior pattern + targeted unit tests

Tech debt noted in Phase 6a (now completed in Phase 6b):
- ~~TD-1: Integration tests for main binary~~ (completed: 64 actor tests)
- ~~TD-2: MockRedis async interface~~ (completed: async traits implemented)

Tech debt remaining (acceptable):
- TECH_DEBT-003: `run_gc_task` and `handle_heartbeat_error` in main.rs not directly testable (acceptable - comprehensive component coverage exists)

---
