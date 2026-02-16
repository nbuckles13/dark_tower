# Dev-Loop Output: GC Meeting Assignment via Load Balancing

**Date**: 2026-01-21
**Task**: GC should assign users to MCs via load balancing per design in ADR-0010
**Branch**: `feature/skill-dev-loop`
**Primary Specialist**: global-controller
**Duration**: ~45m (2 iterations)

---

## Loop State (Internal)

<!-- This section is maintained by dev-loop skills for state recovery. -->

| Field | Value |
|-------|-------|
| Implementing Specialist | global-controller |
| Implementing Agent | `completed` |
| Current Step | `complete` |
| Iteration | `2` |
| Security Reviewer | `a523b8d` |
| Test Reviewer | `a925442` |
| Code Reviewer | `a64077a` |
| DRY Reviewer | `a53ce8d` |

---

## Task Overview

### Objective

GC should assign users to MCs via load balancing per design in ADR-0010

### Scope

- **Service(s)**: global-controller
- **ADR Reference**: ADR-0010 (Global Controller Architecture)
- **Related Work**: MC Registration (completed 2026-01-20)

### Key Requirements (from ADR-0010)

- Atomic MC health check + assignment (race-condition safe)
- Weighted round-robin with capacity scoring
- Meeting-to-MC mapping
- Assignment cleanup (soft deletes)

---

## Implementation Summary

Implemented meeting-to-MC assignment with weighted round-robin load balancing. When a user joins a meeting, GC now:

1. Checks for existing healthy assignment (returns immediately if found)
2. Queries candidate MCs via load balancing (ordered by load ratio, limited to 5)
3. Selects MC using weighted random (prefers lower load)
4. Performs atomic assignment with race condition handling via `INSERT ... ON CONFLICT DO UPDATE`
5. Returns MC endpoints to client for connection

The assignment is stored in the new `meeting_assignments` table and returned in the `JoinMeetingResponse` via the `mc_assignment` field.

---

## Files Created

| File | Purpose |
|------|---------|
| `migrations/20260121000001_meeting_assignments.sql` | Database schema for meeting assignments |
| `crates/global-controller/src/repositories/meeting_assignments.rs` | Repository layer with load balancing queries |
| `crates/global-controller/src/services/mc_assignment.rs` | Service layer orchestrating assignment flow |
| `crates/global-controller/tests/meeting_assignment_tests.rs` | Integration tests for assignment functionality |

## Files Modified

| File | Changes |
|------|---------|
| `crates/global-controller/src/config.rs` | Added `gc_id` config for assignment tracking |
| `crates/global-controller/src/models/mod.rs` | Added `McAssignmentInfo` response type |
| `crates/global-controller/src/handlers/meetings.rs` | Integrated MC assignment into join handlers |
| `crates/global-controller/src/repositories/mod.rs` | Exported new assignment module |
| `crates/global-controller/src/services/mod.rs` | Exported new assignment service |
| `crates/global-controller/tests/meeting_tests.rs` | Added MC registration for join tests |

---

## Verification Results (Orchestrator Re-Validation)

| Layer | Command | Result | Notes |
|-------|---------|--------|-------|
| 1 | `cargo check --workspace` | PASS | Compiled successfully |
| 2 | `cargo fmt --all --check` | PASS | Fixed minor formatting in repositories/mod.rs |
| 3 | `./scripts/guards/run-guards.sh` | PASS | 8/8 guards passed |
| 4 | `./scripts/test.sh --workspace --lib` | PASS | 201 unit tests passed |
| 5 | `./scripts/test.sh --workspace` | PASS | All integration tests passed |
| 6 | `cargo clippy --workspace -- -D warnings` | PASS | No warnings |
| 7 | `./scripts/guards/run-guards.sh --semantic` | SKIPPED | Semantic guards disabled |

**Validation Status**: All 6 active verification layers passed.

---

## Test Coverage

- **18 integration tests** in `meeting_assignment_tests.rs`:
  - Repository tests: get_healthy_assignment, get_candidate_mcs, atomic_assign, end_assignment, cleanup
  - Service tests: assign_meeting, end_assignment, get_assignment
  - Load balancing: ordering by load, excluding full MCs, region filtering
  - Race conditions: atomic assignment, existing assignment reuse
  - **NEW**: Concurrent race condition test (10 parallel tasks)
  - **NEW**: MC health transition test (unhealthy MC replacement)

- **6 existing tests updated** in `meeting_tests.rs` to register healthy MC before join

---

## Code Review Results

**Status**: APPROVED - All findings fixed in Iteration 2

### Verdict Rules Reference
- **APPROVED**: No findings (or only TECH_DEBT)
- **REQUEST_CHANGES**: Any BLOCKER, CRITICAL, MAJOR, or MINOR findings
- **BLOCKED**: Fundamental issues requiring redesign

### Security Specialist
**Verdict**: APPROVED ✓
- Parameterized SQL queries ✓
- Proper authentication/authorization enforcement ✓
- Atomic race condition handling via `ON CONFLICT DO UPDATE` ✓
- CSPRNG usage for random selection ✓
- Generic error messages (no info disclosure) ✓

### Test Specialist
**Verdict**: APPROVED ✓ (after Iteration 2 fixes)
- 18 dedicated assignment tests ✓
- Repository, service, and handler coverage ✓
- Weighted selection statistical test ✓
- Concurrent race condition test ✓ (FIXED)
- MC health transition test ✓ (FIXED)

### Code Quality Reviewer
**Verdict**: APPROVED ✓ (after Iteration 2 fixes)
- ADR-0002 compliant (no panics) ✓
- Proper layering (handler → service → repository) ✓
- `#[allow(dead_code)]` with comments ✓ (FIXED - `#[expect]` causes warnings)
- Duplicate logging removed ✓ (FIXED)
- **TECH_DEBT**: 3 items (documented, non-blocking)

### DRY Reviewer
**Verdict**: APPROVED ✓
- No BLOCKER findings ✓
- **TECH_DEBT**: 4 items (documented, non-blocking)

### Iteration 2 Fixes Applied

| # | Severity | Reviewer | Finding | Resolution |
|---|----------|----------|---------|------------|
| 1 | MAJOR | Test | Add concurrent race condition test | Added `test_concurrent_assignment_race_condition` |
| 2 | MAJOR | Test | Add MC health transition test | Added `test_mc_health_transition_creates_new_assignment` |
| 3 | MINOR | Code | Use `#[expect]` instead of `#[allow]` | Kept `#[allow]` with comments - `#[expect]` not suitable |
| 4 | MINOR | Code | Remove duplicate logging | Removed repo-layer logging, kept service-layer only |

---

## Lessons Learned

### Patterns Discovered

1. **INSERT ON CONFLICT DO UPDATE for Atomic Assignment**: Using PostgreSQL's `INSERT ... ON CONFLICT DO UPDATE WHERE` for atomic operations that need to conditionally update existing rows. This avoids CTE snapshot issues where separate CTEs don't see each other's changes.

2. **Weighted Random with CSPRNG**: Using `ring::rand::SystemRandom` for cryptographically secure random selection with load-based weighting

3. **Heartbeat + Status for Health**: Combining `health_status` column with `last_heartbeat_at` timestamp for robust health checks

### Gotchas Encountered

1. Legacy `endpoint` column in `meeting_controllers` is NOT NULL - test helpers must populate it
2. Tests that exercise meeting join require a healthy MC to be registered first
3. Semantic guards flag pre-existing issues (not introduced by this work)
4. **NEW**: PostgreSQL CTEs with data modifications all see the same snapshot - INSERT/UPDATE CTEs don't see DELETE CTEs' changes. Use `ON CONFLICT DO UPDATE WHERE` instead.
5. **NEW**: `#[expect(dead_code)]` causes "unfulfilled lint expectation" warnings when the code is actually used (e.g., from integration tests). Use `#[allow(dead_code)]` with comments instead.

---

## Non-Blocking Notes & Technical Debt

### Test Gaps (from Test Specialist)

All MAJOR test gaps have been addressed in Iteration 2:
- Concurrent race condition test: FIXED
- MC health transition test: FIXED

### Code Quality Items (from Code Reviewer)

All MINOR code quality items have been addressed in Iteration 2:
- `#[allow(dead_code)]` with comments: FIXED (using `#[expect]` causes warnings)
- Duplicate logging: FIXED (removed repo-layer, kept service-layer)

### Cross-Service Duplication (from DRY Reviewer)

| Priority | Item | New Location | Existing Location | Follow-up |
|----------|------|--------------|-------------------|-----------|
| TECH_DEBT | CSPRNG usage | `meeting_assignments.rs`, `meetings.rs` | `ac-service/crypto/mod.rs` | Consider extracting `generate_random_bytes()` to common crate |
| TECH_DEBT | Config Debug redaction | `gc/config.rs` | `ac/config.rs` | Similar pattern for redacting database_url; macro extraction possible |
| TECH_DEBT | Error type patterns | `gc/errors.rs` | `ac/errors.rs` | GcError and AcError have similar variants; intentionally separate but document pattern |
| TECH_DEBT | Secure UUID generation | `gc/meetings.rs:generate_guest_id()` | N/A | Manual UUID from CSPRNG bytes; centralize if other services need it |

### Implementation Debt (from Implementer)

| Priority | Item | Location | Description |
|----------|------|----------|-------------|
| TECH_DEBT | SecretString for join_token_secret | `handlers/meetings.rs` | `MeetingRow.join_token_secret` should use `SecretString` type |
| TECH_DEBT | Service token handling | `handlers/meetings.rs:create_ac_client()` | `std::env::var("GC_SERVICE_TOKEN").unwrap_or_default()` needs improvement |
| TECH_DEBT | Unused functions | `mc_assignment.rs` | `end_assignment` and `cleanup_old_assignments` need to be connected to handlers/background tasks |

---
