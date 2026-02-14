# Test Specialist - Integration Notes

Cross-specialist test requirements for Dark Tower.

---

## For Security Specialist: Crypto Test Coverage
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Verify: (1) defense-in-depth validation (config + function), (2) cross-version tests for migrations, (3) cost factor extracted and asserted, (4) hash format matches algorithm version.

---

## For Database Specialist: Config Schema Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Request: boundary tests (min, max, default), invalid input tests, constant assertion tests for new MIN/MAX/DEFAULT. Consider if config needs database storage.

---

## For Auth Controller Specialist: Handler Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

Require: `#[sqlx::test(migrations)]`, config propagation tests, error path variant tests, audit logs on success and failure.

---

## For Operations Specialist: Performance Benchmarks
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Bcrypt cost affects latency: cost 10 (~50ms), 12 (~200ms), 14 (~800ms). Load tests for SLO compliance, alerts for cost increase rollouts.

---

## For Security Specialist: SecretBox Refactor Tests
**Added**: 2026-01-12, **Updated**: 2026-01-28
**Related files**: `crates/ac-service/src/crypto/mod.rs`

SecretBox refactors are compiler-verified. Verify: (1) compiler passes, (2) same test count, (3) no >1s access patterns blocking async. Test updates mechanical, no new cases needed. Debug redaction and Clone tests still valuable.

---

## For All Specialists: Test Module Inclusion in mod.rs
**Added**: 2026-01-12
**Related files**: `crates/*/tests/integration/mod.rs`

New test files MUST be added to mod.rs or never compiled. Verify tests execute and count matches expected. Silent failure mode.

---

## For Infrastructure Specialist: env-tests Cluster Setup
**Added**: 2026-01-13
**Related files**: `crates/env-tests/`

Requires: Kind cluster, port-forwards (AC, Prometheus, Grafana, Loki), kubectl configured. CanaryPod additionally needs RBAC (pods.create/delete), NetworkPolicy deployed.

---

## For Security Specialist: JWT Security Test Matrix
**Added**: 2026-01-14, **Updated**: 2026-02-10
**Related files**: `crates/global-controller/tests/auth_tests.rs`

Cover: (1) size limits (exact boundary), (2) algorithm (alg:none, HS256, missing), (3) JWK structure (kty, alg, fields), (4) iat validation (clock skew), (5) time sanity (exp > iat), (6) header injection (kid, jwk, jku), (7) JWKS caching, (8) error messages (generic, no leak). Use descriptive test names for vulnerabilities.

---

## For Implementation Teams: Coverage by Feature Type
**Added**: 2026-01-15
**Related files**: `docs/specialist-knowledge/test/coverage-targets.md`

Security-critical (90-95%): all attack vectors, boundaries, integration. Core business (85%): happy + error paths, database integration. Infrastructure (80%): config boundaries, error handling. Missing integration tests for critical paths = BLOCKER.

---

## For Code Reviewer: Deferred Test Gaps
**Added**: 2026-01-23
**Related files**: `.claude/TODO.md`

Some gaps deferred: sqlx error paths (no mock layer), external service failures, flaky race conditions. Check if documented, ensure tracked in tech debt or comments.

---

## For Service Specialists: State Machine Transition Tests
**Added**: 2026-01-21, **Updated**: 2026-01-24
**Related files**: `crates/global-controller/tests/meeting_assignment_tests.rs`

Test state transitions, not just happy paths: (1) initial state, (2) state change handling, (3) concurrent access, (4) boundary states (Degraded). Without transition tests, failover bugs hidden until production.

---

## For All Specialists: Cross-Service Test Client Pattern
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`

Follow GcClient pattern for new service clients: Error enum (HttpError, RequestFailed, JsonError), custom Debug on sensitive fields, sanitize_error_body(), health_check(), raw_* methods for error testing.

---

## For Meeting Controller Specialist: Test Infrastructure
**Added**: 2026-01-25, **Updated**: 2026-02-10
**Related files**: `crates/mc-test-utils/src/mock_redis.rs`, `crates/meeting-controller/tests/`

MockRedis for unit tests (builder pattern), actor lifecycle tests (spawn, shutdown, cancellation), MockBehavior enum for gRPC server states, start_paused for time-based tests. Phase 6c: 143 tests (126 unit + 13 integration + 4 heartbeat), MockGcServer pattern, Lua behavioral tests, capacity/draining tests.

---

## For Observability Specialist: Metrics Wiring Review
**Added**: 2026-02-05, **Updated**: 2026-02-10
**Related files**: `crates/meeting-controller/src/observability/metrics.rs`

Simple wiring (direct calls): behavior tests + wrapper module tests sufficient. Complex wiring (conditionals, aggregation): explicit tests required. Cardinality bounds in wrapper module. Document missing updates as tech debt (e.g., current_participants never incremented).

---

## For DRY Reviewer: Test Preservation in Extract-Generic Refactors
**Added**: 2026-02-12, **Updated**: 2026-02-12
**Related files**: `crates/global-controller/src/tasks/generic_health_checker.rs`

When extracting shared logic into a generic module with thin wrappers, verify: (1) wrapper public signatures unchanged (tests call wrappers, not generic), (2) constants re-exported so `super::CONSTANT` still resolves in test modules, (3) test count >= original (no silent drops from missing mod.rs entries). Wrapper-preserving pattern is safest for test preservation — zero test code changes. Flag asymmetric test coverage between parallel implementations as pre-existing tech debt (not caused by refactor). Iterative simplification (e.g., config struct → plain parameters) is safe when tests only touch wrappers. Also: `.instrument()` chaining vs `#[instrument]` attribute is invisible to tests — neither requires test changes.

---
