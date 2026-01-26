# Dev-Loop Output: ADR-0023 Phase 6b Actors

**Date**: 2026-01-25
**Task**: ADR-0023 phase6b MeetingControllerActor MeetingActor ConnectionActor CancellationToken
**Branch**: `feature/adr-0023-phase-6b-actors`
**Duration**: ~45m
**Status**: Fix Iteration 3 Complete

---

## Loop State (Internal)

<!-- This section is maintained by dev-loop skills for state recovery. -->

| Field | Value |
|-------|-------|
| Implementing Agent | `ab945c5` |
| Implementing Specialist | `meeting-controller` |
| Current Step | `complete` |
| Iteration | `3` |
| Security Reviewer | `ae5019f` |
| Test Reviewer | `a391c03` |
| Code Reviewer | `a76a660` |
| DRY Reviewer | `a0d0e9e` |

---

## Task Overview

### Objective

ADR-0023 phase6b MeetingControllerActor MeetingActor ConnectionActor CancellationToken

### Scope

- **Service(s)**: meeting-controller
- **Schema**: None (in-memory state only for Phase 6b)
- **Cross-cutting**: Actor model patterns from ADR-0001

### Debate Decision

ADR-0023 (Meeting Controller Architecture) - Accepted 2026-01-23

---

## Matched Principles

The following principle categories were matched:

- `docs/principles/concurrency.md` - Actor model, async patterns, CancellationToken
- `docs/principles/errors.md` - Error handling patterns

---

## Pre-Work

- Reviewed ADR-0023 Meeting Controller Architecture
- Reviewed existing actor patterns in global-controller health_checker tasks
- Reviewed mc-test-utils skeleton for mock patterns
- Identified CancellationToken usage from tokio-util

---

## Implementation Summary

Implemented the three-actor hierarchy defined in ADR-0023 Section 2:

### MeetingControllerActor (singleton per MC instance)

- Supervises N MeetingActors
- Handles meeting creation/removal
- Owns root CancellationToken for graceful shutdown
- Monitors child actor health via JoinHandle
- Provides backpressure metrics via mailbox monitoring
- **NEW**: Passes master_secret to MeetingActors for session binding

### MeetingActor (one per active meeting)

- Owns all meeting state (participants, subscriptions, mute status)
- Supervises N ConnectionActors
- **FIXED**: Proper HMAC-SHA256 session binding tokens (ADR-0023 Section 1)
- **FIXED**: Binding token validation on reconnect with constant-time comparison
- **FIXED**: Host privilege tracking and authorization for host mute
- Implements 30-second disconnect grace period
- Two-tier mute model: self-mute (informational) vs host-mute (enforced)
- Correlation ID to participant ID mapping for reconnection

### ConnectionActor (one per WebTransport connection)

- Handles exactly one WebTransport connection
- 1:1 with meeting participation
- Receives signaling messages from client, forwards to MeetingActor
- Sends signaling messages from MeetingActor to client

### SessionBindingManager (new module)

- HKDF-SHA256 key derivation per meeting (`meeting_key = HKDF(master, salt=meeting_id, info="session-binding")`)
- HMAC-SHA256 token generation (`HMAC(meeting_key, correlation_id || participant_id || nonce)`)
- Constant-time token validation via `ring::hmac::verify`
- StoredBinding struct for tracking nonces and TTL

### Key Features

- **CancellationToken propagation**: Parent->child hierarchy for graceful shutdown
- **Mailbox monitoring**: ADR-0023 thresholds (Meeting: 100/500, Connection: 50/200)
- **Panic recovery**: JoinHandle monitoring detects actor failures
- **Message passing**: All inter-actor communication via tokio::sync::mpsc
- **Session binding security**: HMAC-SHA256 with per-meeting keys, 30s TTL

---

## Files Created/Modified

### Files Created

| File | Description |
|------|-------------|
| `crates/meeting-controller/src/actors/mod.rs` | Actor module organization and re-exports |
| `crates/meeting-controller/src/actors/controller.rs` | MeetingControllerActor with supervision |
| `crates/meeting-controller/src/actors/meeting.rs` | MeetingActor with participant management |
| `crates/meeting-controller/src/actors/connection.rs` | ConnectionActor for signaling |
| `crates/meeting-controller/src/actors/messages.rs` | Message types for actor communication |
| `crates/meeting-controller/src/actors/metrics.rs` | Mailbox monitoring and actor metrics |
| `crates/meeting-controller/src/actors/session.rs` | **NEW**: Session binding token generation/validation |

### Files Modified

| File | Change |
|------|--------|
| `crates/meeting-controller/src/lib.rs` | Added actors module export |
| `crates/meeting-controller/Cargo.toml` | Added `hex` dependency for token encoding; added tokio with test-util for time testing |
| `Cargo.toml` | Added `hex = "0.4"` to workspace dependencies |

---

## Fix Iteration 2 Summary

### Fixes Applied

#### MAJOR-001: Binding Token Implementation (FIXED)
- Created `session.rs` module with `SessionBindingManager`
- Implements `HMAC-SHA256(meeting_key, correlation_id || participant_id || nonce)` per ADR-0023
- Uses HKDF-SHA256 for meeting-specific key derivation
- Token generation produces 64 hex chars (HMAC-SHA256 output)
- Added 13 tests for token generation and validation

#### MAJOR-002: Host Mute Authorization (FIXED)
- Added `is_host` field to `Participant` struct
- Added `is_host` parameter to `connection_join()` and `ConnectionJoin` message
- `handle_host_mute()` now checks `muted_by` has `is_host = true`
- Returns `McError::PermissionDenied` if non-host attempts mute
- Added test `test_meeting_actor_host_mute_denied_for_non_host`

#### MAJOR-003: Binding Token Validation (FIXED)
- `handle_reconnect()` now validates binding token via `SessionBindingManager::validate_token()`
- Uses constant-time comparison via `ring::hmac::verify`
- Checks for expired bindings (30s TTL) before validation
- Returns appropriate `SessionBindingError` variants
- Added test `test_meeting_actor_reconnect_invalid_token`

#### MINOR-001: Meeting ID in Error (FIXED)
- Changed conflict error to `"Meeting already exists"` without meeting ID

#### MINOR-002: Participant IDs in Errors (FIXED)
- Changed error messages to generic `"Participant not found"` and `"Participant already in meeting"`

#### MINOR-003: Display Name from user_id (FIXED)
- Changed to `format!("Participant {}", self.participants.len() + 1)`

#### MINOR-004: Missing Instrumentation (FIXED)
- Added `#[instrument(skip_all, fields(meeting_id = %self.meeting_id))]` to:
  - `handle_join()`
  - `handle_reconnect()`
  - `handle_leave()`
  - `handle_host_mute()`

---

## Dev-Loop Verification Steps (Fix Iteration 2)

| Layer | Command | Result |
|-------|---------|--------|
| 1. Check | `cargo check --workspace` | PASS |
| 2. Format | `cargo fmt --all --check` | PASS (after fmt) |
| 3. Guards | `./scripts/guards/run-guards.sh` | PASS (8/8) |
| 4. Unit Tests | `cargo test -p meeting-controller --lib` | PASS (62/62) |
| 5. All Tests | `cargo test --workspace --lib` | meeting-controller PASS (ac-service has pre-existing DB failures) |
| 6. Clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| 7. Semantic | N/A (semantic guards disabled) | SKIP |

---

## Fix Iteration 3 Summary

### Fixes Applied

#### Test Specialist MINOR-001: Time-based Grace Period Test (FIXED)
- Added `test_disconnect_grace_period_expires` using `#[tokio::test(start_paused = true)]`
- Uses `tokio::time::advance()` to simulate 30+ second passage
- Verifies participant is removed after grace period expires
- Added `tokio = { features = ["test-util"] }` to dev-dependencies

#### Test Specialist MINOR-001 (additional): Reconnect Within Grace Period Test (FIXED)
- Added `test_reconnect_within_grace_period` test
- Verifies participant can reconnect within 30s window with valid binding token
- Confirms participant status transitions back to Connected

#### Code Quality MINOR-001: Incomplete Meeting Info Status (FIXED)
- Changed `get_meeting()` from sync to async in `controller.rs`
- Now queries `MeetingActor.get_state()` to get actual participant count
- Returns `fencing_generation` from actor state instead of hardcoded 0
- Gracefully handles actor communication failures with fallback to cached info

### Files Modified

| File | Change |
|------|--------|
| `crates/meeting-controller/src/actors/meeting.rs` | Added 2 time-based grace period tests using tokio test-util |
| `crates/meeting-controller/src/actors/controller.rs` | Made `get_meeting()` async, queries actor for real participant count |
| `crates/meeting-controller/Cargo.toml` | Added tokio with test-util feature for time testing |

---

## Dev-Loop Verification Steps (Fix Iteration 3)

| Layer | Command | Result |
|-------|---------|--------|
| 1. Check | `cargo check --workspace` | PASS |
| 2. Format | `cargo fmt --all --check` | PASS (after fmt) |
| 3. Guards | `./scripts/guards/run-guards.sh` | PASS (8/8) |
| 4. Unit Tests | `cargo test -p meeting-controller --lib` | PASS (64/64) |
| 5. All Tests | `cargo test --workspace --lib` | meeting-controller PASS (ac-service has pre-existing DB failures) |
| 6. Clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| 7. Semantic | N/A (semantic guards disabled) | SKIP |

---

## Code Review Results (Final - Iteration 3)

**Overall Verdict**: APPROVED (All 4 reviewers approved)

### Security Specialist
**Verdict**: APPROVED (after iteration 2 and 3 fixes)
**Findings**: All MAJOR/MINOR fixed, 1 TECH_DEBT remaining

#### MAJOR Findings (Must Fix) - ALL FIXED

**MAJOR-001: Binding Token Not Implemented (Placeholder Only)** - FIXED
- Created `session.rs` with proper HMAC-SHA256 implementation

**MAJOR-002: Missing Authorization Check for Host Mute** - FIXED
- Added `is_host` field and authorization check

**MAJOR-003: Binding Token Not Validated on Reconnect** - FIXED
- Implemented validation with constant-time comparison

#### MINOR Findings - ALL FIXED

**MINOR-001**: Meeting ID logged in conflict error - FIXED
**MINOR-002**: Participant IDs in error messages - FIXED
**MINOR-003**: Display name constructed from user_id substring - FIXED
**MINOR-004**: Missing `#[instrument(skip_all)]` on token-handling functions - FIXED

#### TECH_DEBT Findings (Documented)

**TECH_DEBT-001**: Signaling message size limits not enforced (`messages.rs:281-294`)

---

### Test Specialist
**Verdict**: APPROVED
**Findings**: 3 TECH_DEBT only

**64 tests** (increased from 62) with comprehensive coverage:
- Actor lifecycle (spawn, run, shutdown, cancellation) - all 3 actor types
- Message handling (CreateMeeting, GetMeeting, RemoveMeeting, GetStatus, ConnectionJoin)
- Cancellation propagation (parent->child token hierarchy)
- Disconnect/reconnection flows
- Mute state model (self-mute and host-mute)
- Mailbox monitoring (all threshold levels per ADR-0023)
- **NEW**: Session binding token generation and validation (13 tests)
- **NEW**: Host mute authorization denied for non-host
- **NEW**: Reconnect with invalid binding token rejected

#### MINOR Findings (Must Fix) - ALL FIXED

**MINOR-001**: ~~Missing time-based grace period test~~ - FIXED in iteration 3

#### TECH_DEBT Findings

**TECH_DEBT-001**: Missing panic recovery test - `check_meeting_health()` panic detection not tested with actual panic
**TECH_DEBT-002**: ~~Binding token validation not tested~~ - NOW TESTED

---

### Code Quality Reviewer
**Verdict**: APPROVED (after iteration 3 fixes)
**Findings**: All MINOR fixed, 2 TECH_DEBT remaining (stubs for Phase 6g)

**ADR Compliance**: PASS on all ADR-0001 (Actor Pattern), ADR-0002 (No-Panic), ADR-0023 (MC Architecture) requirements

#### MINOR Findings (Must Fix) - ALL FIXED

**MINOR-001**: ~~Incomplete meeting info in status~~ - FIXED in iteration 3, now queries actor for real count

#### TECH_DEBT Findings

**TD-001**: ~~Placeholder binding token generation~~ - FIXED
**TD-002**: ~~Placeholder binding token validation~~ - FIXED
**TD-003**: Signaling message routing stubs (`meeting.rs:803-820`) - only MuteUpdate handled
**TD-004**: WebTransport send stubs (`connection.rs:265-305`) - messages logged but not transmitted

---

### DRY Reviewer
**Verdict**: APPROVED
**Findings**: 1 TECH_DEBT only

**Cross-Service Analysis**: No code duplicates patterns from `crates/common/`. Actor model is appropriately MC-specific.

#### TECH_DEBT Findings

**TD-6**: ActorMetrics pattern may be extractable to `common::metrics` when observability patterns stabilize across services

---

## Reflection

### What Worked Well

- Actor handle/task separation pattern provides clean API
- CancellationToken hierarchy simplifies graceful shutdown
- Message-based communication avoids shared state complexity
- Mailbox monitoring enables backpressure detection
- SessionBindingManager encapsulates all token crypto cleanly

### Patterns to Keep

- Extract values before broadcast to avoid borrow checker issues
- Use `values()` iterator when only HashMap values needed
- Separate handle Clone from actor state ownership
- Use `#[allow(clippy::expect_used)]` with ADR-0002 justification for crypto invariants

### Items Deferred

- ~~HMAC-SHA256 binding token implementation~~ - DONE
- Redis nonce management (for distributed nonce tracking)
- WebTransport actual send/receive
- MH enforcement of host-mute

### Technical Debt Remaining

- Signaling message size limits not enforced
- ~~participant_count in status always 0~~ - FIXED in iteration 3
- Most signaling message types not routed
- WebTransport send stubs
- ActorMetrics extraction to common
- Missing panic recovery test for `check_meeting_health()`

---

## Lessons Learned (Specialist Reflections)

### From Meeting Controller Specialist
Added 8 knowledge entries covering: Handle/Task separation pattern, CancellationToken hierarchy, HMAC-SHA256 with HKDF for session binding, async state queries, and tokio::time::pause for tests. Key gotchas: borrow checker issues with broadcast after mutable state update, no IDs in error messages.

### From Security Review
Added 3 entries, updated 1. Key pattern: HKDF key derivation for scoped tokens. Key gotchas: token comparison must use constant-time operations (ring::hmac::verify), error messages leaking internal identifiers. Updated MC session binding integration checklist.

### From Test Review
Added 4 entries, updated 1. Key patterns: tokio::time::pause for deterministic time tests, HMAC validation exhaustive testing (test each field independently), actor lifecycle testing. Key gotcha: time-based tests without pause() are flaky.

### From Code Review
Added 5 entries, updated 1. Key patterns: Actor Handle/Task separation, async state queries for accurate status, #[allow(clippy::expect_used)] with ADR-0002 justification. Key gotchas: synchronous get_* methods return stale data, missing graceful fallback when actor communication fails.

### From DRY Review
Added 4 entries. Key insight: same crypto primitive (HMAC-SHA256) can have different purposes - session binding vs log correlation are NOT duplication candidates. Pattern: check dev-dependency precedent before questioning. Gotcha: Tokio actor pattern is service-specific, not for common/.
