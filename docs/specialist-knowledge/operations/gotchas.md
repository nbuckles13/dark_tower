# Operations Specialist - Gotchas

This file documents operational pitfalls to avoid in the Dark Tower project. Add entries when you encounter issues that could trip up other specialists.

---

## Grafana JSON: gridPos Y Coordinate Calculation

**Added**: 2026-02-09
**Related files**: `infra/grafana/dashboards/mc-overview.json`

Dashboard panels must have non-overlapping `gridPos` coordinates. Each row typically consumes 6-8 Y units depending on panel height (`"h": 6` or `"h": 8`). When adding panels, carefully calculate Y coordinates by summing previous row heights. Miscalculation causes panel overlap rendering dashboards unusable. Use a consistent row pattern (e.g., Row 1: y=0, Row 2: y=6, Row 3: y=14) and document it in comments.

---

## Grafana JSON: Panel IDs Must Be Unique

**Added**: 2026-02-09
**Related files**: `infra/grafana/dashboards/mc-overview.json`

Each panel in a Grafana dashboard requires a unique `"id"` field. Duplicate IDs cause rendering failures or panels failing to display. When creating dashboards, use auto-incrementing IDs (1, 2, 3, ...) and track the highest ID used. If copying panels from other dashboards, always assign new IDs to avoid conflicts.

---

## Prometheus Alerts: Division by Zero in Rate Calculations

**Added**: 2026-02-09
**Related files**: `infra/docker/prometheus/rules/mc-alerts.yaml` (MCHighMessageDropRate alert)

When calculating drop rates or percentages, ensure the denominator cannot be zero. For message drop rate calculations, include both dropped messages and successfully processed messages in the denominator (e.g., `sum(rate(mc_messages_dropped_total[5m])) / (sum(rate(mc_messages_dropped_total[5m])) + sum(rate(mc_message_processing_duration_seconds_count[5m])))`). Without this, alerts fail to evaluate when no messages are being processed.

---

## Prometheus Alerts: Label Cardinality Control

**Added**: 2026-02-09
**Related files**: `infra/docker/prometheus/rules/mc-alerts.yaml`

Only use bounded labels in alert queries - never use unbounded identifiers like `meeting_id`, `participant_id`, or `session_id`. These create unlimited unique time series causing memory exhaustion in Prometheus. Restrict grouping to bounded labels like `actor_type` (fixed set of actor types) or `status` (success/error). This follows ADR-0011 cardinality control requirements.

---

## Runbook Commands: Port-Forward Cleanup Required

**Added**: 2026-02-09
**Related files**: `docs/runbooks/mc-incident-response.md`, `docs/runbooks/gc-incident-response.md`

Always include `kill %1` after `kubectl port-forward` commands in runbooks. Port-forwards run in background and persist after the command completes, blocking the port for subsequent commands. Without explicit cleanup, operators encounter "Address already in use" errors when running the next port-forward. Use the pattern: `kubectl port-forward ... & curl http://localhost:8080/metrics; kill %1`.

---

## Runbook Commands: Namespace Explicit in kubectl

**Added**: 2026-02-09
**Related files**: `docs/runbooks/mc-deployment.md`, `docs/runbooks/gc-deployment.md`

Always include `-n dark-tower` explicitly in kubectl commands within runbooks, even if the namespace is "obvious". Multi-namespace clusters are common in production, and operators may have different default namespaces configured. Explicit namespaces prevent accidents like deploying to the wrong environment or querying metrics from an unintended namespace.

---

## Runbook Pattern: Remediation Option Ordering

**Added**: 2026-02-09
**Related files**: `docs/runbooks/mc-incident-response.md` (all scenario sections)

Order remediation options from least disruptive to most disruptive. Start with low-impact options (scale horizontally, increase limits) before suggesting high-impact options (pod restart, rollback). Each option should include a severity warning if it will disrupt active users (e.g., "WARNING: Active meetings on this pod will be affected"). This helps on-call choose appropriate responses based on incident severity.

---

## Alert Threshold: Rate Window Alignment

**Added**: 2026-02-09
**Related files**: `infra/docker/prometheus/rules/mc-alerts.yaml`

Use consistent rate windows across related alerts (typically `[5m]`) to ensure comparable thresholds. Mixing rate windows (e.g., `[1m]` for one alert, `[5m]` for another) makes threshold comparisons meaningless and creates confusion during incidents. The only exception is critical alerts that need faster detection (e.g., database connection failures using `[1m]` windows).

---

## Grafana Dashboard: Threshold Style Options

**Added**: 2026-02-09
**Related files**: `infra/grafana/dashboards/mc-overview.json` (Actor Mailbox Depth panel)

For metrics with warning/critical thresholds (like mailbox depth), use `"thresholdsStyle": {"mode": "line+area"}` instead of `"mode": "line"`. The area fill provides immediate visual feedback - the graph area turns yellow/red when crossing thresholds, making severity changes obvious even without reading axis values. This is especially important for metrics that operators need to monitor at a glance.
