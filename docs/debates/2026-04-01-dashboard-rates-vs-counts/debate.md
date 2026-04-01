# Debate: Dashboard Metrics Presentation — Rates vs Counts

**Date**: 2026-04-01
**Status**: Complete
**Participants**: Observability, Infrastructure, Operations, Security, Test
**ADR**: [ADR-0029](../../decisions/adr-0029-dashboard-metric-presentation.md)

> **Note**: When cross-cutting specialists (Security, Test, Observability, Operations) score < 70 satisfaction at consensus, this requires explicit user risk acceptance — not implicit majority override. See ADR-0024 S5.7.

## Question

Should our Grafana dashboards display request counts instead of request rates (per-second) as the default metric presentation? What is the right approach for panels that currently use `rate()` so they work well in both low-traffic (local dev/Kind) and high-traffic (production) environments?

## Context

Currently, nearly all dashboard panels across AC, GC, MC, and the errors-overview dashboard use `rate(...[5m])` for traffic metrics (151 rate() calls, 0 uses of `$__rate_interval`). In low-traffic environments (local dev with Kind, early staging), these rates often round to zero because a handful of requests over 5 minutes yields fractional per-second values that Grafana displays as 0.

Alert rules in `gc-alerts.yaml` and `mc-alerts.yaml` also rely heavily on `rate()` for threshold comparisons.

## Positions

### Initial Positions (Round 1)

| Specialist | Position | Satisfaction |
|------------|----------|--------------|
| Observability | Keep rate() for all timeseries, replace [5m] with $__rate_interval, add stat panels | 85% |
| Infrastructure | Same as observability — $__rate_interval, stat panels, no recording rules | 85% |
| Operations | Keep rate() default, add toggle variable or companion panels | 75% |
| Security | Switch stat/counter panels to increase() for security event visibility | 75% |
| Test | Switch overview counter panels to increase() for CI visibility | 75% |

### Final Positions (Round 3)

| Specialist | Position | Satisfaction |
|------------|----------|--------------|
| Observability | Metric-type distinction: counters→increase(), ratios/histograms→rate() | 95% |
| Infrastructure | Full support for metric-type distinction, no infra changes needed | 95% |
| Test | increase() on counter timeseries provides CI visibility | 95% |
| Operations | Alert-dashboard parity preserved, SLO carve-out accepted | 93% |
| Security | Security event integer visibility in stat and timeseries panels | 92% |

## Discussion

### Round 1 — Initial Positions

All five specialists agreed on:
- Keep `rate()` for ratio/histogram/SLO panels
- Replace hardcoded `[5m]` with `$__rate_interval`
- Alert rules must stay unchanged

Disagreements:
- **Operations** proposed a `$view_mode` toggle variable; **Observability** opposed it as adding complexity since many panels only make sense one way
- **Security** and **Test** wanted counter panels to *default* to `increase()`, not just add companion stat panels

### Round 2 — Convergence on Stat Panels

- **Operations** dropped the toggle variable idea (moved to 82%) after acknowledging the complexity argument
- **Security** accepted stat panels as sufficient for integer visibility (moved to 82%)
- **Observability** agreed security stat panels should show counts by default (moved to 90%)
- **Test** pushed back that `$__rate_interval` alone doesn't fix CI visibility for counter timeseries (held at 82%)

### Round 3 — Metric-Type Distinction

**Observability** revised to a principled position: the dividing line should be **metric semantics**, not **panel type**:
- Discrete event counters (`_total` metrics) → `increase()` in both timeseries and stat panels
- Derived metrics (ratios, histograms, burn rates) → `rate()`

This addressed Test's and Security's core concerns while satisfying Operations' alert-parity requirement (ratio alerts match ratio panels). All specialists converged to 90%+.

## Consensus

**Reached at Round 3.** All 5 participants at 90%+ satisfaction.

## Decision

ADR-0029: Dashboard Metric Presentation — Counters vs Rates

Three-category approach based on metric type:
1. **Category A** — Discrete event counters: `increase($__rate_interval)` for timeseries, `increase($__range)` for stat panels
2. **Category B** — Derived/normalized metrics: `rate($__rate_interval)` replacing hardcoded windows
3. **Category C** — SLO dashboards: keep explicit windows matching alert rules

Plus new Traffic Summary and Security Events stat rows on overview dashboards. Alert rules unchanged.
