# Operations Specialist - Patterns

This file documents operational patterns that work well in the Dark Tower project. Add entries when you discover reusable approaches that would benefit other specialists.

---

## Dashboard Structure: Top-Row Summary Stats

**Added**: 2026-02-09
**Related files**: `infra/grafana/dashboards/mc-overview.json`, `infra/grafana/dashboards/gc-overview.json`

Start dashboards with a summary row (y=0) containing stat/gauge panels showing key health indicators (active resources, service status, error counts, pod count). This provides at-a-glance health assessment without scrolling. Use thresholds (green/yellow/red) for immediate visual status. Follow with detailed time-series panels in subsequent rows for diagnosis.

---

## Dashboard Pattern: SLO Line Overlay

**Added**: 2026-02-09
**Related files**: `infra/grafana/dashboards/mc-overview.json` (Message Processing Latency panel)

Use `vector(X)` queries to render SLO threshold lines on latency panels (e.g., `vector(0.5)` for 500ms SLO). Style the line as dashed red (`"lineStyle": {"dash": [10, 10]}`) and increase line width to 2. This makes SLO breaches immediately visible without requiring mental math - operators can see at a glance when p95 crosses the threshold line.

---

## Dashboard Pattern: Actor-Type Grouping

**Added**: 2026-02-09
**Related files**: `infra/grafana/dashboards/mc-overview.json`

For actor-based services, group metrics by `actor_type` label using `sum by(actor_type)` queries. This enables targeted debugging - operators can identify which specific actor types are experiencing mailbox backpressure, high latency, or panics. Essential pattern for services using actor model architecture where different actor types have different workload characteristics.

---

## Alert Pattern: Immediate vs Sustained Firing

**Added**: 2026-02-09
**Related files**: `infra/docker/prometheus/rules/mc-alerts.yaml`

Use `for: 0m` (immediate firing) for invariant violations like actor panics where any occurrence is critical and requires immediate attention. Use sustained firing periods (`for: 2m` to `for: 5m`) for performance degradation alerts (latency, mailbox depth) to avoid alert noise from transient spikes. Critical alerts use shorter sustain periods than warnings.

---

## Alert Pattern: Critical vs Warning Split

**Added**: 2026-02-09
**Related files**: `infra/docker/prometheus/rules/mc-alerts.yaml`, `infra/docker/prometheus/rules/gc-alerts.yaml`

Separate alert rules into two groups: `{service}-critical` (interval: 30s) and `{service}-warning` (interval: 60s). Critical alerts evaluate more frequently for faster page delivery. This also improves readability - on-call can quickly scan critical rules without warning noise. Group separation follows severity levels defined in incident response runbooks.

---

## Alert Pattern: Runbook URL Convention

**Added**: 2026-02-09
**Related files**: `infra/docker/prometheus/rules/mc-alerts.yaml`, `docs/runbooks/mc-incident-response.md`

Link alerts to specific sections within a consolidated incident-response runbook using anchor links (e.g., `https://github.com/yourorg/dark_tower/blob/main/docs/runbooks/mc-incident-response.md#scenario-1-high-mailbox-depth`). This avoids proliferation of many small runbook files while providing direct navigation to relevant troubleshooting steps. Each scenario section in runbook should match the alert naming convention.

---

## Runbook Pattern: Consolidated Incident Response

**Added**: 2026-02-09
**Related files**: `docs/runbooks/mc-incident-response.md`, `docs/runbooks/gc-incident-response.md`

Use a single consolidated incident-response runbook with all failure scenarios rather than creating separate runbooks per scenario. Include a table of contents with anchor links for quick navigation. This pattern is more maintainable (single file to update) and provides better context (on-call can see related scenarios). Separate deployment procedures into a dedicated deployment runbook.

---

## Runbook Pattern: Scenario Template

**Added**: 2026-02-09
**Related files**: `docs/runbooks/mc-incident-response.md` (all scenario sections)

Structure each incident scenario with: Alert name + Severity + Symptoms (bullet list) + Diagnosis (bash commands with output interpretation) + Common Root Causes (numbered list) + Remediation (bash commands with recovery times) + Escalation (when/who). This consistent structure enables rapid response - on-call knows where to find commands and doesn't waste time searching for steps.

---

## Runbook Pattern: Expected Recovery Times

**Added**: 2026-02-09
**Related files**: `docs/runbooks/mc-incident-response.md`, `docs/runbooks/gc-incident-response.md`

Include estimated recovery time after each remediation option (e.g., "Expected recovery time: 30-60 seconds"). This helps on-call engineers make informed decisions during incidents - they can choose faster options for critical situations or more thorough options when time permits. Recovery times also set realistic expectations for stakeholders.

---

## Runbook Pattern: Pre-Deployment Checklist

**Added**: 2026-02-09
**Related files**: `docs/runbooks/mc-deployment.md`, `docs/runbooks/gc-deployment.md`

Start deployment runbooks with an explicit checkbox-style checklist covering code quality, infrastructure, and coordination steps. Each checkbox item includes verification commands where applicable. This ensures critical steps aren't skipped during time-pressured deployments and provides a clear go/no-go gate before proceeding to actual deployment steps.

---

## Runbook Pattern: Dual-Axis for Related Metrics

**Added**: 2026-02-09
**Related files**: `infra/grafana/dashboards/mc-overview.json` (Active Meetings & Connections panel)

When displaying related metrics with different scales (e.g., meetings count and connections count), use dual-axis timeseries panels. Configure the secondary metric with `"custom.axisPlacement": "right"` to show correlation without scale confusion. This reveals relationships between metrics that might be hidden if forced onto the same scale.
