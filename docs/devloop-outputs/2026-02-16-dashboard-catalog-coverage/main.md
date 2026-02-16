# Devloop: Dashboard & Catalog Coverage

## Loop Metadata

| Field | Value |
|-------|-------|
| Task | Add dashboard panels for 27 missing metrics across AC/GC/MC; update catalog docs for 15 undocumented metrics |
| Specialist | observability |
| Mode | full |
| Start Commit | fc276af28b8a5230b966dc09a8498345545a9874 |
| Branch | feature/gc-registered-mc-metrics |
| Date | 2026-02-16 |

## Loop State

| Reviewer | Plan Status | Review Verdict |
|----------|-------------|----------------|
| Security | confirmed | CLEAR |
| Test | confirmed | CLEAR |
| Observability (impl) | n/a | n/a |
| Code Quality | confirmed | CLEAR |
| DRY | confirmed | CLEAR (3 tech debt) |
| Operations | confirmed (caveats) | RESOLVED (1 finding fixed) |

## Phase: complete

## Context

The `validate-application-metrics.sh` guard was tightened (Steps 4 and 5 promoted to hard-fail, _bucket suffix fix). After the suffix fix, actual gaps were 27 dashboard + 15 catalog (down from 32 dashboard initially).

## Changes

### Guard script (`scripts/guards/simple/validate-application-metrics.sh`)
- Step 4: Promoted from warning to hard-fail (metrics in code must be in dashboards)
- Step 4: Fixed _bucket/_count/_sum suffix matching (histogram metrics referenced via suffixes now count as covered)
- Step 5: New — metrics in code must be documented in catalog (`docs/observability/metrics/`)

### Dashboard panels (30 new panels across 3 dashboards)
- AC: 12 panels (key mgmt, database, crypto, rate limiting, audit, admin ops, token validation, HTTP latency)
- GC: 12 panels (token refresh, AC client, gRPC MC, MH selection, fleet health, errors)
- MC: 6 panels (Redis latency, recovery duration, fenced-out, heartbeat latency)

### Catalog documentation (15 metrics)
- AC: 3 metrics added to `ac-service.md`
- GC: 1 metric added to `gc.md` (gc_registered_controllers)
- MC: New `mc.md` created with 11 metrics

### ADR-0003 updates
- 7 pending items added for unwired AC metrics (recording functions exist but not wired to call sites)

## Tech Debt

- **TD-23** (DRY): SLO visual pattern inconsistency across services (MC vector(), AC thresholds, GC none)
- **TD-24** (DRY): AC panels missing editorMode/range target fields that GC/MC include
- **TD-25** (DRY): Catalog file naming inconsistency (ac-service.md vs gc.md vs mc.md)
- **Pre-existing** (Code Quality): GC/MC dashboards lack `job=` filters unlike AC
- **ADR-0003**: 9 AC metrics defined but not wired to call sites (tracked as 7 pending items)

## Iterations

1. Implementation + validation: passed on first attempt

## Human Review (Iteration 2)

**Feedback**: "Fix TD-23 (add vector() SLO reference lines to AC latency panels matching GC/MC pattern) and TD-24 (add explicit editorMode/range fields to AC application-metric panel targets matching GC/MC pattern). Also add a guard check to enforce all Prometheus panel targets in dashboards have explicit editorMode and range fields. TD-25 catalog file rename already done by lead."

**Mode**: light (Security + DRY + Operations reviewers)

### Iteration 2 Loop State

| Reviewer | Review Verdict |
|----------|----------------|
| Security | RESOLVED (1 finding fixed: bcrypt description leak) |
| DRY | CLEAR (all 3 TDs confirmed resolved) |
| Operations | RESOLVED (1 finding fixed: Python null datasource bug) |

### Iteration 2 Changes

#### TD-23: SLO reference lines on AC latency panels
- Panel 10 (Request Latency): `vector(100)` / SLO 100ms
- Panel 29 (DB Query Latency): `vector(0.05)` / SLO 50ms
- Panel 37 (HTTP Request Latency): `vector(0.2)` / SLO 200ms
- All use matching override styling: dashed red line, lineWidth 2

#### TD-24: Explicit editorMode/range on all AC targets
- All 27 AC application-metric targets updated with `"editorMode": "code"` and `"range": true`

#### TD-25: Catalog file renames (done by lead)
- `gc.md` → `gc-service.md`, `mc.md` → `mc-service.md`

#### New guard: Step 6 (target query mode validation)
- Enforces explicit `editorMode` and `range`/`instant` on all Prometheus dashboard targets
- jq primary path with Python3 fallback (recursive panel collection)
- Hard-fail like Steps 4 and 5

#### Bug fixes (from semantic guard + reviewers)
- All `((errors++))` → `((errors++)) || true` across all 6 steps (set -e compatibility)
- Python fallback recursive panel collection (parity with jq recursive descent)
- Python fallback null datasource handling (`"datasource": null`)
- Bcrypt panel description sanitized (removed security mitigation rationale)

### Iteration 2 Validation
- Attempt 1: Failed (semantic guard blocker: ((errors++)) set -e crash)
- Attempt 2: Passed (12/12 guards, compile, format, tests, clippy)

## Tech Debt (Updated)

- ~~**TD-23** (DRY): SLO visual pattern inconsistency~~ — RESOLVED (iteration 2)
- ~~**TD-24** (DRY): AC panels missing editorMode/range~~ — RESOLVED (iteration 2)
- ~~**TD-25** (DRY): Catalog file naming inconsistency~~ — RESOLVED (iteration 2)
- **Pre-existing** (Code Quality): GC/MC dashboards lack `job=` filters unlike AC
- **ADR-0003**: 9 AC metrics defined but not wired to call sites (tracked as 7 pending items)
- **NEW** (Operations): Guard Python3 fallback paths could be removed if jq is a hard dependency — simplifies maintenance

## Summary

**Iteration 1** (full mode): Tightened the application metrics guard (Steps 4+5 hard-fail, histogram suffix fix) and closed all 27 dashboard gaps and 15 catalog gaps across AC/GC/MC services. 30 new Grafana panels added, 15 metrics documented in catalog, 7 pending items added to ADR-0003 for unwired AC metrics.

**Iteration 2** (light mode): Resolved all 3 tech debt items (TD-23/24/25). Added SLO reference lines to AC latency panels, explicit editorMode/range to all AC targets, renamed catalog files for consistency. Added Step 6 guard enforcing target field completeness. Fixed set -e bug in guard error counting, Python fallback bugs, and sanitized a security-sensitive panel description.
