# Devloop Output: ADR-0003 MH Auth Layer 2 + Metrics Fixes

**Date**: 2026-04-16
**Task**: Add Layer 2 service_type routing to MhAuthLayer, fix metrics gap, inject claims in extensions, remove dead MhAuthInterceptor, add failure_reason label, update catalog and dashboard
**Specialist**: media-handler
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-mh-notify`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `16a6551427720e13777f3a5bae268c6c23750c3f` |
| Branch | `feature/mh-quic-mh-notify` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@mh-auth-scopes` |
| Implementing Specialist | `media-handler` |
| Iteration | `1` |
| Security | `security@mh-auth-scopes` |
| Test | `test@mh-auth-scopes` |
| Observability | `observability@mh-auth-scopes` |
| Code Quality | `code-reviewer@mh-auth-scopes` |
| DRY | `dry-reviewer@mh-auth-scopes` |
| Operations | `operations@mh-auth-scopes` |

---

## Task Overview

### Objective
Mirror MC's ADR-0003 two-layer auth implementation for MH. Add Layer 2 service_type routing to MhAuthLayer, fix the metrics gap (scope rejection not counted), inject validated ServiceClaims into request extensions, remove dead MhAuthInterceptor, add failure_reason label to mh_jwt_validations_total, update metrics catalog and dashboard.

### Scope
- **Service(s)**: MH (media-handler)
- **Schema**: No
- **Cross-cutting**: No (MH-only, mirrors MC pattern already implemented)

### Debate Decision
ADR-0003 updated per debate `docs/debates/2026-04-16-grpc-auth-scopes/debate.md`

---

## Planning

Implementer proposed mirroring MC's two-layer auth pattern exactly. All 6 reviewers confirmed.

---

## Pre-Work

None — MC implementation (`crates/mc-service/src/grpc/auth_interceptor.rs`) serves as reference.

---

## Implementation Summary

- Layer 2 service_type routing: `MediaHandlerService/*` → requires `service_type == "meeting-controller"`
- Fail closed on None/unknown paths with PERMISSION_DENIED
- Scope rejection now calls `record_jwt_validation("failure", "service", "scope_mismatch")` (was unmetered)
- `classify_jwt_error()` maps JwtError variants to bounded failure_reason labels
- `failure_reason` label added to `mh_jwt_validations_total` (cardinality 2x2x6=24 max)
- `mh_caller_type_rejected_total` metric added (cardinality 1x1x3=3 max)
- Validated ServiceClaims injected into request extensions
- Dead MhAuthInterceptor removed (struct, impls, tests, re-export)
- Dashboard and catalog updated

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/mh-service/src/grpc/auth_interceptor.rs` | Layer 2 routing, classify_jwt_error, claims injection, dead code removal |
| `crates/mh-service/src/grpc/mod.rs` | Remove MhAuthInterceptor re-export |
| `crates/mh-service/src/grpc/mh_service.rs` | Update doc comment |
| `crates/mh-service/src/observability/metrics.rs` | failure_reason label, caller_type_rejected metric |
| `crates/mh-service/src/webtransport/connection.rs` | Updated record_jwt_validation calls |
| `docs/observability/metrics/mh-service.md` | Catalog updated |
| `infra/grafana/dashboards/mh-overview.json` | Dashboard updated |
| `docs/TODO.md` | MhAuthInterceptor removal marked done, extraction TODO updated |

---

## Devloop Verification Steps

All layers pass: compile, format, guards (15/15), tests (0 failures), clippy (0 warnings), env-tests (0 failures except pre-existing flaky Loki test).

---

## Code Review Results

| Reviewer | Verdict | Findings |
|----------|---------|----------|
| Security | CLEAR | 0 |
| Test | CLEAR | 0 |
| Observability | CLEAR | 1 minor non-blocking |
| Code Quality | CLEAR | 0 |
| DRY | PASS | 2 TODO fixes (applied) |
| Operations | CLEAR | 0 |

---

## Tech Debt

- McAuthLayer/MhAuthLayer now nearly identical — extraction candidate strengthened (TODO.md updated)
- `classify_jwt_error()` duplicated in MC and MH — part of extraction candidate
- connection.rs uses catch-all `validation_failed` reason (vs granular classify_jwt_error) — follow-up when MhError refactored

---

## Rollback Procedure

1. Start commit: `16a6551427720e13777f3a5bae268c6c23750c3f`
2. Hard reset: `git reset --hard 16a6551`

---

## Reflection

All teammates updated INDEX.md files with pointers to MH two-layer auth, classify_jwt_error, failure_reason label, and caller_type_rejected metric.

---

## Issues Encountered & Resolutions

None — clean implementation mirroring MC's established pattern.

---

## Lessons Learned

1. Having a reference implementation (MC) makes the second service implementation fast and predictable
2. The McAuthLayer/MhAuthLayer extraction candidate is now stronger — both have identical 5-step validation chains
