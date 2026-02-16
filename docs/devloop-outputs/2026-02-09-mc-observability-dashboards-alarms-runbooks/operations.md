# Operations Specialist Checkpoint

**Date**: 2026-02-09
**Specialist**: Operations
**Task**: MC Observability Dashboards, Alarms, and Runbooks

---

## Implementation Summary

Created complete observability infrastructure for Meeting Controller following ADR-0011 standards and the reference pattern established by GC/AC implementations.

### Deliverables Created

1. **Grafana Dashboard** (`infra/grafana/dashboards/mc-overview.json`)
   - 15 panels covering all MC-specific metrics
   - SLO-aligned with threshold indicators
   - Privacy-by-default (no meeting_id, participant_id in queries)
   - Cardinality-safe queries (bounded labels only: actor_type, status)

2. **Prometheus Alert Rules** (`infra/docker/prometheus/rules/mc-alerts.yaml`)
   - 6 critical alerts + 8 warning alerts
   - All alerts include runbook links
   - SLO-based thresholds per ADR-0011

3. **Operational Runbooks** (`docs/runbooks/`)
   - `mc-deployment.md`: Deployment procedures, health checks, rollback steps
   - `mc-incident-response.md`: Troubleshooting for 7 common scenarios

---

## Patterns Discovered

### Dashboard Patterns

1. **Top Row Summary Panels**: Start with stat/gauge panels showing key health indicators (active meetings, connections, panics, drop rate, service status, pod count)

2. **SLO Line Overlay**: Use `vector(X)` queries with dashed red line style to show SLO thresholds on latency panels - makes SLO breaches immediately visible

3. **Actor-Type Grouping**: Group metrics by `actor_type` label for debugging actor-specific issues - essential for MC's actor-based architecture

4. **Dual-Axis for Related Metrics**: Use right axis for secondary metrics (e.g., connections on right axis alongside meetings on left) to show correlation without scale confusion

### Alert Patterns

1. **Immediate vs Sustained**: Actor panics fire immediately (`for: 0m`) since any panic is critical. Latency/mailbox alerts sustain for 2-5 minutes to avoid noise.

2. **Critical vs Warning Split**: Separate rule groups with different intervals (30s critical, 60s warning) - critical alerts evaluate more frequently

3. **Runbook URL Convention**: Anchor links to specific sections within consolidated incident-response runbook (e.g., `#scenario-1-high-mailbox-depth`) - avoids multiple small runbook files

4. **Status Label for Integration Metrics**: GC heartbeat metrics use `status` label (success/error) for success rate calculation

### Runbook Patterns

1. **Consolidated Incident Response**: Single runbook with all scenarios (gc-incident-response.md pattern) is more maintainable than many small runbooks

2. **Scenario Template**: Each scenario follows: Alert -> Severity -> Symptoms -> Diagnosis (commands) -> Root Causes -> Remediation (commands) -> Escalation

3. **Expected Recovery Time**: Include estimated recovery time for each remediation option - helps on-call make decisions

4. **Pre-Deployment Checklist**: Explicit checkbox list ensures nothing is missed during deployments

---

## Gotchas Encountered

### Grafana JSON

1. **gridPos Coordinates**: Y coordinates must be carefully calculated to avoid panel overlap. Each row adds 6-8 to Y value.

2. **Panel IDs Must Be Unique**: Auto-incrementing IDs required - duplicate IDs cause rendering issues

3. **Threshold Style Options**: Use `"thresholdsStyle": {"mode": "line+area"}` for mailbox depth to show warning/critical zones clearly

4. **Legend Calculations**: Include `["mean", "max", "lastNotNull"]` for time series to help with debugging - max is essential for peak detection

### Prometheus Alerts

1. **Division by Zero**: Message drop rate calculation must handle case where no messages are processed - added both numerator and denominator terms

2. **Rate Window Alignment**: Use consistent 5m rate windows across related alerts for comparable thresholds

3. **Multi-line YAML Expressions**: Use `|` literal block for complex PromQL to improve readability

4. **Label Cardinality**: Only use bounded labels (actor_type, status) in alerts - never meeting_id or participant_id

### Runbooks

1. **Command Copy-Paste Safety**: All kubectl commands include namespace (`-n dark-tower`) explicitly - prevents accidents in multi-namespace clusters

2. **Port-Forward Cleanup**: Always include `kill %1` after port-forward commands to avoid port conflicts

3. **Remediation Ordering**: Order remediation options from least disruptive to most disruptive - helps on-call choose appropriate response

4. **GC Integration Dependencies**: MC health depends on GC registration - must include GC connectivity checks in MC diagnosis

---

## Key Decisions

### Threshold Choices

| Metric | Warning | Critical | Rationale |
|--------|---------|----------|-----------|
| Mailbox depth | >100 for 5m | >500 for 2m | Based on ACTOR_MAILBOX_SIZE default of 1000 |
| Message latency p95 | N/A | >500ms for 5m | ADR-0011 SLO (500ms for signaling) |
| Message drop rate | N/A | >1% for 5m | Any significant drops affect meeting quality |
| GC heartbeat failures | >10% for 5m | >50% for 2m | Below 50% still allows some assignments |
| Memory usage | >85% for 10m | N/A | Warning only - no immediate critical action |
| CPU usage | >80% for 5m | N/A | Warning only - scale before critical |

### Panel Organization

1. **Row 1 (y=0)**: Summary stats - quick health overview
2. **Row 2 (y=6)**: Mailbox depth + Latency - core actor health
3. **Row 3 (y=14)**: Message drops + Panics - error indicators
4. **Row 4 (y=22)**: Capacity trends + Throughput - load patterns
5. **Row 5 (y=30)**: Resource usage + GC integration - infrastructure

### Runbook Scope

- **mc-deployment.md**: Focused on deployment lifecycle only
- **mc-incident-response.md**: All operational scenarios consolidated
- Avoided splitting into per-scenario runbooks (harder to maintain, less context)

---

## Current Status

**Implementation Complete**

Files created:
- [x] `infra/grafana/dashboards/mc-overview.json` - 15 panels, SLO-aligned
- [x] `infra/docker/prometheus/rules/mc-alerts.yaml` - 14 alerts (6 critical, 8 warning)
- [x] `docs/runbooks/mc-deployment.md` - Deployment procedures
- [x] `docs/runbooks/mc-incident-response.md` - 7 incident scenarios

Verification:
- JSON/YAML syntax valid
- Prometheus queries follow PromQL syntax
- Dashboard follows GC pattern structure
- Alert rules follow GC pattern structure
- Runbooks follow GC/AC pattern structure

---

## Future Considerations

1. **Add mc-slos.json dashboard**: Similar to gc-slos.json for SLO tracking over time
2. **Add PagerDuty integration**: Configure alert routing to actual PagerDuty schedules
3. **Add recording rules**: Pre-compute expensive queries for dashboard performance
4. **Add admin API runbook section**: When MC admin API is implemented, add runbook procedures

---

## Specialist Knowledge Updates

Created initial knowledge files in `docs/specialist-knowledge/operations/`:

### patterns.md (11 entries)
- Dashboard Structure: Top-Row Summary Stats
- Dashboard Pattern: SLO Line Overlay
- Dashboard Pattern: Actor-Type Grouping
- Alert Pattern: Immediate vs Sustained Firing
- Alert Pattern: Critical vs Warning Split
- Alert Pattern: Runbook URL Convention
- Runbook Pattern: Consolidated Incident Response
- Runbook Pattern: Scenario Template
- Runbook Pattern: Expected Recovery Times
- Runbook Pattern: Pre-Deployment Checklist
- Runbook Pattern: Dual-Axis for Related Metrics

### gotchas.md (9 entries)
- Grafana JSON: gridPos Y Coordinate Calculation
- Grafana JSON: Panel IDs Must Be Unique
- Prometheus Alerts: Division by Zero in Rate Calculations
- Prometheus Alerts: Label Cardinality Control
- Runbook Commands: Port-Forward Cleanup Required
- Runbook Commands: Namespace Explicit in kubectl
- Runbook Pattern: Remediation Option Ordering
- Alert Threshold: Rate Window Alignment
- Grafana Dashboard: Threshold Style Options

### integration.md (8 entries)
- MC-GC Registration Dependency
- Actor-Type Label: Key Debugging Dimension
- Dashboard-Alert-Runbook Linkage
- Service Dependencies in Deployment
- Graceful Drain Pattern for Stateful Services
- Privacy-by-Default in Observability
- SLO Alignment Between Services
- Runbook Coordination for Cascading Failures

**Total**: 28 knowledge entries documenting patterns, gotchas, and integration notes from MC observability implementation.

---

## Reflection Summary

This MC observability implementation revealed the critical importance of **pattern consistency across services**. Following the GC/AC reference implementations made this work straightforward - dashboard structure, alert organization, and runbook format were all well-established patterns that transferred directly to MC.

The most valuable pattern discovered was **consolidated incident-response runbooks**. Rather than creating separate runbook files per scenario (which proliferate quickly), a single runbook with clear scenario sections and anchor links provides better context and is far more maintainable.

The key gotcha was **PromQL division-by-zero** in the message drop rate calculation - without including both dropped and processed messages in the denominator, the alert fails to evaluate when no messages flow. This is a subtle issue that could cause silent alert failures in production.

The most important integration insight was **MC's dependency on GC for health**. An MC with a working actor system but failed GC registration appears healthy (liveness passes) but cannot serve meetings (readiness fails). This dependency must be reflected in health checks, alerts, and runbooks.
