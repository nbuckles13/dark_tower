# Observability Patterns

*Accumulates non-obvious observability patterns learned through implementation. Don't restate ADR-0011 — capture what worked that wasn't obvious, or deviations from the ADR that were necessary.*

---

## Pattern: Two-Runbook Structure per Service (ADR-0011)
**Added**: 2026-02-06
**Related files**: `docs/runbooks/gc-deployment.md`, `docs/runbooks/gc-incident-response.md`, `docs/runbooks/ac-service-deployment.md`, `docs/runbooks/ac-service-incident-response.md`

Each service has exactly two comprehensive runbooks:
1. **Deployment runbook**: Pre-deployment checklist, deployment steps, rollback, configuration reference, common deployment issues, smoke tests
2. **Incident response runbook**: Severity classification, escalation paths, numbered failure scenarios with anchor IDs, diagnostic commands, postmortem template

Alerts link to specific scenarios via anchors (e.g., `gc-incident-response.md#scenario-1-database-connection-failures`). This consolidates operational knowledge and ensures alerts always point to relevant context.

---

## Pattern: Cardinality-Safe PromQL with sum by()
**Added**: 2026-02-06
**Related files**: `infra/grafana/dashboards/gc-overview.json`, `infra/grafana/dashboards/gc-slos.json`

All dashboard queries aggregate with `sum by(label)` to control cardinality:
```promql
sum by(endpoint) (rate(gc_http_requests_total[5m]))
```
Labels used: `endpoint` (normalized paths), `status_code` (HTTP codes), `operation` (CRUD), `success` (bool). Never use unbounded labels (user_id, meeting_id, UUIDs). Target max 1,000 unique label combinations per metric per ADR-0011.
### Hybrid Observability Pattern for Generic/Shared Functions
**Learned**: 2026-02-12 (TD-13 health checker extraction)
**Refined**: 2026-02-12 (TD-13 iteration 2 -- `.instrument()` chaining)

When extracting shared logic from service-specific functions, use a hybrid approach for observability:

1. **Wrapper functions** (service-specific): No `#[instrument]` attribute. Instead, chain `.instrument(tracing::info_span!("gc.task.health_checker"))` on the generic function call. Emit lifecycle logs (start/stop) with hardcoded `target:` string literals outside the `.instrument()` span.
2. **Generic function** (shared): No `#[instrument]` attribute. Uses default `module_path!()` target (no explicit `target:`) and includes an `entity` structured field on all log events for programmatic filtering (e.g., `entity = "controllers"` vs `entity = "handlers"`).
3. **Span inheritance**: The generic function's log events automatically inherit the caller's `.instrument()` span context, so traces associate correctly without needing a custom target.

This pattern works because: (a) `target:` requires compile-time constants so it can't be parameterized at runtime, (b) `.instrument(info_span!("..."))` allows ADR-0011-compliant span names without `#[instrument]` on the function signature, (c) structured fields like `entity` accept runtime values and enable equivalent filtering in log aggregation systems, (d) exactly one span is created per call path (no nested spans).

**Why `.instrument()` chaining over `#[instrument]` on wrappers**: Using `#[instrument(skip_all, name = "...")]` on the wrapper AND `#[instrument(skip_all)]` on the generic function creates redundant nested spans. Removing `#[instrument]` from the generic function alone would work, but the wrapper's `#[instrument]` then instruments a function whose only real work is calling the generic function -- adding attribute weight for minimal benefit. The `.instrument()` approach is more precise: the span covers exactly the long-running generic loop, not the lightweight wrapper setup/teardown.

**Lifecycle log placement**: Start/stop logs emitted in the wrapper (before/after the `.instrument().await`) are outside the span. This is acceptable because they carry explicit `target:` literals for identification and don't need span correlation -- they mark boundaries, not ongoing activity.

---

### Structured Fields as Runtime Differentiators
**Learned**: 2026-02-12 (TD-13 health checker extraction)

When `target:` and `#[instrument(name = ...)]` can't be parameterized (compile-time constraint), add a structured field (e.g., `entity = entity_name`) to all log events in shared code. This enables programmatic filtering in log aggregation tools (Loki, CloudWatch Logs Insights, etc.) even when the log target is shared. Include the entity name in both the structured field AND the message text for both human and machine readability.

**Simplification note** (iteration 2): Prefer passing the structured field as a plain `&'static str` parameter rather than wrapping it in a config struct. A config struct adds indirection without benefit when the only field is a single differentiator string. If multiple observability-related fields are needed in the future, a struct may be warranted, but start simple.

---
