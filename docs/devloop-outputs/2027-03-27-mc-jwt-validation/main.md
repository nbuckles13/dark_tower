# Devloop Output: MC JWT validation using common JwksClient + JwtValidator

**Date**: 2026-03-27
**Task**: Implement MC JWT validation using common `JwksClient` + `JwtValidator::validate<MeetingTokenClaims>` + MC-specific config (`ac_jwks_url`) (Task 9, R-23)
**Specialist**: meeting-controller
**Mode**: full
**Branch**: `feature/meeting-join-user-story-devloop-task7`
**Duration**: ~TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `f33a5cb38c8caf7cdc7193ac426910436f64b6a6` |
| Branch | `feature/meeting-join-user-story-devloop-task7` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `reflection` |
| Implementer | `pending` |
| Implementing Specialist | `meeting-controller` |
| Iteration | `1` |
| Security | `pending` |
| Test | `pending` |
| Observability | `pending` |
| Code Quality | `pending` |
| DRY | `pending` |
| Operations | `pending` |

---

## Task Overview

### Objective
Implement JWT validation in MC using the common crate's `JwksClient` + `JwtValidator` (extracted in task 7). MC needs to validate `MeetingTokenClaims` from client WebTransport connections. This includes adding `ac_jwks_url` config, `From<JwtError> for McError`, and wiring the validator into MC startup.

### Scope
- **Service(s)**: mc-service (new JWT validation code), mc-service config
- **Schema**: No
- **Cross-cutting**: No (MC-only, consumes common crate)

---

## Planning

TBD

---

## Implementation Summary

TBD

---

## Files Modified

TBD

---

## Code Review Results

TBD

---

## Tech Debt

TBD
