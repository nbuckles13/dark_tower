# Devloop Output: MH MC Notification Client (McClient)

**Date**: 2026-04-15
**Task**: Implement McClient for MH→MC notifications (NotifyParticipantConnected/Disconnected), wire into WebTransport connection handler, queue for pre-RegisterMeeting connections, retry with backoff
**Specialist**: media-handler
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-mh-notify`
**Duration**: ~50m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `18cc8a2268e7d519b9582a9199f8b7a101cadfe1` |
| Branch | `feature/mh-quic-mh-notify` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@mh-mc-client` |
| Implementing Specialist | `media-handler` |
| Iteration | `2` |
| Security | `security@mh-mc-client` |
| Test | `test@mh-mc-client` |
| Observability | `observability@mh-mc-client` |
| Code Quality | `code-reviewer@mh-mc-client` |
| DRY | `dry-reviewer@mh-mc-client` |
| Operations | `operations@mh-mc-client` |

---

## Task Overview

### Objective
Implement McClient in MH service for MH→MC notifications (NotifyParticipantConnected/Disconnected). Wire into WebTransport connection handler to notify MC on JWT-authenticated connect and disconnect. Queue notifications for pre-RegisterMeeting connections (deliver when RegisterMeeting provides MC endpoint). Retry with backoff (3 attempts, 1s/2s/4s) then give up (best-effort). Authenticate with MH's OAuth service token via TokenReceiver.

### Scope
- **Service(s)**: MH (media-handler)
- **Schema**: No
- **Cross-cutting**: No (MH-only changes, MC already has MediaCoordinationService)

### Debate Decision
NOT NEEDED - Straightforward implementation of an already-designed component (Task 6 from user story).

---

## Planning

Implementer proposed channel-per-call McClient wrapping `MediaCoordinationServiceClient`, with fire-and-forget notifications via `tokio::spawn`, retry with exponential backoff (3 attempts, 1s/2s/4s), and `TokenReceiver` for Bearer auth. All 6 reviewers confirmed the plan.

---

## Pre-Work

None

---

## Implementation Summary

### McClient (`crates/mh-service/src/grpc/mc_client.rs`)
- Channel-per-call gRPC client wrapping `MediaCoordinationServiceClient`
- `notify_participant_connected()` and `notify_participant_disconnected()` methods
- `send_with_retry()` generic with exponential backoff (1s/2s/4s), 3 attempts
- Auth errors (UNAUTHENTICATED/PERMISSION_DENIED) short-circuit retries via `tonic::Code` enum matching
- `add_auth()` helper attaches Bearer token from `TokenReceiver`
- Best-effort: methods return `Result<(), MhError>`, callers use `tokio::spawn`

### WebTransport Wiring (`connection.rs`)
- `spawn_notify_connected()` helper fires connected notification via `tokio::spawn`
- On JWT-authenticated connect (registered meeting): immediate notification
- On pending connection promotion: notification after `Notify` wakeup
- On disconnect: `NotifyParticipantDisconnected` with `DisconnectReason` mapping
- Server shutdown uses `DisconnectReason::Unspecified` (not ClientClosed)

### Metrics (`metrics.rs`)
- `record_mc_notification(event, status)` counter with bounded cardinality (4 combos)
- Documented in `docs/observability/metrics/mh-service.md`

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/mh-service/src/grpc/mc_client.rs` | NEW: McClient gRPC client with retry, auth, tests |
| `crates/mh-service/tests/mc_client_integration.rs` | NEW: 6 integration tests with mock MediaCoordinationService |
| `crates/mh-service/src/grpc/mod.rs` | Added `mc_client` module and `McClient` re-export |
| `crates/mh-service/src/webtransport/server.rs` | Added `mc_client: Arc<McClient>` field, passed to connections |
| `crates/mh-service/src/webtransport/connection.rs` | Wired MC notifications on connect/disconnect |
| `crates/mh-service/src/main.rs` | Create McClient, pass to WebTransportServer |
| `crates/mh-service/src/observability/metrics.rs` | Added `record_mc_notification()` |
| `crates/mh-service/Cargo.toml` | Added `common` dev-dependency with `test-utils` feature |
| `docs/observability/metrics/mh-service.md` | Documented `mh_mc_notifications_total` metric |
| `docs/TODO.md` | Updated `add_auth` tech debt entry (3→4 call sites) |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS

### Layer 3: Simple Guards
**Status**: ALL PASS (15/15)

### Layer 4: Tests
**Status**: PASS (all workspace tests pass, 0 failures)

### Layer 5: Clippy
**Status**: PASS (0 warnings)

### Layer 6: Cargo Audit
**Status**: Pre-existing vulnerabilities in transitive deps (quinn-proto, ring, rsa, rustls-webpki)

### Layer 7: Semantic Guards
**Status**: PASS

### Layer 8: Env-tests
**Status**: Infrastructure failure (pre-existing) — GC→MC token scope mismatch. McAuthLayer applied server-wide in MC main.rs gates both GC→MC and MH→MC, but GC client registration lacks `service.write.mc` scope. Not caused by this devloop.

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 1 found, 1 fixed, 0 deferred
- S-1: Fragile string matching in `is_auth_error` → Fixed: uses `tonic::Code` enum matching

### Test Specialist
**Verdict**: RESOLVED
**Findings**: 5 found, 4 fixed, 1 deferred
- F-1: `is_auth_error` string matching → Fixed (same as S-1)
- F-2: No retry integration tests → Fixed: 6 new integration tests with mock gRPC server
- F-3: Promoted connection notification path → Deferred to Task 14 (E2E integration tests)
- F-4: Shutdown disconnect reason → Fixed: changed to `DisconnectReason::Unspecified`
- F-5: Shared OnceLock test helper → Info only, no action

### Observability Specialist
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed, 0 deferred
- Metrics catalog not updated → Fixed: added entry to `mh-service.md`

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 2 found, 1 fixed, 1 deferred
- F-1: Fragile string matching → Fixed (same as S-1)
- F-2: Latency histogram → Deferred to Task 10 (MH metrics)

### DRY Reviewer
**Verdict**: CLEAR
**Findings**: 0 (tech debt observations only)
- `add_auth` is now 4 call sites — updated TODO.md

### Operations Reviewer
**Verdict**: CLEAR
**Findings**: 0

---

## Tech Debt

### Deferred Findings

| Finding | Reviewer | Location | Deferral Justification | Follow-up Task |
|---------|----------|----------|------------------------|----------------|
| Promoted connection E2E tests | Test | `connection.rs` | Requires full WebTransport+SessionManager integration harness | Task 14 |
| Latency histogram metric | Code Quality | `metrics.rs` | Requires dashboard+catalog entries (Task 10 scope) | Task 10 |

### Cross-Service Duplication (from DRY Reviewer)

| Pattern | New Location | Existing Location | Follow-up Task |
|---------|--------------|-------------------|----------------|
| `add_auth` (4th call site) | `mc_client.rs` | `gc_client.rs` (MH), `gc_client.rs` (MC), `mh_client.rs` (MC) | TODO.md tracked |

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `18cc8a2268e7d519b9582a9199f8b7a101cadfe1`
2. Review all changes: `git diff 18cc8a2..HEAD`
3. Soft reset (preserves changes): `git reset --soft 18cc8a2`
4. Hard reset (clean revert): `git reset --hard 18cc8a2`

---

## Reflection

All teammates updated their INDEX.md files with pointers to new McClient code, integration tests, and notification wiring.

---

## Issues Encountered & Resolutions

### Issue 1: Metric guard failure
**Problem**: Implementer added `mh_mc_notification_duration_seconds` histogram not in user story spec
**Resolution**: Removed histogram, kept only `mh_mc_notifications_total` per spec

### Issue 2: INDEX size violations
**Problem**: 5 INDEX.md files exceeded 75-line limit (pre-existing from prior devloops)
**Resolution**: Consolidated entries to fit within limit

### Issue 3: Env-test GC→MC auth failure
**Problem**: `McAuthLayer` (added in Task 7) applied server-wide in MC, blocking GC→MC calls due to missing `service.write.mc` scope in GC client registration
**Resolution**: Identified root cause (setup.sh scope config + MC main.rs layer application). Fix deferred to separate session.

---

## Lessons Learned

1. Server-wide gRPC auth layers affect all services on that server — scope checks should be per-service or client registrations must include all required scopes
2. Metrics guard enforces dashboard+catalog coverage — don't add metrics not in the user story spec unless also adding dashboard/catalog entries
3. INDEX.md files need regular compaction as pointers accumulate across devloops
