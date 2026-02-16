# Dev-Loop Output: ADR-0023 Phase 6a MC Foundation

**Date**: 2026-01-25
**Task**: ADR-0023 Phase 6a (focused): Core MC protos (JoinRequest/Response, basic signaling), MC crate skeleton, McTestUtils crate skeleton
**Branch**: `feature/adr-0023-mc-architecture`
**Duration**: ~30m
**Status**: Complete

---

## Loop State (Internal)

<!-- This section is maintained by dev-loop skills for state recovery. -->

| Field | Value |
|-------|-------|
| Implementing Agent | `meeting-controller-specialist` |
| Implementing Specialist | `meeting-controller` |
| Current Step | `complete` |
| Iteration | `1` |
| Security Reviewer | `a57ef7a` |
| Test Reviewer | `ad672e7` |
| Code Reviewer | `aacab4e` |
| DRY Reviewer | `a715dae` |

---

## Task Overview

### Objective

ADR-0023 Phase 6a (focused): Core MC protos (JoinRequest/Response, basic signaling), MC crate skeleton, McTestUtils crate skeleton

### Scope

- **Service(s)**: meeting-controller
- **Schema**: Proto definitions (signaling.proto additions)
- **Cross-cutting**: Proto regeneration affects proto-gen crate

### Debate Decision

ADR-0023 (Meeting Controller Architecture) - Accepted 2026-01-23

---

## Matched Principles

The following principle categories were matched:

- `docs/principles/api-design.md` - Protocol buffer design patterns
- `docs/principles/errors.md` - Error handling patterns

---

## Pre-Work

- Read ADR-0023 for full architecture specification
- Analyzed existing proto files (signaling.proto, internal.proto)
- Reviewed GC crate structure for patterns to follow
- Reviewed ac-test-utils and gc-test-utils for test utilities patterns

---

## Implementation Summary

### 1. Proto Updates (signaling.proto)

**Session Binding Fields (ADR-0023 Section 1)**:
- Extended `JoinRequest` with `correlation_id` (field 5) and `binding_token` (field 6)
- Extended `JoinResponse` with `correlation_id` (field 6) and `binding_token` (field 7)
- Added `TIMEOUT = 4` to `LeaveReason` enum for 30s disconnect grace period

**Mute State Messages (ADR-0023 Section 6)**:
- `MuteRequest`: Client self-mute control (audio/video bools)
- `ParticipantMuteUpdate`: Broadcast mute state with self-mute and host-mute fields
- `HostMuteRequest`: Host enforced mute with reason
- `UnmuteRequest`/`UnmuteResponse`: Participant unmute request flow

**Migration Support**:
- `RedirectToMc`: Server notification for MC migration/rebalance

**Message Wrappers Updated**:
- `ClientMessage`: Added mute_request, host_mute_request, unmute_request, unmute_response
- `ServerMessage`: Added participant_mute_update, unmute_request, unmute_response, redirect_to_mc

### 2. Meeting Controller Crate Skeleton

**src/lib.rs**: Module structure with documentation referencing ADR-0023 actor hierarchy

**src/config.rs**:
- Required: `REDIS_URL`, `MC_BINDING_TOKEN_SECRET`
- ADR-0023 parameters: binding_token_ttl, clock_skew, nonce_grace_window, disconnect_grace_period
- Capacity: max_meetings, max_participants
- Endpoints: webtransport, grpc, health bind addresses
- Custom Debug impl redacts secrets

**src/errors.rs**:
- `McError` enum with ErrorCode mapping (UNAUTHORIZED=2, FORBIDDEN=3, NOT_FOUND=4, CONFLICT=5, INTERNAL_ERROR=6, CAPACITY_EXCEEDED=7)
- `SessionBindingError` nested enum (TokenExpired, InvalidToken, NonceReused, SessionNotFound, UserIdMismatch)
- `client_message()` method hides internal details

**src/main.rs**: Skeleton with config loading and TODO markers for Phase 6b+

**Cargo.toml**: Added dependencies (tokio-util, serde_json, chrono, tonic, ring) and mc-test-utils dev-dependency

### 3. McTestUtils Crate Skeleton

**mock_gc.rs**: MockGc with builder pattern for registration accept/reject

**mock_mh.rs**: MockMh with capacity tracking, ID, and registration configuration

**mock_redis.rs**: Full implementation with:
- Session state storage (get/set/delete)
- Fencing generation tracking and validation
- Nonce consumption (SETNX pattern) for replay prevention
- Fenced write operations

**fixtures/mod.rs**: Test data builders:
- `TestMeeting`: ID, name, max_participants, e2e_enabled
- `TestParticipant`: participant_id, user_id, name, is_guest
- `TestBindingToken`: correlation_id, user_id, participant_id, nonce, token

---

## Files Created/Modified

### Created
| File | Purpose |
|------|---------|
| `crates/meeting-controller/src/lib.rs` | MC library module |
| `crates/meeting-controller/src/config.rs` | Configuration with ADR-0023 parameters |
| `crates/meeting-controller/src/errors.rs` | Error types with ErrorCode mapping |
| `crates/mc-test-utils/Cargo.toml` | Test utilities crate manifest |
| `crates/mc-test-utils/src/lib.rs` | Test utilities module |
| `crates/mc-test-utils/src/mock_gc.rs` | Mock Global Controller |
| `crates/mc-test-utils/src/mock_mh.rs` | Mock Media Handler |
| `crates/mc-test-utils/src/mock_redis.rs` | Mock Redis with fencing |
| `crates/mc-test-utils/src/fixtures/mod.rs` | Test data fixtures |
| `docs/dev-loop-outputs/2026-01-25-adr-0023-phase-6a-mc-foundation/meeting-controller.md` | Specialist checkpoint |

### Modified
| File | Changes |
|------|---------|
| `proto/signaling.proto` | Added session binding, mute, and redirect messages |
| `crates/meeting-controller/src/main.rs` | Updated skeleton with config loading |
| `crates/meeting-controller/Cargo.toml` | Added dependencies |
| `Cargo.toml` | Added mc-test-utils to workspace members |
| `crates/proto-gen/src/lib.rs` | Allow struct_excessive_bools for generated code |

---

## Verification Results

| Layer | Command | Result |
|-------|---------|--------|
| 1 | `cargo check --workspace` | PASS |
| 2 | `cargo fmt --all --check` | PASS |
| 3 | `./scripts/guards/run-guards.sh` | PASS (8/8 guards) |
| 4 | `./scripts/test.sh --workspace --lib` | PASS (283+ tests) |
| 5 | `./scripts/test.sh --workspace` | PASS |
| 6 | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| 7 | `./scripts/guards/run-guards.sh --semantic` | PASS (semantic disabled) |

### New Tests Added
- `crates/meeting-controller/src/config.rs`: 6 tests (config loading, validation, redaction)
- `crates/meeting-controller/src/errors.rs`: 4 tests (error codes, client messages, display)
- `crates/mc-test-utils/src/mock_gc.rs`: 2 tests (builder, shortcuts)
- `crates/mc-test-utils/src/mock_mh.rs`: 3 tests (builder, capacity, default)
- `crates/mc-test-utils/src/mock_redis.rs`: 5 tests (session, fencing, nonce, fenced write, builder)
- `crates/mc-test-utils/src/fixtures/mod.rs`: 4 tests (meeting, participant, guest, binding token)

**Total new tests**: 24

---

## Code Review Results

**Overall Verdict**: APPROVED (All 4 reviewers)

### Security Specialist
**Verdict**: APPROVED
- Config properly redacts sensitive fields (redis_url, binding_token_secret)
- Error types define client-safe messages that hide internal details
- Protocol definitions correctly implement session binding pattern from ADR-0023
- **Tech Debt**: TD-SEC-001 (validate binding token secret), TD-SEC-002 (real HMAC in fixtures), TD-SEC-003 (rate limiting in mocks)

### Test Specialist
**Verdict**: APPROVED
- 24 new tests with comprehensive coverage
- All executable code paths covered including config, errors, and mock utilities
- Security-critical tests present (secret redaction, nonce replay prevention, fencing)
- **Tech Debt**: TD-1 (integration tests for main binary), TD-2 (MockRedis async interface)

### Code Quality Reviewer
**Verdict**: APPROVED
- All production code uses Result<T, E> with custom error types (no panics)
- Follows ADR-0002/0004 conventions
- Comprehensive documentation with ADR references
- **Tech Debt**: Config validation, async mutex, placeholder modules, session clone optimization

### DRY Reviewer
**Verdict**: APPROVED
- Follows established patterns from AC and GC services
- No BLOCKER duplication found
- **Tech Debt**: Instance ID generation (extractable), config module pattern, Cargo.toml comments

---

## Reflection

### What Worked Well
1. Following GC patterns made config and error design straightforward
2. Proto field numbering avoided conflicts with existing fields
3. Mock Redis provides good foundation for fencing and session testing
4. Builder pattern in test fixtures makes test code readable

### Lessons Learned
1. Generated protobuf code may trigger clippy lints (struct_excessive_bools)
2. Dead code warnings expected in skeleton implementations - use targeted `#[allow]`
3. Doc markdown lint requires backticks around code identifiers

### Knowledge Files Created/Updated

| Specialist | Added | Updated | Pruned |
|------------|-------|---------|--------|
| meeting-controller | 13 | 0 | 0 |
| security | 2 | 0 | 0 |
| test | 2 | 0 | 0 |
| code-reviewer | 5 | 0 | 0 |
| dry-reviewer | 1 | 0 | 0 |

**New knowledge files created**:
- `docs/specialist-knowledge/meeting-controller/patterns.md` (5 patterns)
- `docs/specialist-knowledge/meeting-controller/gotchas.md` (4 gotchas)
- `docs/specialist-knowledge/meeting-controller/integration.md` (4 integration notes)

### Technical Debt (Non-Blocking)
- 14 tech debt items documented across reviewers (see Code Review Results)

### Next Steps (Phase 6b)
1. Implement actor hierarchy (MeetingControllerActor, MeetingActor, ConnectionActor)
2. Add Redis connection pool with circuit breaker
3. Implement session binding token generation/validation with HMAC-SHA256
4. Add nonce management with Redis SETNX
