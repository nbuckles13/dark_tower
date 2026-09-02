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

**Scope: the whole unit chain — config → metric → panel — not panel rendering alone.**

A unit is chosen three times for the same quantity: once in the configuration key a human sets, once
in the metric the code emits, and once in the panel that renders it. Those three choices must
compose, and the bug is never in any one of them — it is in the seam. This section is the home for
all three, deliberately: a rule about *config* units filed anywhere else is unfindable by the person
adding a config key, which is the exact moment the chain is decided.

Every non-`row`, non-`logs` panel must declare a unit via `fieldConfig.defaults.unit`. Empty string
or missing field is rejected.

Recommended units by metric intent:

| Intent | Unit | Example |
|---|---|---|
| Per-second rate | `reqps`, `ops`, `eps` | HTTP request rate |
| Discrete count (via `increase`) | `short` | "142 tokens issued" |
| Duration / latency | `s` (seconds) | histogram quantiles |
| Ratio / percentage | `percentunit` (0..1) | error rate, CPU utilization |
| Bytes | `bytes` | memory, network bandwidth |
| **Throughput / bandwidth** | **`Bps`** (bytes/sec) | egress budget, datagram send rate — **see below** |
| Time since epoch | `dateTimeFromNow` | last rotation time |
| Days / hours | `d`, `h` | signing key age |

### Throughput: the config → metric → panel chain `[reviewer-only]`

Throughput is the case where the three choices most often disagree, because humans and ADRs reason in
**bits per second** while every metric and panel convention here is **bytes**-based.

| Link | Rule |
|---|---|
| **Config** | MAY be expressed in the unit humans and ADRs reason in — bits/s, or frames of audio. The key name states the unit (`..._BPS`, `..._frames`). Config is the one place optimised for the person setting it. |
| **Conversion** | **Single-point, at config load, upstream of the fork.** Convert once as the value is parsed, before it reaches enforcement, the gauge, or anything else. Every consumer then reads the same converted value. |
| **Metric** | **Bytes-based**, always. Name ends `_bytes` or `_bytes_per_second`. |
| **Panel** | Unit `Bps`. Any bits equivalence goes in the panel **description as prose** — **never as a multiplication inside the query.** |

**Why conversion must be single-point and upstream of the fork.** If enforcement and the published
gauge each convert, they can disagree, and then the alert threshold and the enforced limit drift
apart silently — the gauge says one thing while the code does another, and nothing fails. Converting
once, before the value forks, makes that class of drift structurally impossible rather than merely
unlikely. This is ADR-0036 §11's "derive rather than guard wherever two values encode one
relationship": a derived value cannot drift; a guard only catches drift after someone introduces it.

**Why no `* 8` in a query.** A multiplication inside PromQL puts a unit conversion in a place no one
reviews and no guard checks, and it silently disagrees with the panel's declared `Bps` unit. The
panel then renders bits while claiming bytes. Put the equivalence in prose where a human reads it.

**Live instances** — both are *forward references*; neither config key exists in the tree today:

- **Datagram send buffer** (story 1) — ADR-0036 §1 requires this be expressed and documented **in
  frames of audio**, not bytes, because quinn's 1 MiB default is ≈93 seconds of queued audio and
  "1 MiB" does not make that visible while "93 seconds" does. quinn's API takes bytes, so the
  conversion happens once at config load.
- **`MH_EGRESS_BUDGET_BPS`** (story 2) — specified as "bits/s, converted once at load upstream of the
  enforcement/gauge fork; nothing downstream sees bits", with the derived stream ceiling and the
  published gauge both reading the converted value.

Two independent instances is what makes this a rule rather than a one-off, and why it belongs in a
conventions doc.

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

## Periodicity `[reviewer-only]`

Scrape and export cadence is a deliberate balance between **triage granularity** and **series cost**,
not an inherited default. Operations triages meeting media quality from these signals, so the cadence
determines what a responder can actually see.

**Every value below is read from its one configuration source. This section cites the key; it does
not restate the number as a literal**, because a literal here becomes a second encoding that drifts
from the config silently.

> **One deliberate exception, below.** The cadence *numbers* do appear in the drift table and prose
> that follow, because a drift cannot be described without both sides of it — "the deployed value
> differs from the intended one" is useless without saying by how much. Those figures are
> **point-in-time documentation of a defect, not a specification**; the config remains authoritative,
> and they disappear when D1 closes the gap. Everywhere else, cite the key.

### Which config is authoritative

There are **two live Prometheus configurations**, and they do not agree:

| File | Applies to | Authority |
|---|---|---|
| `infra/kubernetes/observability/prometheus-config.yaml` | The **deployed** Kind cluster (via `infra/kind/scripts/setup.sh` → `infra/kubernetes/overlays/kind/observability/`, a labels-only passthrough) | **AUTHORITATIVE** — this is what runs, and what any triage actually gets |
| `infra/docker/prometheus/prometheus.yml` | The local compose stack (`docker-compose.yml`) only | Local-only. Not deployed. |

Naming which one wins is load-bearing: "cite the config key" is meaningless while two keys per
cadence disagree.

### Media-path cadences

| Signal | Config key | Deployed value | Intended value |
|---|---|---|---|
| MH scrape | `scrape_configs[job_name=mh-service].scrape_interval` — **absent** in the authoritative file, so inherits `global.scrape_interval` | **15 s** (inherited) | 5 s, set only in the compose file, with the rationale "more frequent for real-time media metrics" |
| MC scrape | same shape, `job_name=mc-service` — **absent**, inherits global | **15 s** (inherited) | 10 s (compose only) |
| GC scrape | same shape, `job_name=gc-service` — **absent**, inherits global | **15 s** (inherited) | 10 s (compose only) |
| Client SDK OTel export | *(no key exists)* — `PeriodicExportingMetricReader` in `packages/sdk-core/src/telemetry/telemetryConfig.ts` is constructed without `exportIntervalMillis` | **OTel JS default (60 s)** | 10 s — **open item**, lands with the SDK media pipeline (story task 19) |
| MH latency histogram sample ratio | *(no key exists yet)* — lands with the MH forward path (story task 16), published as a gauge reading the same value the sampler reads | — | Forward reference; see `slos.md` |

### The deployed cadence is 15 s, and that is a gap

**MH is scraped at 15 s, not 5 s.** The authoritative config declares **no per-job
`scrape_interval` at all**; every job inherits the global value. The 5 s and 10 s figures exist only
in the compose file and are **intended-but-not-in-effect on the deployed path**.

This matters operationally rather than cosmetically: **a forwarding-latency histogram is close to
useless for triage at 15 s resolution.** A media-quality incident is typically shorter than a few
scrape intervals, so at 15 s a responder sees two or three points across the whole event — not enough
to distinguish a spike from a ramp, or to tell which of the three latency phases moved. The 5 s
cadence was chosen precisely so that decomposed media latency would be readable during an incident,
and that intent is currently not in effect.

This is **not** "cadences vary by environment". It is a single configuration that was written in one
place and never applied in the other. **Closing it is tracked in `docs/TODO.md` §Observability Debt
(D1)**, owner infrastructure + operations, together with the reciprocal cross-references and a drift
guard so the two files cannot silently rediverge again.

> **This subsection inverts when D1 lands.** Every statement above — the Deployed column, "the
> deployed cadence is 15 s", the Intended/Deployed split, the triage-resolution argument — becomes
> false the moment the per-job overrides are applied. Updating this subsection is listed in D1's
> **Fix** clause; it is the tracking half of documenting the gap truthfully in the meantime. Without
> it, the doc that currently describes the drift accurately becomes the tree's most confident wrong
> statement about cadence.

### Rules

- **Cite the key, never the number.** A cadence written as a literal in any document is a second
  encoding of a config value.
- **State which config is authoritative** whenever a cadence is referenced.
- **A config value published as a gauge must read the same value its consumer reads** — never a
  parallel constant. The pattern this follows is story 2's
  `mh_media_egress_budget_bytes_per_second{basis="unmeasured"}` gauge — a **forward reference**, not
  in the tree today, landing with the egress-budget chain — and the MH sample-ratio gauge (story task
  16) follows the same rule. Consistent with the Throughput subsection above: neither exists yet.
- **Do not tune a scrape interval to fix a dashboard.** A panel that needs finer resolution than the
  scrape provides is a cadence decision (cost, owner: operations), not a panel decision.

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
