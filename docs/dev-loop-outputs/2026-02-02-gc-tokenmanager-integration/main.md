# Dev-Loop Output: GC TokenManager Integration

**Date**: 2026-02-02
**Task**: Integrate TokenManager into Global Controller: 1) Update GC config to accept AC endpoint + client credentials (GC_CLIENT_ID, GC_CLIENT_SECRET), 2) Spawn TokenManager during GC startup with timeout, 3) Pass TokenReceiver to McClient and AcClient, 4) Replace GC_SERVICE_TOKEN env var usage, 5) Add graceful shutdown handling for TokenManager
**Branch**: `feature/gc-token-manager-integration`
**Duration**: ~0m (in progress)

---

## Loop State (Internal)

<!-- This section is maintained by dev-loop skills for state recovery. -->

| Field | Value |
|-------|-------|
| Implementing Agent | `a20b08c` |
| Implementing Specialist | `global-controller` |
| Current Step | `complete` |
| Iteration | `1` |
| Security Reviewer | `a5dc130` |
| Test Reviewer | `a8568f9` |
| Code Reviewer | `a7d358c` |
| DRY Reviewer | `a9b78e1` |

---

## Task Overview

### Objective

Integrate the shared TokenManager (from `common::token_manager`) into Global Controller to replace static `GC_SERVICE_TOKEN` environment variable with dynamic OAuth 2.0 token management that auto-refreshes tokens before expiration.

### Detailed Requirements

#### 1. Update GC Config (`crates/global-controller/src/config.rs`)

Add OAuth client credentials fields to Config struct:

```rust
// Add to imports
use common::secret::SecretString;

// Add to Config struct
/// OAuth client ID for GC to authenticate with AC.
pub gc_client_id: String,

/// OAuth client secret for GC to authenticate with AC (SecretString prevents logging).
pub gc_client_secret: SecretString,
```

Parse from environment variables:
- `GC_CLIENT_ID` - required, no default
- `GC_CLIENT_SECRET` - required, no default

Update Debug impl to redact `gc_client_secret`.

#### 2. Spawn TokenManager in main.rs (`crates/global-controller/src/main.rs`)

Replace lines 100-104 (static GC_SERVICE_TOKEN loading):

```rust
// BEFORE:
let gc_service_token = std::env::var("GC_SERVICE_TOKEN")
    .map_err(|_| "GC_SERVICE_TOKEN environment variable is required")?;
let mc_client: Arc<dyn services::McClientTrait> = Arc::new(services::McClient::new(
    common::secret::SecretString::from(gc_service_token),
));

// AFTER:
use common::token_manager::{spawn_token_manager, TokenManagerConfig};
use std::time::Duration;

// Build TokenManager config
let token_config = TokenManagerConfig::new(
    config.ac_internal_url.clone(),
    config.gc_client_id.clone(),
    config.gc_client_secret.clone(),
);

// Spawn with startup timeout (30 seconds)
let (token_task_handle, token_rx) = tokio::time::timeout(
    Duration::from_secs(30),
    spawn_token_manager(token_config),
)
.await
.map_err(|_| "Token manager startup timed out after 30 seconds")?
.map_err(|e| format!("Token manager failed to start: {}", e))?;

// Create McClient with TokenReceiver
let mc_client: Arc<dyn services::McClientTrait> = Arc::new(
    services::McClient::new(token_rx.clone()),
);
```

#### 3. Update McClient (`crates/global-controller/src/services/mc_client.rs`)

Change from static token to dynamic TokenReceiver:

```rust
// BEFORE:
use common::secret::{ExposeSecret, SecretString};

pub struct McClient {
    channels: Arc<RwLock<HashMap<String, Channel>>>,
    service_token: SecretString,
}

impl McClient {
    pub fn new(service_token: SecretString) -> Self { ... }
}

// AFTER:
use common::secret::ExposeSecret;
use common::token_manager::TokenReceiver;

pub struct McClient {
    channels: Arc<RwLock<HashMap<String, Channel>>>,
    token_receiver: TokenReceiver,
}

impl McClient {
    pub fn new(token_receiver: TokenReceiver) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            token_receiver,
        }
    }
}
```

Update `assign_meeting` to call `self.token_receiver.token().expose_secret()` instead of `self.service_token.expose_secret()`.

#### 4. Update AcClient (`crates/global-controller/src/services/ac_client.rs`)

Similar change to use TokenReceiver:

```rust
// BEFORE:
pub struct AcClient {
    client: Client,
    base_url: String,
    service_token: String,
}

// AFTER:
use common::token_manager::TokenReceiver;

pub struct AcClient {
    client: Client,
    base_url: String,
    token_receiver: TokenReceiver,
}
```

Update all `Authorization` header usage to call `self.token_receiver.token().expose_secret()`.

#### 5. Add Graceful Shutdown Handling

Store the `token_task_handle` and abort it during shutdown:

```rust
// In shutdown section after cancel_token.cancel()
token_task_handle.abort();
```

#### 6. Update Tests

- Update `MockMcClient` if needed
- Update any tests that create McClient/AcClient directly
- Add tests for config with client credentials

### Scope

- **Service(s)**: global-controller
- **Schema**: No database changes
- **Cross-cutting**: Uses common::token_manager

### Acceptance Criteria

1. GC starts successfully with GC_CLIENT_ID and GC_CLIENT_SECRET env vars
2. GC_SERVICE_TOKEN env var is no longer required/used
3. TokenManager auto-refreshes tokens before expiration
4. Graceful shutdown aborts TokenManager task
5. All existing tests pass (with updated setup)
6. New tests cover config parsing and error cases

---

## Matched Principles

The following principle categories were matched based on task keywords (client, credential, oauth, token, auth):

- `docs/principles/crypto.md` - OAuth client credentials handling
- `docs/principles/logging.md` - SecretString redaction, no credential leakage
- `docs/principles/errors.md` - Startup timeout, token acquisition errors
- `docs/principles/concurrency.md` - Background task management, shutdown coordination

---

## Pre-Work

Reviewed existing codebase:
- `common::token_manager` module provides `spawn_token_manager()`, `TokenManagerConfig`, and `TokenReceiver`
- `TokenReceiver` wraps `watch::Receiver<SecretString>` and provides `.token()` method
- GC uses `GC_SERVICE_TOKEN` env var for static token in McClient and AcClient

---

## Implementation Summary

Successfully integrated TokenManager into Global Controller:

1. **Config Changes** (`config.rs`):
   - Added `gc_client_id: String` and `gc_client_secret: SecretString` fields
   - Updated Debug impl to redact `gc_client_secret`
   - Added validation requiring both env vars

2. **Main Startup** (`main.rs`):
   - Spawn TokenManager with 30-second timeout
   - Pass TokenReceiver to AppState
   - Abort token_task_handle during shutdown

3. **AppState** (`routes/mod.rs`):
   - Added `token_receiver: TokenReceiver` field for handler access

4. **McClient** (`services/mc_client.rs`):
   - Changed from `SecretString` to `TokenReceiver`
   - Updated Authorization header to use `.token().expose_secret()`

5. **AcClient** (`services/ac_client.rs`):
   - Changed from `String service_token` to `TokenReceiver`
   - Updated Authorization header to use `.token().expose_secret()`

6. **Handler Update** (`handlers/meetings.rs`):
   - `create_ac_client()` now uses `state.token_receiver.clone()`

7. **Test Updates**:
   - Added `TokenReceiver::from_watch_receiver()` to common module for testing
   - Updated all tests creating McClient/AcClient to use test helper
   - Updated gc-test-utils/server_harness.rs

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/common/src/token_manager.rs` | Added `from_watch_receiver()` constructor |
| `crates/global-controller/src/config.rs` | Added gc_client_id, gc_client_secret; updated Debug; added tests |
| `crates/global-controller/src/main.rs` | Spawn TokenManager; pass to AppState; abort on shutdown |
| `crates/global-controller/src/routes/mod.rs` | Added token_receiver to AppState |
| `crates/global-controller/src/services/mc_client.rs` | Use TokenReceiver; update tests |
| `crates/global-controller/src/services/ac_client.rs` | Use TokenReceiver; update tests |
| `crates/global-controller/src/handlers/meetings.rs` | Update create_ac_client() |
| `crates/gc-test-utils/src/server_harness.rs` | Add mock TokenReceiver; add required config vars |
| `crates/global-controller/tests/auth_tests.rs` | Added TokenReceiver, GC_CLIENT_ID/SECRET for integration tests |
| `crates/global-controller/tests/meeting_tests.rs` | Added TokenReceiver, GC_CLIENT_ID/SECRET, fixed mock token |

---

## Dev-Loop Verification Steps

All 7 layers passed (after fixing integration test files during validation):

| Layer | Command | Result |
|-------|---------|--------|
| 1 | `cargo check --workspace` | PASSED |
| 2 | `cargo fmt --all --check` | PASSED |
| 3 | `./scripts/guards/run-guards.sh` | PASSED (9/9 guards) |
| 4 | Unit tests (`--lib`) | PASSED |
| 5 | All tests (`--workspace`) | PASSED |
| 6 | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASSED |
| 7 | `./scripts/guards/run-guards.sh --semantic` | PASSED (10/10 guards) |

**Note**: During validation, two additional test files required updates:
- `crates/global-controller/tests/auth_tests.rs` - Added TokenReceiver and GC_CLIENT_ID/SECRET
- `crates/global-controller/tests/meeting_tests.rs` - Added TokenReceiver, GC_CLIENT_ID/SECRET, fixed mock token

---

## Review Findings

### Security Reviewer (a5dc130): ✅ APPROVED

**Verdict**: No security issues found.

Key observations:
- `SecretString` properly used for `gc_client_secret` to prevent accidental logging
- Debug impl correctly redacts sensitive credentials
- `TokenReceiver::token()` returns `SecretString`, requiring explicit `.expose_secret()` for access
- Shutdown handling properly aborts TokenManager background task

### Test Reviewer (a8568f9): ✅ APPROVED

**Verdict**: Test coverage adequate.

Key observations:
- Config parsing tests cover required env var validation
- Integration tests properly updated with mock TokenReceiver
- `TokenReceiver::from_watch_receiver()` provides clean testing pattern
- Mock token expectations correctly aligned across test files

### Code Reviewer (a7d358c): ✅ APPROVED

**Verdict**: Code quality meets standards. 3 non-blocking tech debt items noted.

Tech debt items (non-blocking):
1. **TD-GC-001**: `handlers/meetings.rs:89` - TODO comment for captcha validation needs tracking
2. **TD-GC-002**: `services/mc_client.rs` - Channel cache could benefit from TTL-based eviction
3. **TD-GC-003**: Consider adding metrics for token refresh events

### DRY Reviewer (a9b78e1): ✅ APPROVED

**Verdict**: No duplication violations. 1 non-blocking observation.

Tech debt items (non-blocking):
1. **TD-GC-004**: `from_watch_receiver()` pattern in tests (auth_tests.rs, meeting_tests.rs, server_harness.rs) is repeated but acceptable - test utility pattern, not production code duplication

---

## Reflection

### From Global Controller Specialist
Updated 4 existing entries to reflect TokenManager integration. The "SecretString for Service Credentials" pattern was updated to "TokenReceiver for Dynamic OAuth" as the static token approach is replaced by auto-refreshing OAuth. The GC_SERVICE_TOKEN gotcha was updated to document the new GC_CLIENT_ID/GC_CLIENT_SECRET requirements. The AC Internal Token Endpoints integration guide was updated to explain the OAuth 2.0 client credentials flow. No new patterns were added as the TokenReceiver usage builds on existing patterns.

### From Security Review
The existing knowledge files already comprehensively cover the security patterns used in the GC TokenManager integration. The "Constructor Variants for Security Enforcement" pattern and "Common Crate - TokenManager Security" integration entry were added during the initial implementation and remain accurate. This review validated the implementation follows established patterns rather than introducing new ones.

### From Test Review
The GC TokenManager integration review validated existing test knowledge patterns. The "Testing Infinite Retry Loops with Timeout Wrappers" pattern and "Explicitly-Handled Error Paths Often Lack Tests" gotcha already captured the key learnings applicable to this implementation. No new patterns emerged - the test coverage was comprehensive following established patterns.

### From Code Review
Existing knowledge base was sufficient for this code review. The TokenManager patterns (spawn-and-wait API, TokenReceiver wrapper, SecretString usage) were already documented in the 2026-02-02 entries. The GC integration follows established patterns documented in integration.md for AppState design and config handling. No new patterns emerged that would benefit a fresh specialist.

### From DRY Review
Added TD-16 to track the MC static service token pattern identified during this review. This documents the divergence between GC (now using TokenManager) and MC (still using static MC_SERVICE_TOKEN), providing a clear migration path for Phase 6 MC hardening. No patterns or gotchas warranted new entries as the TokenReceiver testing pattern aligns with existing "Test Helper Functions" documentation.
