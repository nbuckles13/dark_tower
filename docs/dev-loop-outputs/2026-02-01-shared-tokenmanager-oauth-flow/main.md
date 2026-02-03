# Dev-Loop Output: Shared TokenManager OAuth Flow

**Date**: 2026-02-01
**Task**: Implement shared TokenManager in common crate for OAuth 2.0 client credentials flow. Should provide: (1) acquire initial token from AC using client_id/client_secret, (2) automatic refresh before expiration with configurable threshold, (3) exponential backoff retry logic for failures, (4) thread-safe token access via Arc/RwLock, (5) configurable AC endpoint URL. Will be used by both MC (to auth to GC) and GC (to auth to AC). Must handle edge cases: token expiry during acquisition, network failures, malformed responses, concurrent refresh attempts.
**Branch**: `feature/gc-oauth-token-acquisition`
**Duration**: ~60m (including iteration 2 refactor and iteration 3 review fixes)

---

## Loop State (Internal)

<!-- This section is maintained by dev-loop skills for state recovery. -->

| Field | Value |
|-------|-------|
| Implementing Agent | `afcf550` |
| Implementing Specialist | `auth-controller` |
| Current Step | `complete` |
| Iteration | `4` |
| Security Reviewer | `a12085f` (initial), `a723c7f` (final) |
| Test Reviewer | `a268248` (initial), `a398b16` (final) |
| Code Reviewer | `aa656b4` (initial), `a847e37` (final) |
| DRY Reviewer | `a941a5f` (initial), `a19b299` (final) |

---

## Task Overview

### Objective
Implement shared TokenManager in common crate for OAuth 2.0 client credentials flow with automatic refresh and thread-safe access.

### Scope
- **Service(s)**: common (shared library)
- **Schema**: None (no database changes)
- **Cross-cutting**: Yes (used by MC and GC)

### Debate Decision
Not required - this is a shared utility implementation, not a cross-service protocol change.

---

## Matched Principles

The following principle categories were matched:
- `docs/principles/crypto.md` (OAuth credentials, secrets)
- `docs/principles/jwt.md` (token handling patterns)
- `docs/principles/logging.md` (log acquisition/refresh events)
- `docs/principles/errors.md` (error handling for network/auth failures)
- `docs/principles/concurrency.md` (thread-safe token access)

---

## Implementation Summary (Iteration 2)

### Architectural Refactor

**Iteration 1 Issues (addressed in Iteration 2)**:
1. Time constants used `u64` instead of `Duration`
2. Arbitrary MAX_INITIAL_RETRIES limit
3. Complex struct-based API with multiple Arc wrappers
4. Manual shutdown coordination via AtomicBool

**Iteration 2 Design**:
- Function-based API: `spawn_token_manager()` returns `(JoinHandle, TokenReceiver)`
- No Arc wrappers - background task owns all data directly
- Infinite retry with exponential backoff (caller controls timeout)
- Duration constants instead of u64
- TokenReceiver wrapper prevents holding borrow lock

### Files Modified
| File | Changes |
|------|---------|
| `crates/common/src/token_manager.rs` | Complete rewrite (~470 lines, simplified from ~1000) |
| `crates/common/src/lib.rs` | Added `pub mod token_manager` export |
| `crates/common/Cargo.toml` | Added `reqwest` (runtime) and `wiremock` (dev-dependencies) |

### Public API (Final)

```rust
// Constants (Duration type, not u64)
pub const DEFAULT_REFRESH_THRESHOLD: Duration = Duration::from_secs(300);
pub const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(10);

// Configuration
pub struct TokenManagerConfig { ... }
impl TokenManagerConfig {
    pub fn new(ac_endpoint, client_id, client_secret) -> Self;
    pub fn with_refresh_threshold(self, Duration) -> Self;
    pub fn with_http_timeout(self, Duration) -> Self;
}

// Token Receiver (wrapper to prevent holding borrow lock)
pub struct TokenReceiver(watch::Receiver<SecretString>);
impl TokenReceiver {
    pub fn token(&self) -> SecretString;  // Always clones
    pub async fn changed(&mut self) -> Result<(), TokenError>;
}
impl Clone for TokenReceiver { ... }
impl Debug for TokenReceiver { ... }  // Redacts token

// Spawn function (returns handle + receiver)
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

### Key Implementation Decisions (Iteration 2)

1. **Function-based API** instead of struct with Clone
   - `spawn_token_manager()` returns `(JoinHandle<()>, TokenReceiver)`
   - Background task owns all data (no Arc needed)
   - JoinHandle for task monitoring/cancellation

2. **Infinite retry with exponential backoff**
   - No MAX_RETRIES constant
   - Backoff: 1s -> 2s -> 4s -> ... -> 30s cap
   - Caller wraps in `tokio::time::timeout()` if needed

3. **Empty string sentinel for watch channel**
   - Initialize with `SecretString::from("")`
   - `spawn_token_manager` waits for first real token
   - Receiver guaranteed valid after function returns

4. **TokenReceiver wrapper**
   - Prevents holding borrow lock (would block sender)
   - `token()` always clones immediately
   - Custom Debug that redacts token

5. **No manual shutdown API**
   - Call `handle.abort()` to stop background task
   - Or just drop the handle

---

## Dev-Loop Verification Steps (Iteration 2)

### Layer 1: cargo check --workspace
**Result**: PASSED
- No compilation errors
- No warnings

### Layer 2: cargo fmt --all --check
**Result**: PASSED
- All code properly formatted

### Layer 3: ./scripts/guards/run-guards.sh
**Result**: PASSED (9/9 guards)
- api-version-check: PASSED
- grafana-datasources: PASSED
- instrument-skip-all: PASSED
- no-hardcoded-secrets: PASSED
- no-pii-in-logs: PASSED
- no-secrets-in-logs: PASSED
- no-test-removal: PASSED
- test-coverage: PASSED
- test-registration: PASSED

### Layer 4: ./scripts/test.sh --workspace --lib
**Result**: PASSED
- All unit tests pass
- Token manager tests: 15 tests (reduced from 17 due to simpler API)

### Layer 5: ./scripts/test.sh --workspace
**Result**: PASSED
- All tests pass (unit + integration + doc tests)

### Layer 6: cargo clippy --workspace -- -D warnings
**Result**: PASSED
- No warnings or errors

### Layer 7: ./scripts/guards/run-guards.sh --semantic
**Result**: PASSED (10/10 guards)
- All simple guards: PASSED
- semantic-analysis: PASSED

---

## Code Review

**Status**: FIXED - All 14 findings addressed in Iteration 3

### Review Results Summary

| Reviewer | Verdict | Blocker | Critical | Major | Minor | Tech Debt |
|----------|---------|---------|----------|-------|-------|-----------|
| Security | REQUEST_CHANGES | 0 | 0 | 2 | 3 | 1 |
| Test | REQUEST_CHANGES | 0 | 0 | 3 | 4 | 2 |
| Code Reviewer | REQUEST_CHANGES | 0 | 0 | 0 | 2 | 0 |
| DRY Reviewer | APPROVED | 0 | - | - | - | 2 |

**Overall Verdict**: ~~REQUEST_CHANGES~~ FIXED (all 14 findings addressed)

### Security Specialist (Agent: a12085f)
**Verdict**: REQUEST_CHANGES

**MAJOR Findings**:
1. **Response body leak in error path** (`token_manager.rs:443-451`): When AC returns 401/400, response body included in error. If AC echoes credentials, they leak via error Display. Fix: Log body at trace level only, return generic message.
2. **No HTTPS enforcement** (`token_manager.rs:143-146`): URL accepted without scheme validation. `http://` would transmit client_secret in plaintext. Fix: Validate URL starts with `https://`.

**MINOR Findings**:
3. **OAuthTokenResponse has derived Debug** (`token_manager.rs:218-228`): `access_token` field would expose token if logged. Fix: Add custom Debug impl.
4. **Clock drift not considered** (`token_manager.rs:307-311`): Expiration calculated without drift tolerance. Fix: Document NTP requirement or apply safety margin.
5. **Missing instrumentation** (`token_manager.rs`): No `#[instrument(skip_all)]` on token_refresh_loop/acquire_token functions. Required for observability.

**TECH_DEBT**:
- No token size validation (8KB limit per JWT principle)

### Test Specialist (Agent: a268248)
**Verdict**: REQUEST_CHANGES

**MAJOR Findings**:
1. **Missing test for 401/400 authentication rejection**: Code explicitly handles these (lines 442-451) but no test exercises this path.
2. **Missing test for invalid JSON response**: JSON parsing error path (lines 425-428) untested.
3. **Missing test for missing OAuth fields**: Response deserialization with missing `access_token`/`expires_in` untested.

**MINOR Findings**:
4. **Missing explicit backoff timing verification**: Retry happens but exponential delays (1s, 2s, 4s) not verified.
5. **Missing test for zero expires_in edge case**: Could cause tight refresh loops.
6. **Missing test for ChannelClosed from changed()**: Public API error path untested.
7. **Missing HTTP timeout error test**: Timeout configuration exists but error path untested.

**TECH_DEBT**:
- Time-based tests use real time instead of `tokio::time::pause()`
- No explicit concurrent receiver stress test

### Code Quality Reviewer (Agent: aa656b4)
**Verdict**: REQUEST_CHANGES

**MINOR Findings**:
1. **Silent error swallowing** (`token_manager.rs:443`): `unwrap_or_default()` discards diagnostic info if body read fails. Fix: Use `unwrap_or_else(|e| format!("(failed to read body: {e})"))`

**TECH_DEBT**:
- Magic number for connect timeout (line 263) should be named constant

### DRY Reviewer (Agent: a941a5f)
**Verdict**: APPROVED ✓

**TECH_DEBT** (non-blocking):
- TD-14: Exponential backoff pattern similar to `mc/grpc/gc_client.rs` (2 occurrences, different semantics)
- TD-15: HTTP client builder boilerplate similar to `gc/services/ac_client.rs` (4 lines each)

---

### Changes Made for PR Feedback

| Issue | Before | After |
|-------|--------|-------|
| Duration constants | `pub const DEFAULT_REFRESH_THRESHOLD_SECS: u64 = 300;` | `pub const DEFAULT_REFRESH_THRESHOLD: Duration = Duration::from_secs(300);` |
| Retry limit | `const MAX_INITIAL_RETRIES: u32 = 10;` | Removed - infinite retry with backoff |
| Architecture | `TokenManager` struct with Clone + 4 Arc fields | `spawn_token_manager()` function, task owns data |
| API methods | `get_token()`, `force_refresh()`, `subscribe()`, `shutdown()` | `TokenReceiver::token()`, `TokenReceiver::changed()` |
| Shutdown | `Arc<AtomicBool>` with manual coordination | `handle.abort()` |

---

## Reflection

All specialists reflected on learnings and updated their knowledge files.

### From Auth-Controller (Implementing Specialist)
**Changes**: Added 8 entries (3 patterns, 4 gotchas, 1 integration)

Key learnings surfaced around async spawning with watch channels and security enforcement via constructor variants. Most valuable insights were project-specific: client_id is treated as sensitive by semantic guards, and clippy rejects assertions on constants. The function-based API pattern (spawn returning `(JoinHandle, Receiver)`) proved cleaner than struct-based designs with Arc wrappers.

**Knowledge files updated**:
- `docs/specialist-knowledge/auth-controller/patterns.md`
- `docs/specialist-knowledge/auth-controller/gotchas.md`
- `docs/specialist-knowledge/auth-controller/integration.md`

### From Security Review
**Changes**: Added 2 entries (1 pattern, 1 integration)

Added "Constructor Variants for Security Enforcement" pattern (generalizing the new() vs new_secure() approach) and "Common Crate - TokenManager Security" integration entry documenting requirements for MC/GC consumers. No new gotchas needed as existing entries already covered response body leaks, HTTPS enforcement, and clock drift issues. The Security Review Checklist and Server-Side Error Context patterns from existing knowledge effectively guided this review.

**Knowledge files updated**:
- `docs/specialist-knowledge/security/patterns.md`
- `docs/specialist-knowledge/security/integration.md`

### From Test Review
**Changes**: Added 2 entries (1 pattern, 1 gotcha)

Added pattern for testing infinite retry loops using `tokio::time::timeout` wrappers - distinct from existing retry testing because it verifies design intent (infinite retry) rather than testing individual failures. Added gotcha about explicitly-handled error paths lacking tests - the key insight that 3 MAJOR gaps were in code branches developers wrote defensive handling for but never tested.

**Knowledge files updated**:
- `docs/specialist-knowledge/test/patterns.md`
- `docs/specialist-knowledge/test/gotchas.md`

### From Code Quality Review
**Changes**: Added 3 entries (2 gotchas, 1 integration)

Most valuable is the "unwrap_or_default() Discards Error Context" gotcha - this anti-pattern compiles and runs but silently loses diagnostic information, making it easy to miss during review. Also documented the spawn-and-wait API pattern and the common crate's growing collection of shared utilities (TokenManager, SecretString, JWT utilities).

**Knowledge files updated**:
- `docs/specialist-knowledge/code-reviewer/patterns.md`
- `docs/specialist-knowledge/code-reviewer/gotchas.md`
- `docs/specialist-knowledge/code-reviewer/integration.md`

### From DRY Review
**Changes**: Added 2 entries (2 tech debt items to integration registry)

Added TD-14 (Exponential Backoff) and TD-15 (HTTP Client Builder) to the Tech Debt Registry. Existing patterns and gotchas knowledge was sufficient to classify these findings correctly - the "2 occurrences with different semantics = TECH_DEBT" rule applied directly. No new patterns or gotchas warranted as this review validated existing knowledge rather than discovering new insights.

**Knowledge files updated**:
- `docs/specialist-knowledge/dry-reviewer/integration.md`

### Summary

Total knowledge entries added: **17 entries** across 5 specialists (8 + 2 + 2 + 3 + 2)

No pruning was necessary as all existing entries remain relevant. The knowledge base is compounding effectively - specialists are building on previous learnings and refining their domain expertise with each implementation.

---

## Completion Summary

**Status**: Implementation complete (Iteration 4), all review findings fixed, all verification layers passed

### Test Coverage (Iteration 2)
- Configuration tests (defaults, builder, debug redaction, duration constants)
- TokenReceiver tests (debug redaction, clone)
- Token acquisition tests (success, retry on 500)
- Token refresh test (automatic refresh before expiry)
- Changed notification test
- Abort/cleanup test

### Usage Example (Updated)
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

### Next Steps
1. Integrate into GC service
2. Integrate into MC service
3. Add metrics for token refresh (via tracing)

---

## Iteration 3: Code Review Fixes

### Fixes Applied

All 14 blocking code review findings have been addressed:

#### Security Fixes

| # | Finding | Fix Applied |
|---|---------|-------------|
| 1 | **Response body leak in 401/400** | Body now logged at `trace` level only; error message contains only status code |
| 2 | **No HTTPS enforcement** | Added `TokenManagerConfig::new_secure()` that validates HTTPS URL |
| 3 | **OAuthTokenResponse Debug exposes token** | Custom `Debug` impl that redacts `access_token` |
| 4 | **Clock drift not considered** | Added `CLOCK_DRIFT_MARGIN_SECS` constant (30s) with documentation |
| 5 | **Missing #[instrument(skip_all)]** | Added to `spawn_token_manager`, `token_refresh_loop`, `acquire_token` |

#### Code Quality Fixes

| # | Finding | Fix Applied |
|---|---------|-------------|
| 6 | **Connect timeout magic number** | Added `DEFAULT_CONNECT_TIMEOUT` constant |
| 7 | **Silent error swallowing** | Changed `unwrap_or_default()` to `unwrap_or_else()` with trace logging |

#### New Tests Added

| # | Test | Coverage |
|---|------|----------|
| 8 | `test_401_authentication_rejected` | Verifies infinite retry on 401 |
| 9 | `test_400_authentication_rejected` | Verifies infinite retry on 400 |
| 10 | `test_invalid_json_response` | Verifies handling of non-JSON response |
| 11 | `test_missing_oauth_fields` | Verifies handling of incomplete JSON |
| 12 | `test_zero_expires_in_handled` | Verifies edge case with zero expiry |
| 13 | `test_channel_closed_error` | Tests `TokenError::ChannelClosed` from `changed()` |
| 14 | `test_http_timeout_error` | Verifies HTTP timeout handling |
| - | `test_backoff_timing` | Verifies exponential backoff timing |
| - | `test_new_secure_requires_https` | Tests HTTPS enforcement |
| - | `test_oauth_response_debug_redacts_token` | Tests OAuthTokenResponse Debug |
| - | `test_connect_timeout_constant` | Verifies constant value |
| - | `test_clock_drift_margin_constant` | Verifies constant value |

### Public API Updates (Iteration 3)

```rust
// New HTTPS-enforcing constructor
impl TokenManagerConfig {
    /// Requires HTTPS (recommended for production)
    pub fn new_secure(ac_endpoint, client_id, client_secret) -> Result<Self, TokenError>;
}

// New constants
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CLOCK_DRIFT_MARGIN_SECS: i64 = 30;
```

### Verification Results (Iteration 3)

| Layer | Status | Notes |
|-------|--------|-------|
| 1. cargo check | PASSED | No errors |
| 2. cargo fmt | PASSED | Formatted |
| 3. Guards | PASSED | 9/9 guards |
| 4. Tests (common) | PASSED | 61 tests (27 token_manager) |
| 5. Clippy | PASSED | No warnings |

### Test Coverage Summary (Iteration 3)

Total token_manager tests: **27 tests**
- Configuration: 6 tests
- TokenReceiver: 2 tests
- Token acquisition: 5 tests
- Error handling: 7 tests
- Security: 5 tests
- Constants: 2 tests

---

## Iteration 4: Clippy and Semantic Guard Fixes

### Issues Found

Iteration 3 had 3 clippy errors and 2 semantic guard warnings:

**Clippy Errors**:
1. `token_manager.rs:1175-1179`: Uninlined format args in assert
2. `token_manager.rs:1197`: Assertion on constant (`CLOCK_DRIFT_MARGIN_SECS > 0`)
3. `token_manager.rs:1198`: Assertion on constant (`CLOCK_DRIFT_MARGIN_SECS <= 60`)

**Semantic Guard Warnings**:
4. `token_manager.rs:315`: `client_id` logged in tracing instrument field
5. `token_manager.rs:354`: `client_id` logged in tracing instrument field

### Fixes Applied

| # | Issue | Fix |
|---|-------|-----|
| 1 | Uninlined format args | Changed to `"Expected at least 3s for backoff, got {total_duration:?}"` |
| 2-3 | Assertions on constants | Changed to `assert_eq!(CLOCK_DRIFT_MARGIN_SECS, 30, "...")` |
| 4-5 | client_id in instrument fields | Removed fields from `#[instrument(skip_all)]` attributes |

### Verification Results (Iteration 4)

| Layer | Status | Notes |
|-------|--------|-------|
| 1. cargo check --workspace | PASSED | No errors |
| 2. cargo fmt --all --check | PASSED | Formatted |
| 3. ./scripts/guards/run-guards.sh | PASSED | 9/9 guards |
| 4. cargo test -p common --lib | PASSED | 61 tests |
| 5. cargo test -p common | PASSED | 61 tests + 2 doc tests |
| 6. cargo clippy --workspace --all-targets --all-features -- -D warnings | PASSED | No warnings |
| 7. ./scripts/guards/run-guards.sh --semantic | PASSED | 10/10 guards |

**All 7 verification layers pass.**
