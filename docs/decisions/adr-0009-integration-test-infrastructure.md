# ADR-0009: Integration Test Infrastructure

**Status**: Accepted

**Date**: 2025-12-03

**Deciders**: Multi-agent debate (Test, Auth Controller, Global Controller, Meeting Controller, Media Handler, Security)

---

## Context

Dark Tower requires integration test infrastructure to validate cross-component behavior, particularly for security-critical features like key rotation. The Auth Controller's key rotation endpoint (`POST /internal/rotate-keys`) has complex requirements:

- JWT authentication with specific scopes (`service.rotate-keys.ac`, `admin.force-rotate-keys.ac`)
- Database-driven rate limiting (6-day normal, 1-hour force)
- TOCTOU protection via `SELECT FOR UPDATE`
- Audit logging for security events

Existing infrastructure gaps:
- `TestAuthServer::spawn()` unimplemented in `ac-test-utils`
- No time manipulation for rate limit tests
- No concurrent request testing patterns for TOCTOU verification
- No clear path for WebTransport testing (MC/MH future needs)

## Decision

We adopt a **phased integration test infrastructure** with HTTP testing now and WebTransport extensibility documented for future phases.

### 1. TestAuthServer Implementation

The test server harness provides a spawned HTTP server for realistic E2E testing:

```rust
pub struct TestAuthServer {
    addr: SocketAddr,
    pool: PgPool,
    master_key: String,
    _handle: JoinHandle<()>,
}

impl TestAuthServer {
    /// Spawn test server with isolated database
    pub async fn spawn(pool: PgPool) -> Result<Self, AcError> {
        let master_key = crate::TEST_MASTER_KEY.to_string();

        // Initialize signing key
        key_management_service::initialize_signing_key(
            &pool,
            master_key.as_bytes(),
            "test-cluster"
        ).await?;

        // Build app with test config
        let config = Config {
            master_key: master_key.as_bytes().to_vec(),
            // ... other test config
        };
        let state = Arc::new(AppState { pool: pool.clone(), config });
        let app = routes::build_routes(state);

        // Bind to random port
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Ok(Self { addr, pool, master_key, _handle: handle })
    }

    pub fn pool(&self) -> &PgPool { &self.pool }
    pub fn master_key(&self) -> &str { &self.master_key }
    pub fn url(&self) -> String { format!("http://{}", self.addr) }

    /// Create test token with specified scopes
    pub async fn create_test_token(&self, scopes: &[&str]) -> Result<String, AcError> {
        // Create test service credential
        let (client_id, secret) = create_test_service(
            &self.pool,
            "test-client",
            "admin-service",
            scopes
        ).await?;

        // Issue token via service
        let response = token_service::issue_service_token(
            &self.pool,
            self.master_key.as_bytes(),
            &client_id,
            &secret,
            "client_credentials",
            Some(scopes.iter().map(|s| s.to_string()).collect()),
            None,
            None,
        ).await?;

        Ok(response.access_token)
    }
}
```

### 2. Time Manipulation for Rate Limit Tests

Database-level time manipulation enables testing time-dependent behavior without production code changes:

```rust
pub mod time_helpers {
    use chrono::{DateTime, Duration, Utc};
    use sqlx::PgPool;

    /// Set last key rotation timestamp
    pub async fn set_last_key_rotation_time(
        pool: &PgPool,
        timestamp: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE signing_keys
            SET created_at = $1
            WHERE key_id = (
                SELECT key_id FROM signing_keys
                ORDER BY created_at DESC LIMIT 1
            )
            "#,
        )
        .bind(timestamp)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Set rotation eligible (7 days ago) - allows normal rotation
    pub async fn set_rotation_eligible(pool: &PgPool) -> Result<(), sqlx::Error> {
        set_last_key_rotation_time(pool, Utc::now() - Duration::days(7)).await
    }

    /// Set force rotation eligible (2 hours ago) - allows force, blocks normal
    pub async fn set_force_rotation_eligible(pool: &PgPool) -> Result<(), sqlx::Error> {
        set_last_key_rotation_time(pool, Utc::now() - Duration::hours(2)).await
    }

    /// Set rate limited (30 minutes ago) - blocks all rotation
    pub async fn set_rotation_rate_limited(pool: &PgPool) -> Result<(), sqlx::Error> {
        set_last_key_rotation_time(pool, Utc::now() - Duration::minutes(30)).await
    }
}
```

**Rationale**: Database manipulation tests the actual production code path without requiring mock clock injection. This is simpler, faster, and lower risk than modifying production code.

### 3. Security Test Infrastructure

**Hardcoded Test Master Key**:
```rust
/// Deterministic test master key for AES-256-GCM
/// SECURITY: Only available in test builds
pub const TEST_MASTER_KEY: &str = "test_master_key_32_bytes_exactly!!";
```

**Production Safety Guard**:
```rust
// In ac-test-utils crate root
#[cfg(not(test))]
compile_error!("ac-test-utils can only be compiled in test mode. \
               This prevents accidental use of test keys in production.");
```

**Audit Log Assertions**:
```rust
pub struct AuditLogAssertions {
    pool: PgPool,
}

impl AuditLogAssertions {
    pub async fn assert_logged<F>(&self, event_type: &str, predicate: F) -> Result<()>
    where
        F: Fn(&serde_json::Value) -> bool,
    {
        // Query audit logs and verify predicate matches
    }
}
```

### 4. Concurrent Request Testing

For TOCTOU vulnerability testing:

```rust
pub struct ConcurrentRequests {
    count: usize,
}

impl ConcurrentRequests {
    pub fn new(count: usize) -> Self { Self { count } }

    /// Execute closure concurrently, all tasks start simultaneously
    pub async fn execute_synchronized<F, Fut, T>(self, f: F) -> Vec<T>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = T> + Send,
        T: Send + 'static,
    {
        use std::sync::Barrier;

        let f = Arc::new(f);
        let barrier = Arc::new(Barrier::new(self.count));

        let handles: Vec<_> = (0..self.count).map(|_| {
            let f = Arc::clone(&f);
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait(); // All tasks wait here
                f().await       // Then execute simultaneously
            })
        }).collect();

        futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect()
    }
}
```

### 5. Database Isolation Strategy

Use `sqlx::test` macro for per-test database isolation:

```rust
#[sqlx::test(migrations = "../../migrations")]
async fn test_rotate_keys_with_valid_scope_succeeds(pool: PgPool) -> Result<()> {
    // Each test gets fresh database with migrations applied
    let server = TestAuthServer::spawn(pool).await?;
    // ... test implementation
}
```

**Benefits**:
- Complete isolation between tests
- Parallel test execution
- Automatic cleanup
- Migration validation on every test

### 6. Phased Infrastructure Approach

| Phase | Service | Test Infrastructure |
|-------|---------|---------------------|
| **4 (Current)** | AC | `TestAuthServer`, HTTP testing, time helpers, security infrastructure |
| **5** | GC | Shared HTTP patterns in `common-test-utils`, cross-service token validation |
| **6** | MC | `TestWebTransportServer`, Redis testing, session state utilities |
| **7** | MH | Performance benchmarks, QUIC datagram testing, network simulation |

**WebTransport Future Design** (documented, not implemented):
```rust
// Phase 6 - to be implemented
pub trait TestServer {
    fn spawn() -> impl Future<Output = Result<Self>>;
    fn pool(&self) -> &PgPool;
    fn shutdown(self) -> impl Future<Output = ()>;
}

pub struct TestWebTransportServer {
    // QUIC listener, cert management, etc.
}
```

### 7. P0 Key Rotation Tests

Seven required tests for Phase 4 completion:

1. **Happy Path**: `test_rotate_keys_with_valid_scope_succeeds`
2. **Missing Scope**: `test_rotate_keys_without_scope_returns_403`
3. **Wrong Token Type**: `test_rotate_keys_user_token_returns_403`
4. **Expired Token**: `test_rotate_keys_expired_token_returns_401`
5. **Normal Rate Limit**: `test_rotate_keys_within_6_days_returns_429`
6. **Force Rate Limit**: `test_force_rotate_within_1_hour_returns_429`
7. **Force Success**: `test_force_rotate_after_1_hour_succeeds`

### 8. Security Requirements by Priority

**P0 (Required for Phase 4)**:
- Production safety compile guard
- Minimal audit log verification helper
- One RFC 8032 EdDSA test vector
- Token security tests (expiration, scope, type)
- Rate limiting tests

**P1 (Phase 5)**:
- Comprehensive RFC 8032 test vectors
- Audit log content validation
- Key rotation edge cases
- Timing attack resistance tests

**P2 (Future)**:
- Fuzz testing integration
- Chaos testing
- Performance benchmarks under attack

## Consequences

### Positive

1. **Unblocks P0 Tests**: Key rotation integration tests can be written immediately
2. **Security by Design**: Production safety guard prevents test key leakage
3. **Realistic Testing**: Spawned server tests actual HTTP stack
4. **Fast Iteration**: Database time manipulation avoids slow wait times
5. **Future-Ready**: Documented WebTransport path for MC/MH
6. **TOCTOU Coverage**: Concurrent request utilities catch race conditions

### Negative

1. **Phase 4 Focus**: WebTransport infrastructure deferred (acceptable trade-off)
2. **Database Coupling**: Time manipulation requires database access
3. **Test Complexity**: Multiple helpers to learn and maintain

### Neutral

1. **Test Duration**: Spawned server adds ~50ms per test (acceptable)
2. **CI Resources**: Requires PostgreSQL service (already configured)

## Alternatives Considered

### 1. Mock Clock (TimeProvider Trait)

Inject `TimeProvider` trait throughout production code.

**Rejected**: Adds complexity to production code, higher risk of bugs, violates simplicity principle.

### 2. In-Process Testing Only (Tower ServiceExt)

Test without spawning actual HTTP server.

**Partially Accepted**: Use Tower ServiceExt for fast unit/integration tests, but spawned server needed for E2E realism.

### 3. Unified Test Infrastructure Now

Build HTTP + WebTransport infrastructure together.

**Rejected**: Over-engineering. AC needs HTTP only. MC/MH months away. YAGNI.

### 4. Shared Test Database (No Isolation)

All tests share same database instance.

**Rejected**: Causes flaky tests, race conditions, non-reproducible failures.

## Implementation Notes

### Files to Create/Modify

**New/Modified in `ac-test-utils`**:
- `src/server_harness.rs` - Implement `TestAuthServer`
- `src/time_helpers.rs` - Time manipulation utilities
- `src/concurrent.rs` - `ConcurrentRequests` utility
- `src/audit.rs` - `AuditLogAssertions` helper
- `src/lib.rs` - Add `TEST_MASTER_KEY`, compile guard, re-exports

**New in `ac-service/tests`**:
- `integration/key_rotation_tests.rs` - P0 tests

### Test Pattern

```rust
#[sqlx::test(migrations = "../../migrations")]
async fn test_rotate_keys_with_valid_scope_succeeds(pool: PgPool) -> Result<()> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    time_helpers::set_rotation_eligible(&pool).await?;
    let token = server.create_test_token(&["service.rotate-keys.ac"]).await?;

    // Act
    let response = reqwest::Client::new()
        .post(&format!("{}/internal/rotate-keys", server.url()))
        .bearer_auth(&token)
        .send()
        .await?;

    // Assert
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["rotated"], true);

    Ok(())
}
```

## References

- **Debate Record**: `docs/debates/2025-12-03-integration-test-infrastructure.md`
- **Related ADRs**:
  - ADR-0005: Integration Testing Strategy
  - ADR-0008: Key Rotation Implementation
- **Existing Code**:
  - `crates/ac-test-utils/` - Test utilities crate
  - `docker-compose.test.yml` - PostgreSQL test container
