# Operations Specialist - Integration Notes

This file documents how operational artifacts (dashboards, alerts, runbooks) integrate with Dark Tower services. Add entries when you discover cross-service dependencies that affect operations.

---

## MC-GC Registration Dependency

**Added**: 2026-02-09
**Related files**: `docs/runbooks/mc-incident-response.md` (Scenario 6), `infra/docker/prometheus/rules/mc-alerts.yaml`

Meeting Controller health depends on successful registration with Global Controller. MC sends periodic heartbeats to GC to maintain "active" status. All MC health checks must include GC connectivity verification - an MC with working actor system but failed GC registration cannot receive meeting assignments. Monitor `mc_gc_heartbeat_total` metric and include GC endpoint connectivity tests in MC troubleshooting procedures.

---

## Actor-Type Label: Key Debugging Dimension

**Added**: 2026-02-09
**Related files**: `infra/grafana/dashboards/mc-overview.json`, `infra/docker/prometheus/rules/mc-alerts.yaml`

Meeting Controller uses actor-based architecture where different actor types (MeetingActor, ParticipantActor, ConnectionActor) handle different workloads. The `actor_type` label is the primary dimension for debugging performance issues. Mailbox depth, message drops, and panics should always be grouped by `actor_type` to identify which specific actor is experiencing issues. This enables targeted remediation rather than service-wide restarts.

---

## Dashboard-Alert-Runbook Linkage

**Added**: 2026-02-09
**Related files**: `infra/grafana/dashboards/mc-overview.json`, `infra/docker/prometheus/rules/mc-alerts.yaml`, `docs/runbooks/mc-incident-response.md`

Observability artifacts form a linked chain: Grafana panels visualize metrics -> Prometheus alerts fire on thresholds -> Alerts link to runbook sections. Maintain consistency across this chain: panel descriptions should explain what triggers alerts, alert annotations should explain symptoms visible in dashboards, runbook diagnostic commands should reference the same metrics shown in panels. Break in this chain causes confusion during incidents.

---

## Service Dependencies in Deployment

**Added**: 2026-02-09
**Related files**: `docs/runbooks/mc-deployment.md`, `docs/runbooks/gc-deployment.md`

MC deployment requires coordination with GC. Before deploying MC, verify GC is healthy and can route new meetings to other MC instances. After MC deployment, verify MC successfully re-registers with GC. MC pods that fail GC registration appear healthy (liveness passes) but cannot receive meetings (readiness fails). Always include GC database query in MC deployment verification steps.

---

## Graceful Drain Pattern for Stateful Services

**Added**: 2026-02-09
**Related files**: `docs/runbooks/mc-deployment.md` (Graceful Drain Procedure)

Stateful services like MC (which holds active meeting sessions) should not be redeployed during high load. Use graceful drain pattern: mark MC as "draining" in GC database (stops new assignments), wait for active meetings to complete, then deploy. This prevents user disruption. Monitor `mc_meetings_active` metric during drain and only proceed when count reaches acceptable level (typically <10 meetings per pod).

---

## Privacy-by-Default in Observability

**Added**: 2026-02-09
**Related files**: `infra/grafana/dashboards/mc-overview.json`, `infra/docker/prometheus/rules/mc-alerts.yaml`

Per ADR-0011, never use unbounded identifiers (`meeting_id`, `participant_id`, `session_id`) in metric labels or dashboard queries. These violate privacy-by-default and cause cardinality explosions. If debugging requires inspecting specific meeting state, use structured logs with privacy-safe correlation IDs, not metrics. Dashboards should show aggregated views only (total meetings, average latency), never per-meeting breakdowns.

---

## SLO Alignment Between Services

**Added**: 2026-02-09
**Related files**: `infra/grafana/dashboards/mc-overview.json`, `infra/grafana/dashboards/gc-overview.json`

Meeting join flow crosses multiple services: Client -> GC (assigns MC) -> MC (establishes session). SLOs must be aligned: GC's MC assignment SLO (20ms p95) plus MC's message processing SLO (500ms p95) should sum to acceptable end-to-end latency. When setting alert thresholds, consider cumulative latency across the call chain. If one service's SLO tightens, downstream services may need adjustment.

---

## Runbook Coordination for Cascading Failures

**Added**: 2026-02-09
**Related files**: `docs/runbooks/mc-incident-response.md`, `docs/runbooks/gc-incident-response.md`

When MC experiences issues (high mailbox depth, panics), GC may also alert (MC assignment failures). Runbooks should guide on-call to check upstream dependencies before assuming local failure. MC runbooks include "Check GC health" steps; GC runbooks include "Check MC pod status" steps. This prevents duplicate investigation and helps identify root cause faster during cascading failures.
