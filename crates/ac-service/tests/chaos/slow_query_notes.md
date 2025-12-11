# Slow Query Timeout Tests - Not Implemented

## Why Not Testable

Slow query timeout tests are not feasible to implement in the current test infrastructure for the following reasons:

### 1. HTTP Timeout Layer (ADR-0012)

The AC service has a 30-second HTTP request timeout configured at the Tower layer:

```rust
// From routes/mod.rs
.layer(TimeoutLayer::new(Duration::from_secs(30)))
```

This timeout applies to the entire HTTP request lifecycle, not individual database queries. Testing this would require:
- Simulating a slow database query that takes >30 seconds
- Verifying the HTTP timeout fires before the database query completes
- Ensuring the connection pool remains healthy after timeout

### 2. Database Connection Pool Timeout

SQLx's connection pool has its own timeout configuration (acquire timeout). Testing slow queries would require:
- Control over the database's query execution time (e.g., `SELECT pg_sleep(31)`)
- Verification that the pool doesn't leak connections
- Confirmation that subsequent queries succeed after a timeout

### 3. Test Infrastructure Limitations

Our test harness uses `#[sqlx::test]` which:
- Creates isolated test databases
- Doesn't provide hooks to inject `pg_sleep()` into production code paths
- Would require raw SQL execution in production code (security risk)

### 4. Security Concerns

Injecting `pg_sleep()` or similar delay mechanisms into production code paths creates security vulnerabilities:
- Opens door to timing-based denial of service attacks
- Complicates production code with test-only paths
- Violates the principle of not testing implementation details

## Alternative Verification Strategies

Instead of chaos tests, slow query behavior is verified through:

### 1. Integration Tests
The existing integration test suite verifies that normal queries complete successfully, implicitly confirming that:
- Connection pool is configured correctly
- HTTP timeout doesn't fire during normal operations
- Database connections are properly released

### 2. Manual Testing
Operators can test slow query timeout behavior in staging environments by:
```sql
-- Simulate slow query (requires superuser privileges)
SELECT pg_sleep(35);
```

Then verify:
- HTTP request times out after 30 seconds (per ADR-0012)
- Connection pool remains healthy
- No connection leaks occur

### 3. Observability
Production monitoring should track:
- `ac_db_query_duration_seconds` - Database query latency
- Connection pool metrics (active, idle, waiting)
- HTTP request timeout events

Alerts should fire if:
- p99 database query latency exceeds 50ms (per SLO)
- Any HTTP request exceeds 30 seconds
- Connection pool exhaustion occurs

## Recommendation

**Do not implement slow query chaos tests** in the current test suite. Instead:

1. Document the HTTP timeout behavior in ADR-0012
2. Add manual testing procedures to the operations runbook
3. Ensure observability metrics cover timeout scenarios
4. Consider load testing in staging with realistic query patterns

## If Tests Are Required

If slow query tests become mandatory, they should be:

1. **Implemented as separate E2E tests** (not unit/integration tests)
2. **Run in dedicated staging environment** with controlled database
3. **Use test-specific database user** with `pg_sleep()` privileges
4. **Explicitly marked as slow tests** (excluded from CI by default)
5. **Documented as requiring manual execution**

Example structure:
```rust
#[test]
#[ignore] // Excluded from CI - requires manual execution
async fn test_slow_query_timeout_e2e() {
    // Requires:
    // - Staging database with pg_sleep() permissions
    // - Extended test timeout (60+ seconds)
    // - Manual verification of connection pool health
}
```

## References

- ADR-0012: HTTP request timeout (30 seconds)
- ADR-0011: Database query SLO (p99 < 50ms)
- `routes/mod.rs`: TimeoutLayer configuration
- SQLx documentation: Connection pool timeout configuration
