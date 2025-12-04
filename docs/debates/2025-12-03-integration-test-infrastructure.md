# Multi-Agent Debate: Integration Test Infrastructure Design

**Date**: 2025-12-03

**Status**: Consensus Achieved (90% average satisfaction)

**Participants**: Test (Lead), Auth Controller, Global Controller, Meeting Controller, Media Handler, Security

**Output**: ADR-0009 Integration Test Infrastructure

---

## Topic

Design foundational integration test infrastructure for Dark Tower services. This infrastructure must:
- Support HTTP endpoint testing (AC, GC) immediately
- Provide extensibility for WebTransport testing (MC, MH) in future phases
- Enable P0 key rotation integration tests for Auth Controller

## Background

### Current State
- `crates/ac-test-utils/` exists with:
  - `crypto_fixtures.rs` - Deterministic Ed25519 keys
  - `token_builders.rs` - Fluent JWT claim builders
  - `test_ids.rs` - Fixed UUIDs and constants
  - `assertions.rs` - TokenAssertions trait
  - `server_harness.rs` - **Unimplemented placeholder**
- `docker-compose.test.yml` - PostgreSQL test container (port 5433)
- ADR-0005 - Three-tier testing strategy (unit/integration/E2E)

### Gaps Identified
- `TestAuthServer::spawn()` unimplemented
- No integration tests for key rotation endpoint
- No time manipulation for rate limit tests (6-day, 1-hour intervals)
- No patterns for WebTransport testing (future)

## Key Questions Debated

1. How should `TestAuthServer::spawn()` be implemented?
2. How should time-dependent tests (rate limiting) be handled?
3. What patterns for test data setup?
4. Database isolation strategy?
5. Shared infrastructure scope across services?
6. WebTransport extensibility for MC/MH?
7. P0 test requirements for key rotation?

---

## Round 1 Summary

### Initial Scores

| Agent | Score | Key Concerns |
|-------|-------|--------------|
| Test (Lead) | 85% | WebTransport design speculative |
| Auth Controller | 72% | **BLOCKING**: `TestAuthServer::spawn()` unimplemented |
| Global Controller | 88% | Cross-service token validation pattern needed |
| Meeting Controller | 75% | WebTransport infrastructure missing |
| Media Handler | 65% | QUIC datagram/performance testing gaps |
| Security | 85% | Need crypto test vectors, audit testing |

**Average: 78%** (below 90% threshold)

### Key Proposals from Round 1

**Test Specialist (Lead)**:
1. Spawned server for E2E realism, Tower ServiceExt for integration speed
2. Database time manipulation for rate limit tests (UPDATE `created_at`)
3. Per-test isolation with `sqlx::test` macro
4. Phased approach: HTTP now, WebTransport in Phase 6
5. Detailed P0 test structure for 7 key rotation tests

**Blocking Concerns**:
- AC: Need `pool()`, `master_key()` accessors on `TestAuthServer`
- Security: Need production safety guard, RFC test vectors
- MC/MH: Need documented WebTransport path (don't block AC)

---

## Round 2 Summary

### Addressed Concerns

**Auth Controller** (72% → 94%):
- Added `pool()`, `master_key()`, `url()` accessors to `TestAuthServer`
- Added `create_test_token(scopes)` helper method
- Added `ConcurrentRequests` utility for TOCTOU testing
- Database time manipulation approach accepted

**Security** (85% → 95%):
- Added hardcoded `TEST_MASTER_KEY` constant
- Added `compile_error!` guard for production safety
- Added minimal RFC 8032 EdDSA test vector
- Added `AuditLogAssertions` helper
- Classified requirements as P0/P1/P2

**Meeting Controller** (75% → 85%):
- Documented Phase 6 WebTransport requirements
- Committed to `TestServer` trait abstraction
- Redis testing strategy deferred to Phase 6

**Media Handler** (65% → 80%):
- Documented Phase 7 performance testing requirements
- Separated fast tests from slow perf tests in CI
- QUIC datagram testing deferred to Phase 7

### Final Scores

| Agent | Score | Status |
|-------|-------|--------|
| Test (Lead) | 95% | ✅ Consensus |
| Auth Controller | 94% | ✅ Consensus |
| Global Controller | 90% | ✅ Consensus |
| Meeting Controller | 85% | ✅ Approved with documentation |
| Media Handler | 80% | ✅ Approved with documentation |
| Security | 95% | ✅ Consensus |

**Average: 90%** - Consensus Achieved!

---

## Consensus Design

### 1. TestAuthServer Implementation

```rust
pub struct TestAuthServer {
    addr: SocketAddr,
    pool: PgPool,
    master_key: String,
    _handle: JoinHandle<()>,
}

impl TestAuthServer {
    pub async fn spawn(pool: PgPool) -> Result<Self, AcError>;
    pub fn pool(&self) -> &PgPool;
    pub fn master_key(&self) -> &str;
    pub fn url(&self) -> String;
    pub async fn create_test_token(&self, scopes: &[&str]) -> Result<String, AcError>;
}
```

### 2. Time Manipulation Helpers

```rust
pub mod time_helpers {
    pub async fn set_last_key_rotation_time(pool: &PgPool, timestamp: DateTime<Utc>);
    pub async fn set_rotation_eligible(pool: &PgPool);       // 7 days ago
    pub async fn set_force_rotation_eligible(pool: &PgPool); // 2 hours ago
    pub async fn set_rotation_rate_limited(pool: &PgPool);   // 30 minutes ago
}
```

### 3. Security Infrastructure

```rust
// Hardcoded test master key (deterministic)
pub const TEST_MASTER_KEY: &str = "test_master_key_32_bytes_exactly!!";

// Production safety guard
#[cfg(not(test))]
compile_error!("ac-test-utils can only be compiled in test mode");

// Audit log assertions
pub struct AuditLogAssertions {
    pub async fn assert_logged(&self, event_type: &str, predicate: impl Fn(&Value) -> bool);
}
```

### 4. Concurrent Request Testing

```rust
pub struct ConcurrentRequests {
    pub fn new(count: usize) -> Self;
    pub async fn execute<F, Fut, T>(self, f: F) -> Vec<T>;
    pub async fn execute_synchronized<F, Fut, T>(self, f: F) -> Vec<T>; // Barrier-based
}
```

### 5. Phased Infrastructure Plan

| Phase | Focus | Test Infrastructure |
|-------|-------|---------------------|
| 4 (Now) | AC | `ac-test-utils`, `TestAuthServer`, HTTP testing |
| 5 | GC | Shared HTTP patterns, time utilities |
| 6 | MC | `webtransport-test-utils`, Redis testing |
| 7 | MH | Performance benchmarks, QUIC datagrams |

### 6. P0 Key Rotation Tests

1. `test_rotate_keys_with_valid_scope_succeeds` - Happy path
2. `test_rotate_keys_without_scope_returns_403` - Missing scope
3. `test_rotate_keys_user_token_returns_403` - Wrong token type
4. `test_rotate_keys_expired_token_returns_401` - Expired JWT
5. `test_rotate_keys_within_6_days_returns_429` - Rate limit (normal)
6. `test_force_rotate_within_1_hour_returns_429` - Rate limit (force)
7. `test_force_rotate_after_1_hour_succeeds` - Force rotation success

### 7. Security Requirements Classification

**P0 (Required for Phase 4)**:
- Production safety compile guard
- Minimal audit log verification
- One RFC 8032 EdDSA test vector
- Token expiration/scope/type tests
- Rate limiting enforcement tests

**P1 (Phase 5)**:
- Comprehensive RFC 8032 test vectors
- Audit log content validation
- Key rotation edge cases
- Timing attack tests

**P2 (Future)**:
- Fuzzing integration
- Chaos testing
- Performance benchmarks

---

## WebTransport Requirements (Documentation Only)

### Phase 6: Meeting Controller

- `TestWebTransportServer` with bidirectional stream support
- Certificate handling (self-signed for tests)
- Session state testing with Redis
- Time-based utilities for connection timeouts

### Phase 7: Media Handler

- QUIC datagram testing (unreliable delivery)
- Network simulation (latency, packet loss, jitter)
- Performance benchmarks (throughput, latency)
- SFU routing test patterns

---

## Implementation Plan

1. **Create ADR-0009**: Document consensus decisions
2. **Implement `TestAuthServer::spawn()`**: Core server harness
3. **Add time helpers**: Database manipulation utilities
4. **Add security infrastructure**: Test key, compile guard, audit assertions
5. **Add concurrent testing**: TOCTOU utilities
6. **Write P0 integration tests**: 7 key rotation tests
7. **Update documentation**: Testing patterns guide

---

## References

- ADR-0005: Integration Testing Strategy
- ADR-0008: Key Rotation Implementation
- Existing: `crates/ac-test-utils/`
- Output: ADR-0009 Integration Test Infrastructure
