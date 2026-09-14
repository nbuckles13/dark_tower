# ADR-0029: Dashboard Metric Presentation — Counters vs Rates

## Status

Accepted

## Context

All Grafana dashboard panels across AC, GC, MC, errors-overview, and SLO dashboards used `rate(...[5m])` for metric presentation (151 `rate()` calls, 0 uses of `$__rate_interval`). In low-traffic environments (local Kind cluster, CI, early staging), per-second rates for sparse events round to near-zero values (e.g., 5 requests in 5 minutes = 0.017 req/s), making dashboards useless for debugging and test verification.

This affected all overview dashboards, the errors dashboard, and made security-relevant events (JWT failures, rate limit triggers) invisible in low-traffic environments where they are most significant.

## Decision

Use a **metric-type distinction** to determine the PromQL function for each dashboard panel. The underlying metric semantics — not the panel type — determines whether to use `increase()` or `rate()`.

### Category A: Discrete Event Counters → `increase($__rate_interval)`

All `_total` counter metrics representing discrete events use `increase()` for both timeseries and stat panels. These answer "how many?" — a count that is always a readable integer.

**Timeseries panels**: `increase(metric[$__rate_interval])`
**Stat panels**: `increase(metric[$__range])`

Applies to per-service `*_total` counters: HTTP requests, meeting lifecycle events, MC assignments, token issuance, security events (JWT validations, rate-limit decisions, connection rejections), session-join outcomes, MC↔MH coordination events, fence events, actor panics, credential operations, DB queries. See `docs/observability/metrics/{ac,gc,mc,mh}-service.md` for the current set. *Example: `ac_token_issuance_total`.*

**Y-axis labels** must reflect counts: "requests", "errors", "events" — not "req/s".

### Category B: Derived/Normalized Metrics → `rate($__rate_interval)`

Metrics requiring per-second normalization or ratio math use `rate()` with `$__rate_interval` replacing hardcoded windows.

Applies to:
- **Error percentages**: `rate(errors[w]) / rate(total[w])` — ratio requires rate/rate
- **Histogram quantiles**: `histogram_quantile(0.95, rate(bucket[w]))` — PromQL requires rate()
- **Latency distributions**: histogram bucket panels (e.g., `mc_session_join_duration_seconds_bucket`, `mc_redis_latency_seconds_bucket`)
- **CPU utilization**: `rate(container_cpu_usage_seconds_total[w])` — must be rate for fraction
- **Any ratio panel**: numerator and denominator both use rate()

### Category C: SLO Dashboards → `rate()` with Explicit Windows

SLO dashboards (`ac-slos.json`, `gc-slos.json`, `mc-slos.json`) keep their current hardcoded windows (30d, 7d, 28d, 1h, 6h) to maintain exact parity with alert rule expressions. SLO burn-rate math is a special case where the window is part of the definition. See per-service catalogs for the metrics underlying each SLO.

### New Stat Panels

Add `increase(metric[$__range])` stat panels to each overview dashboard. Choose service-appropriate `*_total` counters from `docs/observability/metrics/{ac,gc,mc,mh}-service.md`:

**Traffic Summary row** (top of each dashboard, not collapsed):
- AC: token issuance, HTTP requests. *Example: `ac_token_issuance_total`.*
- GC: HTTP requests, MC assignments. *Example: `gc_http_requests_total`.*
- MC: session joins, active meetings (gauge — no change needed). *Example: `mc_session_joins_total`.*

**Security Events row** (on dashboards with security-relevant metrics):
- AC: rate-limit decisions, token-validation failures
- MC: JWT-validation failures, session-join failures, WebTransport connection rejections, caller-type rejections

### Alert Rules

**No changes.** Alert rules run in Prometheus with their own hardcoded windows, completely independent of Grafana's `$__rate_interval`. Ratio-based alerts (error rate thresholds) correspond to Category B dashboard panels which still use `rate()`. Alerts using `increase()` (e.g., `MCActorPanic`) already match.

### Classification Rule for Future Dashboards

When adding new dashboard panels, apply this rule:
- If the metric name ends in `_total` and represents discrete events → `increase($__rate_interval)`
- If the panel computes a ratio, percentage, quantile, or burn rate → `rate($__rate_interval)`
- If the panel is on an SLO dashboard → keep explicit window matching alert rules

## Implementation Guidance

- Suggested specialist: `observability`
- Task breakdown:
  1. Replace hardcoded `[5m]` with `[$__rate_interval]` in all overview/errors dashboard panels (~151 expressions)
  2. Switch Category A counter panels from `rate()` to `increase()` (~60 panels, ~40% of total)
  3. Update Y-axis labels on converted panels (remove "/s" suffixes)
  4. Add Traffic Summary stat row to AC, GC, MC overview dashboards
  5. Add Security Events stat row to AC and MC overview dashboards
  6. Verify SLO dashboards retain explicit windows (no changes)
  7. Run `validate-application-metrics.sh` guard to confirm no metric coverage regression
- Key files:
  - `infra/grafana/dashboards/ac-overview.json`
  - `infra/grafana/dashboards/gc-overview.json`
  - `infra/grafana/dashboards/mc-overview.json`
  - `infra/grafana/dashboards/errors-overview.json`
  - `infra/grafana/dashboards/ac-slos.json` (verify unchanged)
  - `infra/grafana/dashboards/gc-slos.json` (verify unchanged)
  - `infra/grafana/dashboards/mc-slos.json` (verify unchanged)
- Dependencies: None — purely dashboard JSON changes. No Prometheus config, Grafana provisioning, recording rules, or code changes needed.

## Consequences

### Positive
- Low-traffic environments (Kind, CI, staging) show readable integer counts on counter panels
- Security events (JWT failures, rate limits) are immediately visible as discrete counts
- `$__rate_interval` adapts rate windows to scrape interval and dashboard time range automatically
- Single principled rule (metric type determines function) eliminates ambiguity for future dashboards
- No infrastructure or alerting changes required
- Validation guards unaffected (metric name extraction is function-agnostic)

### Negative
- ~60 panel expressions change function (`rate` → `increase`), requiring careful review
- Y-axis labels need updating on converted panels
- Operators familiar with per-second rates on counter panels need to adjust to reading counts
- `increase()` can produce fractional values due to Prometheus extrapolation (cosmetic, not functional)

### Neutral
- SLO dashboards unchanged — burn-rate math is fundamentally rate-based
- Alert rules unchanged — they run in Prometheus independently of Grafana variables
- Dashboard panel count increases slightly with new stat rows (~6-10 new panels total)
- Existing runbook diagnostic PromQL may reference `rate()` — runbooks should be updated to match

## Participants

- **Observability** (95%): Domain lead. Proposed the metric-type distinction principle. Counter timeseries use `increase($__rate_interval)`, derived metrics use `rate($__rate_interval)`, SLO dashboards keep explicit windows.
- **Infrastructure** (95%): Confirmed no Prometheus/Grafana config changes needed. Validated `$__rate_interval` compatibility with all scrape intervals (15s/10s/5s). No recording rules required.
- **Test** (95%): Confirmed env-tests and validation guards are unaffected. `increase()` on counter timeseries provides CI visibility for test verification.
- **Operations** (94%): Confirmed alert-dashboard parity preserved — ratio-based alerts match rate()-based ratio panels, counter panels without ratio alerts switch safely. SLO dashboard carve-out preserves incident response workflow.
- **Security** (95%): Confirmed security event integer visibility in both stat and timeseries panels. No PII exposure changes. Attack indicators more visible in low-traffic environments.

## Amendment (2026-09-10) — zero-init precondition, window enforcement, empty-is-healthy descriptions

This amendment records three things the original decision left implicit or unenforced. It changes **no**
PromQL function: Category A stays `increase()`, Category B `rate()`, Category C explicit windows.

### A. The zero-init precondition (why `increase()` on a counter is honest)

Category A prescribes `increase(metric[$__range])` on a discrete-event counter. That reading is honest **only
if the counter series exists at 0 from process start.** A `metrics`-crate counter is created lazily — it first
appears in `/metrics` at its first increment, already at that value, with no prior `0` sample — so for a single
low-volume event `increase()` reads 0 *forever* (the ADR-0036 story-1 Join Flow defect). The fix is
**present-at-zero**: services zero-initialize every enumerable discrete-event `*_total` counter at startup
(`zero_initialize_counters()` / eager handle registration), giving each series a `0` point so the first real
event is a visible `0→1` edge. This is **enforced by the `dt-guard counter-zero-init` guard** — a future
service that follows the Category A classification rule but skips zero-init is caught, rather than silently
reproducing this defect. Counters whose label domain is genuinely unbounded/runtime-discovered (raw
`status_code`, free-form `operation`/`table`/`reason`), or whose absence is load-bearing for a liveness alert
(`absent_over_time`), are `Zero-init: exempt` in the catalog with a stated reason.

### B. Window split is now enforced (stat `$__range`, timeseries `$__rate_interval`)

Category A's existing stat-vs-timeseries split (a stat counts over the dashboard range; a timeseries rates over
the scrape-adaptive interval) was documented but unenforced — `dt-guard dashboard-panels` accepted either
window on either panel type. It is now enforced: for a `*_total` **counter** ref, a `stat`/`gauge`/`bargauge`
panel's `increase()` window must be `$__range`, and a `timeseries` panel's must be `$__rate_interval`.
Category B (ratio/quantile) and SLO dashboards are unaffected.

### C. Zero-is-healthy DESCRIPTION convention (NO `noValue`)

A panel over a **catalog-declared expected-empty** counter (one whose every series is a bad event — annotated
`- **Expected-empty**: yes — <why>`) must carry a description marker, in **one of TWO variants selected by the
counter's `Zero-init: exempt` status** (OPS-21), matched case-insensitively and enforced by `dt-guard
dashboard-panels`:
- a **zero-init'd** (non-exempt) expected-empty counter reads a flat `0` when healthy and an **absent series is
  a FAULT**, so its panels must carry **`zero is healthy`**;
- an **exempt** (unbounded / lazily-created) expected-empty counter is **legitimately absent** when healthy, so
  its panels must carry **`empty is healthy`**.

A single mandated phrase across both would be false on the zero-init'd majority — a guard checking presence of a
sentence that is wrong on ~25 of 27 panels, which is the very misreading (absent-as-healthy) this devloop
removes. The variant is not a judgment: it is the `Zero-init: exempt` field the shared parser
(`dt-guard`'s `common::metric_catalog::parse_annotations_dir`) already returns to the guard alongside
`Expected-empty`, so `counter-zero-init` and `dashboard-panels` cannot disagree about which counters are exempt
or expected-empty. Both variants also carry the two mandated statements below.

**Why `noValue: 0` was REJECTED (record this, or it gets re-proposed as an obvious improvement).** A dashboard
sweep initially proposed `noValue: 0` on failure panels; **the user ruled it out** (2026-09-10) on
observability's argument. `noValue` and zero-init fix the SAME defect. After zero-init a healthy failure
counter reads `0`, not "No data", so `noValue` is **dead code** there; and an absent series can now only be a
**fault** (recorder not installed, scrape failing, pod down, wrong job label), so painting it a green `0` would
**MASK** that fault — a "fail loudly, never mask" violation. The walk across all three exemption classes finds
no set where `noValue` both fires and is correct (unbounded = never absent; absence-is-load-bearing = hides the
very absence an `absent_over_time` alert fires on; deliberate-absence-is-signal = contradicts it). The one
in-tree instance (mh-media panel 6) was dead code that never fired and nobody had noticed.

**The description must make TWO statements, and the second is the valuable one:** (a) a flat line at zero is the
healthy reading; (b) **an absent series is a metrics-pipeline fault, not a zero** (name where liveness is read,
and the runbook to open on non-zero). Statement (b) is exactly what `noValue` used to destroy; now that we
stopped painting absence as zero, the description is the only thing teaching an operator to read absence as a
fault. **Two variants, selected by the catalog annotation:** for a **zero-init'd** counter, absence IS a fault
(state it). For a `Zero-init: exempt` counter, absence can be **legitimate** — and for the absence-is-load-
bearing class it is exactly what an `absent_over_time` alert keys on — so a copy-pasted "absence is a fault"
sentence there would be actively wrong; the exempt variant gets its own wording. (After the exempt-list
re-audit the only expected-empty ∩ genuinely-unbounded-exempt counter is `mh_errors_total`; its stat tiles
read "No data" when healthy, mitigated by the exempt-variant description — the `noValue` carve-up for that
intersection was NOT taken, per the user ruling.)

### D. Mixed-class panels (both tokens + per-metric attribution)

A single panel can plot BOTH a zero-init'd expected-empty counter (absent ⇒ broken pipeline) AND an exempt
counter (absent ⇒ healthy). `errors-overview` id 5 "Top Error Types" is the one such panel today
(`mc_messages_dropped_total` zero-init'd; `gc_http_requests_total{status_code=~"[45].."}` and `ac_errors_total`
exempt). An empty such panel is **ambiguous** — the 3am failure is *misattribution*, an operator seeing an empty
panel and concluding "pipeline broken" when only the legitimately-absent exempt series are missing. Three
requirements, enforced by `dt-guard dashboard-panels` (`mixed_panel_attribution`):
1. **Both tokens.** The `needs_empty` test is computed over **all exempt refs on the panel, not only those that
   are also expected-empty** — an exempt ref need not itself be expected-empty to need the `empty is healthy`
   statement. So a mixed panel must carry `zero is healthy` **and** `empty is healthy`. A correct-sounding
   exception clause that names the exempt series but contains neither literal token still reds — the trap that
   caught both reviewers drafting this.
2. **Per-metric attribution, by set-equality (not subset).** The description must name **exactly** the panel's
   exempt refs — every one named verbatim, and none stray. The stray half is load-bearing: a subset test would
   pass a description lifted from a *future second* mixed panel that names that panel's exempt metrics instead.
3. **A `count()` discriminator.** The description must name a `count()` on the zero-init'd metric
   (`count(mc_messages_dropped_total)` — a number ⇒ present-and-genuinely-zero, no data ⇒ broken pipeline). This
   adopts an existing in-tree convention (`client-media.json` ids 3, 8), not a new one.

**`mh-overview` id 27 (`mh_errors_total`) is NOT a mixed panel and needs no change.** Its only counter is exempt,
so `needs_zero` is false and only `empty is healthy` is required — which it carries. (An earlier draft asked for
both markers there; withdrawn — the strict token's "an absent series is a fault" is FALSE for an exempt counter,
which is never zero-init'd, so requiring it would inject the very misattribution this fixes. The exempt variant's
prose already states the flat-zero-is-healthy half under one token.) The widening changes the required-marker set
of exactly **one** panel (id 5); the governed set is identical before and after.

**Residual blind spot (measured, not assumed).** The class test and `metric_not_in_catalog` iterate the *same*
service-prefixed ref extractor (`ac|gc|mc|mh`); a governed panel carrying a non-service-prefixed `_total` ref is
classified on an incomplete set and `metric_not_in_catalog` does **not** backstop it — they share the extractor,
they do not cover each other. Vacuous today by measurement (0 governed panels carry such a ref). It stops being
vacuous when the `dt_client_*` panels (already catalogued in `client.md`) gain a marker after the client exporter
lands — the first instance.

**Known limitation (state it honestly):** the description-marker guard checks that a token is PRESENT, not that
the prose is TRUE — it is a checkbox, so a copy-pasted description that contradicts its own panel passes. The
mixed-class set-equality check narrows this for mixed panels (a description naming another panel's metrics reds),
but for single-class panels a cheap future mitigation is flagging byte-identical descriptions across two
marker-carrying panels; not built.

## Debate Reference

See: `docs/debates/2026-04-01-dashboard-rates-vs-counts/debate.md`
