# Meeting Controller Specialist Checkpoint

**Date**: 2026-01-25
**Phase**: 6c (GC Integration)
**Status**: Complete

---

## Patterns Discovered

### 1. Actor Handle/Task Separation for gRPC Services
When integrating gRPC services with the actor model, the service implementation holds references to actor handles. This maintains the actor pattern where all state mutations go through message passing.

```rust
pub struct McAssignmentService {
    controller_handle: Arc<MeetingControllerActorHandle>,
    redis_client: Arc<FencedRedisClient>,
    // ...
}
```

### 2. Fenced Redis Operations with Lua Scripts
Using Lua scripts for fenced operations ensures atomicity:
- Read generation, compare, write is atomic
- Prevents split-brain by rejecting stale generations
- `EVALSHA` with fallback to `EVAL` handles script caching

### 3. Connection Channel Caching with Reconnection
Both GcClient and FencedRedisClient cache connections but handle reconnection:
- `RwLock<Option<Channel>>` allows lock-free reads
- Failed operations trigger channel clearing
- Next operation creates fresh connection

### 4. Capacity Checks Before Assignment Acceptance
Check capacity atomically using AtomicU32:
```rust
fn can_accept_meeting(&self) -> Option<RejectionReason> {
    if self.is_draining.load(Ordering::SeqCst) {
        return Some(RejectionReason::Draining);
    }
    // ... more checks
}
```

---

## Gotchas Encountered

### 1. Redis Script API Borrow Checker Issues
The `Script::key().key()` chain creates temporary values that don't live long enough. Solution: Use raw `redis::cmd("EVALSHA")` or pre-build args vector.

**Wrong:**
```rust
let mut invocation = script.key(&k1).key(&k2);
invocation = invocation.arg(v); // Error: temporary value dropped
```

**Correct:**
```rust
let mut cmd = redis::cmd("EVALSHA");
cmd.arg(script.get_hash()).arg(2).arg(&k1).arg(&k2).arg(v);
```

### 2. Config Field Duplication for API Compatibility
When adding `gc_grpc_endpoint` as alias for `gc_grpc_url`, both fields need to be initialized in `from_vars()`:
```rust
gc_grpc_endpoint: gc_grpc_url.clone(),
gc_grpc_url,
```

### 3. Test Config Completeness
Test configs must include ALL struct fields, even when using defaults in the actual config loader. Missing fields cause compile errors in test modules.

### 4. SecretString in Config Structs (Fix Iteration 2)
When changing fields from `String` to `SecretString`:
1. Update all test configs to use `SecretString::from()`
2. Update assertions that compare secret values to use `.expose_secret()`
3. Doc comments should use backticks: `` `SecretString` `` for clippy::doc_markdown

### 5. Testing Capacity Logic Without Dependencies (Fix Iteration 2)
Instead of creating full service instances with mocked Redis/actors, extract the logic into a standalone function that mirrors the actual implementation. This allows comprehensive testing without complex setup:
```rust
fn check_capacity(is_draining: bool, ...) -> Option<RejectionReason> {
    // Same logic as can_accept_meeting()
}
```

### 6. Auth Interceptor for Defense-in-Depth (Fix Iteration 3)
When exposing gRPC services that accept requests from other services (GC→MC), add an auth interceptor for defense-in-depth even if transport-level security (mTLS) exists:
```rust
pub struct McAuthInterceptor { require_auth: bool }

impl Interceptor for McAuthInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        // Validate authorization header format
        // Full JWT validation deferred to JWKS integration
    }
}
```
The interceptor can be disabled for testing with `#[cfg(test)]` helper.

### 7. Credential Redaction in Error Logs (Fix Iteration 3)
Never log URLs that may contain embedded credentials (e.g., `redis://:password@host`). Remove sensitive fields from error logs or parse and redact them before logging.

---

## Key Decisions

### 1. Redis Key Patterns (ADR-0023 Section 6)
- `meeting:{id}:generation` - Fencing generation counter
- `meeting:{id}:mh` - MH assignment JSON
- `meeting:{id}:state` - Meeting metadata (HASH)

### 2. Accept/Reject Logic (ADR-0023 Section 5b)
MC rejects if:
- `is_draining == true` -> `RejectionReason::Draining`
- `current_meetings >= max_meetings` -> `RejectionReason::AtCapacity`
- `current_participants + estimated > max_participants` -> `RejectionReason::AtCapacity`

### 3. Registration Retry Strategy
- Max 5 retries with exponential backoff (1s, 2s, 4s, 8s, 16s max 30s)
- Store GC-provided heartbeat intervals for subsequent heartbeats
- Clear channel cache on failure to force reconnection

---

## Current Status

### Completed
- [x] `grpc/mod.rs` - Module structure
- [x] `grpc/gc_client.rs` - MC->GC client (Register, FastHeartbeat, ComprehensiveHeartbeat)
- [x] `grpc/mc_service.rs` - GC->MC service (AssignMeetingWithMh)
- [x] `redis/mod.rs` - Module structure
- [x] `redis/lua_scripts.rs` - Fenced write/delete Lua scripts
- [x] `redis/client.rs` - FencedRedisClient with generation-based fencing
- [x] Updated `lib.rs` to export new modules
- [x] Updated `config.rs` with `gc_grpc_endpoint` field

### Verification Results (Post Fix Iteration 3)
- Layer 1 (check): PASS
- Layer 2 (fmt): PASS
- Layer 3 (guards): PASS (8/8)
- Layer 4 (unit tests): PASS (115/115)
- Layer 5 (all tests): PASS (134/134 - meeting-controller + common + mc-test-utils)
- Layer 6 (clippy): PASS
- Layer 7 (semantic guards): PASS (skipped - disabled)

### Fix Iteration 2 Changes
- Changed `redis_url` and `binding_token_secret` to `SecretString` for security
- Added `McError::Grpc` variant for proper error categorization
- Extracted `ESTIMATED_PARTICIPANTS_PER_MEETING` constant with documentation
- Added doc comment for `store_mh_assignment` explaining generation semantics
- Added 31 new tests covering:
  - Capacity check logic (8 tests)
  - GcClient behavior (7 tests)
  - Lua script behavioral verification (11 tests)
  - Redis client edge cases (5 tests)

### Fix Iteration 3 Changes
- Created `McAuthInterceptor` for GC request authorization validation (defense-in-depth)
- Removed Redis URL from connection error logs to prevent credential leakage
- Added 13 new tests for auth interceptor:
  - Missing authorization header
  - Invalid format (Basic auth, Token, lowercase bearer)
  - Empty token
  - Oversized token (8KB limit)
  - Valid token acceptance
  - Token extraction helper
  - Disabled mode (for testing)

### Not Implemented (Future Phases)
- [ ] Actual Redis integration tests (require Redis)
- [ ] Heartbeat background task
- [ ] Main.rs wiring of GC client and MC service
- [ ] WebTransport server integration

---

## Files Changed

### Created
- `crates/meeting-controller/src/grpc/mod.rs`
- `crates/meeting-controller/src/grpc/gc_client.rs`
- `crates/meeting-controller/src/grpc/mc_service.rs`
- `crates/meeting-controller/src/grpc/auth_interceptor.rs` (Fix Iteration 3)
- `crates/meeting-controller/src/redis/mod.rs`
- `crates/meeting-controller/src/redis/lua_scripts.rs`
- `crates/meeting-controller/src/redis/client.rs`

### Modified
- `crates/meeting-controller/src/lib.rs`
- `crates/meeting-controller/src/config.rs`
- `crates/meeting-controller/src/grpc/mod.rs` (Fix Iteration 3 - added auth_interceptor export)
- `crates/meeting-controller/src/redis/client.rs` (Fix Iteration 3 - removed URL from error log)
