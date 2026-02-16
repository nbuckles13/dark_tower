# DRY Review - ADR-0023 Phase 6b Actor Implementation

**Reviewer**: DRY Reviewer
**Date**: 2026-01-25
**Verdict**: APPROVED

## Executive Summary

The Phase 6b actor implementation introduces a clean actor model for the Meeting Controller without duplicating patterns that already exist in `crates/common/`. The implementation uses `tokio_util::sync::CancellationToken` and standard Tokio primitives, which are not candidates for extraction to common. No BLOCKER duplication found. One TECH_DEBT item identified for future consideration.

## Files Reviewed

### New Files
- `crates/meeting-controller/src/actors/mod.rs`
- `crates/meeting-controller/src/actors/controller.rs`
- `crates/meeting-controller/src/actors/meeting.rs`
- `crates/meeting-controller/src/actors/connection.rs`
- `crates/meeting-controller/src/actors/messages.rs`
- `crates/meeting-controller/src/actors/metrics.rs`
- `crates/meeting-controller/src/lib.rs` (modified)

### Comparison References
- `crates/common/src/` (all modules)
- `crates/global-controller/src/tasks/` (background task patterns)
- `crates/ac-service/src/` (service patterns)

---

## Analysis

### Common Crate Check

Verified patterns in `crates/common/`:
- `SecretString`, `SecretBox` - `common::secret` - **Not applicable** (actors don't handle secrets directly)
- Domain IDs (`MeetingId`, `ParticipantId`, etc.) - `common::types` - **Considered below**
- Config structs - `common::config` - **Not applicable** (config defined in Phase 6a)

#### Finding: Domain IDs Not Used

**Observation**: The actor implementation uses `String` for `meeting_id`, `participant_id`, `connection_id`, etc., rather than the strongly-typed `MeetingId`, `ParticipantId` from `common::types`.

**Analysis**: The `common::types` module defines `MeetingId(Uuid)` and `ParticipantId(Uuid)`, but:
1. The actor implementation takes IDs from external sources (GC gRPC, WebTransport)
2. These IDs may arrive as strings that need validation
3. Converting at boundaries would add complexity
4. The current approach is consistent with Phase 6a `McConfig`

**Not a BLOCKER because**: The `common::types` domain IDs are primarily for database operations (UUID columns). The actor layer handles IDs as strings for transport compatibility. Conversion to/from `common::types` can happen at the repository boundary (Phase 6d Redis integration).

---

### Cross-Service Pattern Check

#### Background Task Pattern (GC vs MC)

**GC Pattern** (`tasks/health_checker.rs`, `tasks/assignment_cleanup.rs`):
```rust
pub async fn start_health_checker(
    pool: PgPool,
    staleness_threshold_seconds: u64,
    cancel_token: CancellationToken,
) {
    // tokio::select! with cancel_token.cancelled()
}
```

**MC Pattern** (`actors/controller.rs`):
```rust
async fn run(mut self) {
    loop {
        tokio::select! {
            () = self.cancel_token.cancelled() => { ... }
            msg = self.receiver.recv() => { ... }
        }
    }
}
```

**Observation**: Both use `CancellationToken` with the same `tokio::select!` pattern. This is idiomatic Tokio, not duplication worth extracting.

**Not a BLOCKER because**: The `CancellationToken` pattern is a standard Tokio primitive. Extracting a wrapper would add indirection without benefit. Each service's cancellation needs are slightly different (GC has interval-based tasks, MC has message-driven actors).

---

### TECH_DEBT: ActorMetrics Similar to Existing Metrics Patterns

**Severity**: TECH_DEBT (TD-6)
**Files**: `mc/actors/metrics.rs`

**Pattern**: The `ActorMetrics` struct tracks:
- `active_meetings: AtomicUsize`
- `active_connections: AtomicUsize`
- `actor_panics: AtomicU64`
- `total_messages_processed: AtomicU64`

**Observation**: This is similar to the metrics patterns that will eventually be needed in GC and other services. Currently:
- AC has no similar metrics struct
- GC has no similar metrics struct
- MC introduces this pattern

**Recommendation**: When implementing observability (ADR-0011), consider:
1. A `common::metrics` module with reusable counter/gauge traits
2. Or continue with service-specific metrics structs and export via OpenTelemetry

**Not a BLOCKER because**:
1. The pattern is new to the codebase (MC is first to use it)
2. Can't extract to common until we see the pattern in at least 2 services
3. Metrics naming and labeling needs differ per service

---

### APPROVED: Actor Handle Pattern Is Appropriate

The `{Actor}Handle` / `{Actor}` separation pattern is well-established in async Rust:
- Handle holds `mpsc::Sender<Message>` and `CancellationToken`
- Actor owns `mpsc::Receiver<Message>` and internal state
- Async methods on Handle send messages and await responses via oneshot

This pattern does NOT exist in `crates/common/` and is appropriately service-specific to MC.

---

### APPROVED: MailboxMonitor Is MC-Specific

The `MailboxMonitor` with depth thresholds (Meeting: 100/500, Connection: 50/200) is:
1. Specific to the actor model architecture in MC
2. Not applicable to GC's stateless HTTP handlers
3. Not applicable to AC's request/response pattern

No extraction needed.

---

### APPROVED: Message Types Are Domain-Specific

`ControllerMessage`, `MeetingMessage`, `ConnectionMessage` define the MC actor protocol. These cannot be shared and correctly belong in the MC crate.

---

## Known Tech Debt (From Phase 6a)

Reference existing tech debt from Phase 6a review:
- **TD-2**: Instance ID generation duplicated between GC and MC (~6 lines)
- **TD-3**: Config module pattern duplication (acceptable variation)

No new BLOCKER-level tech debt introduced.

---

## Summary

| Severity | Count | Description |
|----------|-------|-------------|
| BLOCKER | 0 | None |
| TECH_DEBT | 1 | TD-6: ActorMetrics pattern (monitor for future extraction) |

### BLOCKER Analysis

No code was found that duplicates existing `crates/common/` patterns. Specifically:
- No `SecretString`/`SecretBox` usage needed in actors
- Domain ID types (`MeetingId`, etc.) not used - acceptable for transport layer
- No config patterns duplicated (Phase 6a already addressed)

### Cross-Service Analysis

- `CancellationToken` usage follows idiomatic Tokio patterns (not duplication)
- GC tasks and MC actors have fundamentally different structures
- No shared actor infrastructure exists to reuse

---

## Verdict: APPROVED

The Phase 6b actor implementation is clean, well-structured, and does not introduce BLOCKER-level duplication. The actor model is appropriately MC-specific and cannot be meaningfully extracted to common. One TECH_DEBT item (TD-6: ActorMetrics) is documented for potential future extraction once observability patterns stabilize across services.

---

## Re-Review: session.rs (Fix Iteration 2)

**Date**: 2026-01-25
**Status**: APPROVED (prior review)

**Date**: 2026-01-25
**File**: `crates/meeting-controller/src/actors/session.rs`

### Crypto Pattern Analysis

#### Pattern 1: HMAC-SHA256 Usage

**MC session.rs**:
```rust
let hmac_key = hmac::Key::new(hmac::HMAC_SHA256, &meeting_key);
let tag = hmac::sign(&hmac_key, message.as_bytes());
hex::encode(tag.as_ref())  // Full 64-char output
```

**AC observability/mod.rs**:
```rust
let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
let tag = hmac::sign(&key, value.as_bytes());
format!("h:{}", hex::encode(prefix))  // Truncated to 8-char with "h:" prefix
```

**Analysis**: Both use `ring::hmac::HMAC_SHA256` but for fundamentally different purposes:
- **AC**: Log correlation hashing (privacy, truncated, prefixed)
- **MC**: Session binding tokens (security, full HMAC, validated with `hmac::verify`)

**Not a BLOCKER because**: The use cases are semantically different:
1. AC hash is one-way correlation (no validation needed)
2. MC hash requires validation (`hmac::verify`)
3. MC uses meeting-specific derived keys (HKDF)
4. Output formats differ (truncated vs full)

Extracting a common wrapper would force unnatural abstractions for different security requirements.

#### Pattern 2: HKDF-SHA256 Usage

**MC session.rs**:
```rust
let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, meeting_id.as_bytes());
let prk = salt.extract(&self.master_secret);
let okm = prk.expand(&[b"session-binding"], MeetingKeyLen)?;
```

**AC-service**: No HKDF usage (not needed for log correlation).

**Analysis**: HKDF is unique to MC for per-meeting key derivation. No duplication.

#### Pattern 3: hex Crate Usage

**Files using `hex::encode` or `hex::decode`**:
- `crates/meeting-controller/src/actors/session.rs`
- `crates/ac-service/src/observability/mod.rs`
- `docs/decisions/adr-0011-observability-framework.md` (documentation)
- `docs/decisions/adr-0005-integration-testing-strategy.md` (documentation)

**Analysis**: The `hex` crate is already in the workspace as a shared dependency. Both services use it directly, which is the correct pattern (no need for a wrapper).

#### Pattern 4: CSPRNG Usage

**MC session.rs**:
```rust
let rng = rand::SystemRandom::new();
rand::SecureRandom::fill(&rng, &mut nonce_bytes)?;
```

**AC-service**: Uses `ring::rand::SystemRandom` in key rotation and token generation.

**Analysis**: Both correctly use `ring::rand::SystemRandom` for cryptographically secure randomness. This is the standard pattern and does not need extraction.

### Verdict: APPROVED (Re-Review)

| Severity | Count | Description |
|----------|-------|-------------|
| BLOCKER | 0 | None |
| TECH_DEBT | 0 | No new tech debt from session.rs |

**Rationale**:
1. **HMAC patterns differ semantically**: AC uses for log correlation (truncated, prefixed); MC uses for session security (full, validated with HKDF-derived keys)
2. **HKDF is unique to MC**: No other service uses HKDF currently
3. **hex crate usage is correct**: Already a shared workspace dependency, direct usage is appropriate
4. **CSPRNG usage is idiomatic**: Both services correctly use `ring::rand::SystemRandom`

Extracting crypto utilities to `crates/common/` would be premature and would create artificial coupling between semantically different security operations. Each service's crypto needs are appropriately specific to its security model.

---

## Re-Review: Fix Iteration 3 Changes

**Date**: 2026-01-25
**Files Changed**:
- `crates/meeting-controller/src/actors/controller.rs` - Async `get_meeting()` implementation
- `crates/meeting-controller/src/actors/meeting.rs` - Time-based grace period tests
- `crates/meeting-controller/Cargo.toml` - Dev-dependency change for test-util

### Finding 1: Async `get_meeting()` Pattern (Line 383-413)

**Pattern**:
```rust
async fn get_meeting(&self, meeting_id: &str) -> Result<MeetingInfo, McError> {
    match self.meetings.get(meeting_id) {
        Some(managed) => {
            // Query the meeting actor to get actual participant count and state
            match managed.handle.get_state().await {
                Ok(state) => { ... }
                Err(_) => { /* fallback to cached info */ ... }
            }
        }
        None => Err(McError::MeetingNotFound(...))
    }
}
```

**Analysis**: This is an iteration 3 fix to make `get_meeting()` async and query the actor for real participant count. This pattern:
1. Is unique to the MC actor model (no similar patterns in GC, AC, or common)
2. Queries `get_state()` which is MC-specific actor communication
3. Includes graceful fallback for actor communication failures
4. Does not duplicate any `crates/common/` patterns

**Verdict**: APPROVED - No DRY violation. This is a service-specific pattern for querying actor state.

### Finding 2: Time-Based Grace Period Tests (Lines 1624-1763)

**Pattern**:
```rust
#[tokio::test(start_paused = true)]
async fn test_disconnect_grace_period_expires() {
    // ... setup ...
    tokio::time::advance(Duration::from_secs(29)).await;
    // ... verify ...
    tokio::time::advance(Duration::from_secs(6)).await;
    // ... verify removal ...
}
```

**Analysis**: Two new tests added in iteration 3:
1. `test_disconnect_grace_period_expires` - Verifies participant removal after 30s grace period
2. `test_reconnect_within_grace_period` - Verifies participant can reconnect within 30s window

Both use `tokio::test(start_paused = true)` and `tokio::time::advance()` which requires the `test-util` feature.

**Cross-Service Check**: The `test-util` feature is already used in `crates/ac-service/Cargo.toml` dev-dependencies, so MC is following the established pattern.

**Pattern Reusability**: The time-based test pattern itself is:
- Test-only code (appropriate for unit testing async grace periods)
- Not a candidate for extraction to common (test utilities belong in test code)
- Follows Tokio's official testing patterns

**Verdict**: APPROVED - No DRY violation. Test patterns are appropriately located in test code.

### Finding 3: `test_secret()` Duplication

**Files**:
- `crates/meeting-controller/src/actors/controller.rs:584-586` - `fn test_secret() -> Vec<u8> { vec![0u8; 32] }`
- `crates/meeting-controller/src/actors/meeting.rs:1619-1621` - `fn test_secret() -> Vec<u8> { vec![0u8; 32] }`

**Analysis**: Both are identical private test utilities within the same crate. The pattern:
1. Is minimal (3-line function)
2. Is private to each module (appropriate scoping)
3. Both are test-only (conditional compilation not needed)
4. Creating a shared test utility at `tests/helpers.rs` or similar would add module complexity for minimal benefit

**Verdict**: APPROVED - No DRY violation. Private test utilities are appropriately scoped to their usage modules. The 3-line duplication is acceptable given the minimal benefit of extraction.

### Summary: Fix Iteration 3

| Severity | Count | Description |
|----------|-------|-------------|
| BLOCKER | 0 | None |
| CRITICAL | 0 | None |
| MAJOR | 0 | None |
| MINOR | 0 | None |
| TECH_DEBT | 0 | No new tech debt from iteration 3 |

**Overall Verdict**: APPROVED

The three iteration 3 changes (async `get_meeting()`, time-based tests, and test-util dev-dependency) introduce no new DRY violations or patterns that should be extracted to `crates/common/`.

---
