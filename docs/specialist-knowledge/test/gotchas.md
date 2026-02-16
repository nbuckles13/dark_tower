# Test Specialist - Gotchas

Project-specific test pitfalls for Dark Tower. Coverage gaps and surprising behaviors.

---

## Gotcha: TLS Validation Disabled in cfg(test)
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

TLS validation returns early in cfg(test). TLS warning tests need integration tests with real tracing, can't be unit tests.

---

## Gotcha: Bcrypt Timing Makes Tests Slow
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Cost 14 takes ~800ms per hash. Cross-cost tests take seconds. Use #[ignore] for slow tests or test only min/default/max.

---

## Gotcha: Database Tests Need Migrations Attribute
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

Always use `#[sqlx::test(migrations = "../../migrations")]` for database tests. Without it, empty database with no tables.

---

## Gotcha: Integration Test Modules Must Be Included in mod.rs
**Added**: 2026-01-12
**Related files**: `crates/ac-service/tests/integration/mod.rs`

Add new test files to mod.rs or they're never compiled. Symptom: file exists, cargo test shows 0 tests from module. Silent failure.

---

## Gotcha: SecretBox Type Mismatches After Refactor
**Added**: 2026-01-12, **Updated**: 2026-01-28
**Related files**: `crates/ac-service/src/crypto/mod.rs`

SecretBox refactors need wrapping at construction (`SecretBox::new(Box::new(...))`), unwrapping at access (`.expose_secret()`). Compiler catches, but orphaned test files (not in mod.rs) won't compile. Updates mechanical, no new test cases needed.

---

## Gotcha: env-tests Feature Gates Require Explicit Flags
**Added**: 2026-01-13
**Related files**: `crates/env-tests/Cargo.toml`

env-tests has no default features. `cargo test -p env-tests` runs 0 tests. Must use `--features smoke/flows/all`. Intentional - requires cluster.

---

## Gotcha: TestServer Random Port Only Known After Bind
**Added**: 2026-01-14
**Related files**: `crates/gc-test-utils/src/server_harness.rs`

TestServer binds to 127.0.0.1:0 for random port. Address only available AFTER binding. Get with `listener.local_addr()`, not before. Drop impl aborts server.

---

## Gotcha: JWT Size Boundary Off-by-One Errors
**Added**: 2026-01-14
**Related files**: `crates/gc-service/tests/auth_tests.rs`

Test exact boundary (should pass) and one byte over (should fail). Tests checking only "small pass, large fail" miss off-by-one that attackers exploit for DoS.

---

## Gotcha: Algorithm Confusion Needs Multiple Attack Vectors
**Added**: 2026-01-14
**Related files**: `crates/gc-service/tests/auth_tests.rs`

Test alg:none, alg:HS256, missing alg separately. Each exploitable independently. Testing only one vector misses others (CVE-2016-10555, CVE-2017-11424).

---

## Gotcha: JWK Structure Validation Required
**Added**: 2026-01-14
**Related files**: `crates/gc-service/src/auth/jwt.rs`

Validate JWK structure before use (kty, alg, required fields). JWKS endpoint could be compromised/misconfigured or MITM'd. Silent failures accept invalid signatures.

---

## Gotcha: sqlx::test Isolation Prevents Cross-Test Data Sharing
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_service_tests.rs`

Each `#[sqlx::test]` gets separate transaction, rolls back at completion. Tests can't see data from other tests. Migrations run per test. Never write tests depending on state from previous test. By design, catches invalid dependencies.

---

## Gotcha: Custom Debug Not Sufficient for Error Bodies
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`

Custom Debug only redacts in Debug formatting. Error bodies leak through assert_eq!, Display, direct logging. Sanitize at capture time, not just Debug. Remove JWTs/Bearer tokens before storing in error variants.

---

## Gotcha: Response Body Consumed Before Status Check
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`

`response.text().await` consumes body. Store `response.status()` before consuming, else borrow-after-move error.

---

## Gotcha: Time-Based Tests Without pause() Are Flaky
**Added**: 2026-01-25
**Related files**: `crates/mc-service/tests/session_actor_tests.rs`

Real time tests are flaky or slow. Use `#[tokio::test(start_paused = true)]` and `tokio::time::advance()` for deterministic, instant, boundary-precise tests.

---

## Gotcha: Lua Script Structural Tests Miss Logic Errors
**Added**: 2026-01-25
**Related files**: `crates/mc-service/tests/redis_lua_tests.rs`

"Script runs without error" catches syntax, misses logic. Test actual behavior: current generation (accept), higher (accept+update), lower (reject), no generation (first write, accept). Structural tests passed in Phase 6c even when fencing logic wrong.

---

## Gotcha: Boundary Tests Pass For Wrong Reasons
**Added**: 2026-01-31
**Related files**: `crates/common/src/jwt.rs`

Boundary test can pass with broken test if failure mode differs from assertion. Example: JWT size test created 2-part token, asserted "not TokenTooLarge", passed because rejected as MalformedToken first. Prevention: (1) assert exact boundary value, (2) assert SUCCESS not "not error", (3) verify extracted value.

---

## Gotcha: Database Error Paths Hard to Test (Often Deferred)
**Added**: 2026-01-23
**Related files**: `crates/gc-service/src/gc_assignment_cleanup.rs`

sqlx compile-time queries need real database, no built-in mocking. Workarounds (trait abstraction, testcontainers) complex/slow. Error path tests for internal DB functions often deferred to tech debt. Document explicitly to prevent re-flagging.

---

## Gotcha: Integration Tests Can Miss Helper Method Tests
**Added**: 2026-01-31, **Updated**: 2026-02-10
**Related files**: `crates/mc-service/src/actors/metrics.rs`

Integration tests exercise flows but may miss helper methods. Phase 6c: heartbeat tests verified metrics reported but `snapshot()` method itself untested. Add unit tests when: (1) complex logic, (2) public API, (3) subtle failures (swapped fields). Skip when: (1) simple delegation, (2) private single-caller, (3) obvious failures.

---

## Gotcha: Explicitly-Handled Error Paths Often Lack Tests
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

Don't assume error paths tested just because code handles them explicitly. TokenManager had 3 MAJOR gaps: 401/400 status handling, invalid JSON, missing OAuth fields. Review checklist: (1) search for test per error branch, (2) check HTTP status handling, (3) verify JSON/deserialization paths, (4) verify timeout/connection paths. Happens when developers write defensive handling then only write happy-path tests.

---

## Gotcha: Endpoint Behavior Changes Require Multi-File Test Updates
**Added**: 2026-02-08
**Related files**: `crates/gc-service/tests/auth_tests.rs`, `crates/gc-service/tests/health_tests.rs`

When endpoint behavior changes (e.g., `/health` from JSON to plain text "OK"), tests may exist in MULTIPLE test files that all need updating. Search all test files: `grep -r "health" crates/*/tests/`. Check for both direct endpoint tests AND tests that use the endpoint as setup. This is especially common for health/ready endpoints tested as "works without auth" examples in auth test suites.

---

## Gotcha: #[allow(dead_code)] on Metrics Functions Signals Missing Wiring
**Added**: 2026-02-09
**Related files**: `crates/gc-service/src/observability/metrics.rs`, `crates/gc-service/src/services/mh_selection.rs`

`#[allow(dead_code)]` on metric recording functions is a red flag that metrics are defined but NOT wired into production code paths. Detection: search metrics.rs for the annotation, then search for call sites. Contrast with intentional skeleton code which uses `#[allow(dead_code)]` on enum variants planned for future phases — metrics functions are defined for immediate use but forgotten during implementation.

---

## Gotcha: Observability Wiring May Not Need Prometheus Mocking
**Added**: 2026-02-05, **Updated**: 2026-02-10
**Related files**: `crates/mc-service/src/actors/metrics.rs`

Simple wiring (direct calls, no branching) doesn't need explicit "mock Prometheus and assert called" tests. Existing behavior tests + wrapper module tests sufficient. Require explicit tests only for: conditional emission, aggregation/batching, error handling that might suppress. For simple wiring, behavior verification sufficient.

---

## Gotcha: tracing `target:` Requires String Literals
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`

`tracing` macros require `target:` values to be string literals or `&'static str` known at compile time. You cannot pass a runtime `config.log_target` field as `target:`. This means generic/shared functions cannot parameterize log targets. Workarounds: (1) omit explicit `target:` and let it default to module path (preferred — works with `EnvFilter`), (2) keep `target:` in wrapper functions for lifecycle logs only. Note: dot-separated targets like `gc.task.health_checker` are silently filtered by `EnvFilter` directives like `global_controller=debug` — module-path targets are more compatible.

---

## Gotcha: Dot-Separated Log Targets Silently Filtered by EnvFilter
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/health_checker.rs`

Custom dot-separated `target:` values (e.g., `"gc.task.health_checker"`) don't match module-path-based `EnvFilter` directives (e.g., `"global_controller=debug"`). Events with these targets are silently dropped under default config. Tests won't catch this because they don't assert on log visibility. When reviewing code with custom `target:` values, verify they're actually reachable under the configured `EnvFilter`. Module-path-based targets (default when `target:` is omitted) are always compatible.

---

## Gotcha: Crate Rename Silently Breaks EnvFilter Defaults
**Added**: 2026-02-16
**Related files**: `crates/gc-service/src/main.rs`, `crates/mc-service/src/main.rs`, `crates/mh-service/src/main.rs`

When renaming crates (e.g., `global-controller` → `gc-service`), the `EnvFilter` fallback string in `main.rs` must be updated to match the new crate name (e.g., `"global_controller=debug"` → `"gc_service=debug"`). Compiler cannot catch this — it's a runtime string. Tests don't catch it either since they don't assert on log visibility. Result: all debug/info logs silently dropped in production when `RUST_LOG` is not set. Review checklist for crate renames: (1) grep for old crate name in EnvFilter strings, (2) verify default filter matches `[lib] name` or `[[bin]] name` in Cargo.toml.

---

## Gotcha: Guard Step 4 Soft-Fail Masks Dashboard Coverage Gaps
**Added**: 2026-02-16
**Related files**: `scripts/guards/simple/validate-application-metrics.sh`

Before this session, Step 4 (metric coverage in dashboards) was a soft warning, not a hard fail. This allowed 32 metrics to exist in code without any dashboard panel — silently. The guard passed, giving false confidence. When promoting guards from warning to hard-fail, audit the current state first: soft warnings may have been accumulating for weeks. Similarly, when adding new guard steps (Step 5: catalog coverage), verify the catalog actually exists for all services before enabling the check.

---
