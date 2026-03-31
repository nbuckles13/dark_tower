# Devloop Output: Add validate-kustomize CI Guard

**Date**: 2026-03-31
**Task**: Add validate-kustomize CI guard (R-15 through R-20)
**Specialist**: test
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/kustomize-migration`
**Duration**: ~20m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `365848c1b3f135cb91cd54562caa8c9596e22768` |
| Branch | `feature/kustomize-migration` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@kustomize-guard` |
| Implementing Specialist | `test` |
| Iteration | `1` |
| Security | `security@kustomize-guard` |
| Test | `test@kustomize-guard` |
| Observability | `observability@kustomize-guard` |
| Code Quality | `code-reviewer@kustomize-guard` |
| DRY | `dry-reviewer@kustomize-guard` |
| Operations | `operations@kustomize-guard` |

---

## Task Overview

### Objective
Create a unified CI guard script `scripts/guards/simple/validate-kustomize.sh` that validates Kustomize bases, overlays, orphan manifests, security contexts, empty secrets, and dashboard completeness.

### Scope
- **Service(s)**: CI/Guards infrastructure
- **Schema**: No
- **Cross-cutting**: Yes — validates all K8s manifests

### Debate Decision
NOT NEEDED - Implementing checks specified in user story design

---

## Planning

All 6 reviewers confirmed.

---

## Pre-Work

None — depends on tasks 1-2 (completed)

---
