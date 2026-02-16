# Global Controller Specialist Checkpoint

**Date**: 2026-01-24
**Task**: ADR-0010 Section 4a (GC-side) - MH Registry, GC→MC Assignment RPC, MC Rejection Handling

---

## Patterns Discovered

### 1. Tonic Channel Caching for gRPC Clients
HTTP/2 connection pooling is built into tonic Channels. By caching channels per endpoint, we avoid connection churn for repeated calls to the same MC.

```rust
let mut channels = self.channels.write().await;
channels.insert(endpoint.to_string(), channel.clone());
```

### 2. Weighted Random Selection for Load Balancing
Used CSPRNG-based weighted random selection for both MH and MC selection. Weight is inversely proportional to load ratio:
- 0% loaded = weight 1.0
- 90% loaded = weight 0.1

This prevents thundering herd while preferring less-loaded instances.

### 3. Module-Level `#![allow(dead_code)]` for Incremental Development
When building infrastructure that won't be immediately wired into handlers, use module-level dead_code allows with explanatory comments:

```rust
// Allow dead code during incremental development - will be wired into handlers
// in a future phase.
#![allow(dead_code)]
```

### 4. Mock Trait Pattern for Testing gRPC Clients
Created `McClientTrait` trait with both real `McClient` and `MockMcClient` implementations. The mock supports:
- Always accepting
- Always rejecting (with reason)
- Custom response sequences (cycling)
- Error simulation
- Call counting

---

## Gotchas Encountered

### 1. prost Enum Naming
prost generates simpler enum variant names than expected. Instead of `MhRole::MhRolePrimary`, it generates `MhRole::Primary`.

### 2. `#[cfg(test)]` Module Visibility
Modules marked `#[cfg(test)]` are not available to integration tests in `tests/` directory. For test utilities needed in integration tests, either:
- Use a feature flag like `#[cfg(any(test, feature = "test-utils"))]`
- Or make the module always public (simpler for mocks)

### 3. Inner/Outer Doc Comment Conflict
Cannot have both `///` outer doc comments and `//!` inner doc comments on the same module. Clippy will error with `mixed_attributes_style`.

### 4. PostgreSQL Dynamic Interval
When using dynamic interval values (e.g., staleness threshold), cast with `($1 || ' seconds')::INTERVAL` pattern per the specialist knowledge gotchas.

---

## Key Decisions

| Decision | Rationale |
|----------|-----------|
| Made `mock` module public without `#[cfg(test)]` | Integration tests in `tests/` directory need access |
| Used `#![allow(dead_code)]` at module level | Infrastructure not yet wired; cleaner than per-item allows |
| Returned `MhSelection` from `assign_meeting_with_mh` | Handler needs MH info to return to client |
| Max 3 retries for MC rejection | Per ADR-0010 Section 4a specification |
| Call MC BEFORE writing to DB | Per ADR-0010: "GC notifies MC BEFORE writing to database" |

---

## Current Status

### Completed
- [x] Phase 1: Proto + Schema
  - Added proto messages (AssignMeetingWithMhRequest, MhAssignment, MhRole, RejectionReason, RegisterMH, MHLoadReport)
  - Created migration for media_handlers table
  - Regenerated proto-gen crate

- [x] Phase 2: MH Registry
  - MediaHandlersRepository with UPSERT registration
  - Load report handling with health metrics
  - MH health checker background task
  - MH selection service with weighted random

- [x] Phase 3: GC→MC Assignment RPC
  - McClient with tonic Channel caching
  - MockMcClient for testing
  - assign_meeting_with_mh with retry logic
  - Integration tests for retry and MH selection

### Deferred (as planned)
- [ ] Phase 4: MH Cross-Region Sync
- [ ] Phase 5: RequestMhReplacement RPC

### Not Yet Wired
The following are implemented but not wired into handlers/main.rs:
- MhService gRPC handlers
- start_mh_health_checker task
- assign_meeting_with_mh in handlers

These will be wired in a follow-up task when the full flow is ready.

---

## Verification Results

| Layer | Command | Result |
|-------|---------|--------|
| 1 | `cargo check --workspace` | PASS |
| 2 | `cargo fmt --all --check` | PASS |
| 3 | `./scripts/guards/run-guards.sh` | PASS (8/8) |
| 4 | `./scripts/test.sh --workspace --lib` | PASS (245 tests) |
| 5 | `./scripts/test.sh -p global-controller` | PASS (309+ tests) |
| 6 | `cargo clippy --workspace -- -D warnings` | PASS |
| 7 | `./scripts/guards/run-guards.sh --semantic` | PASS |

---

## Iteration 2: Fix Code Review Findings (2026-01-24)

### Fixes Applied

**Security (3 fixes)**:
1. **handler_id validation** in `mh_service.rs`: Added `validate_handler_id()` with length limit (255), character restrictions (alphanumeric, hyphen, underscore)
2. **endpoint URL validation** in `mh_service.rs`: Added `validate_endpoint()` with scheme validation (http/https/grpc), length limit (255)
3. **SecretString for token** in `mc_client.rs`: Changed `service_token: String` to `service_token: SecretString` to prevent accidental logging

**Test (4 fixes)**:
4. **Mixed error/success test**: Added `test_assign_meeting_with_mh_mixed_rejection_then_accept` - verifies retry succeeds after initial rejection
5. **Degraded health status test**: Added `test_load_report_with_degraded_health_status` - boundary value for HealthStatus::Degraded
6. **Concurrent assignment test**: Added `test_concurrent_assignment_same_meeting` - race condition handling for same meeting
7. **Max capacity tests**: Added `test_get_candidate_mhs_all_at_max_capacity` and `test_candidate_selection_load_ratio_boundary` - edge cases when all MHs are full

**Code Quality (1 fix)**:
8. **Timestamp fallback**: Changed from `SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(0)` to `chrono::Utc::now().timestamp() as u64` for consistency with rest of codebase

### Verification Results (Iteration 2)

| Layer | Command | Result |
|-------|---------|--------|
| 1 | `cargo check --workspace` | PASS |
| 2 | `cargo fmt --all --check` | PASS |
| 3 | `./scripts/guards/run-guards.sh` | PASS (8/8) |
| 4 | `./scripts/test.sh --workspace --lib` | PASS (259 tests) |
| 5 | `./scripts/test.sh --workspace` | PASS |
| 6 | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| 7 | `./scripts/guards/run-guards.sh --semantic` | PASS (8/8) |

### New Test Coverage

- **mh_registry_tests.rs**: 11 tests (was 8)
  - Added: `test_load_report_with_degraded_health_status`, `test_get_candidate_mhs_all_at_max_capacity`, `test_candidate_selection_load_ratio_boundary`

- **mc_assignment_rpc_tests.rs**: 11 tests (was 9)
  - Added: `test_assign_meeting_with_mh_mixed_rejection_then_accept`, `test_concurrent_assignment_same_meeting`

- **mh_service.rs unit tests**: 15 validation tests added
  - handler_id: valid, empty, too_long, at_255, invalid_chars
  - region: valid, empty, too_long, at_50
  - endpoint: valid, empty, invalid_scheme, too_long, at_255

---

## Files Created

| Path | Purpose |
|------|---------|
| `migrations/20260124000001_mh_registry.sql` | Media handlers registry table |
| `crates/global-controller/src/repositories/media_handlers.rs` | MH registration, load reports, selection queries |
| `crates/global-controller/src/services/mh_selection.rs` | MH selection with weighted random |
| `crates/global-controller/src/services/mc_client.rs` | MC gRPC client with mock |
| `crates/global-controller/src/grpc/mh_service.rs` | MH registration gRPC handlers |
| `crates/global-controller/src/tasks/mh_health_checker.rs` | Background MH health checker |
| `crates/global-controller/tests/mh_registry_tests.rs` | Integration tests (8 tests) |
| `crates/global-controller/tests/mc_assignment_rpc_tests.rs` | Integration tests (9 tests) |

## Files Modified

| Path | Changes |
|------|---------|
| `proto/internal.proto` | Added MeetingControllerService.AssignMeetingWithMh RPC, MediaHandlerRegistryService, new message types |
| `crates/global-controller/src/services/mc_assignment.rs` | Added assign_meeting_with_mh with retry logic |
| `crates/global-controller/src/services/mod.rs` | Export new services |
| `crates/global-controller/src/repositories/mod.rs` | Export media_handlers |
| `crates/global-controller/src/grpc/mod.rs` | Export mh_service |
| `crates/global-controller/src/tasks/mod.rs` | Export mh_health_checker |
| `crates/global-controller/Cargo.toml` | Added async-trait dependency |
