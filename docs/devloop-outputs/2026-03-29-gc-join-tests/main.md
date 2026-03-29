# Devloop Output: GC join integration tests

**Date**: 2026-03-29
**Task**: GC join integration tests + test harness updates for user auth
**Specialist**: global-controller
**Mode**: full
**Branch**: `feature/meeting-join-user-story-devloop-task7`
**Duration**: ~30 minutes

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `19d2d435dae1e6fe49edc7bedfc77ac87d3a145b` |
| Branch | `feature/meeting-join-user-story-devloop-task7` |
| End Commit | `11c6059` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementing Specialist | `global-controller` |
| Iteration | `1` |

---

## Task Overview

### Objective
Add GC join integration tests covering user auth middleware, failure paths (AC down, MC unavailable), and success path. Update test harness for user auth support.

### Scope
- **Service(s)**: gc-service (integration tests only — no production code changes)
- **Schema**: No
- **Cross-cutting**: No

### Requirements Covered
- R-18: GC join integration tests covering auth middleware, failure paths (AC down, MC unavailable), and success path

---

## Plan Confirmation

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

---

## Planning

Implementer analyzed existing coverage: 33 tests already in meeting_tests.rs covering success paths, auth failures, status checks, cross-org, guest tokens, and settings. Identified 4 gaps vs R-18:
1. Service token rejected on user-auth endpoint (401)
2. AC unavailable during meeting token request (503)
3. MC unavailable / no healthy MCs (503)
4. Active meeting join (status allowlist completeness) + webtransport_endpoint assertion

Test reviewer independently confirmed the same 4 gaps. Security reviewer emphasized service token test must use properly signed token with wrong claims shape (not garbage).

---

## Implementation Summary

### New Helpers
- `create_service_token()` — properly signed EdDSA token with `TestClaims` (scope/service_type) instead of `TestUserClaims` (org_id/roles)
- `spawn_with_ac_failure()` — server variant where JWKS works but AC meeting-token endpoint returns 500

### New Tests (4)
1. `test_join_meeting_service_token_rejected` — valid sig, wrong claims shape → 401
2. `test_join_meeting_ac_unavailable` — AC 500 → GC 503 SERVICE_UNAVAILABLE
3. `test_join_meeting_no_mc_available` — no MCs registered → GC 503 SERVICE_UNAVAILABLE
4. `test_join_meeting_active_status_success` — active meeting join with full response validation (including webtransport_endpoint)

### Files Changed
- `crates/gc-service/tests/meeting_tests.rs` (+298 lines)

8 files changed (incl. INDEX updates), +436/-52 lines. 37 tests total (33 existing + 4 new), 0 regressions.

### R-18 Coverage Summary
| Requirement | Tests |
|---|---|
| 401 without token | existing |
| 401 expired token | existing |
| 401 service token | NEW |
| 403 wrong org | existing |
| AC down (503) | NEW |
| MC unavailable (503) | NEW |
| Meeting not found (404) | existing |
| Wrong status (404) | existing (cancelled + ended) |
| Success scheduled | existing |
| Success active | NEW |

---

## Review Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | Service token properly signed with wrong claims |
| Test | CLEAR | 0 | 0 | 0 | R-18 fully satisfied, 37 tests total |
| Observability | CLEAR | 1 info | 0 | 0 | Tests don't assert metric values (pre-existing pattern) |
| Code Quality | CLEAR | 1 minor | 0 | 0 | spawn_with_ac_failure duplication (existing TD) |
| DRY | CLEAR | 1 obs | 0 | 0 | Same duplication note, no new cross-file issues |
| Operations | CLEAR | 0 | 0 | 0 | All mocked, CI-safe, operationally valuable failure paths |
