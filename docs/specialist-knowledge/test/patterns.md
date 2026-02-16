# Test Specialist - Patterns

Project-specific testing patterns for Dark Tower. Focus: reusable techniques, not generic testing advice.

---

## Pattern: Defense-in-Depth Validation Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

When both config AND function validate the same input (e.g., bcrypt cost), test the function independently. Catches bugs if callers bypass config. Test each validation layer separately.

---

## Pattern: Cross-Version Verification Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

For crypto migrations (bcrypt cost, algorithm upgrades), test that old artifacts verify with new code. Create artifacts at multiple versions, verify ALL work with current code. Essential for zero-downtime deployments.

---

## Pattern: SecretBox Debug Redaction Tests
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/crypto/mod.rs`

When struct contains `SecretBox<T>`, test Debug impl redacts secrets. Assert `[REDACTED]` appears and actual value doesn't. Prevents credential leaks in logs.

---

## Pattern: NetworkPolicy Positive/Negative Test Pair
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`

Test NetworkPolicy with paired tests: (1) positive (same namespace, allowed labels) verifies connectivity works, (2) negative (cross-namespace) verifies blocking. Run positive first to validate infrastructure. If both pass, NetworkPolicy isn't enforced (security gap).

---

## Pattern: CanaryPod for In-Cluster Testing
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

For testing cluster-internal behavior, use CanaryPod with Drop cleanup, AtomicBool for double-cleanup prevention, and UUID pod names to avoid collisions. Prefer synchronous `std::process::Command` over async kubectl client for simplicity.

---

## Pattern: Test Server Harness for HTTP Integration Tests
**Added**: 2026-01-14
**Related files**: `crates/ac-test-utils/src/server_harness.rs`, `crates/gc-test-utils/src/server_harness.rs`

For HTTP service testing, create test harness that: (1) binds to random port (127.0.0.1:0), (2) exposes database pool for assertions, (3) implements Drop for cleanup, (4) uses `#[sqlx::test]` for migrations. Real HTTP server prevents mocking gaps, random ports enable parallel execution.

---

## Pattern: Layered JWT Testing (Defense-in-Depth)
**Added**: 2026-01-15
**Related files**: `crates/gc-service/tests/auth_tests.rs`

JWT security requires testing 4 independent layers: (1) algorithm (reject alg:none, HS256), (2) JWK structure (reject wrong kty), (3) signature verification (reject tampering), (4) claims validation (reject expired). Test each separately - JWKS compromise could bypass token-level checks. Covers CVE-2016-10555, CVE-2017-11424.

---

## Pattern: Rate Limiting Tests via Loop
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

Test rate limiting by looping until lockout triggers. Assert correct status before and after threshold. Avoids hardcoded timing assumptions. Use unique emails for registration tests, same password for login tests.

---

## Pattern: Concurrent Race Condition Testing with Barrier
**Added**: 2026-01-21
**Related files**: `crates/gc-service/tests/meeting_assignment_tests.rs`

Test atomic operations under concurrent load using `tokio::sync::Barrier`. Barrier synchronizes N tasks before they all attempt same operation simultaneously. Verify: (1) all attempts succeed, (2) all return same result (consistency), (3) database state correct. Essential for atomic CTEs, distributed locks, idempotency.

---

## Pattern: RPC Retry Testing with Mixed Success/Failure
**Added**: 2026-01-24
**Related files**: `crates/gc-service/tests/meeting_assignment_tests.rs`

Test RPC retry with mixed sequences (some fail, eventual success) - not just all-succeed or all-fail. Catches bugs in retry counter updates and candidate iteration. Also test backoff timing with `tokio::time::pause` for determinism.

---

## Pattern: Enum State Boundary Value Testing
**Added**: 2026-01-24
**Related files**: `crates/gc-service/tests/meeting_assignment_tests.rs`

Test boundary/transitional enum states explicitly (e.g., Degraded between Healthy/Unhealthy). Code often handles boundaries differently (`if status == Healthy` vs `if status != Unhealthy`). Test: (1) each state in isolation, (2) state transitions, (3) operations in boundary states.

---

## Pattern: Weighted Selection Edge Case Testing
**Added**: 2026-01-24
**Related files**: `crates/gc-service/tests/meeting_assignment_tests.rs`

For weighted random selection, test: (1) all candidates at max capacity (division by zero?), (2) exact boundary values (off-by-one?), (3) single candidate (degenerate case), (4) zero weight handling, (5) equal weights (uniform random).

---

## Pattern: Exhaustive Error Variant Testing
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/errors.rs`

When error enums map to protocol codes, test EVERY variant. Prevents silent regressions from wildcard match arms. Also test Display formatting and client_message() doesn't leak internals. Essential for client-stable protocol codes.

---

## Pattern: Deterministic Time-Based Tests with tokio::time::pause
**Added**: 2026-01-25
**Related files**: `crates/mc-service/tests/session_actor_tests.rs`

Use `#[tokio::test(start_paused = true)]` for timeout/grace period tests. Advance time with `tokio::time::advance()` for instant, deterministic, boundary-precise testing. Works with timeout, sleep, interval.

---

## Pattern: HMAC/Cryptographic Validation Exhaustive Testing
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/session/binding.rs`

Test each HMAC-bound field independently. If token binds session_id + correlation_id + nonce, test wrong value for EACH field separately. Catches bugs where only some fields are included in signature. Error type should be consistent to avoid leaking which field failed.

---

## Pattern: Lua Script Behavioral Testing
**Added**: 2026-01-25
**Related files**: `crates/mc-service/tests/redis_lua_tests.rs`

Test Lua script behavior, not just structure. Don't just verify script runs - verify correct results. For fencing: test current generation (accept), higher (accept+update), lower (reject), no generation (first write, accept). Structural tests miss logic errors.

---

## Pattern: Capacity Check Testing with Atomics
**Added**: 2026-01-25
**Related files**: `crates/mc-service/tests/capacity_tests.rs`

Test capacity enforcement with: (1) basic under-limit check, (2) concurrent exhaustion (barrier + more requesters than capacity, verify exactly N succeed), (3) draining state (rejects new work regardless of numeric limit). Draining often overlooked.

---

## Pattern: Actor Lifecycle Testing
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/session/actor.rs`

Test actor full lifecycle: (1) spawn (responsive), (2) graceful shutdown (pending work processed, cleanup occurs), (3) cancellation (handles abort), (4) recovery (supervisor restarts after panic). Ensures correct behavior at boundaries, not just normal operation.

---

## Pattern: gRPC Interceptor Edge Case Testing
**Added**: 2026-01-25
**Related files**: `crates/mc-service/tests/grpc_interceptor_tests.rs`

Test gRPC interceptor edge cases: (1) empty Authorization header, (2) malformed Bearer prefix (case, multiple spaces, tabs), (3) valid format but expired/invalid token. Catches: case-sensitive parsing, whitespace handling, format-only validation without content check.

---

## Pattern: Error Body Sanitization in Test Clients
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`

Sanitize error response bodies in test client fixtures using regex to redact JWTs and Bearer tokens before logging. Prevents credential leaks in test output that custom Debug alone misses. Apply in error handling paths.

---

## Pattern: Type-Level Refactor Verification
**Added**: 2026-01-28
**Related files**: `crates/mc-service/tests/`, `crates/ac-service/tests/`

Type-level refactors (Vec<u8> → SecretBox, Internal → Internal(String)) are compiler-verified. Checklist: (1) cargo check passes, (2) test count preserved, (3) tests use pattern matching not equality, (4) semantic equivalence. Test updates mechanical (wrap/unwrap), not behavioral. Low-risk, focus on compiler + no perf regressions.

---

## Pattern: Error Path Testing for Pure Refactors
**Added**: 2026-01-29
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Error hiding fixes (|_| → |e| with logging) preserve error types while adding observability. Verify existing tests cover error paths and error types. Don't require new tests for logging itself or log output assertions. Observability improvement, not behavioral change.

---

## Pattern: User POV Testing for Cross-Service env-tests
**Added**: 2026-01-31
**Related files**: `crates/env-tests/tests/22_mc_gc_integration.rs`

env-tests test user-facing HTTP APIs, not internal gRPC. Test what user sees (`JoinMeetingResponse` has `mc_assignment`), not internal contracts (GC's RegisterMC handler). Crate integration tests cover internal gRPC. Scope separation prevents duplicate coverage.

---

## Pattern: MockBehavior Enum for gRPC Test Server State Machines
**Added**: 2026-01-31
**Related files**: `crates/mc-service/tests/gc_integration.rs`

Model gRPC server behavior with enum (Accept, Reject, NotFound, NotFoundThenAccept). Single mock handles multiple scenarios, clearly models state transitions, extensible. Stateful variants use atomics. Enables testing recovery flows without real service. Simpler than separate mocks per scenario or trait-based mocking.

---

## Pattern: OnceLock for Test Channel Senders
**Added**: 2026-02-02
**Related files**: `crates/mc-service/tests/gc_integration.rs`

Use `OnceLock` for test channels where receiver must outlive sender. Static sender keeps channel alive without mem::forget leaks. Thread-safe, reused across tests, dropped at process exit. Cleaner than mem::forget which creates actual leaks and violates Rust idioms.

---

## Pattern: Testing Infinite Retry with Timeout
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

Verify infinite retry design with `tokio::time::timeout`. Timeout proves retry works as designed (token refresh, reconnection). Use wiremock `up_to_n_times` for eventual success, atomic counters for specific retry counts, `start_paused` for backoff timing.

---

## Pattern: Observability Wiring Tests - Implicit Verification
**Added**: 2026-02-05, **Updated**: 2026-02-10
**Related files**: `crates/mc-service/src/actors/metrics.rs`

For simple Prometheus wiring (direct calls, no branching): (1) existing tests verify behavior, (2) wrapper module tests verify emission, (3) wiring exercised indirectly. Don't require explicit "mock Prometheus" tests for simple wiring. For complex logic (conditionals, aggregation), add explicit tests. Verify cardinality bounds in wrapper module, not per-caller.

---

## Pattern: Test Inventory Before DRY Refactor
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/health_checker.rs`, `crates/gc-service/src/tasks/mh_health_checker.rs`

Before reviewing a DRY extraction refactor, catalog every test by name and type (unit/integration/sqlx::test) across all affected files. Track: (1) total count, (2) which wrapper function each test calls, (3) what `super::*` brings into scope. After implementation, verify count >= original. Wrapper-preserving refactors (same public signatures) require zero test changes — the safest DRY pattern. Also note asymmetric coverage gaps between parallel implementations (e.g., MH missing "skips already unhealthy" test that MC has) as pre-existing tech debt.

---

## Pattern: Constant Re-Export for Test Module Compatibility
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`

When extracting shared constants to a new module, wrapper modules should `use` the constant at module scope so test modules accessing `super::CONSTANT` continue to work without changes. Example: `use crate::tasks::generic_health_checker::DEFAULT_CHECK_INTERVAL_SECONDS;` in the wrapper, then tests use `super::DEFAULT_CHECK_INTERVAL_SECONDS` unchanged. Avoids test code churn in pure refactors.

---

## Pattern: .instrument() Chaining Keeps Generic Functions Test-Neutral
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/health_checker.rs`, `crates/gc-service/src/tasks/mh_health_checker.rs`

When extracting a generic async function used by multiple callers, prefer callers chaining `.instrument(tracing::info_span!("name"))` on the returned future over `#[instrument]` on the generic function. Test impact: zero — tests don't assert on span names, so moving span creation from generic to caller is invisible to tests. Also enables callers to use different span names (e.g., `gc.task.health_checker` vs `gc.task.mh_health_checker`). Verify by confirming: (1) no test references span names, (2) wrapper signatures unchanged, (3) `Instrument` trait imported in wrappers.

---

## Pattern: Config Struct Removal is Safe When Tests Use Wrappers
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`

Removing a config struct from a generic function's API (replacing with plain parameters) has zero test impact when all tests go through wrapper functions that construct the config internally. Checklist: (1) grep tests for config struct type name — if zero hits, safe to remove, (2) verify wrapper signatures unchanged, (3) verify parameter count/types in generic function match what wrappers pass. Two-iteration pattern: first extract with config struct for clarity, then simplify to plain parameters once the API is validated.

---
