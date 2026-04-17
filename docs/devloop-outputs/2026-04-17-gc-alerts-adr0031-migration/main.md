# Devloop Output: GC alerts migration to ADR-0031 strict compliance

**Date**: 2026-04-17
**Task**: Migrate `infra/docker/prometheus/rules/gc-alerts.yaml` to conform to `validate-alert-rules.sh` strict mode per ADR-0031.
**Specialist**: global-controller (owner of gc-alerts.yaml per ADR-0031 §Ownership split)
**Mode**: Agent Teams (light)
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `f5f53f81a677d22400ec63c509dc36cb2db62fa7` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-gc-alerts-migration` |
| Implementing Specialist | `global-controller` |
| Iteration | `1` |
| Security | `CLEAR (security@devloop-gc-alerts-migration)` |
| Observability | `RESOLVED (observability@devloop-gc-alerts-migration) — 1 fix` |
| Context Reviewer | observability (third reviewer for --light) |

---

## Task Overview

### Objective

Return CI to green for the alert-rules guard by migrating gc-alerts.yaml to ADR-0031 strict compliance.

### Scope

- Single file: `infra/docker/prometheus/rules/gc-alerts.yaml`
- 23 `runbook_url` rewrites (mechanical sed: strip `https://github.com/yourorg/dark_tower/blob/main/` prefix)
- 7 severity reclassifications from `critical` → `page` or `warning` (specialist judgment per ADR-0031 severity taxonomy)

Affected rules (severity calls):
- `GCDown`
- `GCHighErrorRate`
- `GCHighLatency`
- `GCMCAssignmentSlow`
- `GCDatabaseDown`
- `GCErrorBudgetBurnRateCritical`
- `GCMeetingCreationStopped`

### Debate Decision

NOT NEEDED — ADR-0031 severity taxonomy + alert-conventions.md §severity-taxonomy are the authoritative guidance.

---

## Reference

- Spec: `docs/decisions/adr-0031-service-owned-dashboards-alerts.md`
- Taxonomy: `docs/observability/alert-conventions.md` §Severity Taxonomy
- Guard: `scripts/guards/simple/validate-alert-rules.sh`

---

## Implementation Summary

- 18 `runbook_url` annotations rewritten from `https://github.com/yourorg/dark_tower/blob/main/docs/runbooks/...` → repo-relative `docs/runbooks/gc-incident-response.md#<anchor>`. All anchors preserved verbatim.
- 7 rules reclassified from `severity: critical` → `severity: page`. No rules routed to `warning` — every former-critical alert describes SLO burn or outage-class user impact per the taxonomy anchors.
- Group-name YAML label renamed `gc-service-critical` → `gc-service-page` per observability's review finding.
- No expression / `for:` / threshold changes. Severity labels + URL format + group name only.

### Verdicts
- **Security**: CLEAR. URL migration complete (grep `https?://` → 0); no hygiene violations introduced; guard passes.
- **Observability**: RESOLVED. All 7 severity calls defensible against the taxonomy anchors; 1 finding (stale group-name) fixed.

### Validation
- `validate-alert-rules.sh` → clean on gc-alerts.yaml; mc-alerts.yaml still fails 25 violations (scoped to next devloop).
- L1 (cargo check): PASS; L2/L4/L5 trivial passes (no Rust changes); L6 pre-existing; L7 Lead SAFE (pure YAML migration); L8 skipped.

---

## Files Modified

| File | Changes |
|------|---------|
| `infra/docker/prometheus/rules/gc-alerts.yaml` | 18 runbook_url rewrites + 7 severity reclassifications + 1 group-name rename |

Diff: 1 file, 30 insertions(+) / 30 deletions(-).

---

## Devloop Verification Steps

- L1 (cargo check): PASS
- L2 (cargo fmt --check): PASS
- L3 (guards): `validate-alert-rules` → `OK - gc-alerts.yaml`; mc-alerts.yaml still 25 violations (out of scope, next devloop)
- L4/L5 (tests, clippy): trivial — no Rust changes
- L6 (cargo audit): pre-existing vulnerabilities (not this devloop's concern)
- L7 (semantic): Lead-judgment SAFE — pure YAML migration, no new semantic surface
- L8 (env-tests): skipped — no service code changes

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0

URL migration complete (`grep -E 'https?://|//[a-zA-Z]'` returns 0 hits). No hygiene violations introduced. Guard passes on the file. All 18 runbook_url targets resolve to existing docs.

### Observability Specialist
**Verdict**: RESOLVED
**Findings**: 1 (fixed)

All 7 severity reclassifications defensible against taxonomy anchors: GCDown / GCHighErrorRate / GCDatabaseDown are literal anchor examples in the conventions doc; GCHighLatency / GCMeetingCreationStopped match user-visible-impact anchor; GCErrorBudgetBurnRateCritical matches the canonical fast-burn shape (for: 1h, severity: page) exactly; GCMCAssignmentSlow's 20ms SLO on the meeting-join critical path qualifies as user-visible SLO burn, not resource-pressure leading indicator. Expressions / `for:` / thresholds unchanged.

**Finding (fixed)**: Group-name YAML label `- name: gc-service-critical` at line 12 was stale vs. the new taxonomy. Renamed to `gc-service-page`. Single occurrence, no callers.

---

## Rollback Procedure

1. Start commit: `f5f53f81a677d22400ec63c509dc36cb2db62fa7`
2. Soft reset: `git reset --soft f5f53f8`
3. No schema or deployment changes — simple git revert is sufficient.
