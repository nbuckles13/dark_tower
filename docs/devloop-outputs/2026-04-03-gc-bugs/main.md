# Devloop Output: Fix GC Bugs (home_org_id + allow_guests)

**Date**: 2026-04-03
**Task**: Fix GC bugs — home_org_id 500 errors + allow_guests investigation
**Specialist**: global-controller
**Mode**: Agent Teams (v2) — Full + domain reviewer (auth-controller)
**Branch**: `feature/mh-skeleton`
**Duration**: ~25m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `d19eb0ff94dad758d8cff3c25accb40ab4d2485c` |
| Branch | `feature/mh-skeleton` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@gc-bugs-devloop` |
| Implementing Specialist | `global-controller` |
| Iteration | `2` |
| Security | `security@gc-bugs-devloop` |
| Test | `test@gc-bugs-devloop` |
| Observability | `observability@gc-bugs-devloop` |
| Code Quality | `code-reviewer@gc-bugs-devloop` |
| DRY | `dry-reviewer@gc-bugs-devloop` |
| Operations | `operations@gc-bugs-devloop` |
| Auth Controller | `ac-reviewer@gc-bugs-devloop` |

---

## Task Overview

### Objective
Fix 2 GC bugs found during E2E join flow testing: (1) home_org_id sent as None causing 500, (2) allow_guests not persisted causing 403.

### Scope
- **Service(s)**: gc-service, ac-service, common crate
- **Schema**: No
- **Cross-cutting**: Yes — shared types in common crate

---

## Implementation Summary

### Bug 1: FIXED — home_org_id missing (500 error)
- Created `crates/common/src/meeting_token.rs` with shared types (MeetingTokenRequest, GuestTokenRequest, TokenResponse, ParticipantType, MeetingRole)
- GC and AC now import from common instead of maintaining separate definitions
- GC always sends `home_org_id: user_org_id` (never None)
- 15 unit tests for shared types + 1 regression test for same-org join invariant

### Bug 2: NO CODE DEFECT — allow_guests persistence correct
- Investigated by implementer, security, and operations independently
- SQL INSERT correctly binds allow_guests, handler correctly extracts it
- Likely an E2E test setup issue, tracked separately

---

## Code Review Results

| Reviewer | Verdict |
|----------|---------|
| Security | **PASS** |
| Test | **PASS** (3 findings fixed) |
| Observability | **PASS** |
| Code Quality | **PASS** |
| DRY | **PASS** |
| Operations | **PASS** |
| AC Reviewer | **PASS** |

---

## Tech Debt

- `common::meeting_token::{ParticipantType, MeetingRole}` (3-variant) duplicates `common::jwt::{ParticipantType, MeetingRole}` (2-variant). Wire-compatible but should be unified.
- Guest-token handler lacks metrics instrumentation (observability gap).

---

## Rollback Procedure

1. Start commit: `d19eb0ff94dad758d8cff3c25accb40ab4d2485c`
2. `git reset --hard d19eb0f`
