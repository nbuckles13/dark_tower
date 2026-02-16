# Operations Integration Notes

*Captures non-obvious coordination points between Operations and other specialists. Only add entries for things that broke or surprised you during cross-specialist work.*

---

## Verify claims before blocking — coordinate with observability (TD-13, 2026-02-12)

When raising log target concerns, cross-check with the observability reviewer. In TD-13, I raised a MAJOR finding about log target drift that turned out to be invalid for two reasons: (1) tracing's `target:` requires const expressions, making the proposed fix impossible, and (2) the original custom targets were already invisible under the default EnvFilter. The observability reviewer had already identified both issues. Lesson: before blocking on log/metric concerns, verify the technical constraint with observability and check the actual subscriber configuration in `main.rs`.

## Observability-driven dashboards require checking whether metrics are actually emitted, not just defined (gc-registered-mc-metrics, 2026-02-16)

When reviewing dashboard additions from the observability specialist, operations must cross-check whether metrics are actually wired to production call sites -- not just defined in `metrics.rs`. In this review, the observability specialist added panels for all metrics defined in code to satisfy a metrics-completeness guard, but 10 of the AC metrics were `#[allow(dead_code)]` and never emitted. The guard script (Step 4) only checks that dashboard queries reference metrics that exist in source code, but it cannot verify that the recording functions are actually called. This is an inherent limitation of static analysis on metrics. Operations caught this by reading the AC `metrics.rs` and noting the `dead_code` annotations. Lesson: when observability adds dashboard panels in bulk, spot-check a sample of metrics for `#[allow(dead_code)]` or grep for actual call sites.

## User feedback often simplifies what reviewers overcomplicated (TD-13 iter 2, 2026-02-12)

In iteration 1, six reviewers confirmed a plan involving `HealthCheckerConfig`, `display_name`, `entity_name`, and `#[instrument]` on the generic function. The user's single line of feedback — "HealthCheckerConfig feels like overkill, we could do `.instrument(...)` instead" — led to a cleaner iteration 2 that also resolved two tech debt items (nested spans, fragile `display_name` convention). Lesson: when the user pushes back on API complexity, take it seriously. Reviewers can converge on an overcomplicated design through incremental accommodation of each other's concerns. The user sees the result with fresh eyes.
