# Auth Controller Test Suite

This directory contains integration and end-to-end tests for the Authentication Controller service.

## Directory Structure

```
tests/
├── common/              # Shared test utilities (re-exports from ac-test-utils)
├── integration/         # Integration tests (service layer, real DB, no HTTP)
├── fault_injection/     # Fault injection tests (simulated failures via code)
├── e2e/                # End-to-end tests (full HTTP stack)
├── migrations/         # Migration validation tests
├── integration_tests.rs # Entry point for integration tests
└── fault_injection_tests.rs # Entry point for fault injection tests
```

## Test Registration

Tests in subdirectories must be explicitly registered in their entry point file using `#[path = "..."]`.

**Pattern:**
```rust
// In tests/integration_tests.rs
#[path = "integration/my_new_tests.rs"]
mod my_new_tests;
```

**Why?** Cargo only compiles `.rs` files at the root of `tests/`. Files in subdirectories are not automatically discovered - they must be included via `mod` with `#[path]`.

**Guard:** The `test-registration` guard verifies all test files are registered. It runs as part of `./scripts/guards/run-guards.sh` and will fail if any test files in subdirectories are not registered.

**Adding a new test file:**
1. Create the file in the appropriate subdirectory (e.g., `tests/integration/my_tests.rs`)
2. Add the registration to the entry point (e.g., `tests/integration_tests.rs`):
   ```rust
   #[path = "integration/my_tests.rs"]
   mod my_tests;
   ```
3. Run the guard to verify: `./scripts/guards/simple/test-registration.sh`

## Running Tests

### Prerequisites

1. **Start PostgreSQL**:
   ```bash
   docker-compose -f docker-compose.test.yml up -d
   ```

2. **Set environment variables**:
   ```bash
   source .env.test
   # Or: export $(cat .env.test | xargs)
   ```

### Run All Tests

```bash
cargo test -p ac-service
```

### Run Specific Test Suites

```bash
# Integration tests only
cargo test --test '*integration*'

# E2E tests only
cargo test --test '*e2e*'

# Migration tests only
cargo test --test '*migrations*'
```

### Run with Coverage

```bash
# Install cargo-llvm-cov (first time only)
cargo install cargo-llvm-cov

# Run tests with coverage
cargo llvm-cov --package ac-service --lcov --output-path lcov.info

# Generate HTML report
cargo llvm-cov --package ac-service --html
open target/llvm-cov/html/index.html
```

## Test Categories

### Unit Tests (`src/**/*.rs`)
- Located in `#[cfg(test)] mod tests` within source files
- Test individual functions in isolation
- No database, no HTTP
- Fast execution (<1s total)

### Integration Tests (`tests/integration/`)
- Test service layer with real PostgreSQL
- Uses `sqlx::test` for database isolation
- No HTTP layer (direct function calls)
- Target: <5s total execution

### E2E Tests (`tests/e2e/`)
- Full HTTP stack with real server
- Real PostgreSQL database
- Uses `reqwest` client
- Target: <30s total execution

### Migration Tests (`tests/migrations/`)
- Validate schema correctness
- Test migration idempotency
- Verify constraints and indexes

### Fault Injection Tests (`tests/fault_injection/`)
- **Programmatic** fault simulation within the application
- Uses `pool.close()` to simulate database unavailability
- Tests concurrent operations under stress
- Validates resilience behavior (readiness probes, graceful degradation)

**NOTE**: These are NOT infrastructure-level chaos tests. For true chaos testing
(stopping containers, network partitions, resource exhaustion), ADR-0012 specifies
**LitmusChaos** for Kubernetes-native chaos experiments. Those tests will live in
`infra/chaos/` when implemented.

## Test Utilities

See `crates/ac-test-utils` for shared test utilities:

- **Deterministic crypto fixtures**: `test_signing_key(seed)`
- **Fixed test IDs**: `TEST_USER_ALICE`, `TEST_CREDENTIAL_ID_1`
- **Builder patterns**: `TestTokenBuilder`
- **Custom assertions**: `TokenAssertions` trait
- **Server harness**: `TestAuthServer` (for E2E)

## Writing Tests

### Integration Test Example

```rust
use common::*;

#[sqlx::test]
async fn test_issue_service_token(pool: sqlx::PgPool) {
    // Arrange
    let master_key = test_master_key();
    let (public_key, private_key) = test_signing_key(1)?;

    // Act
    let token = token_service::issue_service_token(
        &pool,
        &master_key,
        "test-client",
        "test-secret",
        "client_credentials",
        &["meeting:create"],
        Some("127.0.0.1"),
        Some("test-agent"),
    ).await?;

    // Assert
    assert!(!token.access_token.is_empty());
    assert_eq!(token.token_type, "Bearer");
}
```

### E2E Test Example

```rust
use common::*;

#[tokio::test]
async fn test_auth_flow_e2e() {
    let server = TestAuthServer::spawn().await;
    let client = reqwest::Client::new();

    let response = client
        .post(&format!("{}/api/v1/auth/service/token", server.url()))
        .basic_auth("client-id", Some("secret"))
        .json(&serde_json::json!({
            "grant_type": "client_credentials"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
}
```

## Coverage Targets (ADR-0005)

| Component | Target | Status |
|-----------|--------|--------|
| Crypto | 100% | ✅ Achieved (Phase 3) |
| Handlers | 95% | ⏳ In Progress |
| Services | 95% | ⏳ In Progress |
| Repositories | 95% | ⏳ In Progress |
| Middleware | 90% | ⏳ In Progress |
| Overall | 90%+ | ⏳ In Progress |

## CI/CD

Tests run automatically on every push and pull request via GitHub Actions:

- ✅ Unit tests
- ✅ Integration tests (with PostgreSQL service container)
- ✅ E2E tests
- ✅ Coverage reporting (Codecov)
- ✅ Performance benchmarks

See `.github/workflows/ci.yml` for configuration.

## Troubleshooting

### Database Connection Errors

```bash
# Check PostgreSQL is running
docker ps | grep postgres-test

# Check database exists
docker exec -it dark-tower-postgres-test psql -U postgres -c "\l"

# Restart PostgreSQL
docker-compose -f docker-compose.test.yml restart postgres-test
```

### Slow Tests

```bash
# Run with timing information
cargo test -- --show-output --nocapture

# Run specific slow test
cargo test test_name -- --nocapture
```

### Flaky Tests

If you encounter flaky tests, ensure:
1. Using deterministic test data (fixed UUIDs, seeded crypto keys)
2. Proper database isolation (`sqlx::test` macro)
3. No shared mutable state between tests
4. Cleanup in Drop/defer handlers

## References

- ADR-0005: Integration and End-to-End Testing Strategy
- `docs/debates/2025-01-testing-strategy.md`: Testing strategy debate
- `crates/ac-test-utils/src/lib.rs`: Test utilities documentation
