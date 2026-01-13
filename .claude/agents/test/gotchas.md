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

---

## Gotcha: Integration Test Modules Must Be Included
**Added**: 2026-01-12
**Related files**: `crates/ac-service/tests/integration/mod.rs`

When adding new integration test files, they MUST be added to `mod.rs` (e.g., `mod clock_skew_tests;`). Otherwise, the test file is never compiled or executed, and test failures are silently ignored. Symptom: file exists but `cargo test` shows 0 tests from that module.

---

## Gotcha: SecretBox/SecretString Type Mismatches After Refactor
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/crypto/mod.rs`, integration tests

When refactoring fields to use `SecretBox<T>` or `SecretString`, existing test code that constructs those structs will have type mismatches. Example: if `EncryptedKey.encrypted_data` changes from `Vec<u8>` to `SecretBox<Vec<u8>>`, tests must change from:
```rust
encrypted_data: signing_key.private_key_encrypted.clone()
```
to:
```rust
encrypted_data: SecretBox::new(Box::new(signing_key.private_key_encrypted.clone()))
```
The compiler catches this, but orphaned test files (not in mod.rs) won't be compiled.

---

## Gotcha: Database Models vs Crypto Structs Have Different Types
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/models/mod.rs`, `crates/ac-service/src/crypto/mod.rs`

Database models (e.g., `SigningKey` from sqlx) store raw `Vec<u8>` for encrypted data. Crypto structs (e.g., `EncryptedKey`) may use `SecretBox<Vec<u8>>`. When constructing crypto structs from DB models, always wrap with `SecretBox::new(Box::new(...))`. This is intentional - DB layer is raw bytes, crypto layer protects them.
