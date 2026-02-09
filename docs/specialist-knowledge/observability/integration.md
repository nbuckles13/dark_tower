# Observability Integration Guide

What other services and specialists need to know when working with observability infrastructure.

---

## Integration: Metric Naming Convention
**Added**: 2026-02-06
**Related files**: `crates/global-controller/src/observability/metrics.rs`

Metrics follow the pattern `{service}_{domain}_{measurement}_{unit}`:
- `gc_http_requests_total` - Counter
- `gc_http_request_duration_seconds` - Histogram
- `gc_mc_assignment_duration_seconds` - Histogram
- `gc_db_query_duration_seconds` - Histogram

Service prefixes: `gc_` (Global Controller), `ac_` (Auth Controller), `mc_` (Meeting Controller), `mh_` (Media Handler). Use `_total` suffix for counters, `_seconds` for durations.

---

## Integration: SLO-Aligned Histogram Buckets
**Added**: 2026-02-06
**Related files**: `crates/global-controller/src/routes/mod.rs`

Histogram buckets must align with SLO targets to enable accurate percentile measurement:
- **HTTP requests**: Buckets around 200ms (p95 target) - [5ms, 10ms, 25ms, 50ms, 100ms, 200ms, 300ms, 500ms, 1s, 2s]
- **MC assignment**: Buckets around 20ms (p95 target) - [5ms, 10ms, 15ms, 20ms, 30ms, 50ms, 100ms, 500ms]
- **Database queries**: Buckets around 50ms (p99 target) - [1ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 1s]

When adding new metrics with SLO targets, ensure buckets have resolution around the target value.

---

## Integration: Privacy-by-Default Label Policy (ADR-0011)
**Added**: 2026-02-06
**Related files**: `crates/global-controller/src/middleware/http_metrics.rs`

Labels must not contain PII or unbounded values:
- **Allowed**: `endpoint` (normalized path), `method`, `status_code`, `operation`, `success`
- **Forbidden**: `user_id`, `email`, `meeting_id`, `participant_id`, UUIDs

Paths with dynamic segments are normalized: `/api/v1/meetings/abc123` becomes `/api/v1/meetings/{code}`. This prevents cardinality explosion while maintaining debuggability.

---

## Integration: Dashboard Catalog Location
**Added**: 2026-02-06
**Related files**: `docs/observability/dashboards.md`

All Grafana dashboards are cataloged in `docs/observability/dashboards.md` with:
- Dashboard name and file location
- Purpose description
- Panel inventory
- Owner and last review date

When adding new dashboards, update the catalog. Dashboard JSON files live in `infra/grafana/dashboards/{service}-*.json`.

---

## Integration: Alert Catalog Location
**Added**: 2026-02-06
**Related files**: `docs/observability/alerts.md`

All Prometheus alerts are cataloged in `docs/observability/alerts.md` with:
- Alert name and severity
- Trigger condition
- Runbook link
- Response procedure summary

When adding new alerts, update the catalog. Alert rule files live in `infra/docker/prometheus/rules/{service}-alerts.yaml`.

---

## Integration: Runbook Catalog Location
**Added**: 2026-02-06
**Related files**: `docs/observability/runbooks.md`

All runbooks are cataloged in `docs/observability/runbooks.md` with:
- Runbook name and file location
- Service ownership
- Alert mapping table (which alerts link to which runbook sections)

When adding new runbooks or scenarios, update the catalog and ensure alert annotations point to correct anchors.

---
