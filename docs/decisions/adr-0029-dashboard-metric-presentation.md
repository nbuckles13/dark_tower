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

## Debate Reference

See: `docs/debates/2026-04-01-dashboard-rates-vs-counts/debate.md`
