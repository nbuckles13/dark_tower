# Observability Integration Notes

*This file captures how Observability coordinates with other specialists.*

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
