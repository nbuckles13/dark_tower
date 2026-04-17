# Alert Conventions

Conventions for authoring Prometheus alert rules under ADR-0031.

This document covers: severity taxonomy, threshold patterns, burn-rate
shapes, `for:` conventions, annotation hygiene, and the reviewer PR checklist.

Ownership: service specialists own per-service `<svc>-alerts.yaml` files;
observability owns this document and the guard that enforces it.

**Authoritative ADRs**:
- [ADR-0011](../decisions/adr-0011-observability-framework.md) — SLO framework, error budget burn-rate thresholds.
- [ADR-0029](../decisions/adr-0029-dashboard-metric-presentation.md) — metric-type → presentation semantics (counter/gauge/histogram).
- [ADR-0031](../decisions/adr-0031-service-owned-dashboards-alerts.md) — service-owned alert authorship, this doc, the guard.

**Machine enforcement**: `scripts/guards/simple/validate-alert-rules.sh`
runs on every CI pipeline. Rules in this document are tagged
`[guard-enforced]` or `[reviewer-only]`; see the rule index at the end
of this document for the full enforcement matrix.

---

## Severity Taxonomy

Three severity values, each with a distinct routing contract in Alertmanager.
**Classify according to user impact, not technical scary-ness.**

### `page` — **user-visible impact now, or SLO burn** `[guard-enforced value]`

Fires when users are experiencing, or are about to experience, a failure
of the service's advertised contract. Pages oncall immediately (PagerDuty +
Slack incident channel).

Anchor examples:
- `GCDown` — whole GC service unreachable; no meetings can be joined or created.
- `GCHighErrorRate` — 1% error SLO violation; availability target at risk.
- `GCDatabaseDown` — 50% DB error rate; all GC operations failing.
- `MCDown` — MC service unreachable; active meetings affected.
- `MCHighLatency` — p95 latency above 500ms SLO for 5m.

### `warning` — **degraded but contained, or approaching limits** `[guard-enforced value]`

Fires when the system is degraded or trending toward trouble, but users are
not yet seeing a contract failure. Routes to Slack (non-paging). Investigate
during business hours.

Anchor examples:
- `GCHighMemory` — 85% memory utilization; OOM risk but no user impact yet.
- `GCDatabaseSlow` — p99 query latency > 50ms; may cascade to SLO but hasn't.
- `MCCapacityWarning` — 80% of active-meeting cap; scale out before hitting limit.
- `MCPodRestartingFrequently` — service instability signal; not yet outage.

### `info` — **awareness / trend / leading indicator** `[guard-enforced value]`

Fires when the system exhibits a condition worth tracking but requiring no
human response. No notification; surfaced in dashboards and alert history
only.

Common pattern: a service emits an `info` alert as a **leading indicator** for
a SLO-guarding `page` alert, so that dashboards show the early-warning signal
without paging.

Anchor examples:
- `GCHighJoinLatency` — p95 join latency > 2s. The aggregate `GCHighLatency`
  `page` alert covers SLO at 200ms; this surfaces join-path degradation for
  observability.
- `MCHighJoinLatency` — analogous leading indicator for MC session joins.

### Severity classification decision tree

Use this when you're not sure which severity applies:

1. Does this mean a **user** (not oncall, not SRE) sees or will imminently see
   a failure, error, or SLO breach? → `page`.
2. Is the system **degrading** (resource pressure, rising error rate, increasing
   latency) but not yet breaking user contracts? → `warning`.
3. Is this a **trend signal** worth tracking without action, or a leading
   indicator already covered by a `page` alert? → `info`.

### Never put identifiers in severity routing labels `[reviewer-only]`

The `severity` label is a routing dimension, not a metadata field. Keep its
value in `{page, warning, info}`. Companion labels like `team`, `service`,
`component` may appear but should carry **abstract roles** (`team:
service-owner`, `service: gc-service`), never concrete oncall identities
(e.g. `team: auth-controller-prod-oncall`). Concrete team/pager names belong
in Alertmanager routing config, not rule labels — changing the team roster
shouldn't require editing alert rules.

### Severity bumping (Alertmanager concern, not rule concern) `[reviewer-only]`

Severity bumping — where a `warning` alert automatically escalates to `page`
after a prolonged firing window — is an **Alertmanager routing-config
concern**, not a rule-side concern. Do not encode bumping behavior in PromQL
expressions or `for:` durations.

See [`docs/observability/alerts.md` §Alert Fatigue Prevention](./alerts.md)
for the current bumping policy. Changes to bumping land in Alertmanager
config, not in `<svc>-alerts.yaml`.

---

## Threshold Patterns `[reviewer-only]`

The canonical alert-rule shape is:

```
<condition> for <duration>
```

- `<condition>` — a PromQL expression that evaluates to a non-empty vector
  when the alert should fire.
- `for <duration>` — minimum time the condition must remain true before the
  alert transitions from `pending` to `firing`. Suppresses single-scrape
  flaps.

### How to pick a threshold

- **SLO-derived**: threshold = SLO value (e.g., error rate above 1% when the
  SLO is 99%). Match the evaluation window to the SLO window.
- **Baseline-derived**: threshold = N× typical observed value; requires
  baseline data. Use P95 or P99 of the historical series as the reference.
- **Hard-limit-derived**: threshold = fraction of a resource cap (e.g., 85%
  memory). Set below the breaking point to give intervention time.

### Anti-patterns (avoid these)

- **Single-scrape trigger** — `expr` without `for:` or with `for: 0s`. Fires on
  every transient spike. Use `for: >= 30s` (guard enforced) or use a rate/increase
  window in the `expr` that already provides smoothing.
- **Threshold exactly at SLO** — when error rate is sometimes at SLO and
  sometimes above, alert pages on noise. Use a burn-rate alert (see next
  section) that accounts for budget consumption, not instantaneous crossings.
- **Cardinality explosion** — alerts grouped by high-cardinality labels
  (`user_id`, `meeting_code`) produce one alert per label combination. Keep
  alert-expr labels to low-cardinality dimensions (`service`, `component`,
  `pod`).

---

## Burn-Rate Alert Shapes `[reviewer-only]`

Burn-rate alerts fire when the SLO error budget is consuming faster than
the sustainable rate. They're preferred over threshold alerts for SLO-tied
conditions because they measure *budget impact* rather than *instantaneous
state*.

### Fast-burn (page severity)

Fires when error budget burns at >10× sustainable rate over a 1-hour window.
At that rate, the 30-day budget is exhausted in <3 days.

```promql
(
  sum(rate(<svc>_<operation>_errors_total[1h]))
  /
  sum(rate(<svc>_<operation>_total[1h]))
) / <slo_error_rate> > 10
```
- `for: 1h`
- `severity: page`

### Slow-burn (warning severity)

Fires when error budget burns at >5× sustainable rate over a 6-hour window.
At that rate, the 30-day budget is exhausted in <6 days — enough warning to
intervene before paging.

```promql
(
  sum(rate(<svc>_<operation>_errors_total[6h]))
  /
  sum(rate(<svc>_<operation>_total[6h]))
) / <slo_error_rate> > 5
```
- `for: 6h`
- `severity: warning`

> **Footnote on multi-window alerting**: The 10×/5× single-window shape above
> follows [ADR-0011](../decisions/adr-0011-observability-framework.md#initial-slo-targets).
> The Google SRE workbook's multi-window multi-burn-rate (MWMBR) approach
> uses fast-burn 14.4× on 5m/1h dual-window conditions and slow-burn 6× on
> 30m/6h dual-window conditions, which reduces false positives from spiky
> traffic. If this project adopts MWMBR, a supersession to ADR-0011 and an
> update to this section is required.

---

## `for:` Conventions

- **Floor: 30 seconds** `[guard-enforced]`. Rejected: `for: 0s`, `for: 10s`,
  omitted. Prevents single-scrape flapping alerts.
- **30s–1m**: outage detection on critical paths. Use for `Up == 0`–style
  alerts where detection speed matters and noise is low.
- **5m–10m**: steady-state SLO-derived thresholds. Most `warning` alerts
  land here.
- **1h+**: long-window burn-rate or trend alerts. Match to evaluation
  window.

Match the `for:` window to the `rate()` / `increase()` window in the `expr`
where applicable. A 5m `rate()` window with `for: 1h` means you need 1h of
sustained breach — rarely what you want.

---

## Annotation Hygiene

Every alert rule MUST include three annotations. `impact` is strongly
recommended.

| Annotation | Required? | Purpose |
|---|---|---|
| `summary` | Yes `[reviewer-only]` | One line with `{{ $value \| ... }}` so oncall sees the triggering number. |
| `description` | Yes `[reviewer-only]` | 1–2 sentences explaining the what and why. May reference dashboards. |
| `impact` | Recommended `[reviewer-only]` | User/business consequence at 3am readability. |
| `runbook_url` | Yes `[guard-enforced]` | Repo-relative `docs/runbooks/<file>.md#<anchor>`. See below. |

### `runbook_url` format `[guard-enforced]`

Must be repo-relative, starting with `docs/runbooks/`. Must resolve to an
existing file. Absolute URLs (`http://`, `https://`, `//`, `file://`) are
rejected — this closes an exfil-on-click vector where an operator clicking
a tampered runbook URL is exited out of the repo into an attacker-controlled
destination.

The flat layout (`docs/runbooks/<file>.md`) is accepted. If a service
chooses to organize into a subdirectory (`docs/runbooks/<svc>/<file>.md`),
that is also accepted — the guard matches the prefix, not an exact pattern.

### Denylist (machine-scanned) `[guard-enforced]`

The guard rejects annotations containing:
- IPv4 addresses (RFC1918 and public both)
- Bearer tokens, `Authorization:` headers
- AWS access keys (`AKIA...`), `aws_secret_access_key=...` markers
- OpenAI / Stripe (`sk-...`, `pk-...`) keys
- GitHub PATs (`ghp_...`, `gho_...`)
- Slack tokens (`xox[baprs]-...`)
- JWTs (`eyJ...`)
- PEM private keys
- Internal DNS suffixes (`.svc.cluster.local`, `.internal`, `.amazonaws.com`)
- Prod/stage hostname fragments (`-prod-`, `-stage-`, `-prd-`, `-stg-`)

Allowed instead: **Go templating** via `{{ $labels.pod }}`,
`{{ $labels.service }}`, `{{ $value | humanizePercentage }}` — these
interpolate at fire time with already-collected label values and are the
sanctioned way to surface runtime state.

### Escape hatch `[reviewer-gated]`

When the hygiene heuristic produces a false positive, add:

```yaml
# guard:ignore(<reason>)
- alert: ...
```

immediately before the `- alert:` line, or on the same line. The reason is
mandatory. The escape hatch **scope is annotation hygiene only** — it does
not bypass severity, runbook, or `for:` checks.

Reviewers should scrutinize every new `guard:ignore` during PR review.

---

## Alert-Rule PR Checklist `[reviewer-only]`

At plan-approval time (per ADR-0031), the following cross-cutting reviewers
apply their lens.

### Observability reviewer

- [ ] Threshold plausibility vs stated SLO, not cargo-culted from another service.
- [ ] `expr` is cardinality-safe (no high-cardinality `by()` clauses).
- [ ] Counter metrics accessed via `rate()` / `increase()`; gauges via instant
  lookup; histograms via `histogram_quantile()` per [ADR-0029](../decisions/adr-0029-dashboard-metric-presentation.md).
- [ ] `for:` window matches the `rate()` / `increase()` window sensibly.
- [ ] Burn-rate alerts use the shape documented above; multipliers follow
  ADR-0011 (10×/5×).

### Operations reviewer

- [ ] Severity classification matches the taxonomy anchors. Would this
  actually warrant paging (for `page`)? Is the degradation contained
  (for `warning`)?
- [ ] `runbook_url` resolves to a real runbook section (not a stub).
- [ ] Runbook covers triage, mitigation, and escalation.
- [ ] Annotation text reads cleanly at 3am — no jargon unknown to rotation
  oncall.
- [ ] SLO budget impact estimated if the alert fires frequently.

### Security reviewer

- [ ] Hygiene guard pass clean. Any `# guard:ignore` has a good reason.
- [ ] No labels or annotations expose user-identifiable data.
- [ ] `runbook_url` is repo-relative (guard enforces this).

### Test reviewer

- [ ] `scripts/guards/run-guards.sh` passes.
- [ ] Reasoning for `for:` choice is clear (SLO-matched, flap-suppressing,
  etc.).

### Service-specialist cross-review (cross-service coordination)

When an alert rule observes a coordination boundary between services (e.g.
`mc_gc_heartbeats_total`, `mc_mh_sessions_total`), add the counterparty
service specialist as a required reviewer. The review-graph rule is:
alerts on `<svc>_<dep>_*` metrics require `<svc>` specialist (author) +
`<dep>` specialist (co-reviewer).

---

## Machine-Enforced vs Reviewer-Only Rule Index

| Rule | Enforcement |
|---|---|
| `runbook_url` annotation present | `[guard-enforced]` |
| `runbook_url` repo-relative | `[guard-enforced]` |
| `runbook_url` target exists | `[guard-enforced]` |
| `severity` label present | `[guard-enforced]` |
| `severity` value in allowed set | `[guard-enforced]` |
| `for:` ≥ 30s | `[guard-enforced]` |
| Annotation text free of secrets/hostnames | `[guard-enforced]` |
| `# guard:ignore(reason)` escape hatch | `[guard-enforced]` (parsed and honored) |
| Severity classification taxonomy | `[reviewer-only]` |
| Threshold plausibility vs SLO | `[reviewer-only]` |
| Burn-rate multiplier choice | `[reviewer-only]` (ADR-0011 authoritative) |
| `for:` window value choice (above 30s floor) | `[reviewer-only]` |
| `summary` / `description` / `impact` readability | `[reviewer-only]` |
| Severity bumping (Alertmanager config) | `[reviewer-only]` (not a rule concern) |
| Cross-service co-reviewer requirement | `[reviewer-only]` |
