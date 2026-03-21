# Devloop Output: Implement ParticipantsRepository with migration

**Date**: 2026-03-21
**Task**: Implement `ParticipantsRepository` with migration for participant tracking + capacity checks
**Specialist**: database
**Mode**: Agent Teams (v2) — Full
**Branch**: `feature/meeting-join-user-story`
**Duration**: ~25m (excluding permission-blocked time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `3a22a5124c93f720c12bf643e4b3c87925215c67` |
| Branch | `feature/meeting-join-user-story` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-participants-repo` |
| Implementing Specialist | `database` |
| Iteration | `3` |
| Security | `security@devloop-participants-repo` |
| Test | `test@devloop-participants-repo` |
| Observability | `observability@devloop-participants-repo` |
| Code Quality | `code-reviewer@devloop-participants-repo` |
| DRY | `dry-reviewer@devloop-participants-repo` |
| Operations | `operations@devloop-participants-repo` |

---

## Task Overview

### Objective
Implement `ParticipantsRepository` with migration for participant tracking and capacity checks (R-9). ALTER existing `participants` table to add `participant_type` and `role` columns with CHECK constraints and partial unique index for active participants.

### Scope
- **Service(s)**: GC Service (`crates/gc-service/`) — repository layer + migration
- **Schema**: Yes — ALTER TABLE migration
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED — Standard repository + migration following existing patterns.

---

## Planning

Implementer proposed creating a new `meeting_participants` table. Test and code-reviewer both identified that the existing `participants` table should be extended instead. Plan revised to ALTER TABLE. All 6 reviewers confirmed revised plan.

---

## Pre-Work

None

---

## Implementation Summary

### Migration
ALTER TABLE `participants` adding `participant_type` (CHECK: member/external/guest), `role` (CHECK: host/participant/guest), partial unique index on `(meeting_id, user_id) WHERE left_at IS NULL`, and partial index on `meeting_id WHERE left_at IS NULL`.

### Repository
`ParticipantsRepository` with three methods:
- `count_active_participants(pool, meeting_id) -> Result<i64>`
- `add_participant(pool, meeting_id, user_id, display_name, participant_type, role) -> Result<Participant>`
- `remove_participant(pool, meeting_id, user_id) -> Result<bool>`

All methods: `#[instrument(skip_all, name = "gc.repo.*")]`, `metrics::record_db_query`, parameterized SQL.

### Model
`Participant` struct with `sqlx::FromRow`: `participant_id`, `meeting_id`, `user_id` (Option<Uuid>), `display_name`, `participant_type`, `role`, `joined_at`, `left_at`.

### Tests
9 integration tests: add, count, remove, remove-nonexistent, duplicate-rejected, rejoin-after-leave, capacity-check, invalid-participant-type, invalid-role.

---

## Files Modified

```
 crates/gc-service/src/models/mod.rs                        | +36
 crates/gc-service/src/repositories/mod.rs                  | +4
 crates/gc-service/src/repositories/participants.rs         | new
 crates/gc-service/tests/participant_tests.rs               | new
 migrations/20260322000001_add_participant_tracking.sql     | new
 docs/specialist-knowledge/*/INDEX.md                       | updates
```

---

## Devloop Verification Steps

### Layer 1: cargo check — PASS
### Layer 2: cargo fmt — PASS (iteration 3, after fmt fix)
### Layer 3: Guards — PASS (14/14)
### Layer 4: Tests — PASS (all pass, 9 new participant tests)
### Layer 5: Clippy — PASS
### Layer 6: Audit — PRE-EXISTING (same 4 vulnerabilities as task 1)
### Layer 7: Semantic Guard — SAFE

---

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED
**Findings**: 5 found, 4 fixed, 1 deferred (accepted)
- CHECK constraints updated to include 'guest'
- display_name parameter added, user_id made Optional
- Deferred: typed enums for participant_type/role (DB CHECK provides protection; typed enums for Task 4/10)

### Test Specialist
**Verdict**: CLEAR
**Findings**: 5 found, all fixed
- display_name/nullable user_id, CHECK constraints include 'guest', negative constraint tests added

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 1 found, 1 fixed
- Explicit span names added (`gc.repo.*` convention)

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 3 found, all fixed
- display_name parameter, participant_id/display_name in struct, nullable user_id

### DRY Reviewer
**Verdict**: CLEAR
**Findings**: 0 (1 tech debt observation: test fixture duplication)

### Operations Reviewer
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed
- Migration rollback steps added, display_name parameter fixed

---

## Tech Debt

### Deferred Findings

| Finding | Reviewer | Deferral Justification |
|---------|----------|------------------------|
| Typed enums for participant_type/role | Security | DB CHECK provides protection; typed enums require changes outside this PR (Task 4/10) |

### Cross-Service Duplication (from DRY Reviewer)

No cross-service duplication in this PR. Three tech debt items added to TODO.md:
1. AC private MeetingTokenClaims/GuestTokenClaims duplicating common types
2. AC sign_meeting_jwt/sign_guest_jwt identical functions
3. GC integration test fixture duplication across 3 test files

---

## Rollback Procedure

1. Start commit: `3a22a5124c93f720c12bf643e4b3c87925215c67`
2. For schema changes: run DOWN migration (rollback steps in migration comment block)
3. Then: `git reset --hard 3a22a5124c93f720c12bf643e4b3c87925215c67`

---

## Reflection

All 7 teammates updated INDEX.md files. DRY reviewer added 3 tech debt items to TODO.md.

---

## Issues Encountered & Resolutions

### Issue 1: Format failure (iteration 1)
**Problem**: Long assert lines in test file
**Resolution**: `cargo fmt --all`

### Issue 2: FK constraint violation (iteration 2)
**Problem**: Tests used `Uuid::new_v4()` without creating user rows
**Resolution**: Added `create_extra_user` helper

### Issue 3: Teammate communication blocked by permissions bug
**Problem**: Remote control connection disabled `--dangerously-skip-permissions` on host (GitHub issue #29214). One teammate's permission prompt blocked entire team.
**Resolution**: Manual approval from desktop client

---

## Lessons Learned

1. Plan review caught incorrect table design (new table vs ALTER existing) before implementation
2. Remote control + `--dangerously-skip-permissions` interaction is a known bug (#29214)
3. Single blocked permission prompt deadlocks entire agent team
