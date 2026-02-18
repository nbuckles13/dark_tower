# Observability Gotchas

*Accumulates observability pitfalls discovered during implementation. Only add entries for things that actually caused bugs, silent failures, or wasted debugging time.*

---

## Gotcha: Health Endpoint Changes Break Tests in Multiple Locations
**Added**: 2026-02-06
**Related files**: `crates/gc-service/tests/auth_tests.rs`, `crates/gc-service/tests/health_tests.rs`, `crates/gc-test-utils/src/server_harness.rs`

When `/health` endpoint behavior changes (e.g., from JSON to plain text "OK" for Kubernetes liveness probes), tests in multiple locations may break:
1. Test utilities (`*-test-utils/src/server_harness.rs`)
2. Integration tests (`crates/*/tests/*.rs`)
3. Any test that calls the endpoint

Search for all usages: `grep -r "/health" crates/*/tests/`. The `/ready` endpoint returns JSON with detailed health status; use that for tests requiring structured responses.

---

## Gotcha: Grafana Datasource UID Must Match Provisioned Configuration
**Added**: 2026-02-06
**Related files**: `infra/grafana/dashboards/*.json`, `infra/grafana/provisioning/datasources/prometheus.yaml`

Dashboard panels reference datasource by UID, not name:
```json
"datasource": { "type": "prometheus", "uid": "prometheus" }
```
The UID must match the datasource provisioning configuration. The `grafana-datasources.sh` guard validates this. If adding new datasources, update both the provisioning config and reference it correctly in dashboards.

---

## Gotcha: Alert Duration Tuning Affects False Positive Rate
**Added**: 2026-02-06
**Related files**: `infra/docker/prometheus/rules/gc-alerts.yaml`

The `for:` duration in alerts controls how long the condition must be true before firing:
- Too short (1m): False positives from transient spikes
- Too long (15m): Delayed detection of real issues

Guidelines:
- Critical alerts: 1-5 minutes (fast detection, accept some noise)
- Warning alerts: 5-10 minutes (reduce noise for non-urgent issues)

Tune based on production data after deployment. Initial values are estimates.
### Tracing `target:` Requires Compile-Time Constants
**Discovered**: 2026-02-12 (TD-13 health checker extraction)

The `target:` parameter in tracing macros (`info!`, `warn!`, `error!`, `event!`) must be a **compile-time constant** — a string literal or `const` value. Struct field accesses like `config.log_target` fail with `error[E0435]: attempt to use a non-constant value in a constant`, even if the field is `&'static str`. This is because tracing macros expand into `static` callsite definitions that require compile-time evaluation. The same restriction applies to `#[instrument(name = ...)]` — the `name` parameter also requires a string literal.

**Impact**: Any generic/shared function that needs service-specific log targets cannot use runtime configuration for `target:`. Two workarounds: (1) use `macro_rules!` to splice literal tokens, or (2) use the default `module_path!()` target and rely on `#[instrument]` span names on calling functions for differentiation.

---

### Custom Dot-Separated Log Targets vs EnvFilter Module Paths
**Discovered**: 2026-02-12 (TD-13 health checker extraction)

Custom log targets using dot separators (e.g., `target: "gc.task.health_checker"`) do NOT match `EnvFilter` directives based on Rust module paths (e.g., `"global_controller=debug"`). `EnvFilter` uses `::` as the hierarchy separator, matching the Rust module path convention. A target of `gc.task.health_checker` is in a completely different namespace from `global_controller::tasks::health_checker`.

**Impact in Dark Tower**: All GC background task logs using `target: "gc.task.*"` are **silently filtered out** under the default `EnvFilter` (`"global_controller=debug,tower_http=debug"`). This affects `health_checker.rs`, `mh_health_checker.rs`, and `assignment_cleanup.rs`. The logs only appear if someone explicitly sets `RUST_LOG=gc.task.health_checker=debug`. This means startup/shutdown lifecycle logs for background tasks have never been visible under default configuration.

**Recommendation**: Either (a) add `gc=debug` to the default filter directive, or (b) stop using custom dot-separated targets and let logs use the default `module_path!()` target, which naturally falls under the `global_controller` hierarchy.

---

### `histogram!` Macro Does Not Accept Bucket Configuration
**Discovered**: 2026-02-15 (gc-token-metrics)

The `metrics` crate's `histogram!` macro only records values -- it does NOT accept bucket configuration. Bucket boundaries are configured at the **recorder** level (e.g., `metrics_exporter_prometheus::PrometheusBuilder`), not at the call site. Suggesting SLO-aligned bucket values as code changes to `histogram!()` calls is incorrect; bucket tuning belongs in the recorder setup or Prometheus/Grafana configuration.

**Impact**: When reviewing metrics code, do not flag missing bucket configuration on `histogram!` calls. Instead, note bucket requirements for the recorder setup or dashboard configuration phase.

---

### MC `status_code` Label Uses Signaling Codes, Not HTTP Codes
**Discovered**: 2026-02-16 (mc-token-metrics)
**Related files**: `crates/mc-service/src/errors.rs`, `crates/mc-service/src/observability/metrics.rs`

When mirroring metrics patterns from GC to MC, the `status_code` label in `mc_errors_total` uses WebTransport signaling error codes (2-7), NOT HTTP status codes (4xx/5xx). GC uses HTTP status codes because it's an HTTP/3 API gateway, but MC communicates via WebTransport signaling and maps errors to `ErrorCode` enum values: UNAUTHORIZED(2), FORBIDDEN(3), NOT_FOUND(4), CONFLICT(5), INTERNAL_ERROR(6), CAPACITY_EXCEEDED(7).

**Impact**: Tests and dashboard queries for MC error metrics must use signaling code values. A test using `record_error("meeting_join", "capacity_exceeded", 429)` is wrong — the correct call is `record_error("meeting_join", "capacity_exceeded", 7)`. Similarly, Grafana panels filtering MC errors by status_code should use `status_code="6"` not `status_code="500"`.

---

### `mod.rs` Re-Exports Go Stale Silently When New Metric Functions Are Added
**Discovered**: 2026-02-16 (mc-token-metrics)
**Related files**: `crates/mc-service/src/observability/mod.rs`, `crates/gc-service/src/observability/mod.rs`

Each service's `observability/mod.rs` re-exports all public metric functions via `pub use metrics::{...}`. When new functions are added to `metrics.rs` (e.g., `record_token_refresh`, `record_error`), the re-export list in `mod.rs` must be updated manually. If forgotten, the build does NOT fail because callers can use full module paths (e.g., `mc_service::observability::metrics::record_token_refresh`). However, this breaks the convention that all metric functions are available directly from `mc_service::observability::*`.

**Why it's silent**: `main.rs` and other callers tend to use the full path to the `metrics` submodule rather than the re-exported shorthand, so missing re-exports cause no compilation error. The gap is only visible during code review or when someone tries to use the re-exported path.

**Prevention**: When adding new public functions to any `observability/metrics.rs`, always check and update the corresponding `observability/mod.rs` re-export list and the module doc comment table.

---

### Guard Metric Extraction Treats Histogram Suffixes as Distinct Metrics
**Discovered**: 2026-02-16 (dashboard coverage gaps)
**Related files**: `scripts/guards/simple/validate-application-metrics.sh`, `infra/grafana/dashboards/*.json`

The `validate-application-metrics.sh` guard extracts metric names from dashboard PromQL using `grep -oP '\b(ac|gc|mc|mh)_[a-z_]+'`. When a dashboard references `histogram_quantile(0.95, rate(foo_bucket[5m]))`, the regex extracts `foo_bucket` -- NOT the base metric name `foo` defined in source code. Without suffix stripping, every histogram metric referenced only via `_bucket`/`_count`/`_sum` in dashboards appears uncovered, inflating gap counts.

The guard was updated to strip `_bucket`, `_count`, and `_sum` suffixes and register base metric names. When adding new histogram panels, the PromQL will naturally reference `_bucket` variants, and the guard now correctly maps those back to the source metric. No special PromQL workarounds are needed.

---

### Cross-Cutting Error Counters Use Subsystem-Prefixed Operation Labels
**Discovered**: 2026-02-16 (gc_errors_total dashboard panel, operations review)
**Related files**: `crates/gc-service/src/observability/metrics.rs`, `infra/grafana/dashboards/gc-overview.json`

The `gc_errors_total` cross-cutting error counter uses subsystem-prefixed operation labels (e.g., `ac_meeting_token`, `ac_guest_token`, `mc_grpc`), while per-subsystem metrics like `gc_ac_requests_total` use unprefixed operation names (`meeting_token`, `guest_token`). This inconsistency means correlating errors across metrics requires knowing which prefix convention applies to which metric.

**Impact**: During incidents, on-call may filter `gc_errors_total{operation="meeting_token"}` and get zero results when the correct value is `operation="ac_meeting_token"`. Always document the label convention in panel descriptions and catalog entries for cross-cutting error counters.

---

### Gauge Metrics Reset to Zero on Service Restart
**Discovered**: 2026-02-17 (ac-metrics-instrumentation)
**Related files**: `crates/ac-service/src/services/key_management_service.rs`, `crates/ac-service/src/main.rs`

Prometheus gauges (e.g., `ac_active_signing_keys`, `ac_signing_key_age_days`, `ac_key_rotation_last_success_timestamp`) reset to zero when the service restarts. If gauges are only updated on events (e.g., key rotation), they stay at zero until the next event occurs -- which could be days or weeks.

**Impact**: On-call sees `ac_signing_key_age_days=0` and interprets it as "fresh key" when the key is actually 90 days old. `ac_active_signing_keys=0` looks like no keys exist. Both would trigger false alerts.

**Fix**: Always initialize gauges at startup by querying current state from the database (or other source of truth). The pattern is: (1) startup query sets initial gauge values, (2) event-driven updates keep them current thereafter. No periodic refresh needed -- just startup + events.

**Example**: `key_management_service::init_key_metrics()` queries active keys from DB at startup and sets all three key management gauges. Called from `main.rs` after `initialize_signing_key()` with non-fatal error handling.

---

### Silent Test Skips Mask Observability Endpoint Drift
**Discovered**: 2026-02-18 (env-tests fix)
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`, `crates/env-tests/src/cluster.rs`, `crates/env-tests/tests/21_cross_service_flows.rs`, `crates/env-tests/tests/22_mc_gc_integration.rs`

Env-test fixtures that check service availability before running tests (`if !cluster.is_gc_available().await { println!("SKIPPED"); return; }`) can silently mask broken test infrastructure. The GC client fixture had wrong URL paths (`/v1/health` instead of `/health`, `/v1/meetings/{code}` instead of `/api/v1/meetings/{code}`) for an unknown period. Because every test checked GC availability first and silently skipped on failure, the URL mismatch was never surfaced -- the health check itself was hitting the wrong endpoint, getting a 404, and causing the skip.

**Why this is an observability concern**: Tests in `30_observability.rs` that validate Prometheus scraping and metrics endpoints depend on the same `ClusterConnection` infrastructure. If the cluster fixture silently fails, observability validation tests silently skip too. A full green test suite gives false confidence that metrics endpoints, Prometheus scraping, and log aggregation are validated when they may not have been exercised at all.

**Fix applied**: GC availability is now checked once in the `cluster()` helper with a hard `expect()` failure. Only truly optional infrastructure (Loki) retains soft-skip behavior, because `loki: Option<u16>` in `ClusterPorts` makes it explicitly optional.

**Prevention**: When reviewing env-tests, treat any `eprintln!("SKIPPED..."); return;` pattern as suspicious. Ask: "Is this dependency truly optional, or should the test fail to alert that the test environment is broken?" Reserve soft-skips only for infrastructure declared as `Option<T>` in the cluster connection.

---

### Loki `query_range` Without Time Bounds Returns Empty Results Silently
**Discovered**: 2026-02-18 (env-tests Loki fix, iteration 2)
**Related files**: `crates/env-tests/tests/30_observability.rs`

Loki's `/loki/api/v1/query_range` endpoint does NOT fail when `start` and `end` parameters are omitted -- it uses a server-side default window that may be extremely narrow or may not cover recently ingested logs. The test `test_logs_appear_in_loki` was timing out at 20s despite logs being visible in Grafana dashboards querying the same `{app="ac-service"}` label. The root cause was that Grafana sends explicit time bounds (from the dashboard time picker), while the test omitted them entirely.

**Why it's silent**: The Loki API returns HTTP 200 with `{"status":"success","data":{"result":[]}}` -- a valid empty result, not an error. The test's retry loop interpreted this as "logs not yet available" and kept retrying until timeout.

**Fix**: Always provide explicit `start` and `end` parameters on `query_range`. Use nanosecond Unix epoch timestamps with a lookback window generous enough to cover Promtail batching + Loki ingestion (5 minutes works well):
```rust
let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
let start = now.saturating_sub(5 * 60 * 1_000_000_000);
format!("...query_range?query={{app=\"ac-service\"}}&start={}&end={}&limit=100", start, now)
```

**Prevention**: When reviewing any Loki API call in tests or tooling, check for explicit time bounds. Treat `query_range` without `start`/`end` the same way you'd treat a SQL query without a `WHERE` clause on a time-partitioned table.

---

### Multi-Value Regex Fields Survive Partial Renames
**Discovered**: 2026-02-16 (service rename: global-controller -> gc-service, meeting-controller -> mc-service)

Grafana dashboard queries that match multiple services in a single regex (e.g., `{app=~"ac-service|global-controller|meeting-controller"}`) are vulnerable to partial renames. During the service rename, `meeting-controller` was correctly updated to `mc-service` in `errors-overview.json` line 418, but `global-controller` in the same regex was missed. A simple find-and-replace for one old name does not guarantee the adjacent old name in the same field was also caught.

**Why it happens**: When doing bulk renames, each old name is typically searched/replaced independently. If the regex contains multiple old names on a single line, each replacement pass must independently match its target -- but a reviewer (or automated tool) checking "did we rename all `meeting-controller` references?" will see this line as clean, while a check for `global-controller` might also pass if the tool stops at the first match per line.

**Prevention**: After any service rename, grep the entire `infra/grafana/` directory for ALL old names simultaneously (e.g., `grep -E "global-controller|meeting-controller|media-handler"`), not one at a time. Pay special attention to multi-service dashboards like `errors-overview.json` that aggregate across services.

---
