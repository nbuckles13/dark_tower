# Observability Patterns

*This file accumulates successful observability patterns. Updated after each implementation.*

---

## Pattern: Two-Runbook Structure per Service (ADR-0011)
**Added**: 2026-02-06
**Related files**: `docs/runbooks/gc-deployment.md`, `docs/runbooks/gc-incident-response.md`, `docs/runbooks/ac-service-deployment.md`, `docs/runbooks/ac-service-incident-response.md`

Each service has exactly two comprehensive runbooks:
1. **Deployment runbook**: Pre-deployment checklist, deployment steps, rollback, configuration reference, common deployment issues, smoke tests
2. **Incident response runbook**: Severity classification, escalation paths, numbered failure scenarios with anchor IDs, diagnostic commands, postmortem template

Alerts link to specific scenarios via anchors (e.g., `gc-incident-response.md#scenario-1-database-connection-failures`). This consolidates operational knowledge and ensures alerts always point to relevant context.

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
