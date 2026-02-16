# Observability Integration Notes

*Captures non-obvious coordination points between Observability and other specialists. Only add entries for things that broke or surprised you during cross-specialist work.*

---

## Integration: Metric Naming Convention
**Added**: 2026-02-06
**Related files**: `crates/gc-service/src/observability/metrics.rs`

Metrics follow the pattern `{service}_{domain}_{measurement}_{unit}`:
- `gc_http_requests_total` - Counter
- `gc_http_request_duration_seconds` - Histogram
- `gc_mc_assignment_duration_seconds` - Histogram
- `gc_db_query_duration_seconds` - Histogram

Service prefixes: `gc_` (Global Controller), `ac_` (Auth Controller), `mc_` (Meeting Controller), `mh_` (Media Handler). Use `_total` suffix for counters, `_seconds` for durations.

---

## Integration: SLO-Aligned Histogram Buckets
**Added**: 2026-02-06
**Related files**: `crates/gc-service/src/routes/mod.rs`

Histogram buckets must align with SLO targets to enable accurate percentile measurement:
- **HTTP requests**: Buckets around 200ms (p95 target) - [5ms, 10ms, 25ms, 50ms, 100ms, 200ms, 300ms, 500ms, 1s, 2s]
- **MC assignment**: Buckets around 20ms (p95 target) - [5ms, 10ms, 15ms, 20ms, 30ms, 50ms, 100ms, 500ms]
- **Database queries**: Buckets around 50ms (p99 target) - [1ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 1s]

When adding new metrics with SLO targets, ensure buckets have resolution around the target value.

---

## Integration: Privacy-by-Default Label Policy (ADR-0011)
**Added**: 2026-02-06
**Related files**: `crates/gc-service/src/middleware/http_metrics.rs`

Labels must not contain PII or unbounded values:
- **Allowed**: `endpoint` (normalized path), `method`, `status_code`, `operation`, `success`
- **Forbidden**: `user_id`, `email`, `meeting_id`, `participant_id`, UUIDs

Paths with dynamic segments are normalized: `/api/v1/meetings/abc123` becomes `/api/v1/meetings/{code}`. This prevents cardinality explosion while maintaining debuggability.
### Guard Pipeline Does NOT Require `#[instrument]` on All Functions
**Discovered**: 2026-02-12 (TD-13 health checker extraction)
**Corrected**: 2026-02-12 (TD-13 iteration 2)
**Coordination**: Observability + Code Quality

The `instrument-skip-all` guard (`scripts/guards/simple/instrument-skip-all.sh`) does NOT require all async functions with parameters to have `#[instrument(skip_all)]`. It only catches the denylist pattern: `#[instrument(skip(...))]` without `skip_all`. Functions that omit `#[instrument]` entirely are not flagged.

**Implication**: When using `.instrument()` chaining on call sites, you can safely remove `#[instrument]` from both the generic function and the wrapper function without triggering guard violations. This was the key insight that made the iteration 2 simplification possible -- the nested span workaround from iteration 1 was unnecessary.

**Original incorrect assumption**: We believed the guard required `#[instrument(skip_all)]` on all async functions with parameters, which forced `#[instrument(skip_all)]` on the generic function even though it created redundant nested spans. In reality, the guard only prevents the dangerous `skip()` denylist pattern.

---

### DRY Extraction Triggers Observability Review for Target/Span Changes
**Discovered**: 2026-02-12 (TD-13 health checker extraction)
**Coordination**: Observability + DRY Reviewer

When DRY reviewer identifies extraction opportunities, always involve Observability early in planning. Extracting shared logic from service-specific functions can silently change log targets (from explicit `target:` literals to default `module_path!()`) and span hierarchy. These changes affect: (a) `EnvFilter`-based log filtering, (b) dashboard/alert queries that filter by target, (c) trace hierarchy in distributed tracing. The TD-13 extraction changed inner-loop log targets from `gc.task.health_checker` to `global_controller::tasks::generic_health_checker` -- which turned out to be an improvement (fixing a silent filtering bug), but could have been a regression in other contexts.

---
