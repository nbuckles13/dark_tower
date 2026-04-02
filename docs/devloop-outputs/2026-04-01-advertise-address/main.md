# Devloop Output: Advertise Address Config for MC/MH GC Registration

**Date**: 2026-04-01
**Task**: Add advertise address config to MC and MH for GC registration
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — Full + domain reviewers (meeting-controller, media-handler)
**Branch**: `feature/mh-skeleton`
**Duration**: ~15m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `b94497e8260e097cc7af018da1253bf8f893a6b6` |
| Branch | `feature/mh-skeleton` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@advertise-addr-devloop` |
| Implementing Specialist | `infrastructure` |
| Iteration | `2` |
| Security | `security@advertise-addr-devloop` |
| Test | `test@advertise-addr-devloop` |
| Observability | `observability@advertise-addr-devloop` |
| Code Quality | `code-reviewer@advertise-addr-devloop` |
| DRY | `dry-reviewer@advertise-addr-devloop` |
| Operations | `operations@advertise-addr-devloop` |
| Meeting Controller | `mc-reviewer@advertise-addr-devloop` |
| Media Handler | `mh-reviewer@advertise-addr-devloop` |

---

## Task Overview

### Objective
Replace hardcoded 0.0.0.0→localhost endpoint replacement in MC and MH gc_client.rs with proper config-based advertise addresses, using K8s downward API for pod IP.

### Scope
- **Service(s)**: mc-service, mh-service (Rust code + K8s manifests)
- **Schema**: No
- **Cross-cutting**: Yes — touches both MC and MH

### Debate Decision
NOT NEEDED — straightforward config fix.

---

## Planning

Implementer drafted plan covering 4 Rust files, 2 K8s deployments, 2 test files. All 8 reviewers confirmed with detailed domain input.

---

## Implementation Summary

### Rust Source (4 files)
| File | Change |
|------|--------|
| `crates/mc-service/src/config.rs` | Added `grpc_advertise_address`, `webtransport_advertise_address` (required) |
| `crates/mh-service/src/config.rs` | Same pattern |
| `crates/mc-service/src/grpc/gc_client.rs` | Replaced 0.0.0.0→localhost hack with config fields in register() + attempt_reregistration() |
| `crates/mh-service/src/grpc/gc_client.rs` | Same replacement + removed IP from log spans + added #[instrument] to attempt_reregistration |

### K8s Manifests (2 files)
| File | Change |
|------|--------|
| `infra/services/mc-service/deployment.yaml` | Added POD_IP downward API + MC_GRPC/WEBTRANSPORT_ADVERTISE_ADDRESS |
| `infra/services/mh-service/deployment.yaml` | Same pattern with MH ports |

### Tests (2 files + inline)
| File | Change |
|------|--------|
| `crates/mc-service/tests/gc_integration.rs` | test_config + registration content assertions |
| `crates/mh-service/tests/gc_integration.rs` | Same |
| MC/MH config.rs tests | base_vars, missing-var tests, default assertions |
| MC gc_client.rs tests | Inline Config structs updated |

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed |
|----------|---------|----------|-------|
| Security | **PASS** | 1 optional | — |
| Test | **PASS** | 3 must-fix | 3 |
| Observability | **PASS** | 4 (1 false positive) | 3 |
| Code Quality | **PASS** | 2 suggestions | already done |
| DRY | **PASS** | 2 low-severity | — |
| Operations | **PASS** | 2 non-blocking | — |
| MC Reviewer | **PASS** | 2 minor test items | 2 |
| MH Reviewer | **PASS** | 2 issues | 2 |

---

## Tech Debt

None — all findings fixed.

---

## Rollback Procedure

1. Start commit: `b94497e8260e097cc7af018da1253bf8f893a6b6`
2. `git diff b94497e..HEAD`
3. `git reset --soft b94497e` or `git reset --hard b94497e`

---

## Reflection

All 9 teammates updated INDEX.md files with advertise address pointers.

---

## Lessons Learned

1. Hardcoded endpoint construction breaks in multi-replica K8s — always use explicit advertise addresses
2. K8s downward API `status.podIP` is the correct way to get per-pod addresses
3. Advertise addresses are pod-specific — belong in deployment.yaml inline env vars, not configmaps
