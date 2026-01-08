# Principle: Database Queries

**All database queries MUST use sqlx compile-time checking and parameterization.** Never concatenate strings for SQL.

**ADRs**: ADR-0002 (No-Panic), ADR-0007 (Token Lifetime)

---

## DO

### Query Safety
- **Use sqlx compile-time macros** - `sqlx::query!`, `sqlx::query_as!`, or `sqlx::query_as::<_, Type>()` for all queries
- **ALWAYS parameterize queries** - Use PostgreSQL `$1`, `$2`, `$3` placeholders for ALL user input
- **Validate input lengths BEFORE queries** - Check string lengths, array sizes, numeric ranges at API boundaries
- **Map database errors at boundaries** - Convert sqlx errors to domain types (e.g., `AcError::Database`)
- **Return `Result<T, E>`** from all database functions - never unwrap in repository code
- **Test SQL injection prevention** - Include tests with malicious inputs (`' OR '1'='1`, `UNION SELECT`)

### Transactions & Locking
- **Use transactions for multi-step operations** - Wrap related updates in `pool.begin()` / `tx.commit()` blocks
- **Handle transaction cleanup** - Always commit or rollback, even on errors (use `?` to auto-rollback)
- **Use advisory locks for critical sections** - Prevent TOCTOU races with `pg_advisory_xact_lock()` in transactions

### Schema & Performance
- **Create indexes on queried columns** - WHERE clauses, JOINs, ORDER BY columns
- **Use partial indexes** - Add `WHERE is_active = true` for filtered queries
- **Explicit column selection** - List columns instead of `SELECT *` for stability

---

## DON'T

### SQL Injection Vectors
- **NEVER concatenate strings for SQL** - Even for table/column names, use whitelisting
- **NEVER use `format!()` for SQL** - Bypasses parameterization, enables injection
- **NEVER trust user input** - Always validate, sanitize, and parameterize

### Anti-Patterns
- **Don't forget WHERE clauses** - Unfiltered queries cause table scans or data leaks
- **Don't skip transaction cleanup** - Always handle commit/rollback
- **Don't use `SELECT *`** - Explicit columns only
- **Don't ignore sqlx warnings** - Fix type mismatches immediately
- **Don't mix raw and type-safe queries** - Prefer `query_as::<_, Type>()`
- **Don't commit secrets to migrations** - Use environment variables

---

## Quick Reference

| Operation | sqlx Method | Returns |
|-----------|-------------|---------|
| Single row (optional) | `.fetch_optional(pool)` | `Option<T>` |
| Single row (required) | `.fetch_one(pool)` | `T` (errors if 0 or >1) |
| Multiple rows | `.fetch_all(pool)` | `Vec<T>` |
| INSERT/UPDATE/DELETE | `.execute(pool)` | `PgQueryResult` |
| Transaction start | `pool.begin()` | `Transaction` |
| Transaction commit | `tx.commit()` | `()` |
| Advisory lock | `SELECT pg_advisory_xact_lock($1)` | Auto-releases on commit |

| Placeholder | Usage |
|-------------|-------|
| `$1`, `$2`, `$3` | PostgreSQL parameter binding |
| `.bind(value)` | Bind parameter to query |

---

## Guards

**Compile-Time**:
- `DATABASE_URL` env var enables sqlx compile-time checking
- `cargo sqlx prepare` caches query metadata for CI

**Runtime**:
- Input validation at API boundaries
- Connection pool limits
- Query timeouts

**Security Tests** (P0/P1):
- SQL injection prevention tests with malicious inputs
- Fuzz testing for query parsers
