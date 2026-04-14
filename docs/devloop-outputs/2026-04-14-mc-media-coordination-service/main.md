# Devloop Output: MC MediaCoordinationService + MhConnectionRegistry + MediaConnectionFailed

**Date**: 2026-04-14
**Task**: Implement MediaCoordinationService gRPC handler for MH→MC notifications (NotifyParticipantConnected/Disconnected) with JWKS-based MH service token validation. Create MhConnectionRegistry for tracking participant→MH connection state. Handle MediaConnectionFailed signaling message from clients. Register MediaCoordinationService on MC's existing gRPC server :50052.
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/mh-quic-mc-coordination`
**Duration**: ~35m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `aea04cdc13baae5c710f0a547dce20d013fee20b` |
| Branch | `feature/mh-quic-mc-coordination` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@mc-media-coordination` |
| Implementing Specialist | `meeting-controller` |
| Iteration | `3` |
| Security | `security@mc-media-coordination` |
| Test | `test@mc-media-coordination` |
| Observability | `observability@mc-media-coordination` |
| Code Quality | `code-reviewer@mc-media-coordination` |
| DRY | `dry-reviewer@mc-media-coordination` |
| Operations | `operations@mc-media-coordination` |

---

## Task Overview

### Objective
Implement MC-side handling for MH→MC coordination: MediaCoordinationService gRPC handler, MhConnectionRegistry for participant connection tracking, MediaConnectionFailed signaling handler, all with JWKS-based service token validation.

### Scope
- **Service(s)**: MC (meeting-controller)
- **Schema**: No
- **Cross-cutting**: No (MC-only, but receives gRPC calls from MH)

### Debate Decision
NOT NEEDED - Implementation follows established patterns from user story design §meeting-controller items 6-9.

---

## Planning

Implementer proposed approach covering 4 main components:
1. McAuthLayer — async JWKS-based tower Layer/Service for MH→MC gRPC auth (R-22)
2. MediaCoordinationService — gRPC handler for NotifyParticipantConnected/Disconnected (R-15)
3. MhConnectionRegistry — per-meeting participant→MH connection tracking with RwLock (R-18)
4. MediaConnectionFailed handling — log + metric in WebTransport bridge loop (R-20)

All 6 reviewers confirmed the plan. Key decisions: per-service auth layer (not global), reuse existing JwksClient, `tokio::sync::RwLock` for registry.

---

## Pre-Work

None

---

## Implementation Summary

### MediaCoordinationService (`grpc/media_coordination.rs`)
- `McMediaCoordinationService` implementing `MediaCoordinationService` gRPC trait
- `NotifyParticipantConnected`: validates fields, adds to registry, logs, records metric
- `NotifyParticipantDisconnected`: validates fields, removes from registry, logs, records metric
- Input validation: non-empty IDs, max 256 bytes per field

### MhConnectionRegistry (`mh_connection_registry.rs`)
- `HashMap<meeting_id, HashMap<participant_id, Vec<MhConnectionInfo>>>` with `tokio::sync::RwLock`
- Bounded storage: 1000 connections per meeting
- Duplicate detection, empty entry cleanup on removal
- Lifecycle cleanup: `remove_meeting()` wired into `MeetingControllerActor`

### McAuthLayer (`grpc/auth_interceptor.rs`)
- Async tower Layer/Service for JWKS-based JWT validation
- Structural fast-path checks (empty token, size limit at 8KB)
- Full EdDSA signature verification via `CommonJwtValidator`
- Scope enforcement: `service.write.mc`
- `#[cfg(test)]` gated `disabled()` method

### MediaConnectionFailed Handler (`webtransport/connection.rs`)
- `handle_client_message()` in bridge loop
- Logs warning with truncated client fields (UTF-8 safe via `floor_char_boundary`)
- Records `mc_media_connection_failures_total` metric
- No reallocation (deferred per R-20)

### Metrics (`observability/metrics.rs`)
- `record_mh_notification(event)` — counter with "connected"/"disconnected" labels
- `record_media_connection_failed(all_failed)` — counter with "true"/"false" labels

### Wiring (`main.rs`)
- `MhConnectionRegistry` created as `Arc`, shared between actor and gRPC service
- `McAuthLayer` applied to `MediaCoordinationServiceServer`
- Both services registered on existing gRPC server :50052

---

## Files Modified

```
 crates/mc-service/Cargo.toml                       |   4 +-
 crates/mc-service/src/actors/controller.rs         |  26 ++
 crates/mc-service/src/grpc/auth_interceptor.rs     | 516 ++++++++++++++++++
 crates/mc-service/src/grpc/media_coordination.rs   | NEW
 crates/mc-service/src/grpc/mod.rs                  |  15 +-
 crates/mc-service/src/lib.rs                       |   1 +
 crates/mc-service/src/main.rs                      |  26 +-
 crates/mc-service/src/mh_connection_registry.rs    | NEW
 crates/mc-service/src/observability/metrics.rs     |  69 +++
 crates/mc-service/src/webtransport/connection.rs   | 124 +++++
 crates/mc-service/tests/gc_integration.rs          |   2 +
 crates/mc-service/tests/join_tests.rs              |   5 +
 docs/TODO.md                                       |   6 +-
 docs/observability/metrics/mc-service.md           |  39 +-
 infra/docker/prometheus/rules/mc-alerts.yaml       |  14 +
 infra/grafana/dashboards/mc-overview.json          | 203 +++++++
 21 files changed, 1102 insertions(+), 109 deletions(-)
```

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS

### Layer 3: Simple Guards
**Status**: ALL PASS (15/15)

### Layer 4: Tests
**Status**: PASS (240 lib + 31 integration, 0 failures)

### Layer 5: Clippy
**Status**: PASS (0 warnings)

### Layer 6: Audit
**Status**: PASS (pre-existing vulnerabilities only)

### Layer 7: Semantic Guard
**Status**: SAFE — no credential leaks, blocking, PII, or error context issues

### Layer 8: Env-tests
**Status**: SKIPPED — Kind cluster infrastructure failure (node-exporter/kube-state-metrics timeout), not related to code changes

---

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED
**Findings**: 3 found, 3 fixed, 0 deferred

1. Scope authorization not enforced in McAuthLayer — fixed: added `REQUIRED_SCOPE = "service.write.mc"` check
2. Client-controlled fields logged without truncation — fixed: truncated to 256 bytes
3. UTF-8 panic in byte-index truncation — fixed: uses `floor_char_boundary(256)`

### Test Specialist
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed, 0 deferred

1. Missing `handle_client_message` unit tests — fixed: 5 tests added
2. Scope check not enforced (same as security #1) — fixed

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 2 found, 2 fixed, 0 deferred

1. Dashboard panel units incorrect ("ops" → "short") — fixed
2. Missing `error_reason` in structured log — fixed

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 2 found (low, non-blocking), 0 deferred

1. `#[allow]` vs `#[expect]` — matches existing codebase convention
2. Module doc inconsistency on scope enforcement — subsequently fixed by scope addition

### DRY Reviewer
**Verdict**: RESOLVED

**True duplication findings**: 1
- Duplicate `MAX_ID_LENGTH` in `media_coordination.rs` — fixed: imports from `mh_connection_registry`

**Extraction opportunities** (tech debt):
1. GrpcAuthLayer extraction (McAuthLayer/MhAuthLayer near-identical)
2. TestKeypair + build_pkcs8_from_seed + NoopService to shared test-utils
3. Dead McAuthInterceptor removal

### Operations Reviewer
**Verdict**: CLEAR
**Findings**: 1 fix + 2 advisory, 1 fixed, 0 deferred

1. OPS-1: MhConnectionRegistry lifecycle not wired — fixed: `remove_meeting()` called in actor cleanup
2. OPS-2: Server-wide auth layer — acknowledged (intentional, stronger security)
3. OPS-3: Alert detection lag — acknowledged (acceptable for warning-severity)

---

## Tech Debt

### Deferred Findings

No deferred findings — all findings were fixed.

### Cross-Service Duplication (from DRY Reviewer)

| Pattern | New Location | Existing Location | Follow-up Task |
|---------|--------------|-------------------|----------------|
| GrpcAuthLayer | `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthLayer` | `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer` | Extract to common when third service needs it |
| TestKeypair/NoopService | `crates/mc-service/src/grpc/auth_interceptor.rs:tests` | `crates/mh-service/src/grpc/auth_interceptor.rs:tests` | Extract to shared test-utils crate |
| Dead McAuthInterceptor | `crates/mc-service/src/grpc/auth_interceptor.rs` | N/A | Remove (replaced by McAuthLayer) |

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `aea04cdc13baae5c710f0a547dce20d013fee20b`
2. Review all changes: `git diff aea04cdc13baae5c710f0a547dce20d013fee20b..HEAD`
3. Soft reset (preserves changes): `git reset --soft aea04cdc13baae5c710f0a547dce20d013fee20b`
4. Hard reset (clean revert): `git reset --hard aea04cdc13baae5c710f0a547dce20d013fee20b`

---

## Reflection

All 7 specialists updated their INDEX.md files with pointers to new code locations. DRY reviewer updated TODO.md with 3 tech debt items.

---

## Issues Encountered & Resolutions

### Issue 1: Metrics guard failure
**Problem**: New metrics lacked dashboard and catalog coverage
**Resolution**: Added MH Coordination dashboard row and metrics catalog entries

### Issue 2: UTF-8 truncation panic
**Problem**: Byte-index slicing on client-controlled strings could panic on multi-byte UTF-8
**Resolution**: Used `str::floor_char_boundary(256)` for safe truncation

### Issue 3: Registry memory leak
**Problem**: `MhConnectionRegistry::remove_meeting()` was never called from actor lifecycle
**Resolution**: Wired `Arc<MhConnectionRegistry>` into `MeetingControllerActor` with cleanup on meeting end

### Issue 4: Kind cluster infrastructure failure
**Problem**: Layer 8 env-tests could not run due to Kind observability stack timeouts
**Resolution**: Skipped Layer 8; env-tests to be run separately when cluster is healthy

---

## Lessons Learned

1. JWKS-based async auth requires tower Layer/Service pattern — tonic's sync Interceptor cannot support async JWKS lookups
2. Client-controlled string inputs must use UTF-8-safe truncation (`floor_char_boundary`), not byte-index slicing
3. In-memory registries must be wired into actor lifecycle for cleanup to prevent unbounded growth
