# Meeting Controller Specialist Checkpoint

**Date**: 2026-01-25
**Task**: ADR-0023 Phase 6a (focused): Core MC protos, MC crate skeleton, McTestUtils crate skeleton
**Status**: Complete

---

## Patterns Discovered

### 1. Proto Evolution for Session Binding
- Extended `JoinRequest` and `JoinResponse` with session recovery fields
- Added `correlation_id` and `binding_token` for reconnection support
- Added `TIMEOUT` to `LeaveReason` enum for 30s disconnect grace period
- Proto field numbers carefully chosen to avoid conflicts with existing fields

### 2. Mute State Model (Two-Tier)
- Self-mute: Informational only (client-controlled, not enforced)
- Host-mute: Server-enforced (MC + MH enforce)
- Separate bool fields for audio/video in each tier
- `host_muted_by` field tracks who applied the mute

### 3. Config Pattern Following GC
- Required secrets: `REDIS_URL`, `MC_BINDING_TOKEN_SECRET`
- All ADR-0023 parameters configurable via environment
- Debug impl redacts sensitive fields
- `from_vars()` for testability with explicit HashMap input

### 4. Error Hierarchy Design
- `McError`: Top-level errors with `ErrorCode` mapping
- `SessionBindingError`: Nested enum for binding-specific failures
- Client-safe messages hide internal details
- `From<SessionBindingError>` impl for ergonomic conversion

### 5. Test Utils Builder Pattern
- Fluent API with `builder().with_*().build()` pattern
- `MockRedis` supports fencing validation and nonce consumption
- Fixtures for meetings, participants, and binding tokens
- All test fixtures generate random IDs by default for isolation

---

## Gotchas Encountered

### 1. Clippy: Excessive Bools in Proto
- `ParticipantMuteUpdate` has 4 bools (audio/video x self/host)
- Proto-gen crate needed `#![allow(clippy::struct_excessive_bools)]`
- This is acceptable for generated code; real structs should avoid this

### 2. Dead Code Warnings in Skeleton
- Config fields and error types unused until Phase 6b+
- Added `#[allow(dead_code)]` with clear comments about future use
- Tests exercise the code, but main.rs skeleton doesn't yet

### 3. Doc Markdown Lint
- Clippy requires backticks around code identifiers in docs
- `ErrorCode` -> `` `ErrorCode` ``
- `HashMap` -> `` `HashMap` ``

### 4. Workspace Dependencies
- mc-test-utils must be added to workspace Cargo.toml members
- Meeting-controller needs `[lib]` and `[[bin]]` sections for both targets

---

## Key Decisions

### Proto Location
- Session binding fields added to `signaling.proto` (client-facing)
- Mute state messages added to `signaling.proto` (client-facing)
- `RedirectToMc` added to `signaling.proto` (server->client notification)
- Migration/drain protos deferred to Phase 6e per scope agreement

### Config Required vs Optional
- **Required**: `REDIS_URL`, `MC_BINDING_TOKEN_SECRET` (fails without these)
- **Optional with defaults**: All other config values
- Rationale: Better to fail early than run with missing critical config

### Test Utils Scope
- `MockGc`: Registration accept/reject only for now
- `MockMh`: Capacity tracking and registration
- `MockRedis`: Full session/fencing/nonce support (most complex)
- Fixtures: Basic builders, will expand in Phase 6b+

---

## Current Status

### Files Created
- `/home/nathan/code/dark_tower/crates/meeting-controller/src/lib.rs` - MC library module
- `/home/nathan/code/dark_tower/crates/meeting-controller/src/config.rs` - Config with ADR-0023 parameters
- `/home/nathan/code/dark_tower/crates/meeting-controller/src/errors.rs` - Error types with ErrorCode mapping
- `/home/nathan/code/dark_tower/crates/mc-test-utils/Cargo.toml` - Test utils crate manifest
- `/home/nathan/code/dark_tower/crates/mc-test-utils/src/lib.rs` - Test utils module
- `/home/nathan/code/dark_tower/crates/mc-test-utils/src/mock_gc.rs` - Mock GC
- `/home/nathan/code/dark_tower/crates/mc-test-utils/src/mock_mh.rs` - Mock MH
- `/home/nathan/code/dark_tower/crates/mc-test-utils/src/mock_redis.rs` - Mock Redis with fencing
- `/home/nathan/code/dark_tower/crates/mc-test-utils/src/fixtures/mod.rs` - Test fixtures

### Files Modified
- `/home/nathan/code/dark_tower/proto/signaling.proto` - Added session binding and mute messages
- `/home/nathan/code/dark_tower/crates/meeting-controller/src/main.rs` - Updated skeleton with TODO markers
- `/home/nathan/code/dark_tower/crates/meeting-controller/Cargo.toml` - Added dependencies
- `/home/nathan/code/dark_tower/Cargo.toml` - Added mc-test-utils to workspace
- `/home/nathan/code/dark_tower/crates/proto-gen/src/lib.rs` - Allow struct_excessive_bools

### Verification Results
- Layer 1 (cargo check): PASS
- Layer 2 (cargo fmt): PASS
- Layer 3 (guards): PASS (8/8)
- Layer 4 (unit tests): PASS (283+ tests)
- Layer 5 (all tests): PASS
- Layer 6 (clippy): PASS
- Layer 7 (semantic guards): PASS (skipped - disabled)

---

## Next Steps (Phase 6b)

1. Implement `MeetingControllerActor` with supervision
2. Implement `MeetingActor` with state management
3. Implement `ConnectionActor` for WebTransport handling
4. Add Redis connection pool with circuit breaker
5. Implement session binding token generation/validation
6. Add nonce management with Redis SETNX
