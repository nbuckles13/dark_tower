# Chaos Tests for AC Service

This directory contains chaos tests that validate the AC service's behavior under adverse infrastructure conditions.

## Overview

Chaos tests simulate infrastructure failures and concurrent stress scenarios to ensure the service degrades gracefully and maintains consistency during:
- Database connection loss
- Key rotation under load
- Concurrent operations

## Test Categories

### 1. Database Failure Tests (`db_failure_tests.rs`)

Tests that validate graceful degradation when the database becomes unavailable:

#### `test_readiness_returns_503_when_db_unavailable`
- **Purpose**: Verify readiness probe fails when DB is down
- **Behavior**: `/ready` returns 503, signaling K8s to stop routing traffic
- **Security**: Error messages don't leak connection details

#### `test_health_returns_200_when_db_unavailable`
- **Purpose**: Verify liveness probe remains healthy
- **Behavior**: `/health` returns 200 even when DB is down
- **Rationale**: K8s shouldn't restart pods that are alive but temporarily disconnected

#### `test_readiness_error_messages_dont_leak_details`
- **Purpose**: Security validation that errors don't expose infrastructure details
- **Checks**: Error messages don't contain postgres, localhost, ports, credentials, etc.
- **Expected**: Generic "Service dependencies unavailable" message

#### `test_readiness_recovers_when_db_restored`
- **Purpose**: Verify automatic recovery when database connectivity is restored
- **Behavior**: Readiness checks are stateless and immediately report healthy

### 2. Key Rotation Stress Tests (`key_rotation_stress_tests.rs`)

Tests that validate key rotation maintains consistency under concurrent load:

#### `test_key_rotation_during_validation`
- **Purpose**: Verify tokens issued before rotation remain valid after rotation
- **Validates**:
  - Old tokens (signed with Key A) still work after rotating to Key B
  - New tokens use the new key (Key B)
  - JWKS exposes both keys during grace period
  - Tokens are structurally valid JWTs

#### `test_concurrent_rotations_are_serialized`
- **Purpose**: Verify advisory lock prevents race conditions
- **Behavior**: Launch 10 concurrent rotation requests
- **Expected**: Exactly 1 succeeds, 9 are rate-limited
- **Security**: No partial rotations or race conditions

#### `test_validation_works_during_rotation`
- **Purpose**: Verify tokens can be issued and validated during rotation
- **Scenario**:
  1. Issue 5 tokens before rotation
  2. Perform rotation
  3. Issue 5 tokens after rotation
  4. Verify all 10 tokens are well-formed

#### `test_jwks_updates_after_rotation`
- **Purpose**: Verify JWKS endpoint reflects changes immediately
- **Behavior**: New key appears in JWKS immediately after rotation
- **Rationale**: Token validators must discover new keys without delay

#### `test_force_rotation_under_load`
- **Purpose**: Verify force rotation (admin scope) works under concurrent load
- **Scenario**: Issue tokens in background while force rotating
- **Expected**: Rotation succeeds, tokens continue to be issued successfully

## Running Chaos Tests

### Prerequisites

1. **PostgreSQL test database** must be running:
   ```bash
   docker compose -f docker-compose.test.yml up -d
   ```

2. **Environment variable** must be set:
   ```bash
   export DATABASE_URL=postgresql://postgres:postgres@localhost:5432/dark_tower_test
   ```

### Execute Tests

```bash
# Run all chaos tests
cargo test --test chaos_tests

# Run specific test category
cargo test --test chaos_tests db_failure_tests::
cargo test --test chaos_tests key_rotation_stress_tests::

# Run specific test
cargo test --test chaos_tests test_concurrent_rotations_are_serialized
```

### CI/CD Integration

Chaos tests run automatically in CI via GitHub Actions:
- Tests run on every push to main
- Tests run on all pull requests
- PostgreSQL is automatically provisioned via service containers
- DATABASE_URL is set automatically in CI environment

## Test Design Principles

### 1. Real Infrastructure
- Tests use real database connections (not mocks)
- Tests spawn actual HTTP servers (via `TestAuthServer`)
- Tests issue real JWT tokens and verify signatures

### 2. Isolation
- Each test gets isolated database via `#[sqlx::test]`
- Database is automatically cleaned up after test
- No test pollution or interdependencies

### 3. Determinism
- Time manipulation via `rotation_time` helpers (no real delays)
- Deterministic client secrets for reproducibility
- Fixed test master keys from `ac-test-utils`

### 4. Security Focus
- Verify error messages don't leak infrastructure details
- Test concurrent operations (TOCTOU protection)
- Validate cryptographic properties (token signatures)

## What's NOT Tested

### Slow Query Timeouts

Slow query timeout tests are **intentionally not implemented**. See `slow_query_notes.md` for detailed rationale.

**Summary**: Testing slow queries requires injecting delays into production code paths (security risk) and doesn't align with test infrastructure. Instead:
- HTTP timeout (30s) is configured at Tower layer (ADR-0012)
- Manual testing in staging is recommended
- Observability metrics track query latency and timeouts

## Test Utilities

Chaos tests leverage shared test utilities from `ac-test-utils`:

- **`TestAuthServer`**: Spawns real HTTP server with isolated database
- **`rotation_time`**: Manipulates database timestamps to simulate time passage
- **`crypto_fixtures`**: Provides deterministic test keys
- **`test_ids`**: Provides fixed UUIDs for reproducible tests

## Debugging Failed Tests

### Database Connection Errors
```
Error: DATABASE_URL must be set
```
**Solution**: Start PostgreSQL and set DATABASE_URL environment variable

### Pool Closed Errors
```
Error: PoolClosed
```
**Solution**: This is expected behavior in DB failure tests. Verify the test logic is correct.

### Rate Limit Failures
```
Expected 1 success, got 2
```
**Solution**: Check if `rotation_time::set_eligible()` is being called. Verify advisory lock is working.

### JWKS Mismatch
```
New key not found in JWKS
```
**Solution**: Keys may have expired. Check validity windows in test setup.

## Future Enhancements

Potential additions to chaos test suite:

1. **Network Partition Tests** (requires Docker network manipulation)
2. **Redis Failure Tests** (when Redis is added for rate limiting)
3. **JWKS Federation Failures** (when multi-cluster support is added)
4. **Cryptographic Failure Tests** (master key unavailable scenarios)

## References

- ADR-0009: Key Rotation Strategy (6-day normal, 1-hour force limits)
- ADR-0012: HTTP Request Timeout (30 seconds)
- ADR-0011: Observability and Metrics
- `ac-test-utils` crate documentation
