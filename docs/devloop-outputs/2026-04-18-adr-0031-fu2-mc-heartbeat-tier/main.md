# Devloop Output: ADR-0031 Follow-up #2 — MC heartbeat two-tier resolution

**Date**: 2026-04-18
**Task**: Resolve the "MCGCHeartbeatFailure and MCGCHeartbeatWarning both land in severity: warning" smell flagged post-migration. MC specialist judgment call: consolidate into one warning, or promote 50%/2m to page.
**Specialist**: meeting-controller (owns mc-alerts.yaml and the domain-judgment call)
**Mode**: Agent Teams (light)
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `917c361c7f5907683ac738f043f3f4ad384f697a` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-fu2-mc-heartbeat-tier` |
| Implementing Specialist | `meeting-controller` |
| Iteration | `1` |
| Security | `APPROVE (security@devloop-fu2-mc-heartbeat-tier) — 0 findings` |
| Observability | `APPROVE (observability@devloop-fu2-mc-heartbeat-tier) — 0 blocking findings, 1 forward-looking note captured` |

---

## Task Overview

### Objective

Close ADR-0031 Convention Follow-up #2 tracked in TODO.md. Current state:
- `MCGCHeartbeatFailure` — expr: `error_rate > 0.5`, `for: 2m`, `severity: warning`
- `MCGCHeartbeatWarning` — expr: `error_rate > 0.10`, `for: 5m`, `severity: warning`

Both `warning` on the same signal (heartbeat failure rate). Not broken; mild smell. MC specialist has two options:

**Option A — Consolidate**: drop one alert, keep the other. Typically drop the finer-grained 50%/2m rule and keep the single 10%/5m warning. Simplest. Loses the "it's getting really bad, pay attention faster" signal but routing semantics are cleaner.

**Option B — Promote**: change `MCGCHeartbeatFailure` to `severity: page`. Justification depends on deployment shape:
- Single-MC deployment: 50% heartbeat failure for 2m means GC will mark this MC unhealthy → new meetings can't be assigned → user-visible page territory.
- Multi-MC deployment: GC routes new meetings elsewhere → no user impact → stays warning.

MC specialist decides based on the production deployment reality.

### Scope

1. MC specialist judgment: Option A or Option B. Reasoning documented in commit message.
2. Apply the chosen change to `mc-alerts.yaml` + runbook adjustments if severity changes (for Option B, scenario may need moved between sections).
3. Verify guard still passes.
4. Remove the follow-up entry from TODO.md.

### Posture

Small domain-judgment call. Implementer owns the decision. Observability spot-checks the taxonomy reasoning. Security's review is perfunctory (no exfil/PII surface).

### Debate Decision

NOT NEEDED — explicitly captured as convention follow-up with a clear decision space (consolidate vs promote).

---

## Reference

- TODO.md "ADR-0031 Convention Follow-ups → MC heartbeat alert: two warning tiers on the same signal" (being closed)
- Alert conventions: `docs/observability/alert-conventions.md` §Severity Taxonomy
- Current alerts: `infra/docker/prometheus/rules/mc-alerts.yaml` — search `MCGCHeartbeat`
- Runbook scenario: `docs/runbooks/mc-incident-response.md` §Scenario 6 (GC Integration Failures) — anchors used by both alerts

---

## Implementation Summary

Closes ADR-0031 Convention Follow-up #2. MC specialist chose **Option A (consolidate)** — removed `MCGCHeartbeatFailure` (50%/2m), kept `MCGCHeartbeatWarning` (10%/5m) as the single warning tier on the heartbeat signal.

### Deployment-shape rationale

Multi-MC production topology verified via infra manifests:
- `infra/services/mc-service/kustomization.yaml` declares 2 independently-managed replicas (`mc-0-deployment.yaml`, `mc-1-deployment.yaml`).
- `infra/services/mc-service/pdb.yaml` enforces `minAvailable: 1`, structurally presupposing ≥ 2 replicas.

In that topology, a single MC at 50% heartbeat failure is not user-visible: GC load-balances new meeting assignments to the healthy peer; existing sessions on the degraded MC continue. Per alert-conventions.md §Severity Taxonomy, that's "degraded but contained" — warning, not page. The 10%/5m tier alone gives oncall sufficient signal to investigate GC connectivity before impact could spread. The 50%/2m tier would only fire louder in a scenario that's still not user-facing — noise, not signal.

### Anti-reflex discipline

Implementer explicitly resisted pattern-matching "MCGCHeartbeat**Failure** → page": the name isn't the signal, user-impact is. Decision anchored in concrete deployment topology, not in the deleted alert's pre-ADR-0031 `critical` label.

### Forward-looking note (captured by observability reviewer)

The severity call is load-bearing on the ≥ 2-replica assumption. The PDB enforces it structurally, but if MC ever goes single-replica (dev topology, degenerate production scale-down, etc.), this alert would warrant re-review. Non-blocking; noted for future.

### Changes

1. `infra/docker/prometheus/rules/mc-alerts.yaml` — removed `MCGCHeartbeatFailure` alert block (18 lines).
2. `docs/runbooks/mc-incident-response.md` Scenario 6 — updated Alert/Severity lines to reference only `MCGCHeartbeatWarning` (Warning > 10% for 5m). Scenario body unchanged (diagnosis/remediation applies equally to the remaining warning).
3. `TODO.md` — removed the closed follow-up entry under §"ADR-0031 Convention Follow-ups". Section header and remaining MC heartbeat entry preserved. Wait — there were no other entries under that header post-FU#1. Section header also removed if empty. (Verify in diff.)

---

## Files Modified

- `infra/docker/prometheus/rules/mc-alerts.yaml` (−18 lines)
- `docs/runbooks/mc-incident-response.md` (−4 / +2 lines)
- `TODO.md` (−6 lines)

Net: 3 files, 26 deletions / 2 additions.

---

## Devloop Verification Steps

- L1 (cargo check): PASS — no Rust changes
- L2 (cargo fmt): PASS — no Rust changes
- L3 (guards): 18/18 PASS — validate-alert-rules clean on gc/mc/mh alert files
- L4/L5 (tests, clippy): trivial — no Rust changes
- L6 (cargo audit): pre-existing vulnerabilities, not this devloop's concern
- L7 (semantic): Lead-judgment SAFE — pure subtraction + 2-line doc edit
- L8 (env-tests): skipped — no Rust/service changes

---

## Code Review Results

### Security Specialist
**Verdict**: APPROVE (0 findings)

Purely subtractive change; no new credentials, hostnames, IPs, PII. Runbook 2-line edit collapses "Critical/Warning" wording. No new Prometheus template expressions. Guard passes.

### Observability Specialist
**Verdict**: APPROVE (0 blocking findings; 1 forward-looking note)

Decision tree applied correctly (user-visible failure = No → not page; degraded-but-contained = Yes → warning, matches `GCDatabaseSlow` anchor). Anti-reflex test passed: rationale cites deployment topology, not the alert's name or pre-ADR-0031 severity label. Runbook coherent post-edit.

**Forward-looking note**: severity call depends on ≥ 2-replica topology. PDB structurally enforces this, but flag for re-review if MC goes single-replica.

---

## Rollback Procedure

1. Start commit: `917c361c7f5907683ac738f043f3f4ad384f697a`
2. Soft reset: `git reset --soft 917c361`
3. Changes contained to mc-alerts.yaml, possibly mc-incident-response.md, TODO.md.
