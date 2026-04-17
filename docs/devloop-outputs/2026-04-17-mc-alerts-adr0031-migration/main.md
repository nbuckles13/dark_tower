# Devloop Output: MC alerts migration to ADR-0031 strict compliance

**Date**: 2026-04-17
**Task**: Migrate `infra/docker/prometheus/rules/mc-alerts.yaml` to conform to `validate-alert-rules.sh` strict mode per ADR-0031.
**Specialist**: meeting-controller (owner of mc-alerts.yaml per ADR-0031 §Ownership split)
**Mode**: Agent Teams (light)
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `ee5dabc35583617ee508bfdccf0629e2c8141722` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-mc-alerts-migration` |
| Implementing Specialist | `meeting-controller` |
| Iteration | `1` |
| Security | `CLEAR (security@devloop-mc-alerts-migration)` |
| Observability | `RESOLVED (observability@devloop-mc-alerts-migration) — 1 fix, 1 tech-debt note` |
| Context Reviewer | observability (third reviewer for --light) |

---

## Task Overview

### Objective

Return CI to green for the alert-rules guard by migrating mc-alerts.yaml to ADR-0031 strict compliance. Final step in the three-devloop prerequisite sequence for ADR-0031 alert-rules guard rollout.

### Scope

- Single file: `infra/docker/prometheus/rules/mc-alerts.yaml`
- ~19 `runbook_url` rewrites (mechanical sed: strip `https://github.com/yourorg/dark_tower/blob/main/` prefix)
- 6 severity reclassifications from `critical` → `page` or `warning` (specialist judgment per ADR-0031 severity taxonomy)

Affected rules (severity calls):
- `MCDown`
- `MCActorPanic` (subtle: actor supervision bounds impact)
- `MCHighMailboxDepthCritical`
- `MCHighLatency`
- `MCHighMessageDropRate`
- `MCGCHeartbeatFailure`

### Debate Decision

NOT NEEDED — ADR-0031 severity taxonomy + alert-conventions.md §severity-taxonomy are the authoritative guidance.

---

## Reference

- Spec: `docs/decisions/adr-0031-service-owned-dashboards-alerts.md`
- Taxonomy: `docs/observability/alert-conventions.md` §Severity Taxonomy
- Guard: `scripts/guards/simple/validate-alert-rules.sh`
- Prior GC migration (pattern): commit `ee5dabc`, `docs/devloop-outputs/2026-04-17-gc-alerts-adr0031-migration/main.md`

---

## Implementation Summary

- 19 `runbook_url` annotations rewritten from `https://github.com/yourorg/dark_tower/blob/main/docs/runbooks/...` → repo-relative `docs/runbooks/mc-incident-response.md#<anchor>` form. Anchors preserved verbatim.
- 6 rules reclassified from `severity: critical`:
  - 5 → `page`: `MCDown`, `MCActorPanic`, `MCHighMailboxDepthCritical`, `MCHighLatency`, `MCHighMessageDropRate`
  - 1 → `warning`: `MCGCHeartbeatFailure` (existing sessions unaffected; partial-outage shape, degraded-but-contained)
- Group-name YAML label `mc-service-critical` → `mc-service-page` (matches GC convention).
- Header comment rewritten to reference the new taxonomy and `alert-conventions.md`.
- Observability finding F1: annotation text "MCHighLatency critical alert" → "MCHighLatency page alert" (stale taxonomy reference in description prose). Fixed.

No expression / `for:` / threshold changes. Severity labels + URL format + group name + annotation prose only.

**Domain judgment notes**:
- `MCActorPanic` → `page` grounded in ADR-0023: root MeetingControllerActor panic is fatal (full shutdown), MeetingActor panic triggers meeting migration (user-visible disruption). Expression `increase(mc_actor_panics_total[5m]) > 0` admits no tolerance. Not a pattern-matched carryover from `critical`.
- `MCGCHeartbeatFailure` → `warning` grounded in the actual impact: MC drops off GC registry = no new assignments, but existing sessions continue. That's the degraded-but-contained anchor exactly. This is the tier-break we explicitly designed ADR-0031's taxonomy to enable — resisting the "critical→page" reflex was the point.

---

## Files Modified

| File | Changes |
|------|---------|
| `infra/docker/prometheus/rules/mc-alerts.yaml` | 19 runbook_url rewrites + 6 severity reclassifications + 1 group-name rename + header refresh + 1 annotation prose fix |

Diff: 1 file, 50 insertions(+) / 50 deletions(-).

---

## Devloop Verification Steps

- L1 (cargo check): PASS
- L2 (cargo fmt --check): PASS
- L3 (guards): **all 16 PASS, CI green.** `validate-alert-rules.sh` clean on both gc-alerts.yaml and mc-alerts.yaml — exit 0, "All alert rules pass."
- L4/L5 (tests, clippy): trivial — no Rust changes
- L6 (cargo audit): pre-existing vulnerabilities (not this devloop's concern)
- L7 (semantic): Lead-judgment SAFE — pure YAML migration, no new semantic surface
- L8 (env-tests): skipped — no service code changes

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0

URL migration complete (0 absolute URLs remain). No hygiene violations introduced. Guard passes cleanly on both files.

### Observability Specialist
**Verdict**: RESOLVED
**Findings**: 1 fixed, 0 deferred

All 6 severity reclassifications defensible against taxonomy anchors. `MCActorPanic → page` (ADR-0023 fatal semantics) and `MCGCHeartbeatFailure → warning` (partial outage, existing sessions unaffected) both specifically audited and approved.

**Finding (fixed)**: `mc-alerts.yaml:334` annotation text referred to "MCHighLatency critical alert" post-migration — stale taxonomy reference. Changed to "page alert". Fix verified.

**Tech-debt note** (not a finding, captured to TODO.md): MCGCHeartbeatFailure (50%/2m) and MCGCHeartbeatWarning (10%/5m) are now both `warning` — two warning tiers for the same signal. Post-migration tuning candidate: consolidate into one warning, or re-examine whether the 50% threshold warrants `page` if "new assignments fail" is reframed as user-visible.

---

## Rollback Procedure

1. Start commit: `ee5dabc35583617ee508bfdccf0629e2c8141722`
2. Soft reset: `git reset --soft ee5dabc`
3. No schema or deployment changes — simple git revert is sufficient.
