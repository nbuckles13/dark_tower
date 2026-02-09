# Observability Patterns

Reusable patterns discovered and established for observability infrastructure.

---

## Pattern: Two-Runbook Structure per Service (ADR-0011)
**Added**: 2026-02-06
**Related files**: `docs/runbooks/gc-deployment.md`, `docs/runbooks/gc-incident-response.md`, `docs/runbooks/ac-service-deployment.md`, `docs/runbooks/ac-service-incident-response.md`

Each service has exactly two comprehensive runbooks:
1. **Deployment runbook**: Pre-deployment checklist, deployment steps, rollback, configuration reference, common deployment issues, smoke tests
2. **Incident response runbook**: Severity classification, escalation paths, numbered failure scenarios with anchor IDs, diagnostic commands, postmortem template

Alerts link to specific scenarios via anchors (e.g., `gc-incident-response.md#scenario-1-database-connection-failures`). This consolidates operational knowledge and ensures alerts always point to relevant context.

---

## Pattern: Alert-to-Runbook Section Anchoring
**Added**: 2026-02-06
**Related files**: `infra/docker/prometheus/rules/gc-alerts.yaml`

Every alert annotation includes `runbook_url` pointing to a specific section anchor in the incident response runbook:
```yaml
annotations:
  runbook_url: "https://github.com/yourorg/dark_tower/blob/main/docs/runbooks/gc-incident-response.md#scenario-1-database-connection-failures"
```
Multiple alerts can point to the same scenario (e.g., GCHighMemory and GCHighCPU both point to Resource Pressure scenario). The incident response runbook includes an "Alert Mapping" table showing which alerts correspond to which scenarios.

---

## Pattern: Cardinality-Safe PromQL with sum by()
**Added**: 2026-02-06
**Related files**: `infra/grafana/dashboards/gc-overview.json`, `infra/grafana/dashboards/gc-slos.json`

All dashboard queries aggregate with `sum by(label)` to control cardinality:
```promql
sum by(endpoint) (rate(gc_http_requests_total[5m]))
```
Labels used: `endpoint` (normalized paths), `status_code` (HTTP codes), `operation` (CRUD), `success` (bool). Never use unbounded labels (user_id, meeting_id, UUIDs). Target max 1,000 unique label combinations per metric per ADR-0011.

---

## Pattern: SLO Threshold Lines in Dashboards
**Added**: 2026-02-06
**Related files**: `infra/grafana/dashboards/gc-overview.json`

Latency panels include visual SLO threshold lines as additional series:
```json
{
  "expr": "vector(0.2)",
  "legendFormat": "SLO: 200ms",
  "color": "#FF0000",
  "lineStyle": { "dash": [4, 4] }
}
```
This provides at-a-glance SLO compliance visibility. Use red dashed line for thresholds. Standard thresholds: HTTP p95 = 200ms, MC assignment p95 = 20ms, DB query p99 = 50ms.

---

## Pattern: Dashboard-Alert-Runbook Triangle Design
**Added**: 2026-02-06
**Related files**: `infra/grafana/dashboards/`, `infra/docker/prometheus/rules/`, `docs/runbooks/`

Design dashboards, alerts, and runbooks together as a coherent triangle:
1. **Dashboards** visualize metrics with SLO thresholds
2. **Alerts** fire when thresholds are breached, linking to runbooks
3. **Runbooks** reference dashboard panels for diagnosis and include PromQL queries matching alert expressions

Each artifact reinforces the others. Alert thresholds match dashboard threshold lines. Runbook diagnosis steps say "Check the X panel in GC Overview dashboard."

---
