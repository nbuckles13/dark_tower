# Devloop Output: ADR-0003 MC Auth Scopes + Layer 2

**Date**: 2026-04-16
**Task**: Update AC default_scopes() + setup.sh to ADR-0003 scopes, add Layer 2 service_type routing to McAuthLayer, remove dead McAuthInterceptor, inject claims in extensions
**Specialist**: meeting-controller
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-mh-notify`
**Duration**: ~50m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `1302493128030684e9f3113d4f2cd2acf81fcdd0` |
| Branch | `feature/mh-quic-mh-notify` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@mc-auth-scopes` |
| Implementing Specialist | `meeting-controller` |
| Iteration | `3` |
| Security | `security@mc-auth-scopes` |
| Test | `test@mc-auth-scopes` |
| Observability | `observability@mc-auth-scopes` |
| Code Quality | `code-reviewer@mc-auth-scopes` |
| DRY | `dry-reviewer@mc-auth-scopes` |
| Operations | `operations@mc-auth-scopes` |

---

## Task Overview

### Objective
Implement ADR-0003 scope alignment and two-layer gRPC auth for MC. Standardize scope naming to `service.write.{target}` convention across AC, setup.sh, and auth interceptors. Add Layer 2 service_type caller routing to McAuthLayer. Remove dead McAuthInterceptor. Inject validated ServiceClaims into request extensions.

### Scope
- **Service(s)**: MC (primary), AC (scope data)
- **Schema**: No
- **Cross-cutting**: Yes (AC scope data affects all services)

### Debate Decision
ADR-0003 updated per debate `docs/debates/2026-04-16-grpc-auth-scopes/debate.md`

---

## Planning

Implementer proposed 5-part plan: (1) update AC scope data, (2) add Layer 2 service_type routing, (3) inject claims in extensions, (4) remove dead McAuthInterceptor, (5) add scope contract tests + Layer 2 metric. All 6 reviewers confirmed.

---

## Pre-Work

None

---

## Implementation Summary

### AC Scope Data (Part 1)
- `ServiceType::default_scopes()` updated: GC→`service.write.mc, internal:meeting-token`, MC→`service.write.mh, service.write.gc`, MH→`service.write.mc, service.write.gc`
- `setup.sh` seed SQL aligned with `default_scopes()`
- Legacy domain scopes (`meeting:create`, `media:forward`, etc.) removed
- 5 scope contract tests added asserting cross-service scope requirements

### Layer 2 Service Type Routing (Part 2)
- After Layer 1 (JWKS + `service.write.mc` scope check), Layer 2 matches URI path to expected `service_type`
- `MeetingControllerService/*` → requires `service_type == "global-controller"`
- `MediaCoordinationService/*` → requires `service_type == "media-handler"`
- Fail closed on `service_type: None` and unknown paths
- PERMISSION_DENIED status for Layer 2 rejections (not UNAUTHENTICATED)

### Claims Injection (Part 3)
- Validated `ServiceClaims` inserted into `request.extensions_mut()` after successful auth

### Dead Code Removal (Part 4)
- `McAuthInterceptor` struct, impls, and 13 tests removed
- Re-export removed from `mod.rs`
- Doc comments fixed in `media_coordination.rs` and `main.rs`

### Observability
- New metric: `mc_caller_type_rejected_total{grpc_service, expected_type, actual_type}` (cardinality 24 max)
- Grafana panel with red threshold at 1 (any non-zero is a bug)
- Metrics catalog updated

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/mc-service/src/grpc/auth_interceptor.rs` | Layer 2 routing, claims injection, dead code removal |
| `crates/mc-service/src/grpc/mod.rs` | Remove McAuthInterceptor re-export |
| `crates/mc-service/src/grpc/media_coordination.rs` | Fix doc comment |
| `crates/mc-service/src/main.rs` | Update comment |
| `crates/mc-service/src/observability/metrics.rs` | Add record_caller_type_rejected() |
| `crates/ac-service/src/models/mod.rs` | Update default_scopes() + scope contract tests |
| `crates/ac-service/src/handlers/admin_handler.rs` | Update test assertions |
| `crates/ac-service/src/handlers/auth_handler.rs` | Update test scope values |
| `infra/kind/scripts/setup.sh` | Update seed SQL scopes |
| `infra/grafana/dashboards/mc-overview.json` | New Layer 2 rejection panel |
| `docs/observability/metrics/mc-service.md` | New metric catalog entry |
| `docs/TODO.md` | Mark McAuthInterceptor removal done |

---

## Devloop Verification Steps

### Layer 1-3: Compile, Format, Guards
**Status**: ALL PASS (15/15 guards)

### Layer 4: Tests
**Status**: PASS (0 failures across all workspace crates)

### Layer 5: Clippy
**Status**: PASS (0 warnings)

### Layer 6: Audit
**Status**: Pre-existing transitive dep vulnerabilities

### Layer 7: Semantic Guard
**Status**: PASS

### Layer 8: Env-tests
**Status**: PASS (only failure is pre-existing flaky Loki log test)
- All join flow tests (24_join_flow.rs) pass with ADR-0003 scopes
- GC→MC calls now succeed with `service.write.mc` scope

---

## Code Review Results

### Security
**Verdict**: CLEAR — 0 findings

### Test
**Verdict**: CLEAR — 0 findings

### Observability
**Verdict**: PASS — 5 minor doc fixes, all resolved

### Code Quality
**Verdict**: CLEAR — 1 minor pre-existing doc gap noted

### DRY
**Verdict**: CLEAR — tech debt notes (McAuthLayer/MhAuthLayer similarity tracked)

### Operations
**Verdict**: CLEAR — 0 findings

---

## Tech Debt

### Deferred Findings
None — all findings fixed.

### Cross-Service Duplication (from DRY Reviewer)
- McAuthLayer/MhAuthLayer Layer 1 similarity remains tracked in TODO.md
- Layer 2 divergence (MC has it, MH/GC pending) — extraction candidate when all 3 services have Layer 2

---

## Rollback Procedure

1. Start commit: `1302493128030684e9f3113d4f2cd2acf81fcdd0`
2. Hard reset: `git reset --hard 1302493`

---

## Reflection

All teammates updated INDEX.md files with pointers to ADR-0003 scope alignment, Layer 2 service_type routing, dead code removal, and new metric.

---

## Issues Encountered & Resolutions

### Issue 1: Dashboard metric guard failure
**Problem**: `mc_caller_type_rejected_total` not in any Grafana dashboard
**Resolution**: Added timeseries panel to mc-overview.json

### Issue 2: AC test assertions stale
**Problem**: 2 AC tests asserted old scope counts/values after default_scopes() update
**Resolution**: Updated test assertions to match new ADR-0003 scopes

### Issue 3: DB scopes not updated during cluster setup
**Problem**: setup.sh changes not picked up by devloop container's cached cluster
**Resolution**: Manually updated DB scopes via kubectl exec; filed as infrastructure issue

---

## Lessons Learned

1. Scope data changes require both code (default_scopes) AND infrastructure (setup.sh) updates — scope contract tests prevent drift
2. The validate-application-metrics guard catches metrics without dashboard coverage — add dashboard panel at implementation time, not as a follow-up
3. Devloop container may cache setup.sh — manual DB updates may be needed during development
