# Observability Gotchas

*Accumulates observability pitfalls discovered during implementation. Only add entries for things that actually caused bugs, silent failures, or wasted debugging time.*

---

## Gotcha: Health Endpoint Changes Break Tests in Multiple Locations
**Added**: 2026-02-06
**Related files**: `crates/global-controller/tests/auth_tests.rs`, `crates/global-controller/tests/health_tests.rs`, `crates/gc-test-utils/src/server_harness.rs`

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
