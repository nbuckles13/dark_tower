# Principle: Database Queries

## DO

- **Use sqlx with compile-time query checking** - Use `sqlx::query!`, `sqlx::query_as!`, or `sqlx::query_as::<_, Type>()` macros for all queries
- **ALWAYS parameterize queries** - Use PostgreSQL's `$1`, `$2`, `$3` placeholders for ALL user input
- **Use transactions for multi-step operations** - Wrap related updates in `pool.begin()` / `tx.commit()` blocks to ensure atomicity
- **Include proper indexing** - Create indexes on columns used in WHERE clauses, JOINs, and ORDER BY clauses
- **Validate input lengths BEFORE queries** - Check string lengths, array sizes, and numeric ranges at API boundaries
- **Map database errors at boundaries** - Convert sqlx errors to domain-specific error types (e.g., `AcError`)
- **Use partial indexes for filtered queries** - Add `WHERE` clauses to indexes for commonly filtered columns (e.g., `WHERE is_active = true`)
- **Return `Result<T, E>` from all database functions** - Never unwrap or expect in repository code
- **Use advisory locks for critical sections** - Prevent TOCTOU races with `pg_advisory_xact_lock()` in transactions
- **Test SQL injection prevention** - Include tests with malicious inputs (`' OR '1'='1`, `UNION SELECT`, etc.)

## DON'T

- **NEVER concatenate strings for SQL** - Even for table/column names, use whitelisting instead
- **NEVER use `format!()` or string interpolation for SQL** - This bypasses parameterization and enables SQL injection
- **NEVER trust user input in queries** - Always validate, sanitize, and parameterize
- **Don't forget WHERE clauses** - Unfiltered queries can cause table scans or data leaks
- **Don't skip transaction cleanup** - Always handle commit/rollback, even on errors
- **Don't use `SELECT *` in production code** - Explicitly list columns for stability and performance
- **Don't index every column** - Over-indexing slows writes; index only frequently queried columns
- **Don't ignore sqlx compile-time warnings** - Fix type mismatches and missing columns immediately
- **Don't mix raw queries with type-safe queries** - Prefer `query_as::<_, Type>()` over manual deserialization
- **Don't commit secrets to migrations** - Use environment variables for sensitive data

## Examples

### Good: Parameterized Query with sqlx

```rust
// ✅ GOOD: Type-safe, compile-time checked, parameterized
let credential = sqlx::query_as::<_, ServiceCredential>(
    r#"
    SELECT credential_id, client_id, client_secret_hash, service_type, region,
           scopes, is_active, created_at, updated_at
    FROM service_credentials
    WHERE client_id = $1
    "#,
)
.bind(client_id)  // Automatically escaped and parameterized
.fetch_optional(pool)
.await
.map_err(|e| AcError::Database(format!("Failed to fetch credential: {}", e)))?;
```

### Bad: String Concatenation (SQL Injection Vulnerable)

```rust
// ❌ BAD: SQL injection vulnerability!
let query = format!(
    "SELECT * FROM service_credentials WHERE client_id = '{}'",
    client_id  // If client_id = "' OR '1'='1", leaks all rows!
);
let credential = sqlx::query(&query).fetch_one(pool).await?;
```

### Good: Transaction with Error Handling

```rust
// ✅ GOOD: Atomic multi-step operation
let mut tx = pool.begin().await
    .map_err(|e| AcError::Database(format!("Failed to start transaction: {}", e)))?;

// Deactivate old keys
sqlx::query("UPDATE signing_keys SET is_active = false WHERE is_active = true")
    .execute(&mut *tx)
    .await?;

// Activate new key
sqlx::query("UPDATE signing_keys SET is_active = true WHERE key_id = $1")
    .bind(new_key_id)
    .execute(&mut *tx)
    .await?;

// Commit transaction (or auto-rollback on error)
tx.commit().await
    .map_err(|e| AcError::Database(format!("Failed to commit rotation: {}", e)))?;
```

### Good: Indexed Query

```sql
-- ✅ GOOD: Index on frequently queried column
CREATE INDEX idx_service_credentials_client_id ON service_credentials(client_id);

-- ✅ GOOD: Partial index for filtered queries
CREATE INDEX idx_service_credentials_active
    ON service_credentials(is_active)
    WHERE is_active = true;
```

### Good: Advisory Lock to Prevent Race Conditions

```rust
// ✅ GOOD: Prevent TOCTOU with advisory lock
let mut tx = pool.begin().await?;

// Acquire transaction-scoped lock (auto-released on commit/rollback)
sqlx::query("SELECT pg_advisory_xact_lock($1)")
    .bind(lock_id)
    .execute(&mut *tx)
    .await?;

// Perform critical operation (e.g., check-then-update)
let exists = check_exists(&mut tx).await?;
if !exists {
    create_resource(&mut tx).await?;
}

tx.commit().await?;  // Lock released here
```

### Good: SQL Injection Test

```rust
#[sqlx::test(migrations = "../../migrations")]
async fn test_boolean_injection_prevented(pool: PgPool) -> Result<(), AcError> {
    let malicious_id = "' OR '1'='1";

    // Create credential with malicious client_id
    service_credentials::create_service_credential(
        &pool,
        malicious_id,
        &secret_hash,
        "global-controller",
        None,
        &["test:scope".to_string()],
    )
    .await?;

    // Verify sqlx parameterization treats it as literal string
    let result = service_credentials::get_by_client_id(&pool, malicious_id).await?;
    assert!(result.is_some(), "Should find exact match");

    // Verify only ONE row returned (not all rows)
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM service_credentials WHERE client_id = $1"
    )
    .bind(malicious_id)
    .fetch_one(&pool)
    .await?;

    assert_eq!(count.0, 1, "Should have exactly 1 match, not all rows");
    Ok(())
}
```

## Guards

**Compile-Time Guards** (Recommended):
- Enable sqlx compile-time checking with `DATABASE_URL` environment variable
- Use `sqlx::query_as!()` macro for compile-time type verification
- Run `cargo sqlx prepare` to cache query metadata for CI

**Runtime Guards**:
- Input validation at API boundaries (check lengths, formats, ranges)
- Rate limiting on database-backed endpoints
- Connection pool limits to prevent resource exhaustion
- Query timeouts to prevent long-running queries

**Security Guards**:
- SQL injection tests in test suite (P0/P1 priority)
- Fuzz testing for query parsers
- Database user permissions (least privilege)
- Audit logging for sensitive operations

## ADR References

- **ADR-0002**: No-Panic Policy - All database functions return `Result<T, E>`
- **ADR-0007**: Token Lifetime Strategy - Token validation queries use indexed `valid_until` column

## Migration Best Practices

```sql
-- ✅ GOOD: Use migrations for schema changes
-- File: migrations/20250122000001_add_user_table.sql

CREATE TABLE users (
    user_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes immediately after table
CREATE INDEX idx_users_email ON users(email) WHERE is_active = true;

-- Add constraints for data integrity
ALTER TABLE users ADD CONSTRAINT users_email_check CHECK (email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$');
```

## Performance Tips

- **Use `EXPLAIN ANALYZE`** - Profile slow queries to identify missing indexes
- **Batch inserts** - Use `INSERT ... VALUES ($1, $2), ($3, $4)` for multiple rows
- **Limit result sets** - Always use `LIMIT` for paginated queries
- **Use connection pooling** - Reuse connections with `sqlx::PgPool`
- **Avoid N+1 queries** - Use JOINs or batch fetches instead of loops

## Common Patterns

**Fetch Optional**:
```rust
let credential = sqlx::query_as::<_, ServiceCredential>(query)
    .bind(client_id)
    .fetch_optional(pool)  // Returns Option<T>
    .await?;
```

**Fetch One (Errors if 0 or >1 rows)**:
```rust
let credential = sqlx::query_as::<_, ServiceCredential>(query)
    .bind(client_id)
    .fetch_one(pool)  // Errors if not exactly 1 row
    .await?;
```

**Fetch All**:
```rust
let credentials = sqlx::query_as::<_, ServiceCredential>(query)
    .bind(service_type)
    .fetch_all(pool)  // Returns Vec<T>
    .await?;
```

**Execute (For INSERT/UPDATE/DELETE)**:
```rust
let result = sqlx::query("UPDATE users SET is_active = $1 WHERE user_id = $2")
    .bind(false)
    .bind(user_id)
    .execute(pool)  // Returns rows_affected
    .await?;
```
