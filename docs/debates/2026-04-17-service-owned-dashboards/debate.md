---
name: Service-owned dashboards and alerts
date: 2026-04-17
status: Complete
adr: ADR-0031
---

# Debate: Should service specialists own their own Grafana dashboards and Prometheus alert rules?

**Date**: 2026-04-17
**Status**: Complete — consensus reached, ADR-0031 drafted
**Participants**:
- observability (cross-cutting)
- operations (cross-cutting)
- security (cross-cutting)
- test (cross-cutting)
- meeting-controller (domain)
- media-handler (domain)

> **Note**: When cross-cutting specialists (Security, Test, Observability, Operations) score < 70 satisfaction at consensus, this requires explicit user risk acceptance — not implicit majority override. See ADR-0024 §5.7.

## Question

Should service specialists (media-handler, meeting-controller, global-controller, auth-controller) own their own Grafana dashboards and Prometheus alert rules instead of having a separate observability specialist task?

## Context

Current user story structure splits observability into two phases:
- **Phase 4 (service specialist)**: add metrics to the service code (`observability/metrics.rs`)
- **Phase 5 (observability specialist)**: add Grafana dashboards and Prometheus alert rules

**Observed problem**: Guards require dashboards whenever metrics are added (see `scripts/guards/simple/validate-application-metrics.sh` and `scripts/guards/simple/validate-kustomize.sh` R-20 bidirectional check). This means when the service specialist adds a metric in Phase 4, the metric guard fails unless a dashboard panel and kustomize entry already exist. Devloops end up doing dashboards + alerts as unplanned work during Phase 4 — so Phase 5 becomes redundant or empty.

**Proposal**: Collapse `metrics + dashboards + alerts` into the service specialist's Phase 4 implementation task. Observability remains as a **cross-cutting reviewer** (already present in every devloop via `/devloop`'s mandatory cross-cutting reviewers, per ADR-0024) rather than a separate implementer. This eliminates 2 tasks (`add dashboards`, `add alerts`) and 1 phase from user stories.

**Dimensions to consider**:
1. Dashboard consistency across services
2. Alert threshold quality (who knows the right thresholds?)
3. Specialist knowledge boundaries
4. Guard compatibility (current R-20 bidirectional check)
5. Impact on `/user-story` task decomposition
6. Existing artifacts: `infra/grafana/dashboards/{ac,gc,mc,mh}-overview.json`, `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml`, `docs/observability/alerts.md`, `docs/observability/dashboards.md`

## Positions

### Round 1 (initial)

| Specialist | Satisfaction | Position summary |
|------------|-------------:|------------------|
| observability | 82 | Lean yes; needs 5 guardrails (mandatory plan-time review, shared threshold authority, template library, ADR-0029 panel guard, runbook-link guard) |
| operations | 78 | Yes; needs runbook-link guard, cross-service dashboard ownership split, threshold-at-plan-approval workflow, per-service skeleton |
| security | 88 | Yes; surface is narrow (metric-definition PR), doesn't shift with dashboard authorship |
| test | 85 | Yes; task boundary now matches CI guard boundary, needs reviewer-gate preserved |
| meeting-controller | 85 | Strong support; already owns alert file and thresholds, wants observability as reviewer only |
| media-handler | ~85 | Strong support; closes MH alerts gap, conditional on observability owning conventions doc |

### Round 2 (after team-lead synthesis)

Team-lead broadcast consolidated proposal incorporating all raised guardrails. Specialists DM'd each other to converge on details (runbook-URL format, panel-classification guard scope, conditional Phase 5 triggers, label hygiene, exemplar rollout).

| Specialist | Satisfaction | Decision |
|------------|-------------:|----------|
| observability | 92 | Accept |
| operations | 93 | Accept |
| security | 95 | Accept |
| test | 93 | Accept |
| meeting-controller | 95 | Accept |
| media-handler | 95 | Accept |

## Discussion

### Round 1

All six specialists broadcast initial positions within minutes of each other. Three patterns emerged:

1. **Everyone agreed the current Phase 4/Phase 5 split is fiction**: guards force metrics+dashboards+catalog+kustomize to land together, so Phase 4 cannot pass CI without doing Phase 5's work. The question reduced to "which guardrails accompany the collapse?" rather than "should we collapse?"

2. **Threshold authority was the central contested point**: observability and operations both want authority over alert thresholds; service specialists (MC, MH) claimed thresholds live in domain knowledge they already possess. Resolution: specialist proposes, observability + operations ratify at plan approval.

3. **Cross-service dashboards emerged as the explicit carve-out**: raised by operations, confirmed by observability. Per-service dashboards move to service specialists; cross-service artifacts (errors-overview, fleet SLO views, logs-aggregation) stay with observability as implementer.

### Round 2

Team-lead synthesized the 7 mandatory guardrails. Specialists converged via DM on:

- Runbook-URL guard scope (repo-relative paths only, rejected by security's sharpening)
- Panel-classification guard coverage (counter→increase/rate, gauge→last, histogram→quantile per ADR-0029)
- Conditional Phase 5 triggers (new SLO, severity routing changes, cross-service dashboard changes)
- Plan-template artifact structure (alert threshold table with reviewers)
- Prerequisite sequencing (guards land before template change)

All specialists moved to 90+ within one round of discussion after the synthesis.

## Consensus

Reached at Round 2. All participants at 90%+ satisfaction.

Unanimous decision: collapse metrics + per-service dashboards + per-service alerts into the service specialist's Phase 4 task. Observability becomes a mandatory cross-cutting reviewer (ADR-0024), not a separate implementer. Cross-service dashboards + conventions doc + guard infrastructure remain with observability.

## Decision

See: [ADR-0031](../../decisions/adr-0031-service-owned-dashboards-alerts.md)
