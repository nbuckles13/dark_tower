# Operations Gotchas

*Accumulates operational pitfalls discovered during deployment, incidents, and near-misses. Only add entries for things that actually caused outages, degradation, or wasted debugging time.*

---

## Tracing `target:` requires const expressions (TD-13, 2026-02-12)

The tracing crate's `info!`/`warn!`/`error!` macros place the `target:` value inside a `static __CALLSITE` initializer via `callsite2!`. This means `target:` must be a const-evaluable expression — you cannot pass a struct field like `config.log_target`. Only string literals or `const` values work. This matters when refactoring duplicated code with different log targets: you cannot parameterize the target at runtime. Instead, keep lifecycle logs with literal targets in wrapper functions and let the generic code use the default module-path target.

## Custom log targets are invisible under default EnvFilter (TD-13, 2026-02-12)

The GC default `EnvFilter` is `"global_controller=debug,tower_http=debug"`, which filters by target prefix. Custom targets like `gc.task.health_checker` do NOT match `global_controller` — they are silently invisible. Before raising log target drift as an issue during refactoring, verify whether the existing targets were actually reachable under the configured filter. In this case, switching to the default module-path target (`global_controller::tasks::*`) was a net improvement because it made the logs visible.

## Nested `#[instrument]` spans are easy to miss in generic extraction (TD-13, 2026-02-12)

When extracting a generic function from wrapper functions that each have `#[instrument(skip_all, name = "...")]`, putting `#[instrument(skip_all)]` on the generic function too creates a nested span that is invisible during normal operation but inflates trace storage. In TD-13 iteration 1, this was retained because the `instrument-skip-all` guard required it. Iteration 2 resolved this by removing `#[instrument]` from the generic function entirely and having callers chain `.instrument(tracing::info_span!(...))` on the future instead — satisfying the guard at the call site while keeping a single span per invocation. If you see `#[instrument]` on both a wrapper and its delegate, check whether the nesting is intentional.
