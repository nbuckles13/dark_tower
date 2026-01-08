# Principle: Testing

**All tests MUST be deterministic, isolated, and fast.** Use real PostgreSQL, fixed test data, and clear tier boundaries.

**ADRs**: ADR-0005 (Integration Testing), ADR-0009 (Test Infrastructure), ADR-0014 (Environment Tests)

---

## DO

### Test Ownership
- **Service specialists write unit and subsystem tests** - they know the domain deeply
- **Test specialist writes integration and E2E tests** - ensures coverage, isolation, quality
- **Security tests require Security specialist review** - all security-critical paths

### Test Organization
- **Use three tiers** - Unit (no DB), Integration (real DB + Tower), E2E (real server)
- **Use `#[sqlx::test(migrations = "...")]`** for database tests - automatic isolation
- **Name tests** `test_<function>_<scenario>_<expected_result>`
- **Structure as Arrange-Act-Assert** - clear separation of setup, execution, verification

### Database Testing
- **Use real PostgreSQL** - matches production, tests actual queries and constraints
- **Per-test isolation** via sqlx::test macro - each test gets fresh database
- **Run migrations in tests** - validates schema correctness
- **Manipulate time via database** for rate limit tests - not mock clocks

### Determinism
- **Use fixed UUIDs** - `Uuid::from_u128(1)` for reproducibility
- **Use seeded RNG** for crypto fixtures - same seed produces same keys
- **No random test data** - every test run must produce identical results

### Test Utilities
- **Return `Result<T, E>`** from test utility functions (ADR-0002 compliance)
- **Use builder patterns** - `TestTokenBuilder::new().with_scope(...).build()`
- **Use custom assertions** - `token.assert_valid_jwt().assert_has_scope(...)`
- **Add production safety guards** - `#[cfg(not(test))] compile_error!(...)`

### Environment Tests
- **Use Cargo features** - smoke, flows, observability, resilience
- **No default features** - `cargo test` from repo root runs 0 env-tests
- **Document prerequisites** - kind cluster, port-forwards must be running
- **Use eventual consistency helpers** - `assert_eventually` with category timeouts

---

## DON'T

### Test Utilities
- **NEVER use `.unwrap()` in test utility library code** - return Result instead
- **NEVER use test secrets in production** - compile guards prevent this

### Database
- **NEVER use mock/in-memory databases** for integration tests - use real PostgreSQL
- **NEVER share database state** between tests - causes flaky failures
- **NEVER use SQLite** - doesn't support PostgreSQL-specific features

### Test Data
- **NEVER use random test data** - breaks reproducibility
- **NEVER use non-deterministic timestamps** - use fixed or seeded values

### Test Quality
- **NEVER accept flaky tests** - fix or remove immediately
- **NEVER skip coverage thresholds** - enforce in CI

---

## Quick Reference

### Test Ownership

| Test Type | Owner | Scope |
|-----------|-------|-------|
| Unit | Service specialist | Single function/module |
| Subsystem | Service specialist | Single service layer |
| Integration | Test specialist | Cross-layer within service |
| E2E | Test specialist | Full stack, multi-service |
| Security | Test + Security review | Security-critical paths |

### Test Tiers

| Tier | Database | HTTP | External Services | Timeout |
|------|----------|------|-------------------|---------|
| Unit | None/Mock | No | Mock | 10s |
| Integration | Real PostgreSQL | Tower ServiceExt | Mock | 30s |
| E2E | Real PostgreSQL | Real server + reqwest | Mock or Real | 2min |

### Coverage Targets

| Module | Target |
|--------|--------|
| Crypto | 100% |
| Handlers | 95% |
| Services | 95% |
| Repositories | 95% |
| Middleware | 90% |
| **Overall** | **90%** |

### Performance Targets

| Test Type | Target | CI Timeout |
|-----------|--------|------------|
| Unit | <1s | 10s |
| Integration | <5s | 30s |
| E2E | <30s | 2min |
| **Total CI** | **<2min** | **5min** |

### Naming Convention

`test_<function>_<scenario>_<expected_result>`

Examples:
- `test_issue_token_valid_credentials_returns_jwt`
- `test_issue_token_expired_credentials_returns_401`

---

## Guards

**Coverage**: `cargo-llvm-cov` with thresholds in `.codecov.yml`
**Performance**: CI timeouts enforce test speed limits
**Safety**: `#[cfg(not(test))] compile_error!(...)` prevents test utilities in production
