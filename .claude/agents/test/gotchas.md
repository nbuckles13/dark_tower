# Test Specialist - Gotchas

Common test coverage gaps and pitfalls to watch for.

---

## Gotcha: Warning Log Tests Require tracing-test
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Config warns when bcrypt_cost < DEFAULT or clock_skew < 60. Testing warning log emission requires `tracing-test` or `tracing-subscriber` test utilities. Currently skipped - add to TODO when tracing-test is added as dev dependency.

---

## Gotcha: TLS Validation Disabled in cfg(test)
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

The `validate_tls_config()` function returns early when `cfg!(test)` is true. This means TLS warning tests cannot be written as unit tests. Requires integration test with real tracing subscriber or manual E2E testing.

---

## Gotcha: Bcrypt Timing Makes Higher Cost Tests Slow
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Bcrypt cost 14 takes ~800ms per hash. Tests like `test_hash_verification_works_across_cost_factors` that hash at all valid costs (10-14) take several seconds. Consider using `#[ignore]` for slow tests or only testing min/default/max in CI.

---

## Gotcha: u32 Parse Rejection vs Validation Rejection
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Negative bcrypt cost like "-5" is rejected at u32::parse() (not a positive integer), not at MIN_BCRYPT_COST validation. Test both paths: parse failure (negative, float, non-numeric) vs. validation failure (9, 15). Error messages differ.

---

## Gotcha: Database Tests Need Migrations
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

Handler integration tests use `#[sqlx::test(migrations = "../../migrations")]`. Without this attribute, tests get empty database without tables. Always use migration attribute for database-dependent tests.

---

## Gotcha: Auth Events Foreign Key Constraint
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

Delete client tests create credentials directly via repository to avoid creating `auth_events` records. Using `handle_create_client` creates audit records which may cause FK constraint issues on delete in some test scenarios.

---

## Gotcha: Config from_vars vs from_env
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Tests use `Config::from_vars()` with HashMap, but production uses `Config::from_env()`. Ensure both paths are tested. Currently `from_env()` is a thin wrapper around `from_vars()`, but if that changes, tests could miss bugs.

---

## Gotcha: Claims service_type Skip Serialization
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

The Claims struct uses `#[serde(skip_serializing_if = "Option::is_none")]` for service_type. Tests verify this omission in serialized JSON. If this attribute is accidentally removed, user tokens would include `service_type: null`.
