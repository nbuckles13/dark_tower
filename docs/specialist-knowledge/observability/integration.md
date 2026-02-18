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
**Updated**: 2026-02-16 (mc-token-metrics — added MC/token refresh buckets)
**Related files**: `crates/gc-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`

Histogram buckets must align with SLO targets to enable accurate percentile measurement. Buckets are configured at the **recorder** level via `PrometheusBuilder` with `Matcher::Prefix`, not at `histogram!` call sites:
- **HTTP requests**: Buckets around 200ms (p95 target) - [5ms, 10ms, 25ms, 50ms, 100ms, 200ms, 300ms, 500ms, 1s, 2s]
- **MC assignment**: Buckets around 20ms (p95 target) - [5ms, 10ms, 15ms, 20ms, 30ms, 50ms, 100ms, 500ms]
- **Database queries**: Buckets around 50ms (p99 target) - [1ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 1s]
- **Token refresh** (GC + MC): Buckets around 1s (p99 SLO <2s) - [10ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s]

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

### Dead-Code Metrics Require Dashboard Panels with Instrumentation Notes
**Discovered**: 2026-02-16 (AC dashboard coverage gaps, operations review)
**Coordination**: Observability + Operations
**Related files**: `crates/ac-service/src/observability/metrics.rs`, `infra/grafana/dashboards/ac-overview.json`

The `validate-application-metrics.sh` guard greps `counter!`/`histogram!`/`gauge!` macro invocations from `metrics.rs` files, regardless of whether the recording functions are wired to call sites. Functions marked `#[allow(dead_code)]` still register their metrics as "defined" and require dashboard coverage and catalog entries.

This creates a tension: the guard requires panels, but the panels will show "No data" until instrumentation is wired. The agreed approach is to add "Pending instrumentation." to the panel `description` field so on-call understands "No data" is expected, not a scrape failure. Operations strongly prefers this annotation to avoid wasted incident debugging time.

**AC service had 10 dead-code metrics as of 2026-02-16**: `ac_active_signing_keys`, `ac_signing_key_age_days`, `ac_key_rotation_last_success_timestamp`, `ac_token_validations_total`, `ac_rate_limit_decisions_total`, `ac_db_queries_total`, `ac_db_query_duration_seconds`, `ac_bcrypt_duration_seconds`, `ac_audit_log_failures_total`, `ac_credential_operations_total` (renamed from `ac_admin_operations_total`). **As of 2026-02-18, 9 of 10 are wired** and their "Pending instrumentation." notes removed. `ac_token_validations_total` remains partially wired (clock_skew errors only) with annotation "Partially instrumented (clock_skew errors only)."

**Lifecycle**: When wiring a dead-code metric, always: (1) remove `#[allow(dead_code)]` from the function, (2) remove "Pending instrumentation." from the dashboard panel description, (3) update catalog doc to replace "Status: Defined but not currently exported" with "Call Sites: ..." listing where it's used. For partially wired metrics, update the annotation to describe what IS wired (e.g., "Partially instrumented (clock_skew errors only).").

---

### Env-Test Observability Assertions Must Test Content, Not Just Reachability
**Discovered**: 2026-02-18 (env-tests fix)
**Updated**: 2026-02-18 (env-tests fix iteration 2 -- Loki JSON parsing)
**Coordination**: Observability + Test
**Related files**: `crates/env-tests/tests/30_observability.rs`

When reviewing env-tests that validate observability endpoints, distinguish between tests that verify *reachability* (HTTP 200 from `/metrics`) and tests that verify *content* (metric names, label structure, counter increments via Prometheus queries). Reachability-only tests can pass even when the metrics endpoint returns garbage or an empty response.

The existing `30_observability.rs` suite has good coverage on this spectrum:
- `test_ac_metrics_exposed`: Verifies content (`# TYPE` comments, `ac_token_issuance_total` metric name)
- `test_metrics_have_expected_labels`: Verifies label structure (`grant_type=`, `status=`)
- `test_token_counter_increments_after_issuance`: Verifies Prometheus scrape + counter increment (end-to-end)
- `test_logs_appear_in_loki`: Parses Loki JSON response, checks `status == "success"` and non-empty `result` array

**Loki assertion improvement**: The Loki test originally used `body.contains("ac-service")` -- a string match on the raw HTTP response body. This was replaced with proper JSON parsing (`serde_json::Value`) that checks the structured response fields. String matching on API responses is fragile: an error response containing "ac-service" in an error message would pass, and a valid response with different JSON formatting might fail. Always parse structured API responses rather than string-matching them.

A removed test (`test_logs_have_trace_ids`) demonstrated the anti-pattern: it queried Loki but never asserted on the result -- every code path was a warning or no-op. Tests that never fail provide false confidence and should be removed or converted to real assertions. When OpenTelemetry trace propagation is implemented, the replacement test should use `assert!` on trace ID presence, not `eprintln!`.

---

### DRY Extraction Triggers Observability Review for Target/Span Changes
**Discovered**: 2026-02-12 (TD-13 health checker extraction)
**Coordination**: Observability + DRY Reviewer

When DRY reviewer identifies extraction opportunities, always involve Observability early in planning. Extracting shared logic from service-specific functions can silently change log targets (from explicit `target:` literals to default `module_path!()`) and span hierarchy. These changes affect: (a) `EnvFilter`-based log filtering, (b) dashboard/alert queries that filter by target, (c) trace hierarchy in distributed tracing. The TD-13 extraction changed inner-loop log targets from `gc.task.health_checker` to `global_controller::tasks::generic_health_checker` -- which turned out to be an improvement (fixing a silent filtering bug), but could have been a regression in other contexts.

---
