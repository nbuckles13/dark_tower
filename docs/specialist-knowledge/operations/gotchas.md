# Operations Gotchas

*Accumulates operational pitfalls discovered during deployment, incidents, and near-misses. Only add entries for things that actually caused outages, degradation, or wasted debugging time.*

---

## Tracing `target:` requires const expressions (TD-13, 2026-02-12)

The tracing crate's `info!`/`warn!`/`error!` macros place the `target:` value inside a `static __CALLSITE` initializer via `callsite2!`. This means `target:` must be a const-evaluable expression — you cannot pass a struct field like `config.log_target`. Only string literals or `const` values work. This matters when refactoring duplicated code with different log targets: you cannot parameterize the target at runtime. Instead, keep lifecycle logs with literal targets in wrapper functions and let the generic code use the default module-path target.

## Custom log targets are invisible under default EnvFilter (TD-13, 2026-02-12)

The GC default `EnvFilter` is `"global_controller=debug,tower_http=debug"`, which filters by target prefix. Custom targets like `gc.task.health_checker` do NOT match `global_controller` — they are silently invisible. Before raising log target drift as an issue during refactoring, verify whether the existing targets were actually reachable under the configured filter. In this case, switching to the default module-path target (`global_controller::tasks::*`) was a net improvement because it made the logs visible.

## Cross-cutting vs per-subsystem metric operation labels use different naming (gc-token-metrics, 2026-02-15)

`gc_errors_total` is a cross-cutting error counter shared across all GC subsystems. Its `operation` label uses subsystem prefixes for global uniqueness: `"ac_meeting_token"`, `"ac_guest_token"`, `"mc_grpc"`. But per-subsystem metrics like `gc_ac_requests_total` use unprefixed operations: `"meeting_token"`, `"guest_token"`. During incident response, if you're correlating `gc_errors_total{operation="meeting_token"}` you'll get zero results — the correct query is `gc_errors_total{operation="ac_meeting_token"}`. This convention is documented in the `record_error()` doc comment in `metrics.rs` but is easy to forget under pressure.

## NetworkPolicy cross-references span more services than you expect (service-rename, 2026-02-16)

When renaming a service's `app:` label, the obvious NetworkPolicy updates are the service's own policy and its direct peers (GC↔MC). But third-party services also have ingress rules referencing the renamed service. In this project, `infra/services/redis/network-policy.yaml` allows ingress only from `app: mc-service` — missing this during the MC rename would have silently broken Redis connectivity for MC pods. Checklist for service renames: grep ALL NetworkPolicy files for the old `app:` label, not just the renamed service's own directory. The zero-trust architecture means every allowed connection is an explicit rule that must be updated.

## Grafana dashboard Loki queries use app label selectors that mirror K8s labels (service-rename, 2026-02-16)

The errors-overview dashboard uses `{app=~"ac-service|gc-service|mc-service"}` to aggregate error logs across services. When renaming services, these Loki query regexes must be updated to match the new K8s pod `app:` labels. In this rename, a partial update occurred: `meeting-controller` was changed to `mc-service` but `global-controller` was left unchanged in the same regex on the same line. The fix is mechanical, but the failure mode is silent — the dashboard simply shows no GC errors, which looks like "healthy" rather than "broken query." Always verify Grafana JSON dashboards with a targeted grep after service renames.

## Metric prefix scoping makes `job` filters redundant but SLO dashboards apply them inconsistently (mc-token-metrics, 2026-02-16)

All Dark Tower services use unique metric prefixes (`gc_`, `mc_`, `ac_`), which means metric names alone disambiguate services in a shared Prometheus instance. Adding `{job="gc-service"}` to queries is defense-in-depth, not functionally necessary. However, the SLO dashboards apply `job` filters inconsistently: MC SLO panels include `{job="mc-service"}` on all queries, while GC SLO panels omit `job` filters entirely (relying on the `gc_` prefix). During incident response, this inconsistency can waste time — an engineer copying a GC SLO query to investigate MC will get unexpected results because the query patterns don't match. When reviewing new SLO dashboard panels, check whether `job` filter usage matches the existing convention for that specific dashboard, not the other service's dashboard.

## Nested `#[instrument]` spans are easy to miss in generic extraction (TD-13, 2026-02-12)

When extracting a generic function from wrapper functions that each have `#[instrument(skip_all, name = "...")]`, putting `#[instrument(skip_all)]` on the generic function too creates a nested span that is invisible during normal operation but inflates trace storage. In TD-13 iteration 1, this was retained because the `instrument-skip-all` guard required it. Iteration 2 resolved this by removing `#[instrument]` from the generic function entirely and having callers chain `.instrument(tracing::info_span!(...))` on the future instead — satisfying the guard at the call site while keeping a single span per invocation. If you see `#[instrument]` on both a wrapper and its delegate, check whether the nesting is intentional.
