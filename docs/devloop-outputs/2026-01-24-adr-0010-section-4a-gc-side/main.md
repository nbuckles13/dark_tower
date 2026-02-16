# Dev-Loop Output: ADR-0010 Section 4a (GC Side)

**Date**: 2026-01-24
**Task**: ADR-0010 tasks marked 4a under Implementation Status (GC-side only)
**Branch**: `feature/adr-0023-mc-architecture`
**Primary Specialist**: global-controller
**Duration**: ~0m (in progress)

---

## Loop State (Internal)

<!-- This section is maintained by dev-loop skills for state recovery. -->

| Field | Value |
|-------|-------|
| Implementing Agent | `a992bf4` |
| Implementing Specialist | `global-controller` |
| Current Step | `complete` |
| Iteration | `2` |
| Security Reviewer | `a10e8f5` |
| Test Reviewer | `ac0ea95` |
| Code Reviewer | `aca77f3` |
| DRY Reviewer | `a31aa29` |
| MC Reviewer | `a59feb4` |

---

## Task Overview

### Objective

ADR-0010 tasks marked 4a under Implementation Status (GC-side only, Phases 1-3):
- GC→MC AssignMeeting RPC (GC sends meeting assignment to MC)
- MC Rejection Handling in GC (Retry with different MC on rejection)
- MH Registry in GC (MH registration + load reports)

**Deferred to follow-up tasks:**
- MH Cross-Region Sync (Sync MH registry via GC-to-GC)
- RequestMhReplacement RPC (GC handles MC's request for MH replacement)

### Scope

- **Service(s)**: global-controller (primary), meeting-controller (review only)
- **Schema**: May need `media_handlers` table, MH assignment tracking
- **Cross-cutting**: Protocol changes in `proto/gc_mc_internal.proto`

### Special Review Requirements

- **MC Specialist**: Included in code review to comment on protocol/message design choices made by GC

### Debate Decision

Not required - implementing existing ADR-0010 design decisions.

---

## Matched Principles

The following principle categories were matched:

- `docs/principles/api-design.md` - gRPC protocol design (AssignMeeting, RequestMhReplacement)
- `docs/principles/concurrency.md` - Async MH registry, cross-region sync
- `docs/principles/errors.md` - Rejection handling, error responses
- `docs/principles/logging.md` - Meeting/MH assignment event logging

---

## Knowledge Files

Global-controller specialist knowledge available:
- `docs/specialist-knowledge/global-controller/patterns.md`
- `docs/specialist-knowledge/global-controller/gotchas.md`
- `docs/specialist-knowledge/global-controller/integration.md`

---

## Pre-Work

TBD - To be defined during planning phase

---

## Planning Phase

### Status: Ready for Implementation

**Refined Objective**: Implement ADR-0010 Section 4a GC-side components (Phases 1-3): MH registry with registration/load reports, GC→MC AssignMeeting RPC with MH assignments, and MC rejection handling with retry logic

### Proposed Approach

This is a substantial implementation spanning proto changes, database schema, services, and handlers. The task should be broken into phases:

**Phase 1: Foundation (Proto + Schema)**
- Add proto messages per ADR-0010 Section 4a: AssignMeetingRequest with mh_assignments, MhAssignment, MhRole enum, RejectionReason enum, RegisterMh, MhLoadReport, MhReplacementRequest/Response
- Create MH registry table migration (media_handlers)
- Regenerate proto-gen crate

**Phase 2: MH Registry**
- MH registration repository with UPSERT pattern
- MH load report handling (update current_streams, health metrics)
- MH health checker background task (mark stale MHs unhealthy)
- MH selection service with weighted scoring per ADR-0023 Section 5f

**Phase 3: GC→MC Assignment RPC**
- MC gRPC client wrapper with connection pooling (tonic Channel cache)
- AssignMeeting RPC that sends meeting_id + MH assignments to MC
- Modify meeting join flow: select MC, select MHs, call MC RPC, then write to DB
- MC rejection handling with retry logic (max 3 attempts per ADR-0010)

**Phase 4: Cross-Region MH Sync** *(DEFERRED)*

**Phase 5: MH Replacement RPC** *(DEFERRED)*

### Files to Modify

| Path | Changes |
|------|---------|
| `proto/internal.proto` | Add AssignMeetingRequest, MhAssignment, MhRole, RejectionReason, RegisterMh, MhLoadReport to MeetingControllerService |
| `crates/global-controller/src/services/mc_assignment.rs` | Modify assign_meeting to call MC via gRPC before database write, add retry logic |
| `crates/global-controller/src/handlers/meetings.rs` | Update join_meeting and get_guest_token to use new assignment flow with MH selection |
| `crates/global-controller/src/grpc/mc_service.rs` | Add MH registration and load report handlers |

### Files to Create

| Path | Purpose |
|------|---------|
| `migrations/20260124000001_mh_registry.sql` | Media handlers registry table with load/health tracking |
| `crates/global-controller/src/repositories/media_handlers.rs` | MH registration, load reports, selection queries |
| `crates/global-controller/src/services/mh_selection.rs` | MH selection with weighted scoring per ADR-0023 |
| `crates/global-controller/src/services/mc_client.rs` | MC gRPC client wrapper with connection pooling |
| `crates/global-controller/src/grpc/mh_service.rs` | MH registration and load report gRPC handlers |
| `crates/global-controller/src/tasks/mh_health_checker.rs` | Background task to mark stale MHs as unhealthy |
| `crates/global-controller/tests/mh_registry_tests.rs` | Integration tests for MH registry operations |
| `crates/global-controller/tests/mc_assignment_rpc_tests.rs` | Integration tests for GC→MC assignment with retry |
| `crates/global-controller/src/services/mc_client/mock.rs` | Mock MC client for unit testing |

### Key Decisions

| Decision | Rationale |
|----------|-----------|
| Extend existing `MeetingControllerService` | User decision - keeps proto organization consistent |
| Use tonic Channel with caching for MC connections | HTTP/2 connection pooling is built into tonic; caching channels by endpoint avoids connection churn |
| Order of operations: Call MC before writing to DB | Per ADR-0010: "GC notifies MC BEFORE writing to database. This ensures MC has accepted before the assignment is recorded." |
| Max 3 retries for MC rejection | Per ADR-0010 Section 4a: "Max 3 retries before returning 503" |
| Mock MC client + full MC harness | Unit tests use mock for isolation; integration tests use real MC for end-to-end validation |
| Defer Phases 4-5 to follow-up tasks | Cross-region sync and MH replacement add complexity; core functionality works without them |

### Clarifications (Resolved)

| Question | Answer |
|----------|--------|
| Scope | **Phases 1-3 only** - defer cross-region sync (Phase 4) and MH replacement (Phase 5) to follow-up tasks |
| Proto service naming | **Extend existing** `MeetingControllerService` rather than creating new service |
| Testing strategy | **Both** - mock MC client for unit tests AND full MC harness for integration tests |

### Escalation Assessment

**Not recommending escalation.** ADR-0010 and ADR-0023 provide sufficient specification. Proto changes are additive and will be reviewed by MC specialist during code review

---

## Implementation

### Summary

Implemented ADR-0010 Section 4a GC-side components across three phases:

**Phase 1: Foundation (Proto + Schema)**
- Added new proto messages: `AssignMeetingWithMhRequest`, `AssignMeetingWithMhResponse`, `MhAssignment`, `MhRole`, `RejectionReason`, `RegisterMHRequest`, `RegisterMHResponse`, `MHLoadReportRequest`, `MHLoadReportResponse`
- Added new RPC: `MeetingControllerService.AssignMeetingWithMh`
- Added new service: `MediaHandlerRegistryService` (RegisterMH, SendLoadReport)
- Created migration `20260124000001_mh_registry.sql` for media_handlers table

**Phase 2: MH Registry**
- `MediaHandlersRepository` with UPSERT registration, load report updates, stale handler detection
- `MhSelectionService` with weighted random selection (CSPRNG-based)
- `MhService` gRPC handlers for RegisterMH and SendLoadReport
- `start_mh_health_checker` background task

**Phase 3: GC→MC Assignment RPC**
- `McClient` with tonic Channel caching for connection pooling
- `MockMcClient` for testing with cycling responses
- `assign_meeting_with_mh` service method with:
  - MH selection before MC selection
  - MC notification BEFORE DB write (per ADR-0010)
  - Retry logic with max 3 attempts
  - Different MC selection on rejection

### Files Modified

| Path | Changes |
|------|---------|
| `proto/internal.proto` | Added AssignMeetingWithMh RPC, MediaHandlerRegistryService, 8 new message types |
| `crates/global-controller/src/services/mc_assignment.rs` | Added assign_meeting_with_mh with retry logic |
| `crates/global-controller/src/services/mod.rs` | Export mc_client, mh_selection |
| `crates/global-controller/src/repositories/mod.rs` | Export media_handlers |
| `crates/global-controller/src/grpc/mod.rs` | Export mh_service |
| `crates/global-controller/src/tasks/mod.rs` | Export mh_health_checker |
| `crates/global-controller/Cargo.toml` | Added async-trait dependency |

### Files Created

| Path | Purpose |
|------|---------|
| `migrations/20260124000001_mh_registry.sql` | Media handlers registry table |
| `crates/global-controller/src/repositories/media_handlers.rs` | MH CRUD operations |
| `crates/global-controller/src/services/mh_selection.rs` | MH weighted selection |
| `crates/global-controller/src/services/mc_client.rs` | MC gRPC client + mock |
| `crates/global-controller/src/grpc/mh_service.rs` | MH gRPC handlers |
| `crates/global-controller/src/tasks/mh_health_checker.rs` | Background health checker |
| `crates/global-controller/tests/mh_registry_tests.rs` | 8 integration tests |
| `crates/global-controller/tests/mc_assignment_rpc_tests.rs` | 9 integration tests |

### Notes

- New code uses `#![allow(dead_code)]` module-level allows since not yet wired into handlers
- Mock module made public for integration test access
- Checkpoint written to `global-controller.md`

---

## Dev-Loop Verification Steps

### Orchestrator Validation (2026-01-24)

| Layer | Command | Result |
|-------|---------|--------|
| 1. Check | `cargo check --workspace` | PASS |
| 2. Format | `cargo fmt --all --check` | PASS |
| 3. Guards | `./scripts/guards/run-guards.sh` | PASS (8/8) |
| 4. Unit Tests | `./scripts/test.sh --workspace --lib` | PASS (245 tests) |
| 5. All Tests | `./scripts/test.sh --workspace` | PASS |
| 6. Clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS (after fix) |
| 7. Semantic | `./scripts/guards/run-guards.sh --semantic` | PASS (8/8) |

**Note**: Fixed unnecessary cast in `mh_registry_tests.rs:270` (`i as i32` → `i`) during validation

### Iteration 2 - Fix Code Review Findings (2026-01-24)

All 8 blocking findings from code review have been fixed:

**Security Fixes (3)**:
1. Added `validate_handler_id()` to `mh_service.rs` (length limit, character restrictions)
2. Added `validate_endpoint()` to `mh_service.rs` (scheme, length validation)
3. Changed `service_token` in `mc_client.rs` from `String` to `SecretString`

**Test Fixes (4)**:
4. Added `test_assign_meeting_with_mh_mixed_rejection_then_accept` - MC rejects first, accepts on retry
5. Added `test_load_report_with_degraded_health_status` - Degraded status boundary value
6. Added `test_concurrent_assignment_same_meeting` - Race condition handling
7. Added `test_get_candidate_mhs_all_at_max_capacity` and `test_candidate_selection_load_ratio_boundary` - All candidates at max load

**Code Quality Fix (1)**:
8. Changed timestamp fallback in `mh_service.rs` from `SystemTime::now().unwrap_or(0)` to `chrono::Utc::now().timestamp()` for consistency

| Layer | Command | Result |
|-------|---------|--------|
| 1. Check | `cargo check --workspace` | PASS |
| 2. Format | `cargo fmt --all --check` | PASS |
| 3. Guards | `./scripts/guards/run-guards.sh` | PASS (8/8) |
| 4. Unit Tests | `./scripts/test.sh --workspace --lib` | PASS (259 tests) |
| 5. All Tests | `./scripts/test.sh --workspace` | PASS (11 mh_registry tests, 11 mc_assignment_rpc tests) |
| 6. Clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| 7. Semantic | `./scripts/guards/run-guards.sh --semantic` | PASS (8/8) |

### Orchestrator Re-Validation (2026-01-24)

| Layer | Command | Result |
|-------|---------|--------|
| 1. Check | `cargo check --workspace` | PASS |
| 2. Format | `cargo fmt --all --check` | PASS |
| 3. Guards | `./scripts/guards/run-guards.sh` | PASS (8/8) |
| 4. Unit Tests | `./scripts/test.sh --workspace --lib` | PASS (259 tests) |
| 5. All Tests | `./scripts/test.sh --workspace` | PASS (flaky timing test retried) |
| 6. Clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| 7. Semantic | `./scripts/guards/run-guards.sh --semantic` | PASS (8/8) |

**Note**: `test_issue_user_token_timing_attack_prevention` in ac-service is a known flaky timing test unrelated to this change. Passed on retry.

---

## Validation

All 7 verification layers passed successfully. Key test coverage:

- **mh_registry_tests.rs** (8 tests): Registration, re-registration, load reports, stale detection, candidate selection
- **mc_assignment_rpc_tests.rs** (9 tests): Success flow, retry on rejection, max retries, no MCs, no MHs, existing assignment, backup MH selection

---

## Code Review

### Overall Verdict: REQUEST_CHANGES

| Reviewer | Verdict | Findings |
|----------|---------|----------|
| Security | REQUEST_CHANGES | 3 MINOR |
| Test | REQUEST_CHANGES | 4 MINOR |
| Code Reviewer | APPROVED | 1 MINOR, 2 TECH_DEBT |
| DRY Reviewer | APPROVED | 4 TECH_DEBT |
| MC Reviewer | APPROVED | 2 MINOR, 2 TECH_DEBT |

---

### Security Specialist (`a3f9912`)

**Verdict**: REQUEST_CHANGES

| ID | Severity | Description |
|----|----------|-------------|
| MINOR-001 | MINOR | `mh_service.rs` lacks handler_id format validation (length limit, character restrictions) that exists in `mc_service.rs` |
| MINOR-002 | MINOR | `mh_service.rs` lacks endpoint URL validation (scheme, length) that exists in `mc_service.rs` |
| MINOR-003 | MINOR | `mc_client.rs` stores `service_token` as plain `String` instead of `SecretString` per project patterns |

---

### Test Specialist (`a2a354d`)

**Verdict**: REQUEST_CHANGES

| ID | Severity | Description |
|----|----------|-------------|
| MINOR-1 | MINOR | RPC error retry test lacks mixed error/success test |
| MINOR-2 | MINOR | Health status `Degraded` (value 2) and invalid values not tested |
| MINOR-3 | MINOR | No concurrent assignment test for race condition handling |
| MINOR-4 | MINOR | Weighted random fallback (all candidates at max load) not tested |

---

### Code Quality Reviewer (`a401144`)

**Verdict**: APPROVED

| ID | Severity | Description |
|----|----------|-------------|
| MINOR-001 | MINOR | Timestamp fallback `.unwrap_or(0)` could return confusing epoch timestamp |
| TECH_DEBT-001 | TECH_DEBT | `weighted_random_select` duplicated for MH and MC - violates DRY |
| TECH_DEBT-002 | TECH_DEBT | Health status proto conversion uses inline magic numbers |

---

### DRY Reviewer (`ab6f35b`)

**Verdict**: APPROVED

**Tech Debt Items**:
1. TD-DRY-001: Extract generic `weighted_random_select<T: LoadBalancedCandidate>` to common crate
2. TD-DRY-002: Create generic health checker task builder
3. TD-DRY-003: Document registry SQL patterns as intentional structural similarity
4. TD-DRY-004: Consider extracting common gRPC validation helpers

---

### MC Protocol Reviewer (`a53443a`)

**Verdict**: APPROVED

| ID | Severity | Description |
|----|----------|-------------|
| MINOR-1 | MINOR | Consider adding `grpc_endpoint` to `MhAssignment` for MC-to-MH gRPC |
| MINOR-2 | MINOR | Consider whether `MeetingConfig` should be in assignment request |
| TECH_DEBT-1 | TECH_DEBT | Two assignment messages should eventually be consolidated |
| TECH_DEBT-2 | TECH_DEBT | Legacy `RegisterMeetingController` comment needs migration path |

---

### Summary of Blocking Findings (Must Fix)

**Security (3)**:
1. Add handler_id format validation to `mh_service.rs`
2. Add endpoint URL validation to `mh_service.rs`
3. Use `SecretString` for `service_token` in `mc_client.rs`

**Test (4)**:
1. Add mixed error/success RPC retry test
2. Add health status boundary value tests
3. Add concurrent assignment race condition test
4. Add weighted random edge case test (all high load)

**Code Quality (1)**:
1. Fix timestamp fallback in `mh_service.rs`

---

### Tech Debt (Non-Blocking, Documented)

- TD-DRY-001 through TD-DRY-004 (DRY improvements)
- Code-TECH_DEBT-001, 002 (weighted_random_select, proto conversion)
- MC-TECH_DEBT-1, 2 (proto consolidation)

---

### Re-Review Results (Iteration 2)

| Reviewer | Verdict | Notes |
|----------|---------|-------|
| Security | **APPROVED** | All 3 previous findings properly fixed |
| Test | **APPROVED** | All 4 test gaps addressed with quality tests |
| Code Reviewer | **APPROVED** | MINOR timestamp fix verified, 2 TECH_DEBT remain |
| DRY Reviewer | **APPROVED** | No BLOCKER duplication, 4 TECH_DEBT documented |
| MC Reviewer | **APPROVED** | Protocol stable, MC-implementable |

**Overall Verdict**: APPROVED - All 5 reviewers approved

---

## Reflection

### Lessons Learned

1. **prost Enum Naming**: prost generates simpler enum variant names than expected. Instead of `MhRole::MhRolePrimary`, it generates `MhRole::Primary`. Check generated code before writing conversion logic.

2. **`#[cfg(test)]` Module Visibility**: Modules marked `#[cfg(test)]` are not available to integration tests in `tests/` directory. For test utilities needed in integration tests, make the module public (or use feature flags).

3. **Inner/Outer Doc Comment Conflict**: Cannot have both `///` outer doc comments and `//!` inner doc comments on the same module. Clippy will error with `mixed_attributes_style`.

4. **Weighted Random Selection Pattern**: Using CSPRNG-based weighted random (weight = 1.0 - load_ratio) prevents thundering herd while preferring less-loaded instances.

5. **Mock Trait Pattern for gRPC**: Creating a trait (e.g., `McClientTrait`) with real and mock implementations enables unit testing of code that calls external gRPC services.

6. **Channel Caching**: Tonic's HTTP/2 connection pooling is built into Channels. Cache channels per endpoint to avoid connection churn.

7. **SecretString for Credentials**: Always use `SecretString` (from `secrecy` crate) for tokens and credentials to prevent accidental logging.

8. **Input Validation Consistency**: When adding new gRPC handlers, check existing handlers for validation patterns and apply consistently (length limits, character restrictions, URL scheme validation).

### Knowledge Files Updated

| Specialist | Added | Updated | Pruned |
|------------|-------|---------|--------|
| global-controller | 6 | 0 | 0 |
| security | 3 | 0 | 0 |
| test | 3 | 1 | 0 |
| code-reviewer | 0 | 0 | 0 |
| dry-reviewer | 2 | 1 | 0 |

### Non-Blocking Notes & Technical Debt

1. **TD-DRY-001**: Extract generic `weighted_random_select<T: LoadBalancedCandidate>` to common crate
2. **TD-DRY-002**: Create generic health checker task builder
3. **TD-DRY-003**: Document registry SQL patterns as intentional structural similarity
4. **TD-DRY-004**: Consider extracting common gRPC validation helpers
5. **Code-TECH_DEBT-001**: `weighted_random_select` duplicated for MH and MC
6. **Code-TECH_DEBT-002**: Health status proto conversion uses inline magic numbers
7. **MC-TECH_DEBT-1**: Two assignment messages should eventually be consolidated
8. **MC-TECH_DEBT-2**: Legacy `RegisterMeetingController` comment needs migration path

### Deferred Work

Per original scope, the following are deferred to follow-up tasks:
- **Phase 4**: MH Cross-Region Sync (Sync MH registry via GC-to-GC)
- **Phase 5**: RequestMhReplacement RPC (GC handles MC's request for MH replacement)

### Not Yet Wired

The following are implemented but not wired into handlers/main.rs:
- MhService gRPC handlers
- start_mh_health_checker task
- assign_meeting_with_mh in handlers

These will be wired in a follow-up task when the full flow is ready.
