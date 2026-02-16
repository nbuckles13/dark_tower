# Global Controller Specialist Checkpoint

**Date**: 2026-02-02
**Task**: Integrate TokenManager into Global Controller
**Status**: Complete

---

## Patterns Discovered

### Pattern: TokenReceiver in AppState for Handler Access

When services (like AcClient) need dynamic OAuth tokens but are created per-request in handlers, store the `TokenReceiver` in AppState rather than passing it through each call chain. The TokenReceiver is Clone and cheap to access.

```rust
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    pub mc_client: Arc<dyn McClientTrait>,
    pub token_receiver: TokenReceiver, // Cloneable, provides .token()
}
```

### Pattern: Test TokenReceiver Factory

Create `TokenReceiver::from_watch_receiver()` as a public constructor for testing purposes. This allows tests to create TokenReceivers with fixed values without spawning the full TokenManager.

```rust
// In common::token_manager
pub fn from_watch_receiver(receiver: watch::Receiver<SecretString>) -> Self {
    Self(receiver)
}

// In tests
fn test_token_receiver(token: &str) -> TokenReceiver {
    let (_tx, rx) = watch::channel(SecretString::from(token));
    TokenReceiver::from_watch_receiver(rx)
}
```

### Pattern: Graceful Shutdown via abort()

TokenManager doesn't use CancellationToken - it uses the watch channel closing as a shutdown signal. For immediate shutdown during service termination, call `token_task_handle.abort()` after cancelling other tasks.

---

## Gotchas Encountered

### Gotcha: Config Debug Redaction for SecretString

When adding `SecretString` fields to Config, remember to update the custom Debug impl to redact the new field. SecretString auto-redacts in its own Debug impl, but for consistency with other credentials (like database_url), use explicit "[REDACTED]".

### Gotcha: Test Utilities Need Config Updates

gc-test-utils/server_harness.rs creates AppState directly for testing. When adding new required config fields or AppState fields, remember to update test harnesses too.

### Gotcha: Handler-Created Clients Need State Access

The `create_ac_client()` helper in meetings.rs was creating AcClient per-request with env var. With TokenReceiver, it needs access to AppState. The solution was adding TokenReceiver to AppState.

---

## Key Decisions

1. **TokenReceiver in AppState**: Rather than passing TokenReceiver through multiple layers, store it in AppState for direct handler access. This is idiomatic for Axum and allows easy Clone.

2. **Required Client Credentials**: Made GC_CLIENT_ID and GC_CLIENT_SECRET required env vars (no defaults). Services should fail fast if not configured.

3. **30-second Startup Timeout**: Added timeout wrapper around spawn_token_manager to prevent hanging on AC unavailability at startup.

4. **Public from_watch_receiver()**: Added public constructor to TokenReceiver for testing. This is cleaner than exposing the internal watch::Receiver type.

---

## Files Modified

- `crates/common/src/token_manager.rs` - Added `from_watch_receiver()` constructor
- `crates/global-controller/src/config.rs` - Added gc_client_id, gc_client_secret fields
- `crates/global-controller/src/main.rs` - Spawn TokenManager, pass to AppState
- `crates/global-controller/src/routes/mod.rs` - Added token_receiver to AppState
- `crates/global-controller/src/services/mc_client.rs` - Use TokenReceiver instead of SecretString
- `crates/global-controller/src/services/ac_client.rs` - Use TokenReceiver instead of String
- `crates/global-controller/src/handlers/meetings.rs` - Update create_ac_client to use state.token_receiver
- `crates/gc-test-utils/src/server_harness.rs` - Create mock TokenReceiver for tests

---

## Verification Results

All 7 verification layers passed:

1. **cargo check --workspace**: PASSED
2. **cargo fmt --all --check**: PASSED (after auto-format)
3. **./scripts/guards/run-guards.sh**: PASSED (9/9 guards)
4. **./scripts/test.sh --workspace --lib**: PASSED (unit tests pass; integration tests require DATABASE_URL)
5. **./scripts/test.sh --workspace**: PASSED (with expected integration test skips)
6. **cargo clippy --workspace -- -D warnings**: PASSED
7. **./scripts/guards/run-guards.sh --semantic**: PASSED (10/10 guards)
