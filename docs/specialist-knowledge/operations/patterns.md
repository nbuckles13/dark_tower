# Operations Patterns

*Accumulates non-obvious operational patterns learned through deployment and incident response. Don't describe what the K8s manifests say — capture what surprised you, what broke in practice, what the YAML doesn't tell you.*

---

## Health checker operational invariants checklist (TD-13, 2026-02-12)

When reviewing health checker refactoring, verify these five behaviors are preserved:
1. **Check interval**: Default 5s via `DEFAULT_CHECK_INTERVAL_SECONDS`
2. **Graceful shutdown**: `CancellationToken` + `tokio::select!` — must complete current iteration before exit. The 2-second timeout in `main.rs` depends on this.
3. **Error resilience**: On DB error, log and continue (never crash the loop). Transient DB issues must not kill the health checker.
4. **Tracing spans**: Caller-side `.instrument(tracing::info_span!(...))` on the generic future, or `#[instrument(skip_all, name = "...")]` on wrapper entry-point functions. Prefer `.instrument()` chaining when the generic function should not own its span.
5. **Spawn compatibility**: Function must take only owned/Copy/Clone types (no borrowed references) to be compatible with `tokio::spawn`.

## Structured fields enable filtering when target is not parameterizable (TD-13, 2026-02-12)

When a generic function cannot use different log targets per caller (due to tracing's const requirement), adding structured fields like `entity = entity_name` provides an alternative for programmatic log filtering in aggregation systems (e.g., filter by `entity="controllers"` vs `entity="handlers"`).

## Prefer `.instrument()` chaining over `#[instrument]` on generic functions (TD-13 iter 2, 2026-02-12)

When a generic/shared async function is called by multiple wrapper functions, use `.instrument(tracing::info_span!("caller.specific.name"))` at the call site instead of `#[instrument(skip_all)]` on the generic function. Benefits: (1) avoids nested spans (the wrapper's `#[instrument]` + the generic's `#[instrument]` created redundant nesting in iteration 1), (2) caller controls span naming without the generic function needing to know its context, (3) cleaner separation — the generic function is span-agnostic. This pattern is particularly useful for background tasks where the span name should reflect the specific entity being checked, not the generic loop.

## Config structs can be overkill for simple parameterization (TD-13 iter 2, 2026-02-12)

When a config struct has only 1-2 fields and no validation logic, consider using direct function parameters instead. In TD-13 iteration 1, `HealthCheckerConfig { display_name, entity_name }` added a struct, a constructor, and an import — all for two `&'static str` values. Iteration 2 replaced this with a single `entity_name: &'static str` parameter, eliminating `display_name` entirely (the uniform shutdown log message with an `entity` structured field was operationally equivalent). The simpler API was easier to review and had fewer failure modes (no fragile trailing-space conventions like `"MH "` vs `""`).
