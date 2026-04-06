# Devloop Output: Per-Instance Deployments for MC/MH

**Date**: 2026-04-05
**Task**: Convert MC/MH to per-instance Deployments with per-pod host-reachable QUIC endpoints
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — Full
**Branch**: `feature/mh-skeleton`
**Duration**: ~30m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `3cb95f3b0b015ddac4f13f6979eb2ee41a6ad225` |
| Branch | `feature/mh-skeleton` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@statefulset-devloop` |
| Implementing Specialist | `infrastructure` |
| Iteration | `3` |
| Security | `security@statefulset-devloop` |
| Test | `test@statefulset-devloop` |
| Observability | `observability@statefulset-devloop` |
| Code Quality | `code-reviewer@statefulset-devloop` |
| DRY | `dry-reviewer@statefulset-devloop` |
| Operations | `operations@statefulset-devloop` |

---

## Task Overview

### Objective
Fix architectural issue where MC/MH use load-balanced Services for WebTransport. Clients are assigned to specific pods by GC and must connect directly. Convert to per-instance Deployments with explicit per-pod addressing.

### Scope
- **Service(s)**: mc-service, mh-service (K8s manifests + Rust config), env-tests, common crate
- **Schema**: No
- **Cross-cutting**: Yes

---

## Implementation Summary

### Initial approach (StatefulSet)
Converted Deployment → StatefulSet with ordinal-based port computation in Rust config.rs.

### Human review rework (per-instance Deployments)
Feedback: ordinal parsing is infrastructure knowledge leaking into application code. Reworked to per-instance Deployments with fully explicit config — zero Rust-side computation.

### Final implementation
- Per-instance Deployments: mc-0, mc-1, mh-0, mh-1 (single replica each)
- Per-instance ConfigMaps with explicit advertise addresses
- Per-pod NodePort Services for WebTransport
- ClusterIP Service for internal gRPC/health
- Removed `parse_statefulset_ordinal` and all ordinal fallback logic from Rust
- Env-tests use pod-specific endpoints from GC join response
- TODO added for env-test portability (remove localhost hardcoding)

---

## Code Review Results

| Reviewer | Verdict |
|----------|---------|
| Security | **PASS** |
| Test | **PASS** |
| Observability | **PASS** |
| Code Quality | **PASS** |
| DRY | **PASS** |
| Operations | **PASS** |

---

## Tech Debt

- Env-test portability: `ClusterConnection` hardcodes localhost — added to TODO.md

---

## Rollback Procedure

1. Start commit: `3cb95f3b0b015ddac4f13f6979eb2ee41a6ad225`
2. `git reset --hard 3cb95f3`
