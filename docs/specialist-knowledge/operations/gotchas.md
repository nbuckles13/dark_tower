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

## Dashboard panel descriptions must track metric wiring state in both directions (gc-registered-mc-metrics, 2026-02-16, updated 2026-02-17)

Dashboard panel descriptions that say "Pending instrumentation" serve as on-call context when metrics show "No data" — but they must be removed when metrics get wired. In this project, 9 AC metrics were wired to production call sites but the dashboard descriptions still said "Pending instrumentation." On-call would see the metric emitting data alongside a description claiming it's not wired, eroding trust in dashboard documentation. The operational risk runs both ways: (1) unwired metrics without the label cause wasted debugging ("is scrape broken or is this expected?"), and (2) wired metrics retaining the label cause distrust in all panel descriptions ("if this one is wrong, which others are wrong?"). When wiring previously dead-code metrics, always grep dashboard JSON for stale description text. Only metrics that are genuinely unwired (like `ac_token_validations_total` which retains `#[allow(dead_code)]`) should carry the label.

## Transaction vs non-transaction code paths can silently diverge on instrumentation (ac-metrics-instrumentation, 2026-02-17)

When a service has both a transactional (`_tx`) and non-transactional version of the same operation, gauge updates added to one path may be missing from the other. In ac-service, `rotate_signing_key` (non-tx, used by background rotation) sets `ac_active_signing_keys`, `ac_signing_key_age_days`, and `ac_key_rotation_last_success_timestamp` after rotation. But `rotate_signing_key_tx` (tx, used by the admin API's `handle_rotate_keys`) did NOT set those gauges. The admin API rotation path left dashboard gauges stale until the next service restart triggered `init_key_metrics()`. This was made worse because a code-reviewer finding to remove "duplicate" gauge updates from the handler actually removed the only gauge updates for the tx path — the gauges looked duplicated but were covering different call chains. When reviewing instrumentation removal or consolidation, trace every caller of the function being instrumented and verify each call path still has coverage.

## Loki query_range without explicit start/end uses server defaults that miss recent logs (env-tests-fix iter 2, 2026-02-18)

The `test_logs_appear_in_loki` test was calling `/loki/api/v1/query_range?query={app="ac-service"}&limit=100` without `start` and `end` parameters. Loki's default time range when these are omitted is implementation-dependent and may not include the last few seconds/minutes of ingested logs. The fix is to compute `start` and `end` as nanosecond-epoch timestamps (Loki's native format) covering the last 5 minutes. The non-obvious detail: the time window must be recomputed on each `assert_eventually` retry (not captured once before the loop), because a static window computed at loop start would drift as retries consume the Promtail flush interval (~10s). The current implementation correctly computes timestamps inside the closure, so the window slides forward with each attempt.

## Silent test skips mask URL mismatches that would fail in a real cluster (env-tests-fix, 2026-02-18)

The env-tests GC client fixtures used `/v1/health`, `/v1/me`, `/v1/meetings/{code}` while the actual GC routes (in `crates/gc-service/src/routes/mod.rs`) are `/health`, `/api/v1/me`, `/api/v1/meetings/:code`. This mismatch was invisible because: (1) tests used `if !cluster.is_gc_available() { return; }` which silently skipped when GC wasn't deployed, and (2) the GC health check itself used the wrong URL (`/v1/health`), so `is_gc_available()` returned false even when GC was running, triggering the skip path. The result: every GC-dependent test appeared to pass (via skip) when it was actually untested. When reviewing test fixtures that target a specific service's API, always cross-reference URLs against the service's route definitions (`routes/mod.rs`), not against doc comments or other test files that may have copied the same wrong URL. Comments propagate mistakes; source code is the authority.

## Seeded test credential scopes must match test requests — silent downscoping hides failures (env-tests-fix iter 2, 2026-02-18)

The `test-client` credential is seeded in `setup.sh` with `ARRAY['test:all']` as its only allowed scope. But `test_multiple_tokens_validated` in `21_cross_service_flows.rs` was requesting `test:scope0`, `test:scope1`, `test:scope2` — scopes that don't exist in the seeded data. This never failed because: (1) the per-test `is_gc_available()` skip hid the fact that GC was unreachable (wrong health URL), and (2) even if GC were reachable, the AC token endpoint behavior on unrecognized scopes depends on implementation (some implementations silently downscope, others reject). The fix was changing the test to request `test:all`. When reviewing env-tests that use seeded credentials, always cross-reference the requested scopes against the `seed_test_data()` function in `setup.sh` to verify the test is requesting scopes that actually exist.

## Nested `#[instrument]` spans are easy to miss in generic extraction (TD-13, 2026-02-12)

When extracting a generic function from wrapper functions that each have `#[instrument(skip_all, name = "...")]`, putting `#[instrument(skip_all)]` on the generic function too creates a nested span that is invisible during normal operation but inflates trace storage. In TD-13 iteration 1, this was retained because the `instrument-skip-all` guard required it. Iteration 2 resolved this by removing `#[instrument]` from the generic function entirely and having callers chain `.instrument(tracing::info_span!(...))` on the future instead — satisfying the guard at the call site while keeping a single span per invocation. If you see `#[instrument]` on both a wrapper and its delegate, check whether the nesting is intentional.
