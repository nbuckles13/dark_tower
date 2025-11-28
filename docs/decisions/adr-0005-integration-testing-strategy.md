# ADR-0005: Integration and End-to-End Testing Strategy

**Status**: Accepted

**Date**: 2025-01-28

**Deciders**: Multi-specialist debate (Auth Controller, Database, Code Reviewer, Test)

---

## Context

Dark Tower is a distributed real-time video conferencing system with multiple services (Authentication Controller, Global Controller, Meeting Controller, Media Handler). As we build out each service, we need a consistent, comprehensive testing strategy that:

1. **Ensures Security**: 100% coverage for cryptographic operations and authentication flows
2. **Validates Data Integrity**: Tests against real PostgreSQL with proper migrations
3. **Prevents Regressions**: High coverage (90%+) with zero flaky tests
4. **Enables Fast Iteration**: CI runs complete in <2 minutes
5. **Maintains Quality**: Test code follows same standards as production code (ADR-0002)
6. **Scales to Multi-Service**: Clear patterns for testing service-to-service interactions

### Current Situation

- Auth Controller Phase 3 implemented with 8-10% test coverage (crypto only)
- No integration tests for handlers, services, or repositories
- No end-to-end tests for complete authentication flows
- No established patterns for database testing, HTTP testing, or test data management
- Need to establish patterns before implementing other services (GC, MC, MH)

### Requirements

1. Test against real PostgreSQL (encrypted keys require real DB features)
2. Deterministic tests (no flaky tests, reproducible failures)
3. ADR-0002 compliant test utilities (no `.unwrap()` in library code)
4. Performance targets: <1s unit, <5s integration, <30s E2E, <2min total CI
5. Coverage targets: 100% crypto, 95% handlers/services/repos, 90% overall
6. Migration validation (schema correctness, idempotency, constraints)
7. Clear boundaries between unit, integration, and E2E tests

## Decision

We adopt a **three-tier testing strategy** with real PostgreSQL, deterministic test data, and comprehensive test utilities.

### 1. Test Database Strategy

**Real PostgreSQL with Isolated Databases**:
- Use `sqlx::test` macro for automatic per-test database isolation
- Run full migrations in tests to validate schema correctness
- Each test gets a fresh database with clean slate

**Tiered Cleanup Strategy** (optimized for speed):

| Test Type | Cleanup Method | Speed | When to Use |
|-----------|---------------|-------|-------------|
| Repository/Service | Transaction rollback | ~50ms | Single-table operations |
| Handler/E2E | TRUNCATE CASCADE | ~100ms | Multi-table, single service |
| Cross-service | Drop/recreate | ~2s | Multi-service integration |

**Migration Testing**:
```rust
#[sqlx::test]
async fn test_migrations_apply_cleanly(pool: PgPool) {
    // Migrations already applied by sqlx::test
    // Verify schema exists
    assert_table_exists(&pool, "service_credentials").await;
    assert_table_exists(&pool, "signing_keys").await;
}

#[sqlx::test]
async fn test_migrations_are_idempotent(pool: PgPool) {
    // Re-run migrations (should be no-op)
    sqlx::migrate!().run(&pool).await?;
}
```

### 2. Test Organization & Structure

**Directory Layout**:
```
crates/ac-service/
├── src/
│   ├── crypto/mod.rs              # Unit tests in #[cfg(test)] modules
│   ├── handlers/                  # Unit tests in #[cfg(test)] modules
│   ├── services/                  # Unit tests in #[cfg(test)] modules
│   └── repositories/              # Unit tests in #[cfg(test)] modules
├── tests/
│   ├── integration/
│   │   ├── service_registration_test.rs    # Service layer integration
│   │   ├── token_issuance_test.rs          # Token flow integration
│   │   └── key_rotation_test.rs            # Key management integration
│   ├── e2e/
│   │   ├── auth_flow_test.rs               # Complete OAuth flow
│   │   ├── jwks_endpoint_test.rs           # Public endpoint E2E
│   │   └── admin_endpoints_test.rs         # Protected endpoints E2E
│   ├── migrations/
│   │   ├── schema_validation_test.rs       # Schema correctness
│   │   ├── idempotency_test.rs             # Re-run safety
│   │   └── constraint_test.rs              # FK/index validation
│   └── common/
│       └── mod.rs                          # Re-export from ac-test-utils
└── Cargo.toml

crates/ac-test-utils/               # Shared test utilities crate
├── src/
│   ├── crypto_fixtures.rs          # Deterministic key generation
│   ├── token_builders.rs           # TestTokenBuilder pattern
│   ├── server_harness.rs           # TestAuthServer for E2E
│   ├── test_ids.rs                 # Fixed UUIDs, constants
│   └── assertions.rs               # Custom TokenAssertions trait
└── Cargo.toml
```

### 3. HTTP Testing Approach

**Three-Tier Strategy**:

**Tier 1: Unit Tests** (Direct function calls, no HTTP):
```rust
#[tokio::test]
async fn test_issue_service_token() {
    let token = token_service::issue_service_token(
        &pool, &master_key, "client-id", "secret", ...
    ).await?;
    assert!(token.access_token.len() > 0);
}
```

**Tier 2: Integration Tests** (Tower ServiceExt, HTTP layer without network):
```rust
#[tokio::test]
async fn test_token_endpoint_integration() {
    let app = create_test_app().await;
    let response = app.oneshot(
        Request::builder()
            .uri("/api/v1/auth/service/token")
            .method("POST")
            .body(Body::from(json_payload))
            .unwrap()
    ).await?;
    assert_eq!(response.status(), 200);
}
```

**Tier 3: E2E Tests** (Real server + reqwest, full stack):
```rust
#[tokio::test]
async fn test_auth_flow_e2e() {
    let server = TestAuthServer::spawn().await;
    let client = reqwest::Client::new();

    let response = client
        .post(&format!("{}/api/v1/auth/service/token", server.url()))
        .basic_auth("client-id", Some("secret"))
        .json(&ServiceTokenRequest { grant_type: "client_credentials" })
        .send()
        .await?;

    assert_eq!(response.status(), 200);
    let token: TokenResponse = response.json().await?;
    token.assert_valid_jwt().assert_has_scope("meeting:create");
}
```

### 4. Deterministic Test Data

**Fixed UUIDs for Reproducibility**:
```rust
// crates/ac-test-utils/src/test_ids.rs
pub const TEST_CREDENTIAL_ID: Uuid = Uuid::from_u128(1);
pub const TEST_USER_ALICE: Uuid = Uuid::from_u128(100);
pub const TEST_ORG_ACME: Uuid = Uuid::from_u128(1000);
```

**Deterministic Crypto Keys**:
```rust
// Uses seeded RNG for reproducible Ed25519 keys
pub fn test_signing_key(seed: u64) -> (String, Vec<u8>) {
    // Deterministic key generation using seed
    // Same seed always produces same keypair
}
```

**Builder Patterns for Test Data**:
```rust
let token = TestTokenBuilder::new()
    .for_user("alice")
    .with_scope("user.read.gc meeting:create")
    .expires_in(60)
    .signed_by(test_signing_key(1))
    .build();
```

### 5. ADR-0002 Compliance in Tests

**Test Utilities Return `Result`**:
```rust
// ✅ CORRECT: Test utilities return Result
pub fn test_signing_key(seed: u64) -> Result<(String, Vec<u8>), AcError> {
    // No .unwrap() in library code
}

// Usage in tests:
#[tokio::test]
async fn test_example() {
    let key = test_signing_key(1)?;  // Propagate errors
    // Test code...
}
```

**Exception: Test Assertions**:
```rust
// ✅ ALLOWED: assert! macros in test code
assert_eq!(response.status(), 200);
assert!(token.access_token.len() > 0);
```

### 6. Custom Test Assertions

**TokenAssertions Trait**:
```rust
pub trait TokenAssertions {
    fn assert_valid_jwt(&self) -> &Self;
    fn assert_has_scope(&self, scope: &str) -> &Self;
    fn assert_signed_by(&self, key_id: &str) -> &Self;
    fn assert_expires_in(&self, seconds: u64) -> &Self;
    fn assert_for_subject(&self, subject: &str) -> &Self;
}

// Usage:
token
    .assert_valid_jwt()
    .assert_has_scope("user.read.gc")
    .assert_signed_by("test-key-2025-01")
    .assert_expires_in(3600);
```

### 7. Cryptographic Test Vectors

**Source: RFC 7515 Appendix A.4** (Ed25519 test vectors):
```rust
#[test]
fn test_jwt_signature_against_rfc7515_vectors() {
    // Test vector from RFC 7515, Appendix A.4
    let private_key = hex::decode("...").unwrap();
    let payload = "...";
    let expected_signature = "...";

    let jwt = sign_jwt(&claims, &private_key)?;
    // Validate against known-good signature
}
```

**Coverage**:
- ✅ Positive tests (valid signatures verify)
- ✅ Negative tests (invalid signatures reject)
- ✅ Tampering detection (modified JWTs fail)

### 8. Coverage Thresholds

| Module Type | Required Coverage | Rationale |
|-------------|------------------|-----------|
| Crypto (`crypto/`) | 100% | Security-critical, zero tolerance for untested code |
| Handlers (`handlers/`) | 95% | Public API, must handle all inputs correctly |
| Services (`services/`) | 95% | Business logic, critical for correctness |
| Repositories (`repositories/`) | 95% | Data integrity, must validate all queries |
| Middleware (`middleware/`) | 90% | Auth/logging, lower risk but still critical |
| Models (`models/`) | 85% | Simple structs, lower complexity |
| **Overall** | **90%+** | Project-wide minimum |

**Tool**: `cargo-llvm-cov` (superior async Rust support vs. tarpaulin)

**Configuration** (`.codecov.yml`):
```yaml
coverage:
  status:
    project:
      default:
        target: 90%
        threshold: 1%
    patch:
      default:
        target: 95%
```

### 9. E2E vs Integration Boundaries

| Test Type | Scope | DB | HTTP | External Services |
|-----------|-------|----|----|-------------------|
| **Unit** | Single function | Mock/None | No | Mock |
| **Integration** | Single service layer | Real PostgreSQL | Tower ServiceExt | Mock |
| **E2E (Single Service)** | Full AC stack | Real PostgreSQL | Real server + reqwest | Mock |
| **E2E (Cross-Service)** | Multiple services | Real PostgreSQL | Real servers | Real (other DT services) |

**Examples**:
- **Integration**: `token_service::issue_service_token()` with real DB, no HTTP
- **E2E**: POST `/api/v1/auth/service/token` via HTTP, full stack
- **Cross-Service**: GC calls AC to authenticate, then creates meeting

### 10. CI/CD Performance Targets

| Test Tier | Target | Timeout | Rationale |
|-----------|--------|---------|-----------|
| Unit tests | <1s | 10s | Fast feedback, run frequently |
| Integration tests | <5s | 30s | Real DB adds latency |
| E2E tests | <30s | 2min | Full server startup overhead |
| **Total CI time** | **<2min** | **5min** | Developer productivity |

**GitHub Actions Configuration**:
```yaml
- name: Run Tests with Coverage
  run: |
    cargo llvm-cov --workspace --lcov --output-path lcov.info
  timeout-minutes: 5

services:
  postgres:
    image: postgres:16
    env:
      POSTGRES_PASSWORD: postgres
    options: >-
      --health-cmd pg_isready
      --health-interval 10s
      --health-timeout 5s
      --health-retries 5
```

## Consequences

### Positive

1. **High Confidence in Security**:
   - 100% crypto coverage ensures cryptographic correctness
   - Migration tests validate schema integrity
   - Deterministic tests prevent timing/race condition bugs

2. **Fast Development Velocity**:
   - <2min CI enables rapid iteration
   - Deterministic tests reduce debugging time
   - Custom assertions make tests readable and expressive

3. **Maintainability**:
   - Clear test tier boundaries prevent confusion
   - Test utilities in separate crate encourage reuse
   - ADR-0002 compliance in test code demonstrates best practices

4. **Scalability**:
   - Patterns established for Auth Controller apply to all services
   - Cross-service E2E testing strategy defined
   - Clear migration path from single-service to multi-service tests

5. **Quality Gates**:
   - Coverage thresholds enforce minimum standards
   - Performance targets prevent test suite bloat
   - Migration tests catch schema bugs early

### Negative

1. **Initial Setup Cost**:
   - Creating `ac-test-utils` crate requires upfront effort (~2-4 days)
   - Writing custom assertions adds complexity
   - Setting up deterministic crypto fixtures is non-trivial

2. **PostgreSQL Dependency**:
   - Tests require PostgreSQL running (Docker or service container)
   - Slightly slower than in-memory database (~50-100ms overhead per test)
   - CI requires PostgreSQL service configuration

3. **Learning Curve**:
   - Developers must understand three test tiers
   - Builder patterns and custom assertions require familiarity
   - `sqlx::test` macro has specific behavior to learn

4. **Test Code Volume**:
   - 95% coverage target means significant test code (potentially 2x production code)
   - Maintenance burden for test fixtures and utilities
   - Risk of brittle tests if not designed carefully

### Neutral

1. **Test Execution Time**:
   - 2-minute CI is fast for most teams, but some prefer <1min
   - Trade-off: comprehensive testing vs. speed

2. **Coverage Tool Choice**:
   - `cargo-llvm-cov` is superior for async, but less mature than tarpaulin
   - Requires LLVM toolchain (widely available)

3. **Deterministic UUIDs**:
   - Makes tests reproducible, but diverges from production (random UUIDs)
   - Acceptable trade-off for test reliability

## Alternatives Considered

### Alternative 1: In-Memory Database (SQLite)
- **Pros**: Faster test execution (~10ms vs ~50ms), no external dependencies
- **Cons**: Auth Controller stores encrypted keys with PostgreSQL-specific features (bytea, transaction isolation). SQLite doesn't support all features, leading to false positives in tests.
- **Decision**: Rejected. Real PostgreSQL required for accurate testing.

### Alternative 2: Shared Test Database (No Isolation)
- **Pros**: Faster (no per-test DB creation), simpler setup
- **Cons**: Tests interfere with each other, non-deterministic failures, cannot run in parallel
- **Decision**: Rejected. Isolation critical for reliability.

### Alternative 3: Mock All Database Calls
- **Pros**: No database dependency, extremely fast
- **Cons**: Doesn't test SQL queries, migration testing impossible, miss database-level bugs (constraint violations, query performance)
- **Decision**: Rejected for integration/E2E tests. Acceptable for pure unit tests.

### Alternative 4: Random Test Data (No Determinism)
- **Pros**: Tests real-world variety, might catch edge cases
- **Cons**: Flaky tests, non-reproducible failures, harder debugging
- **Decision**: Rejected. Determinism is non-negotiable per consensus.

### Alternative 5: Single Test Tier (Only Integration Tests)
- **Pros**: Simpler mental model, comprehensive coverage
- **Cons**: Slow (all tests use DB + HTTP), harder to diagnose failures, violates test pyramid
- **Decision**: Rejected. Three tiers provide better granularity.

### Alternative 6: Tarpaulin for Coverage
- **Pros**: More mature, widely used
- **Cons**: Poor async Rust support, known issues with tokio tests
- **Decision**: Rejected. `cargo-llvm-cov` superior for async workloads.

## Implementation Notes

### Phase 4 Implementation Checklist

**Phase 4.1: Test Infrastructure Setup** (Days 1-2)
- [ ] Create `crates/ac-test-utils` crate
- [ ] Set up PostgreSQL in Docker Compose for local development
- [ ] Configure GitHub Actions with PostgreSQL service container
- [ ] Add `cargo-llvm-cov` to development dependencies
- [ ] Create `tests/common/mod.rs` re-exporting utilities

**Phase 4.2: Test Utilities** (Days 3-4)
- [ ] Implement `crypto_fixtures.rs` (deterministic keys)
- [ ] Implement `test_ids.rs` (fixed UUIDs)
- [ ] Implement `token_builders.rs` (`TestTokenBuilder`)
- [ ] Implement `server_harness.rs` (`TestAuthServer`)
- [ ] Implement `assertions.rs` (`TokenAssertions` trait)
- [ ] Add RFC 7515 test vectors

**Phase 4.3: Integration Tests** (Days 5-7)
- [ ] Repository tests (auth_events, service_credentials, signing_keys)
- [ ] Service tests (token_service, registration_service, key_management)
- [ ] Migration tests (schema validation, idempotency, constraints)

**Phase 4.4: E2E Tests** (Days 8-10)
- [ ] Handler tests (auth_handler, admin_handler, jwks_handler)
- [ ] Full authentication flow E2E
- [ ] JWKS endpoint E2E
- [ ] Admin endpoints with JWT middleware E2E
- [ ] Error response validation

**Phase 4.5: Coverage & CI** (Days 11-13)
- [ ] Configure `cargo-llvm-cov` in CI
- [ ] Set up Codecov integration
- [ ] Verify coverage thresholds met (100% crypto, 95% handlers/services/repos, 90% overall)
- [ ] Optimize slow tests to meet performance targets

**Phase 4.6: Performance Testing** (Days 14-15)
- [ ] Benchmark token issuance (target: p99 <50ms)
- [ ] Benchmark JWT verification (target: p99 <5ms)
- [ ] Database query performance validation (EXPLAIN ANALYZE)
- [ ] Load test: 100 concurrent token requests

**Phase 4.7: Documentation** (Days 16-17)
- [ ] Document test utilities API
- [ ] Add testing guide to `docs/DEVELOPMENT.md`
- [ ] Document common test patterns
- [ ] Add troubleshooting guide for test failures

### Code Patterns to Follow

**Test Naming Convention**:
```rust
// Pattern: test_<function>_<scenario>_<expected_result>
#[tokio::test]
async fn test_issue_service_token_valid_credentials_returns_jwt() { ... }

#[tokio::test]
async fn test_issue_service_token_invalid_credentials_returns_unauthorized() { ... }
```

**Arrange-Act-Assert Structure**:
```rust
#[sqlx::test]
async fn test_create_service_credential(pool: PgPool) {
    // Arrange
    let client_id = "test-client";
    let secret_hash = hash_client_secret("secret")?;

    // Act
    let credential = service_credentials::create_service_credential(
        &pool, client_id, &secret_hash, "global-controller", None, &["meeting:create"]
    ).await?;

    // Assert
    assert_eq!(credential.client_id, client_id);
    assert_eq!(credential.scopes, vec!["meeting:create"]);
}
```

### Files/Components Affected

**New Files**:
- `crates/ac-test-utils/` (entire crate)
- `crates/ac-service/tests/integration/` (12-15 test files)
- `crates/ac-service/tests/e2e/` (5-7 test files)
- `crates/ac-service/tests/migrations/` (3-5 test files)
- `.github/workflows/ci.yml` (updated for PostgreSQL service)
- `.codecov.yml` (coverage configuration)

**Modified Files**:
- `Cargo.toml` (add `ac-test-utils` to workspace)
- `crates/ac-service/Cargo.toml` (add test dependencies)
- `docs/DEVELOPMENT.md` (add testing guide section)

### Migration Strategy

This is a new testing infrastructure, not a migration. Implementation is additive:

1. **No Breaking Changes**: Existing Phase 3 tests continue to work
2. **Incremental Adoption**: Teams can write new tests using utilities immediately
3. **Backward Compatible**: Old test patterns (if any) still compile
4. **Opt-In**: Services can adopt utilities as they add tests

## References

- **Debate Record**: `docs/debates/2025-01-testing-strategy.md` (full specialist discussion)
- **Related ADRs**:
  - ADR-0002: No-Panic Error Handling Policy (test utilities must comply)
  - ADR-0003: Service Authentication & Federation (crypto coverage requirements)
- **External Standards**:
  - RFC 7515 (JSON Web Signature) - Ed25519 test vectors in Appendix A.4
  - [Rust Testing Best Practices](https://doc.rust-lang.org/book/ch11-00-testing.html)
  - [sqlx Testing Guide](https://github.com/launchbadge/sqlx/blob/main/sqlx-macros/README.md#testing)
- **Tools**:
  - [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) - Coverage tool
  - [Codecov](https://codecov.io/) - Coverage reporting
  - [Tower Test Utilities](https://docs.rs/tower/latest/tower/util/trait.ServiceExt.html) - HTTP testing without network
