# Auth Controller Specialist Checkpoint

**Date**: 2026-02-01
**Specialist**: auth-controller
**Task**: Implement shared TokenManager in common crate for OAuth 2.0 client credentials flow
**Iteration**: 4 (Final - All Reviews Approved)

---

## Architectural Changes (Iteration 2)

### Major Refactor: Struct to Function-Based API

**Before (Iteration 1)**:
```rust
pub struct TokenManager { ... }  // With Clone trait
impl TokenManager {
    pub fn new(config) -> Result<Self, TokenError>;
    pub async fn get_token(&self) -> Result<String, TokenError>;
    pub async fn force_refresh(&self) -> Result<(), TokenError>;
    pub fn subscribe(&self) -> watch::Receiver<TokenState>;
    pub fn shutdown(&self);
}
```

**After (Iteration 2)**:
```rust
pub struct TokenReceiver(watch::Receiver<SecretString>);
impl TokenReceiver {
    pub fn token(&self) -> SecretString;  // Always valid after spawn
    pub async fn changed(&mut self) -> Result<(), TokenError>;
}

pub async fn spawn_token_manager(config) -> Result<(JoinHandle<()>, TokenReceiver), TokenError>;
```

### Key Changes

1. **Removed TokenManager struct** - No more clonable struct with multiple Arc fields
2. **Function-based API** - `spawn_token_manager()` returns `(JoinHandle, TokenReceiver)`
3. **No Arc wrappers** - Background task owns all data directly
4. **Removed methods**:
   - `get_token()` -> Use `receiver.token()` instead
   - `force_refresh()` -> Removed (task handles everything)
   - `subscribe()` -> Function returns receiver
   - `shutdown()` -> Use `handle.abort()` or drop handle
5. **Removed MAX_INITIAL_RETRIES** - Infinite retry, caller controls timeout
6. **Duration constants** - Changed from `u64` to `Duration`
7. **Empty string sentinel** - Watch channel starts with empty string
8. **Blocking initial acquisition** - `spawn_token_manager` waits for first token

---

## Patterns Discovered

### 1. Function-Based Async Spawning Pattern
- Return `(JoinHandle<()>, Receiver)` tuple
- Function waits for initialization before returning
- Caller has guaranteed valid state after call returns
- Simplifies API - no explicit shutdown needed

### 2. Owned Data in Spawned Tasks
- Task owns config, http_client, sender directly
- No Arc wrappers needed when task is sole owner
- Simpler memory model, no reference counting overhead

### 3. Empty Sentinel for Watch Channel
- Initialize with empty string as "not ready" sentinel
- Wait for `changed()` to know first real value arrived
- Check for empty as defensive validation

### 4. Duration Constants (const fn)
- `pub const DEFAULT_REFRESH_THRESHOLD: Duration = Duration::from_secs(300);`
- Type-safe, no conversion needed at use sites
- Clear intent in API

### 5. Form-Encoded OAuth Request
- AC expects `application/x-www-form-urlencoded` body
- Using reqwest `.form()` builder for correct encoding
- Grant type: `client_credentials`

---

## Gotchas Encountered

### 1. Receiver Borrow Lock
- `watch::Receiver::borrow()` holds a lock that blocks sender
- Must clone immediately to avoid blocking: `self.0.borrow().clone()`
- TokenReceiver wrapper enforces this pattern

### 2. Infinite Retry Semantics
- Without MAX_RETRIES, function can hang forever
- Caller must wrap in `tokio::time::timeout()` if needed
- This is intentional - service decides its startup timeout

### 3. SecretString in Watch Channel
- Watch channel contains `SecretString` not `String`
- Receivers get `SecretString` from `token()` method
- Caller must use `.expose_secret()` when needed

---

## Key Decisions

### 1. Function vs Struct API
**Chose**: Function-based `spawn_token_manager()`
**Reason**: Simpler ownership, no Clone needed, clearer lifecycle

### 2. Infinite Retry
**Chose**: No retry limit on initial acquisition
**Reason**: Caller controls timeout at service startup level

### 3. JoinHandle for Shutdown
**Chose**: Return JoinHandle, caller calls `abort()`
**Reason**: Standard tokio pattern, no custom shutdown mechanism

### 4. Duration Constants
**Chose**: `const Duration` instead of `const u64`
**Reason**: Type-safe, clearer API, no conversion at use sites

### 5. TokenReceiver Wrapper
**Chose**: Wrapper struct around `watch::Receiver`
**Reason**: Enforces clone-on-read pattern, adds custom Debug

---

## Current Status

**Implementation**: Complete (Iteration 2)
**Verification**: All 7 layers passed

### Files Modified
- `crates/common/src/token_manager.rs` - Complete rewrite (~470 lines, down from ~1000)

### API Surface (Final)
```rust
// Constants
pub const DEFAULT_REFRESH_THRESHOLD: Duration = Duration::from_secs(300);
pub const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(10);

// Configuration
pub struct TokenManagerConfig { ... }
impl TokenManagerConfig {
    pub fn new(ac_endpoint, client_id, client_secret) -> Self;
    pub fn with_refresh_threshold(self, Duration) -> Self;
    pub fn with_http_timeout(self, Duration) -> Self;
}

// Token Receiver
pub struct TokenReceiver(watch::Receiver<SecretString>);
impl TokenReceiver {
    pub fn token(&self) -> SecretString;
    pub async fn changed(&mut self) -> Result<(), TokenError>;
}
impl Clone for TokenReceiver { ... }
impl Debug for TokenReceiver { ... }  // Redacts token

// Spawn function
pub async fn spawn_token_manager(config) -> Result<(JoinHandle<()>, TokenReceiver), TokenError>;

// Errors
pub enum TokenError {
    AcquisitionFailed(String),
    HttpError(String),
    AuthenticationRejected(String),
    InvalidResponse(String),
    Configuration(String),
    ChannelClosed,
}
```

### Test Coverage
- Configuration tests (defaults, builder, debug redaction, duration constants)
- TokenReceiver tests (debug redaction, clone)
- Token acquisition tests (success, retry on 500)
- Token refresh test (automatic refresh before expiry)
- Changed notification test
- Abort/cleanup test

---

## Usage Example (Updated)

```rust
use common::token_manager::{spawn_token_manager, TokenManagerConfig};
use common::secret::{SecretString, ExposeSecret};
use std::time::Duration;

// Create configuration
let config = TokenManagerConfig::new(
    "http://localhost:8082".to_string(),
    "gc-service".to_string(),
    SecretString::from("client-secret"),
);

// Spawn manager - blocks until first token acquired
// Wrap in timeout if needed:
// tokio::time::timeout(Duration::from_secs(30), spawn_token_manager(config)).await??
let (task_handle, token_rx) = spawn_token_manager(config).await?;

// Pass token_rx.clone() to all tasks that need tokens
let rx_for_task = token_rx.clone();

// Get token (always valid after spawn returns)
let token = token_rx.token();
let header = format!("Bearer {}", token.expose_secret());

// To shutdown: abort the task handle
task_handle.abort();
```

---

## Next Steps

1. Integrate into GC service
2. Integrate into MC service
3. Add metrics for token refresh (via tracing)

---

## Reflection (Iteration 4)

### Iterations Summary

| Iteration | Focus | Outcome |
|-----------|-------|---------|
| 1 | Initial implementation | Working but complex API |
| 2 | Architectural refactor | Simplified to function-based API |
| 3 | Code review fixes | 14 findings fixed (security, tests, code quality) |
| 4 | Clippy/semantic fixes | 5 findings fixed (format args, constant assertions, client_id sensitivity) |

### Key Learnings

1. **Function-based APIs beat struct-based for async spawning** - Returning `(JoinHandle, Receiver)` eliminates Arc wrappers and simplifies ownership. The spawning function can block until ready, guaranteeing valid state to callers.

2. **client_id is considered sensitive** - The semantic guard treats client_id similarly to credentials. Even though it's less sensitive than client_secret, it should not appear in tracing instrument fields.

3. **Clippy `assertions_on_constants`** - Using `assert!(CONST > 0)` in tests triggers warnings because the assertion is optimized away. Use `assert_eq!(CONST, expected)` instead.

4. **Watch channel borrow semantics matter** - `borrow()` holds a lock that blocks senders. Must clone immediately. Wrapper types (TokenReceiver) can enforce this pattern.

5. **HTTPS enforcement via constructor variants** - `new()` for dev, `new_secure()` for prod provides flexibility while making security explicit.

### Knowledge Updates

Added 3 patterns, 4 gotchas, 1 integration entry to auth-controller specialist knowledge files.

### Final Status

- **Code**: 27 tests, ~470 lines production code
- **Reviews**: All 4 reviewers approved
- **Verification**: All 7 layers passed
