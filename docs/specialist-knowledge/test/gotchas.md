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

---

## Gotcha: env-tests Feature Gates Require Explicit Flags
**Added**: 2026-01-13
**Related files**: `crates/env-tests/Cargo.toml`, `crates/env-tests/tests/*.rs`

env-tests has no default features - tests require explicit `--features` flag:
```bash
cargo test -p env-tests                     # Runs 0 tests!
cargo test -p env-tests --features smoke    # Runs smoke tests (~30s)
cargo test -p env-tests --features flows    # Runs flow tests (~2-3min)
cargo test -p env-tests --features all      # Runs all tests (~8-10min)
```
Symptom: `cargo test --workspace` shows env-tests compiles but runs 0 tests. This is intentional - env-tests require cluster infrastructure.

---

## Gotcha: NetworkPolicy Tests Require Matching Pod Labels
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`

When testing that same-namespace traffic is ALLOWED by NetworkPolicy, the canary pod must have labels that match the NetworkPolicy's ingress rules. If AC service's NetworkPolicy only allows `app=global-controller`, a canary with `app=canary` will be blocked even in the same namespace.

Solution: Make canary pod labels configurable. Positive tests use allowed labels, negative tests use non-matching labels.

---

## Gotcha: Clippy Warns on Unused Structs in Tests
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/10_auth_smoke.rs`

If you define structs for deserialization but only use pattern matching on raw text, clippy will warn about unused structs. Example: Defining `PrometheusResponse` for JSON parsing but checking `metrics_text.contains("rate_limit")` instead.

Solution: Remove unused structs OR add `#[allow(dead_code)]` with explanation OR actually use the parsed data.

---

## Gotcha: Synchronous kubectl in Async Context
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

`std::process::Command` is synchronous but used in async test functions. This works but blocks the executor during kubectl calls. For test code this is acceptable - test execution is sequential anyway.

For production async code, consider:
- `tokio::process::Command` for async subprocess
- `kube` crate for native async Kubernetes API
- Spawning blocking task: `tokio::task::spawn_blocking(...)`

---

## Gotcha: TestGcServer Random Port Binding
**Added**: 2026-01-14
**Related files**: `crates/gc-test-utils/src/server_harness.rs`

TestGcServer binds to "127.0.0.1:0" which gives a random available port. This is correct and intentional for:
- Parallel test execution (no port conflicts)
- Running tests on systems where specific ports are unavailable

However, the address is ONLY available AFTER binding:
```rust
// WRONG: addr is not determined yet
let config = build_config(8000);  // Hardcoded port

// RIGHT: get addr after binding
let listener = TcpListener::bind("127.0.0.1:0").await?;
let addr = listener.local_addr()?;  // Now we know the port
```

Also remember: Drop impl calls `_handle.abort()` which stops the background server when test ends. Tests that create a server must complete before server is needed elsewhere.

---

## Gotcha: reqwest Client in Tests Without Connection Info
**Added**: 2026-01-14
**Related files**: `crates/global-controller/tests/health_tests.rs`

The test server uses `into_make_service_with_connect_info::<SocketAddr>()` for remote address extraction. This is needed if routes extract ConnectInfo. However, if a route tries to extract ConnectInfo but tests use a plain reqwest Client, the extraction will fail or return a dummy address.

Solution: For tests with HTTP client, ensure either:
1. Routes don't extract ConnectInfo (simplest for GC Phase 1)
2. Use TestGcServer which provides proper SocketAddr propagation (required for Phase 2+ when we track client IPs)

---

## Gotcha: IntoResponse Body Reading Requires Full Buffering
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/errors.rs` (tests)

Reading response body from axum IntoResponse:
```rust
// Helper needed:
async fn read_body_json(body: Body) -> serde_json::Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

// WRONG: body.to_string() - body is not a String
// RIGHT: use collect().to_bytes() to buffer full body
let json: serde_json::Value = read_body_json(response.into_body()).await;
assert_eq!(json["error"]["code"], "DATABASE_ERROR");
```

Body is a stream that must be fully buffered before JSON parsing. The `http_body_util::BodyExt` trait provides `.collect()` which buffers the full body into bytes.

---

## Gotcha: #[allow(dead_code)] on Skeleton Code
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/models/mod.rs`, `crates/global-controller/src/errors.rs`

GC Phase 1 defines enum variants and methods that won't be used until Phase 2+:
- `MeetingStatus::Active` (used in Phase 2 meeting lifecycle)
- `GcError::Conflict` (used when managing concurrent operations)
- Methods like `MeetingStatus::as_str()` that aren't called in Phase 1

These require `#[allow(dead_code)]` to prevent clippy warnings. Document the phase when they'll be used. This is intentional skeleton code - DO NOT remove these.

Symptom: Adding a `#[test]` for a skeleton variant should also work fine; the test will be the only usage of that variant until Phase 2 implementation begins.

---

## Gotcha: JWT Size Boundary Off-by-One Errors
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`, `crates/global-controller/tests/auth_tests.rs`

Size checks like "token.len() > 8192" can be error-prone:
- `> 8192` means 8192 is accepted but 8193 is rejected (correct for 8KB limit)
- `>= 8192` means 8192 is rejected (off by one - 8191 is max)
- `> 8191` means 8192 is accepted (correct, but confusing - prefer `> 8192`)

The gotcha: Tests that only check "small tokens pass, large tokens fail" won't catch off-by-one errors. You need explicit boundary tests:
- Exactly at limit (should pass)
- One byte over (should fail)

Without these tests, an attacker could exploit an off-by-one to bypass the limit and cause DoS.

---

## Gotcha: Algorithm Confusion Tests Need Multiple Attack Vectors
**Added**: 2026-01-14
**Related files**: `crates/global-controller/tests/auth_tests.rs`

Testing that "EdDSA tokens are accepted" is not enough. You must also test:
1. **alg:none** - Attacker removes signature requirement (CVE-2016-10555)
2. **alg:HS256** - Attacker uses symmetric algorithm (CVE-2017-11424)
3. **Missing alg** - Attacker removes algorithm header

Each attack vector can be exploited independently. Testing only "alg:none" won't catch "alg:HS256" vulnerabilities.

---

## Gotcha: JWK Structure Validation vs Signature Validation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

Don't assume a JWK from a JWKS endpoint is valid just because it's in the endpoint response:
- JWKS endpoint could be compromised
- JWKS endpoint could be misconfigured
- A man-in-the-middle could modify the response

Always validate JWK structure BEFORE using it:
- Check `kty` (key type) matches expected value
- Check `alg` (if present) matches expected value
- Check required fields are present (e.g., `x`, `y` for OKP/EdDSA)

Silent failures here lead to accepting invalid signatures from the wrong key type.

---

## Gotcha: BLOCKER Enforcement for Missing Integration Tests
**Added**: 2026-01-15
**Related files**: Code review process, test specialist role

During code review, issuing a BLOCKER for missing integration tests is the primary mechanism to ensure coverage completeness. A single BLOCKER finding that "register_user and issue_user_token need integration tests" should lead to 12-16 new tests being implemented before review is complete. The BLOCKER is not a suggestion - it blocks approval until resolved.

This works because:
1. Test coverage gaps are easily overlooked in code review if only unit tests are visible
2. Integration tests catch real-world failures (database interactions, service boundaries)
3. BLOCKER status forces implementation, not deferral to "Phase 3+"

Example: GC Phase 2 code review found 10 tests for JWT validation but missing boundary tests (8KB limit, algorithm confusion). BLOCKER → 5 new security tests added → re-review → approved. The additional tests prevented real vulnerabilities from being overlooked.

---

## Gotcha: Integration Test Database Setup Isolation
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_service_tests.rs`

Each test using `#[sqlx::test(migrations = "...")]` gets its own database transaction that rolls back at test completion. This is excellent for isolation BUT creates a gotcha:
- Tests can see data from within their own transaction
- Tests CANNOT see data from other tests (each runs in separate transaction)
- Migrations run for EACH test (not once for test suite)

This means:
1. You CANNOT have one test create a user and another test fetch it - separate transactions
2. If a test inserts 100 records and rolls back, the next test doesn't see them (correct isolation)
3. Never write tests that depend on state from a previous test

This is by design and catches invalid test dependencies early. If you need cross-test data:
- Use a single `#[test]` function with multiple assertions
- Or use fixtures to create test data within each test

---

## Gotcha: BLOCKER vs Non-Blocker Distinction in Security Reviews
**Added**: 2026-01-15
**Related files**: Code review process, security specialist role

When a security reviewer finds issues, classifying as BLOCKER (must fix) vs MAJOR (important) vs MINOR (fix eventually) has different enforcement:

- **BLOCKER** (e.g., JWK validation missing): Must be fixed before approval. Code cannot ship without fix.
- **MAJOR** (e.g., "could add HTTPS validation"): Recommended but doesn't block approval. Implementation is higher priority than release.
- **MINOR** (e.g., "Consider logging this edge case"): Nice to have. Can be deferred to next phase.

Example from GC Phase 2:
- BLOCKER: JWK kty/alg validation missing → Required immediate fix
- MAJOR: HTTPS validation not present → Recommended for Phase 3, documented as debt
- MINOR: JWKS response size limit → Phase 3+ nice-to-have

The distinction prevents "death by a thousand paper cuts" where all feedback is treated as blocking.
