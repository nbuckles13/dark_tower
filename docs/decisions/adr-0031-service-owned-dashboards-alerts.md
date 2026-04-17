# ADR-0031: Service Specialists Own Dashboards and Alert Rules

## Status

Accepted

## Context

Dark Tower's `/user-story` workflow previously decomposed observability work across two phases per story:

- **Phase 4 (service specialist)**: add metrics to service code (`crates/<svc>/src/observability/metrics.rs`)
- **Phase 5 (observability specialist)**: add Grafana dashboards (`infra/grafana/dashboards/`) and Prometheus alert rules (`infra/docker/prometheus/rules/`)

This split was observed to produce consistently unplanned work:

1. **Guards force atomicity.** `scripts/guards/simple/validate-application-metrics.sh` requires every metric defined in code to appear in a Grafana dashboard panel and in the metric catalog (`docs/observability/metrics/<svc>.md`). `scripts/guards/simple/validate-kustomize.sh` enforces R-20 bidirectional dashboard↔kustomize coverage. A Phase 4 PR that adds a metric without a corresponding dashboard panel fails CI — so Phase 4 cannot complete without Phase 5's artifacts.

2. **Phase 5 becomes fiction.** Devloops routinely ship dashboards + alerts under the Phase 4 task name (because CI forces it), leaving Phase 5 empty or redundant.

3. **Handoff tax without handoff benefit.** Alert thresholds are derived from service-internal SLOs, invariants, and error-enum shapes that already live with the service specialist. The observability implementer had to interview the service specialist to set thresholds, who would then re-review the PR — a round trip for information that didn't change hands meaningfully.

4. **Gaps fall through the cracks.** MH has `mh-overview.json` but no `mh-alerts.yaml` (GC and MC both have alert files) — exactly the failure mode expected when alert authorship is deferred to a separate phase that may never be scheduled.

5. **ADR-0024's cross-cutting reviewer model already places observability in every devloop.** The two-phase split duplicated observability's involvement: once as a separate implementer, once as a cross-cutting reviewer.

The question: should service specialists (ac/gc/mc/mh) own per-service dashboards and alert rules as part of their Phase 4 task, with observability serving as a mandatory cross-cutting reviewer instead of a separate implementer?

## Decision

**Collapse metrics + per-service dashboards + per-service alert rules into a single service-specialist task within Phase 4.** Eliminate the dedicated observability-implementer Phase 5 from the `/user-story` template. Observability becomes a mandatory cross-cutting reviewer (alongside security, test, operations, code-reviewer per ADR-0024 §5), not a separate implementer.

### Ownership split

| Artifact | Owner (implementer) |
|----------|---------------------|
| `crates/<svc>/src/observability/metrics.rs` | Service specialist |
| `docs/observability/metrics/<svc>.md` | Service specialist |
| `infra/grafana/dashboards/<svc>-overview.json` | Service specialist |
| `infra/docker/prometheus/rules/<svc>-alerts.yaml` | Service specialist |
| `infra/grafana/dashboards/errors-overview.json` (cross-service) | Observability |
| Fleet-level SLO views, logs-aggregation dashboards | Observability |
| Conventions set (`dashboard-conventions.md`, `alert-conventions.md`, `label-taxonomy.md` — see Prerequisite guardrails #4) | Observability |
| Guard infrastructure for metrics/dashboards/alerts | Observability |
| `docs/runbooks/<svc>/` | Operations |
| Alert severity routing + SLO budget allocation | Operations |

### Mandatory cross-cutting review

On every service-owned metrics/dashboard/alert PR, the following cross-cutting specialists review (per ADR-0024):

- **Observability**: dashboard layout, panel classification per ADR-0029, PromQL hygiene, alert threshold plausibility, SLO linkage, cardinality budgets, conventions conformance.
- **Operations**: alert severity routing, runbook linkage, SLO budget impact, annotation hygiene.
- **Security**: metric label content (PII, denylist composition) — unchanged review surface per ADR-0011 line 133. Co-owns the label guard denylist with observability.
- **Test**: guard-gate passage, test coverage for metric recording paths.

**Cross-service coordination co-review**: when a metric observes cross-service coordination (e.g. MC↔MH dial/session/caller-type metrics), the counterparty service specialist is added as a required reviewer on the Phase 4 PR. This is a review-graph rule codified in `alert-conventions.md`, not a central pattern library — symmetry catches are domain specialists' expertise, not observability's.

**Review gate timing**: plan approval (not post-hoc). Thresholds, panel classifications, and alert shapes are agreed as part of the plan artifact before implementation, so review is collaborative rather than gatekeeping.

### Plan-template artifact

Plans for devloops that add or modify metrics/alerts MUST include a structured Alert Thresholds block reviewable at plan approval:

```markdown
## Alert thresholds (cross-cutting review required)

| Metric | Condition | For | Severity | Runbook |
|--------|-----------|-----|----------|---------|
| <metric_name> | <promql_condition> | <duration> | <page/ticket> | docs/runbooks/<svc>/<file>.md#<anchor> |

Reviewers: observability, operations
```

This gives cross-cutting reviewers a concrete artifact to gate on rather than a tacit expectation.

### Conditional Phase 5 (observability implementer)

A conditional Phase 5 is scheduled by `/user-story` *only* when any of the following triggers apply:

- Story introduces a **new SLO** or modifies existing SLO budgets
- Story modifies **alert severity routing** or PagerDuty integration
- Story requires changes to a **cross-service dashboard** (errors-overview, fleet SLO views, logs-aggregation)
- Story introduces a **new service** (bootstrap dashboards/conventions)

Routine stories that add or modify per-service metrics + dashboards + alerts do **not** trigger Phase 5. This keeps decomposition deterministic; the trigger checklist belongs in `/user-story`'s SKILL.md.

### Prerequisite guardrails

Before the `/user-story` template is modified to eliminate Phase 5, the following must land. Ownership split was finalized in debate between operations and observability (alert-rule semantics → operations; dashboard/metric semantics → observability; label/PII semantics → observability + security co-owned).

1. **`scripts/guards/simple/validate-alert-rules.sh`** — owner: operations. Enforces per `*-alerts.yaml` rule:
   - `runbook_url` annotation present and **resolves to a repo-relative `docs/runbooks/` path** (absolute URLs rejected — closes an exfil-on-click vector and keeps runbooks in-repo)
   - Severity label present and ∈ {`page`, `warning`, `info`}
   - `for:` duration ≥ 30s (no flapping single-scrape alerts)
   - Annotation text contains no hostnames, credentials, or secrets
2. **`scripts/guards/simple/validate-dashboard-panels.sh`** — owner: observability. Enforces ADR-0029 + dashboard hygiene:
   - Counter metrics presented via `increase()` or `rate()`, gauges via `last()`, histograms via `histogram_quantile()`
   - Panel unit declared
   - `$datasource` / `$interval` template vars used (no hard-coded datasource)
   - Canonical metric references (panel references metrics that exist in code + catalog)
3. **`scripts/guards/simple/validate-metric-labels.sh`** — owner: observability (mechanism) + security (denylist composition). Enforces:
   - PII-ish label denylist (`email`, `phone`, `display_name`, raw `user_id`, etc.)
   - Cardinality budgets per ADR-0011: ≤1000 combos/metric, ≤64 char values, 5M series total fleet-wide
   - `# pii-safe: <reason>` escape-hatch comment for intentional exceptions (reviewer-gated)
4. **Conventions doc set** — owner: observability. Concrete deliverables:
   - `docs/observability/dashboard-conventions.md` (panel layout, bucket naming, units, template vars, legend format)
   - `docs/observability/alert-conventions.md` (threshold patterns, burn-rate shapes, `for:` conventions, annotation hygiene rules, and an explicit `severity` label taxonomy — `page`/`warning`/`info` — with user-impact calibration anchors so service specialists classify consistently across services, preserving the Alertmanager routing contract)
   - `docs/observability/label-taxonomy.md` (shared label names, PII denylist, cardinality budgets, `# pii-safe:` usage)
   - `infra/grafana/dashboards/_template-service-overview.json` + matching `_template-service-alerts.yaml` starter templates

## Implementation Guidance

- Suggested first devloop: observability lands guardrails #1, #2, #3 above as a single "observability prerequisites" devloop. Operations co-implements the runbook-URL guard.
- Second devloop: update `.claude/skills/user-story/SKILL.md` to remove Phase 5 from the default template and add the conditional Phase 5 trigger checklist. Update the plan-template to include the Alert Thresholds block shown above.
- Third devloop (exemplar, recommended per test specialist): observability authors a complete dashboard+alerts reference for one service (suggested: MH, which currently lacks an alerts file) as the first working example of the conventions doc. This amortizes the PromQL/alert-annotation learning curve for downstream service specialists.
- Key files:
  - `.claude/skills/user-story/SKILL.md` (phase template)
  - `scripts/guards/simple/validate-alert-rules.sh` (new — alert-rule semantics: runbook_url, severity taxonomy, `for:` duration, annotation hygiene)
  - `scripts/guards/simple/validate-dashboard-panels.sh` (new — ADR-0029 panel types, unit, template vars, canonical metric refs)
  - `scripts/guards/simple/validate-metric-labels.sh` (new — PII denylist + cardinality budgets)
  - `docs/observability/dashboard-conventions.md`, `docs/observability/alert-conventions.md`, `docs/observability/label-taxonomy.md` (new)
  - `infra/grafana/dashboards/_template-service-overview.json`, `_template-service-alerts.yaml` (new templates)
- Dependency ordering: guardrails #1–#3 MUST merge before the `/user-story` template change, or the first devloop under the new regime ships bad alerts without CI catching them.

## Consequences

### Positive

- **Atomic landing**: metric + panel + catalog + alert + runbook-link arrive in one PR, eliminating the broken intermediate state where a metric exists but no dashboard queries it.
- **Ownership matches knowledge**: threshold selection stays with the specialist who wrote the code path; observability's expertise is applied as review, not as interview.
- **Deployment safety improves**: no lag between "metric emitted" and "oncall can see it." Dashboards ship with the code.
- **Guard pipeline alignment**: the task boundary now matches the CI boundary, making phase completion honest.
- **Story decomposition simpler**: 2 tasks and 1 phase eliminated from routine stories; conditional Phase 5 retained for cross-cutting concerns.
- **MH alerts gap closes naturally**: MH's next Phase 4 task that touches metrics now owns producing `mh-alerts.yaml`.

### Negative

- **Service specialist workload per task grows**: each Phase 4 now includes dashboard + alert authorship. Mitigated by conventions doc, template skeletons, and the fact that devloops already did this work unplanned.
- **Risk of dashboard drift across services**: service specialists work in parallel without a central author. Mitigated by conventions doc + observability reviewer + panel-classification guard.
- **Risk of poor alert thresholds**: service specialists may be expert in their domain but less experienced at alert design. Mitigated by plan-time observability + operations review with structured threshold artifact.

### Neutral

- **Observability specialist workload shifts**: from implementing per-service dashboards to maintaining conventions, guard infrastructure, cross-service dashboards, and reviewing all service-owned observability PRs. Net workload is roughly neutral.
- **Security review surface unchanged**: security gates the metric-definition PR (labels, PII, cardinality) per ADR-0011 — this is already where the risk lives, independent of dashboard authorship.

## Follow-ups (non-blocking)

These emerged during debate and are scheduled as work but are not blocking for this ADR:

- **Exemplar-first rollout** (raised by test, accepted by observability + media-handler): observability's first post-ADR devloop fills the MH alerts gap as a worked exemplar for the conventions doc — proves the template is usable by the first service that has no alerts file yet. Amortizes the PromQL/alert-annotation learning curve before other service specialists fly solo.
- **Partner-PR coordination exemplars**: MC + observability pair on the next MC histogram metric; MH + observability on the MH alerts fill-in. Sets the review-at-plan-approval pattern.
- **Guardable-checklist expansion** (raised by test, confirmed by operations): the conventions docs explicitly tag each rule as "guard-enforced" vs "reviewer-only" so machine-enforceable checks land as guards rather than rotting into conventions. Test proposes guard specs once observability drafts the conventions set.

## Participants

- **observability** (final: 93): Accept. All guardrails captured with concrete deliverables owned: `dashboard-conventions.md`, `alert-conventions.md`, `label-taxonomy.md`, `_template-service-overview.json`, panel-classification guard, label guard. Sequencing confirmed (guards before template change). Plan-template artifact incorporated.
- **operations** (final: 93): Accept. Owns `validate-alert-rules.sh` (runbook_url, severity label, `for:` duration, annotation hygiene). Conditional Phase 5 trigger checklist codified. Severity routing + SLO budgets remain central.
- **security** (final: 97): Accept. Review surface unchanged (metric-definition PR per ADR-0011); label-denylist guard co-owned with observability and promoted from follow-up to prerequisite. Repo-relative runbook paths close an exfil vector.
- **test** (final: 93): Accept. Task boundary now matches CI guard boundary; phase completion becomes honest. Guard-able subset of conventions explicitly tagged as prerequisite work. Exemplar-first rollout confirmed.
- **meeting-controller** (final: 95): Accept. Threshold authorship fits MC specialist's domain knowledge; mandatory plan-time review keeps observability collaborative. MC↔MH cross-service coordination co-review convention incorporated into decision.
- **media-handler** (final: 95): Accept. Closes MH alerts gap as a forcing function; first post-ADR devloop fills that gap as conventions exemplar. Conventions doc + cross-service dashboard carve-out keep MH's scope clean.

## Debate Reference

See: `docs/debates/2026-04-17-service-owned-dashboards/debate.md`
