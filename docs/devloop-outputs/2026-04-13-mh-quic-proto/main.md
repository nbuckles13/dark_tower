# Devloop Output: Proto changes for MH QUIC connection

**Date**: 2026-04-13
**Task**: Add grpc_endpoint to MhAssignment, remove MhRole enum + role field, remove connection_token from MediaServerInfo, add RegisterMeeting RPC, add MediaCoordinationService, add MediaConnectionFailed signaling message
**Specialist**: protocol
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-proto`
**Duration**: ~30m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `799ddac4f2d6452f2cdb1b62bacc374f6e8737fa` |
| Branch | `feature/mh-quic-proto` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@mh-quic-proto` |
| Implementing Specialist | `protocol` |
| Iteration | `1` |
| Security | `security@mh-quic-proto` |
| Test | `test@mh-quic-proto` |
| Observability | `observability@mh-quic-proto` |
| Code Quality | `code-reviewer@mh-quic-proto` |
| DRY | `dry-reviewer@mh-quic-proto` |
| Operations | `operations@mh-quic-proto` |

---

## Task Overview

### Objective
Update proto definitions for the MH QUIC/WebTransport connection story: modify existing messages (MhAssignment, MediaServerInfo), add new RPCs (RegisterMeeting), add new service (MediaCoordinationService), and add new signaling message (MediaConnectionFailed).

### Scope
- **Service(s)**: proto-gen (proto definitions), affects all services consuming protos
- **Schema**: No
- **Cross-cutting**: Yes — proto changes affect GC, MC, MH consumers

### Debate Decision
NOT NEEDED - Changes are specified in user story design section with exact field numbers and message definitions.

---

## Planning

All 6 reviewers confirmed plan.

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

**Key reviewer input incorporated:**
- Observability: `DisconnectReason` enum instead of string for `ParticipantMediaDisconnected.reason` (bounded cardinality for metrics labels) — adopted
- Security: Token_type anti-confusion, guest token handling, endpoint validation — noted for downstream tasks
- Code Quality: ADR compliance verified, notes for downstream tasks (#2-#8)
- DRY: Auth interceptor extraction opportunity noted for tasks #3/#7
- Test: Proto field numbering verified correct, test impact is low
- Operations: Wire-compatible changes confirmed, no deployment risk

---

## Pre-Work

None

---

## Implementation Summary

### Proto: internal.proto
| Item | Before | After |
|------|--------|-------|
| MhRole enum | Present (UNSPECIFIED, PRIMARY, BACKUP) | Removed |
| MhAssignment.role (field 3) | MhRole role | reserved 3 |
| MhAssignment.grpc_endpoint (field 4) | Not present | string grpc_endpoint = 4 |
| MediaHandlerService RPCs | Register, RouteMedia, StreamTelemetry | + RegisterMeeting |
| DisconnectReason enum | Not present | UNSPECIFIED, CLIENT_CLOSED, TIMEOUT, ERROR |
| MediaCoordinationService | Not present | NotifyParticipantConnected, NotifyParticipantDisconnected |

### Proto: signaling.proto
| Item | Before | After |
|------|--------|-------|
| MediaServerInfo.connection_token (field 2) | string connection_token | reserved 2 |
| MediaConnectionFailed message | Not present | media_handler_url, error_reason, all_handlers_failed |
| ClientMessage oneof field 11 | Not present | MediaConnectionFailed media_connection_failed |

### Rust Code
- GC `mc_client.rs`: Removed MhRole import and primary/backup role assignment; added grpc_endpoint propagation
- GC `mh_selection.rs`: Added `grpc_endpoint: String` to `MhAssignmentInfo`; updated 5 construction sites (2 runtime + 3 test)
- MH `mh_service.rs`: Added `register_meeting` stub with validation for all 3 required fields
- MH `metrics.rs`: Updated cardinality comments and tests for new `register_meeting` method label

---

## Files Modified

```
 crates/gc-service/src/services/mc_client.rs        | 13 +----
 crates/gc-service/src/services/mh_selection.rs     |  8 +++
 crates/mh-service/src/grpc/mh_service.rs           | 39 ++++++++++-
 crates/mh-service/src/observability/metrics.rs     | 12 ++--
 proto/internal.proto                               | 66 ++++++++++++++---
 proto/signaling.proto                              | 10 ++-
```

### Key Changes by File
| File | Changes |
|------|---------|
| `proto/internal.proto` | Removed MhRole, updated MhAssignment, added RegisterMeeting RPC, added DisconnectReason enum, added MediaCoordinationService |
| `proto/signaling.proto` | Removed connection_token from MediaServerInfo, added MediaConnectionFailed to ClientMessage |
| `crates/gc-service/src/services/mc_client.rs` | Removed MhRole usage, added grpc_endpoint to MhAssignment construction |
| `crates/gc-service/src/services/mh_selection.rs` | Added grpc_endpoint field to MhAssignmentInfo |
| `crates/mh-service/src/grpc/mh_service.rs` | Added register_meeting stub with field validation |
| `crates/mh-service/src/observability/metrics.rs` | Updated cardinality for register_meeting method |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS
**Duration**: ~15s

### Layer 2: cargo fmt
**Status**: PASS
**Duration**: ~1s

### Layer 3: Simple Guards
**Status**: ALL PASS
**Duration**: ~5s

| Guard | Status |
|-------|--------|
| api-version-check | PASS |
| grafana-datasources | PASS |
| instrument-skip-all | PASS |
| no-hardcoded-secrets | PASS |
| no-pii-in-logs | PASS |
| no-secrets-in-logs | PASS |
| test-coverage | PASS |
| test-registration | PASS |
| test-rigidity | PASS |
| validate-application-metrics | PASS |
| validate-env-config | PASS |
| validate-histogram-buckets | PASS |
| validate-infrastructure-metrics | PASS |
| validate-knowledge-index | PASS |
| validate-kustomize | PASS |

### Layer 4: Tests
**Status**: PASS
**Duration**: ~36s
**Tests**: All workspace tests pass, 0 failures

### Layer 5: Clippy
**Status**: PASS
**Duration**: ~6s
**Output**: Zero warnings

### Layer 6: Audit
**Status**: Pre-existing vulnerabilities only (quinn-proto 0.10.6, ring 0.16.20, rsa 0.9.10)
**Notes**: No new dependencies added by this devloop

### Layer 7: Semantic Guards
**Status**: PASS
**Verdict**: No blocking issues. Two minor non-blocking observations (mc_id/mc_grpc_endpoint validation — subsequently fixed; SSRF note for future real implementation).

### Layer 8: Env-tests
**Status**: PASS (pre-existing WebTransport infra failures)
**Duration**: ~11s
**Notes**: 3 WebTransport connection timeout failures in 24_join_flow.rs are pre-existing (confirmed by testing base commit). All non-WebTransport tests pass (96 tests).

---

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed, 0 deferred

Finding: register_meeting stub only validated meeting_id, not mc_id or mc_grpc_endpoint. Fixed by adding empty checks for all 3 required fields.

### Test Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

Proto field numbering verified correct. Wire compatibility maintained. No test regressions.

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 1 found, 1 fixed, 0 deferred

Finding: MH metrics.rs cardinality comment, test coverage, and bounds test didn't account for new register_meeting method. Fixed by updating all 3 locations.

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 2 found, 2 fixed, 0 deferred

Finding 1: DisconnectReason enum placed in wrong proto section (cosmetic). Fixed by relocating to MediaCoordinationService section.
Finding 2: register_meeting validation (same as security finding). Already fixed.

ADR Compliance: ADR-0002 (no-panic), ADR-0003 (service auth), ADR-0004 (versioning), ADR-0010 (GC architecture), ADR-0011 (observability), ADR-0019 (DRY) — all compliant.

### DRY Reviewer
**Verdict**: CLEAR

**True duplication findings**: None
**Extraction opportunities**: None new (existing auth interceptor duplication already tracked in TODO.md)

### Operations Reviewer
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

No deployment surface impact. Wire-compatible changes. No new infra requirements.

---

## Tech Debt

### Deferred Findings

No deferred findings — all findings were fixed.

### Cross-Service Duplication (from DRY Reviewer)

No cross-service duplication detected in this task. Existing auth interceptor duplication (MC vs MH) already tracked in `docs/TODO.md`.

### Temporary Code (from Code Reviewer)

| Item | Location | Reason | Follow-up Task |
|------|----------|--------|----------------|
| register_meeting stub | `crates/mh-service/src/grpc/mh_service.rs` | Stub returns accepted without real registration | Task #5: MH RegisterMeeting handler + SessionManager |

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `799ddac4f2d6452f2cdb1b62bacc374f6e8737fa`
2. Review all changes: `git diff 799ddac4f2d6452f2cdb1b62bacc374f6e8737fa..HEAD`
3. Soft reset (preserves changes): `git reset --soft 799ddac4f2d6452f2cdb1b62bacc374f6e8737fa`
4. Hard reset (clean revert): `git reset --hard 799ddac4f2d6452f2cdb1b62bacc374f6e8737fa`

---

## Reflection

All 7 teammates updated their INDEX.md navigation files with new pointers for proto services, messages, and Rust code locations. INDEX guard passed after fixing 2 stale pointers (proto service reference syntax).

---

## Issues Encountered & Resolutions

### Issue 1: Env-test WebTransport timeouts
**Problem**: 3 env-tests in 24_join_flow.rs failed with WebTransport ConnectionError(TimedOut) after rebuild-all
**Resolution**: Confirmed pre-existing by testing base commit — same failures occur without proto changes. Infrastructure issue with UDP port forwarding through Kind NodePorts in container environment.

### Issue 2: INDEX guard stale pointers
**Problem**: Two INDEX.md files used `proto/internal.proto:MediaHandlerService.RegisterMeeting` syntax which the guard flagged as stale
**Resolution**: Changed to `proto/internal.proto` (service name in parenthetical comment instead of colon-separated symbol)

---

## Lessons Learned

1. DisconnectReason enum (observability recommendation) was a valuable improvement over the user story's string design — prevents unbounded metric label cardinality
2. Pre-commit hooks catch formatting issues that `cargo fmt` fixes but aren't staged — always run fmt before commit
3. INDEX.md pointer syntax must use file paths only (with optional `:symbol`), not proto service method references

---

## Appendix: Verification Commands

```bash
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
cargo test --workspace
cargo clippy --workspace -- -D warnings
```
