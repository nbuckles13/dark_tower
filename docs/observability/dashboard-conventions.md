# Dashboard Conventions

Conventions for authoring Grafana dashboards under ADR-0031.

This document covers: panel classification, layout, bucket naming, units,
template variables, legend format, color/threshold conventions, and the
reviewer PR checklist.

Ownership: service specialists own per-service `<svc>-overview.json`,
`<svc>-slos.json`, and `<svc>-logs.json` files; observability owns this
document, the `_template-service-overview.json` starter, and the guard that
enforces it.

**Authoritative ADRs**:
- [ADR-0011](../decisions/adr-0011-observability-framework.md) — metric taxonomy, SLO framework, cardinality budget.
- [ADR-0029](../decisions/adr-0029-dashboard-metric-presentation.md) — metric-type → presentation semantics. **The authoritative reference for panel classification**; this doc operationalizes it.
- [ADR-0031](../decisions/adr-0031-service-owned-dashboards-alerts.md) — service-owned dashboard authorship, this doc, the guard.

**Machine enforcement**: `scripts/guards/simple/validate-dashboard-panels.sh`
runs on every CI pipeline. Rules in this document are tagged
`[guard-enforced]` or `[reviewer-only]`; see the rule index at the end of
this document for the full enforcement matrix.

---

## Panel Classification

The rule from ADR-0029: **the underlying metric type — not the panel type —
determines which PromQL function wraps the metric**. Mis-classification makes
dashboards either unreadable (counter read bare shows cumulative total since
process start) or wrong (gauge wrapped in rate gives a meaningless derivative).

### Counter metrics `[guard-enforced]`

Metrics defined via `counter!(...)` in `crates/*/src/observability/metrics.rs`.
Canonically named with `_total` suffix.

- **Timeseries panel** → `increase($__rate_interval)` for discrete event counts
  (ADR-0029 §Category A), or `rate($__rate_interval)` when the panel computes
  a ratio or per-second normalized series (§Category B).
- **Stat panel** (time-range aggregate) → `increase($__range)`. Reads as
  "how many events in the selected window" — always a readable integer in
  low-traffic environments.
- **NEVER**: bare reference like `sum(metric)` or `metric{...}`. The guard
  rejects this — a raw counter is cumulative since process start, which is
  almost never what the author intended. The one legitimate use is a
  deliberate investigative snapshot; those require `# guard:ignore(<reason>)`.

### Gauge metrics `[guard-enforced]`

Metrics defined via `gauge!(...)`. Represent an instantaneous value
(active meetings, signing key age, queue depth).

- **Timeseries panel** → bare reference (`metric{...}`) or wrapped in
  `sum()` / `avg()` / `max()` aggregations.
- **Stat panel** → bare reference with `instant: true` and
  `reduceOptions.calcs: ["lastNotNull"]`.
- **NEVER**: `rate()` / `increase()`. Gauges don't "count" and their
  difference is semantically undefined — going from 5 to 3 active meetings
  doesn't mean "−2 meetings happened per second".

### Histogram metrics `[guard-enforced]`

Metrics defined via `histogram!(...)`. Prometheus exposes each histogram as
three derived series: `_bucket`, `_sum`, `_count`.

- **Quantile panel (p50 / p95 / p99)** →
  `histogram_quantile(<q>, sum(rate(<metric>_bucket[$__rate_interval])) by (le))`.
  The `by (le)` is required — bucket-aware quantile interpolation needs the
  `le` label preserved. Unit is `s` (or whatever unit the histogram measures).
- **Distribution heatmap** →
  `sum(rate(<metric>_bucket[$__rate_interval])) by (le)` with `format: heatmap`.
  Use these when quantile plots risk hiding bimodal behavior.
- **Average / count panels** → `rate(<metric>_sum[$__rate_interval]) /
  rate(<metric>_count[$__rate_interval])` for average latency; counts of
  observations use `increase(<metric>_count[$__rate_interval])` (counter
  rules apply — it's a counter internally).
- **NEVER**: bare `_bucket` reference (not wrapped in `rate()`) — the guard
  rejects. `histogram_quantile` on a cumulative bucket gives nonsense.

### Classification decision tree

1. Lookup the metric in `crates/<svc>-service/src/observability/metrics.rs`.
   What macro defines it — `counter!`, `gauge!`, or `histogram!`?
2. If `counter!` → use `increase()` (counts) or `rate()` (ratios). Pick window
   via §Template Variables below.
3. If `gauge!` → bare reference, possibly inside `sum()`/`avg()`/`max()`.
4. If `histogram!` → `histogram_quantile(…, rate(_bucket[…]))` or
   `rate(_sum)/rate(_count)`.
5. If the metric doesn't exist in a `metrics.rs` file, stop — guard rejects.

---

## Panel Layout Conventions

Dashboards are organized into **rows** that group panels by audience
question. The standard overview-dashboard row sequence:

1. **Service Health** — is the service up? Basic reachability gauges.
   Top of dashboard, never collapsed. One row of ≤4 stat panels.
2. **Request Metrics** — what load is the service carrying? Counters
   (requests, operations) in stat + timeseries panels.
3. **Error Metrics** — what's failing? Error-rate ratios, error counts by
   type.
4. **Latency** — how slow? Histogram quantiles + heatmap.
5. **Resource Usage** — memory, CPU, pod counts. Gauges and container
   counters.
6. **Service-Specific** — zero or more rows for domain metrics
   (DB queries, meeting signaling, forwarding throughput). Ordered by
   audience importance, not alphabetically.

### Panel sizing (reviewer-only)

Grafana's grid is 24 columns wide. Conventions (not guard-enforced):

- **Stat panels**: `{h: 4, w: 6}` — four per row.
- **Timeseries**: `{h: 8, w: 12}` — two per row for overview; full-width
  (`w: 24`) acceptable for high-cardinality rollup panels.
- **Heatmap**: `{h: 8, w: 12}`.
- **Logs (Loki)**: `{h: 10, w: 24}` — full-width, since log lines wrap poorly.

### Grouping by audience, not implementation

The row headings answer a question an operator would ask at 3am. Do not
group by "metric family" (e.g., all histograms in one row). The answer to
"is latency bad?" lives next to the answer to "what's the error rate?",
because an oncall triaging an incident will read them together.

---

## Bucket Naming

Histogram buckets are configured in
`crates/<svc>-service/src/observability/metrics.rs` via
`PrometheusBuilder::set_buckets_for_metric`. Bucket choice is a metrics-authoring
concern; the dashboard guard does not enforce it — but `validate-histogram-buckets.sh`
does at the source-code level.

Dashboard authoring notes for histogram panels:

- **Use `by (le)` on quantile queries** `[guard-enforced via shape rule]` —
  `histogram_quantile(q, sum(rate(metric_bucket[$__rate_interval])) by (le))`.
  Without `by (le)`, the `le` label gets aggregated away and the quantile
  function receives a malformed input.
- **Match quantile choice to the SLO** (reviewer-only) — dashboard p95 next
  to alert-rule p95. If the SLO is p99, show p99.
- **Show at least p50, p95, p99** on any latency quantile panel (reviewer-only).
  A single percentile hides the distribution shape.

---

## Units `[guard-enforced]`

Every non-`row`, non-`logs` panel must declare a unit via
`fieldConfig.defaults.unit`. Empty string or missing field is rejected.

Recommended units by metric intent:

| Intent | Unit | Example |
|---|---|---|
| Per-second rate | `reqps`, `ops`, `eps` | HTTP request rate |
| Discrete count (via `increase`) | `short` | "142 tokens issued" |
| Duration / latency | `s` (seconds) | histogram quantiles |
| Ratio / percentage | `percentunit` (0..1) | error rate, CPU utilization |
| Bytes | `bytes` | memory, network bandwidth |
| Time since epoch | `dateTimeFromNow` | last rotation time |
| Days / hours | `d`, `h` | signing key age |

### `percent` vs `percentunit` (reviewer-only)

PromQL division produces a dimensionless ratio in [0, 1]. Use `percentunit`;
Grafana renders it as a percentage (e.g., 0.023 → "2.3%"). Using `percent`
with a ratio input shows "0.023%" which is off by 100×.

### Logs panels (exempt)

Grafana `logs` panel type renders log lines, not numbers — unit is
meaningless and the guard exempts them. Log-volume bar charts (`timeseries`
with Loki datasource counting log lines) still require a unit; `short` is
the convention.

---

## Template Variables

Every dashboard MUST declare at least a `$datasource` variable. Most should
also declare `$namespace`. Service dashboards scoped to one service may add
more (`$pod`, `$operation`).

### `$datasource` `[guard-enforced]`

Every panel and every target MUST reference datasource via
`{"type": "<prom|loki>", "uid": "$datasource"}`. Hard-coded UIDs like
`"prometheus"` or `"loki"` are rejected.

Rationale: datasource UIDs change across environments (local Kind vs
staging vs prod vs disaster-recovery standby). A dashboard pinned to one
UID is a migration liability.

Declaration:

```json
{
  "name": "datasource",
  "type": "datasource",
  "label": "Datasource",
  "query": "prometheus",
  "current": {"text": "prometheus", "value": "prometheus"}
}
```

The `query` field filters datasources of that type — use `"prometheus"` for
metric dashboards, `"loki"` for log dashboards. A dashboard with both types
(rare) declares two variables with different names (`$datasource`,
`$loki_datasource`).

### `$__rate_interval` `[guard-enforced]`

Rate / increase windows on non-SLO dashboards MUST use `$__rate_interval`,
not hard-coded durations. Grafana computes this to be at least 4× the scrape
interval AND scaled to the dashboard time range, so the dashboard shows
sensible smoothing at both 5-minute and 30-day views.

Allowed exceptions:
- `$__range` is accepted (stat-panel aggregates over the dashboard's
  selected window — e.g., "requests in last N hours").
- `$__interval` is accepted but discouraged (no scrape-interval floor;
  reviewer-only preference).

Hard-coded `[5m]`, `[1h]`, `[30s]` are rejected **on non-SLO dashboards**.
SLO dashboards are exempt — see next section.

### SLO dashboard carve-out `[guard-enforced]`

Files matching `*-slos.json` intentionally use hard-coded windows (5m, 30m,
1h, 6h, 7d, 28d, 30d) to maintain parity with alert-rule burn-rate math
(ADR-0029 §Category C). The guard exempts these files from the
`$__rate_interval` rule. Reviewers should still ensure windows match the
corresponding alert rules.

### `$namespace` (reviewer-only)

Standard convention: every service dashboard accepts a Kubernetes namespace
filter, default `$__all`. Declaration:

```json
{
  "name": "namespace",
  "type": "query",
  "datasource": {"type": "prometheus", "uid": "$datasource"},
  "query": "label_values(up, namespace)",
  "includeAll": true
}
```

This lets one dashboard serve dev, stage, and prod without duplication.

---

## Legend Format Conventions `[reviewer-only]`

- **Low-cardinality series** (status codes, operation types): use the label
  as legend — `"{{status}}"`, `"{{operation}}"`.
- **Multi-label series**: concatenate sparingly — `"{{method}} {{status}}"`.
  Three labels is the practical limit before the legend becomes unreadable.
- **Single aggregate**: use a descriptive name — `"Total"`, `"p95"`,
  `"5xx error rate"`.
- **Never include** timestamps, pod IDs, or other high-cardinality values in
  legend format strings — the legend becomes a wall of text and each series
  gets its own line.

---

## Color and Threshold Conventions `[reviewer-only]`

### Semantic color

- `green` — healthy / within SLO.
- `yellow` / `orange` — degraded but not breaching.
- `red` — SLO breach or outage.
- `blue` — informational / no health meaning.

Use `thresholds` mode `absolute` with numeric breakpoints matching the
SLO or alerting threshold. Don't use `percentage` mode — it varies with the
current value range and gives inconsistent behavior across environments.

### Service-up stat panels

Use value mappings — `0 → "DOWN"` (red), `1 → "UP"` (green). Skip the
numeric display; an operator wants to see DOWN, not 0.

### Latency panels

Thresholds should reference the SLO numeric value. A p95-latency panel for
a 200ms SLO sets the red threshold at 0.200 (seconds), yellow at half
that. Latency-histogram heatmaps don't use thresholds — the color scheme
already encodes density.

---

## Panel Escape Hatch `[reviewer-gated]`

Sometimes a panel legitimately wants to bypass the classification rule — for
example, an investigative dashboard that intentionally reads a cumulative
counter at a specific instant. Add:

```json
{
  "description": "<explanation>. # guard:ignore(<reason with >=10 chars>)",
  "type": "stat",
  "targets": [...]
}
```

inside the panel's `description` field. The reason is mandatory, must be
≥10 characters, and must not start with `test`, `tmp`, `todo`, `fixme`, or
`wip`. The escape hatch **scope is classification + rate-window only** —
it does not bypass unit, datasource, or metric-exists checks.

Reviewers should scrutinize every new `guard:ignore` during PR review.

---

## Metric Existence `[guard-enforced]`

Every `ac_`/`gc_`/`mc_`/`mh_`-prefixed metric referenced in a panel target's
`expr` MUST:

1. Be defined in the corresponding `crates/<svc>-service/src/observability/metrics.rs`
   (as `counter!`/`gauge!`/`histogram!`), AND
2. Be documented in `docs/observability/metrics/<svc>-service.md` with a
   `### \`<metric_name>\`` heading.

The `_bucket`/`_sum`/`_count` suffixes on histograms are recognized — the
guard checks the base name. This rule overlaps with
`validate-application-metrics.sh`; we duplicate it here so the dashboard
guard fails with a targeted, dashboard-centric message rather than deferring
to a different guard's cross-cutting coverage check.

---

## Dashboard PR Checklist `[reviewer-only]`

At plan-approval time (per ADR-0031), the following cross-cutting reviewers
apply their lens.

### Observability reviewer

- [ ] Row structure follows the standard sequence (Health → Requests →
  Errors → Latency → Resources → Service-specific).
- [ ] Panel classification matches ADR-0029 for every metric.
- [ ] Quantile panels use `sum(rate(_bucket[…])) by (le)` shape; bucket
  choice from source code matches the SLO.
- [ ] Units match metric intent (`percentunit` not `percent` for ratios;
  `bytes` not `short` for memory).
- [ ] Legend format strings are low-cardinality.

### Operations reviewer

- [ ] Row ordering matches incident-triage flow — what an oncall reads first.
- [ ] Threshold colors match alert-rule severities.
- [ ] Dashboard title, description, and tags are oncall-discoverable.

### Security reviewer

- [ ] No PII in legend format, panel title, or annotation text.
- [ ] No internal hostnames, IP addresses, or credential markers in panel
  descriptions.
- [ ] Any `# guard:ignore` has a good reason.

### Test reviewer

- [ ] `scripts/guards/run-guards.sh` passes.
- [ ] Dashboard displays sensible data in local Kind environment (low
  traffic). Integer counts visible via `increase()` per ADR-0029.

### Service-specialist cross-review

When a dashboard panel observes a coordination boundary between services
(e.g., an `mc_gc_heartbeats_total` panel on the MC dashboard), add the
counterparty service specialist as a required reviewer — same review-graph
rule as alert rules.

---

## Machine-Enforced vs Reviewer-Only Rule Index

| Rule | Enforcement |
|---|---|
| Panel `fieldConfig.defaults.unit` set (non-`row`/`logs` panels) | `[guard-enforced]` |
| Panel datasource.uid via `$datasource` template var | `[guard-enforced]` |
| Target datasource.uid via `$datasource` template var | `[guard-enforced]` |
| Counter metric wrapped in `rate()`/`increase()` | `[guard-enforced]` |
| Gauge metric NOT wrapped in `rate()`/`increase()` | `[guard-enforced]` |
| Histogram `_bucket` wrapped in `rate()` | `[guard-enforced]` |
| Histogram `_sum`/`_count` wrapped in `rate()`/`increase()` | `[guard-enforced]` |
| Rate window `$__rate_interval` (non-SLO dashboards) | `[guard-enforced]` |
| `*-slos.json` exempt from `$__rate_interval` rule | `[guard-enforced]` (carve-out) |
| Metric exists in source `metrics.rs` | `[guard-enforced]` |
| Metric documented in per-service catalog | `[guard-enforced]` |
| `# guard:ignore(reason)` escape hatch | `[guard-enforced]` (parsed and honored) |
| Panel classification (choice of increase vs rate) | `[reviewer-only]` |
| Row structure / layout convention | `[reviewer-only]` |
| Panel sizing grid conventions | `[reviewer-only]` |
| Bucket choice (matches SLO) | `[reviewer-only]` |
| Unit matches metric intent (e.g., `percentunit` vs `percent`) | `[reviewer-only]` |
| Legend format cardinality | `[reviewer-only]` |
| Threshold numeric values match SLO / alert | `[reviewer-only]` |
| Color semantics (green/yellow/red mapping) | `[reviewer-only]` |
| Quantile `by (le)` shape | `[reviewer-only]` (shape-checked only indirectly) |
