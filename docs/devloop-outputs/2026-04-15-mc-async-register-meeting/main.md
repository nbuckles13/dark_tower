# Devloop Output: MC async RegisterMeeting trigger on first participant join

**Date**: 2026-04-15
**Task**: Implement async RegisterMeeting trigger on first participant join — spawn tokio task calling MhClient::register_meeting() per MH after JoinResponse, with retry/backoff, first-participant-only
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2)
**Branch**: `feature/mh-quic-mc-register`
**Duration**: ~25m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `18cc8a2268e7d519b9582a9199f8b7a101cadfe1` |
| Branch | `feature/mh-quic-mc-register` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `meeting-controller` |
| Iteration | `1` |
| Security | `RESOLVED` |
| Test | `RESOLVED` |
| Observability | `CLEAR` |
| Code Quality | `RESOLVED` |
| DRY | `CLEAR` |
| Operations | `RESOLVED` |

---

## Task Overview

### Objective
Implement async RegisterMeeting trigger on first participant join. After JoinResponse is sent, MC spawns a tokio task that calls MhClient::register_meeting() for each assigned MH (gRPC endpoints from Redis MhAssignmentData). Does NOT block JoinResponse. Retry with backoff on failure. Only fires for the first participant in a meeting (not subsequent joins).

### Scope
- **Service(s)**: MC (mc-service)
- **Schema**: No
- **Cross-cutting**: No (MC-only changes, MhClient already exists)

### Debate Decision
NOT NEEDED - Implementation follows existing user story design (task #8)

---

## Planning

Approach agreed by all 6 reviewers:
1. Add `MhRegistrationClient` trait for testability (same pattern as `MhAssignmentStore`)
2. Wire `Arc<dyn MhRegistrationClient>`, `mc_id`, `mc_grpc_endpoint` through WebTransportServer to handle_connection
3. First-participant detection via `join_result.participants.is_empty()` (actor serializes joins)
4. Spawn tokio task with CancellationToken, tracing span, retry/backoff
5. MockMhRegistrationClient for tests with call recording

---

## Pre-Work

None

---

## Implementation Summary

### MhRegistrationClient Trait
Added trait in `mh_client.rs` with `register_meeting()` method using `Pin<Box<dyn Future>>` pattern. Implemented on `MhClient`. Re-exported from `grpc/mod.rs`.

### Async RegisterMeeting Trigger
After JoinResponse sent in `handle_connection()`, checks if first participant via `join_result.participants.is_empty()`. If true, spawns task with:
- Tracing span (target: `mc.register_meeting.trigger`, field: `meeting_id`)
- CancellationToken (child of connection cancel_token) checked before each RPC and during backoff
- Iterates MH handlers, skips those without `grpc_endpoint`
- 3 attempts with 1s/2s exponential backoff between retries
- Per-handler independent retry (partial failure handled)

### Wiring
- `WebTransportServer` stores `Arc<dyn MhRegistrationClient>`, `mc_id`, `mc_grpc_endpoint`
- `main.rs` creates `Arc<MhClient>` from `token_rx.clone()`, casts to `Arc<dyn MhRegistrationClient>`
- `build_join_response()` returns `(JoinResponse, MhAssignmentData)` tuple to pass MH data to spawn

### Tests
- T12: First participant triggers RegisterMeeting with correct args
- T13: Second participant does NOT trigger RegisterMeeting
- 4 unit tests for retry logic: success-on-2nd, all-retries-exhausted, partial-failure, skip-none-endpoint

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/mc-service/src/grpc/mh_client.rs` | +29: MhRegistrationClient trait + impl on MhClient |
| `crates/mc-service/src/grpc/mod.rs` | +1: Re-export MhRegistrationClient |
| `crates/mc-service/src/main.rs` | +11: Create Arc<MhClient>, pass to WebTransportServer |
| `crates/mc-service/src/webtransport/connection.rs` | +332: First-participant detection, spawn, register_meeting_with_handlers(), unit tests |
| `crates/mc-service/src/webtransport/server.rs` | +19: Wire new params through accept loop |
| `crates/mc-service/tests/join_tests.rs` | +164: MockMhRegistrationClient, T12, T13 |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS (auto-fixed 2 files)

### Layer 3: Simple Guards
**Status**: PASS (14/15 — 1 pre-existing INDEX size violation)

### Layer 4: Tests
**Status**: PASS (52 suites, 0 failures)

### Layer 5: Clippy
**Status**: PASS

### Layer 6: Audit
**Status**: PASS (pre-existing transitive dep vulnerabilities only)

### Layer 7: Semantic Guards
**Status**: SAFE

### Layer 8: Env-tests
**Status**: SKIPPED (Kind cluster not available in environment)

---

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed, 0 deferred
- Success log level changed from `info!` to `debug!` to reduce endpoint exposure

### Test Specialist
**Verdict**: RESOLVED
**Findings**: 3 found, 1 fixed, 2 deferred
- 4 unit tests added for retry logic (fixed)
- Integration mock per-call sequencing deferred (unit tests provide coverage)
- Success log level kept at debug per security (cross-reviewer coordination)

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 0

### Code Quality Reviewer
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed, 0 deferred
- Step comment numbering fixed

### DRY Reviewer
**Verdict**: CLEAR

**Extraction opportunities** (tech debt observations):
- `add_auth` helper (pre-existing, 3 call sites) — no new duplication
- Exponential backoff idiom (3 instances with different parameters) — acceptable structural similarity

### Operations Reviewer
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed, 0 deferred
- Added cancellation check before each RPC (not just during backoff sleep)

---

## Tech Debt

### Deferred Findings

| Finding | Reviewer | Deferral Justification |
|---------|----------|------------------------|
| Integration mock per-call sequencing | Test | Unit tests provide equivalent coverage at lower layer |
| Success log at debug vs info | Test | Cross-reviewer: security requested debug level |

### Cross-Service Duplication (from DRY Reviewer)

No new cross-service duplication detected.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `18cc8a2268e7d519b9582a9199f8b7a101cadfe1`
2. Review all changes: `git diff 18cc8a2268e7d519b9582a9199f8b7a101cadfe1..HEAD`
3. Soft reset (preserves changes): `git reset --soft 18cc8a2268e7d519b9582a9199f8b7a101cadfe1`
4. Hard reset (clean revert): `git reset --hard 18cc8a2268e7d519b9582a9199f8b7a101cadfe1`

---

## Reflection

All specialist INDEX.md files updated with new code pointers for MhRegistrationClient trait and register_meeting_with_handlers(). INDEX guard passes.

---

## Issues Encountered & Resolutions

None

---

## Lessons Learned

1. MhRegistrationClient trait (test reviewer feedback) enabled meaningful test coverage — same pattern as MhAssignmentStore
2. CancellationToken in spawned tasks (operations feedback) is low-cost insurance for graceful shutdown
3. First-participant detection via actor-serialized JoinResult is race-free without additional synchronization

---
