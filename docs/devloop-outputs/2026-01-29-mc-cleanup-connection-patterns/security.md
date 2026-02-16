# Security Review: MC Cleanup - Connection Patterns

**Reviewer**: Security Specialist
**Date**: 2026-01-30
**Task**: MC cleanup: remove legacy proto methods and fix connection patterns (Arc<RwLock> removal)

## Summary

This review examines the changes to the Meeting Controller that simplify connection handling by removing unnecessary `Arc<RwLock>` wrappers around connections that are designed to be cheaply cloneable.

## Files Reviewed

1. `proto/internal.proto` - Removed legacy RPC methods
2. `crates/meeting-controller/src/grpc/gc_client.rs` - Simplified channel handling
3. `crates/meeting-controller/src/grpc/mc_service.rs` - Removed legacy method implementations
4. `crates/meeting-controller/src/redis/client.rs` - Made Clone, removed locking

## Security Analysis

### 1. Credential Protection

**Status**: PASS

- **Redis URL**: Stored as `SecretString` in `Config` (line 45 in config.rs)
- **Redis client**: Does NOT log the URL on error (line 99-106 in client.rs):
  ```rust
  // Note: Do NOT log redis_url as it may contain credentials
  // (e.g., redis://:password@host:port)
  error!(
      target: "mc.redis.client",
      error = %e,
      "Failed to open Redis client"
  );
  ```
- **Service token**: Stored as `SecretString` in `GcClient` (line 64 in gc_client.rs)
- **Binding token secret**: Stored as `SecretString` in `Config` (line 89 in config.rs)
- **Config Debug impl**: Properly redacts `redis_url` and `binding_token_secret` (lines 96, 116 in config.rs)

### 2. Authorization Header Handling

**Status**: PASS

- Bearer token format uses `"Bearer "` prefix correctly (line 142 in gc_client.rs)
- Token is exposed only at the point of use via `expose_secret()`
- Authorization header is inserted using tonic's metadata API

### 3. Error Message Sanitization

**Status**: PASS

- `McError::client_message()` returns generic messages for internal errors (lines 131-134 in errors.rs)
- Redis, gRPC, Config, and Internal errors all return "An internal error occurred"
- No credential or internal details leak to clients

### 4. Race Condition Analysis

**Status**: PASS

The simplification from `Arc<RwLock<T>>` to direct `T` is safe:

**GcClient (tonic Channel)**:
- Tonic `Channel` is explicitly designed for concurrent use
- Documentation states: "Channel provides a Clone implementation that is cheap"
- Internal buffering handles concurrent requests safely
- No shared mutable state requires external synchronization

**FencedRedisClient (MultiplexedConnection)**:
- Redis-rs `MultiplexedConnection` documentation: "cheap to clone and can be used safely concurrently"
- Each operation clones the connection for isolation
- Lua scripts provide atomicity for fenced operations
- Generation counters use atomic Redis operations (INCR)

**Atomic counters in McAssignmentService**:
- `current_meetings`, `current_participants`, `is_draining` use `AtomicU32`/`AtomicBool`
- `Ordering::SeqCst` ensures visibility across threads (conservative but correct)

### 5. Fencing Token Security

**Status**: PASS

- Fencing tokens prevent split-brain scenarios during failover
- Generation counters are monotonically increasing
- Lua scripts enforce atomic check-and-set semantics
- `FencedOut` error is logged server-side, client receives generic message

### 6. Input Validation

**Status**: PASS

- Meeting IDs, MH IDs are validated through proto message parsing
- Redis key construction uses format strings (no injection risk)
- Endpoint validation happens at tonic Endpoint parsing

### 7. No Panic Policy

**Status**: PASS

- All fallible operations return `Result<T, E>`
- Error mapping provides context without panicking
- Test code is properly annotated with `#[allow(clippy::unwrap_used)]`

## Findings

### TECH_DEBT-001: Local Generation Cache Unused

**Severity**: TECH_DEBT
**Location**: `crates/meeting-controller/src/redis/client.rs:72-74`

The `local_generation` cache is populated but never read:
```rust
#[allow(dead_code)]
local_generation: Arc<RwLock<std::collections::HashMap<String, u64>>>,
```

The comment indicates this is intentional for Phase 6d. No security impact, but the `#[allow(dead_code)]` annotation correctly documents this.

**Recommendation**: Track in tech debt; ensure Phase 6d implements the read path or removes the cache.

### TECH_DEBT-002: Redundant gc_grpc_url and gc_grpc_endpoint Fields

**Severity**: TECH_DEBT
**Location**: `crates/meeting-controller/src/config.rs:60-63`

Config has both `gc_grpc_url` and `gc_grpc_endpoint` which are set to the same value:
```rust
gc_grpc_endpoint: gc_grpc_url.clone(),
gc_grpc_url,
```

No security impact, but could cause confusion.

**Recommendation**: Consolidate to single field in future cleanup.

## Verdict

**APPROVED**

The changes correctly simplify the connection handling by removing unnecessary locking around types that are designed for concurrent use. Security controls remain intact:

- Credentials are protected by `SecretString`
- Error messages are sanitized for clients
- Fencing tokens prevent split-brain scenarios
- Authorization headers are properly formed
- No race conditions introduced by the simplification

## Finding Summary

| Severity | Count | Description |
|----------|-------|-------------|
| BLOCKER | 0 | - |
| CRITICAL | 0 | - |
| MAJOR | 0 | - |
| MINOR | 0 | - |
| TECH_DEBT | 2 | Unused generation cache, redundant config fields |
