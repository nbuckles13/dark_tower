# Devloop Output: ADR-0031 dashboard-panels guard + dashboard conventions

**Date**: 2026-04-17
**Task**: Implement `scripts/guards/simple/validate-dashboard-panels.sh`, `docs/observability/dashboard-conventions.md`, and `infra/grafana/dashboards/_template-service-overview.json` per ADR-0031 prerequisite #2 + dashboard portion of #4.
**Specialist**: observability (implementer; also domain owner per ADR-0031)
**Mode**: Agent Teams (light)
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `48da61fb0c83ec4697014b8184f847e9e32ffdad` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-dashboard-panels-guard` |
| Implementing Specialist | `observability` |
| Iteration | `1` |
| Security | `CLEAR (security@devloop-dashboard-panels-guard)` |
| Code Quality | `CLEAR (code-reviewer@devloop-dashboard-panels-guard)` |
| Context Reviewer | code-reviewer (third reviewer for --light) |

---

## Task Overview

### Objective

Land the second of three ADR-0031 prerequisite guard+conventions bundles:
1. `scripts/guards/simple/validate-dashboard-panels.sh` — enforces ADR-0029 panel classification + dashboard hygiene
2. `docs/observability/dashboard-conventions.md` — panel layout, bucket naming, units, template vars, legend format
3. `infra/grafana/dashboards/_template-service-overview.json` — starter template

Plus mechanical conformance fixes to existing dashboards as needed.

### Scope

- **Cross-cutting**: Yes (guard runs against all `infra/grafana/dashboards/*.json`)
- **Existing dashboards**: 11 files, ~16.7K lines of JSON
- **Following the lesson**: posture is "mechanical is mechanical, just edit the files." Panel-unit additions, `ds_prometheus` → `$datasource` substitutions, and other mechanical conformance fixes land in-devloop. Service-domain judgment calls (e.g., "is this metric a counter or gauge?") flag to Lead, not tech-debt'd.

### Debate Decision

NOT NEEDED — ADR-0031 + ADR-0029 already define the guard semantics.

---

## Reference

- Spec: `docs/decisions/adr-0031-service-owned-dashboards-alerts.md` §Prerequisite guardrails #2
- Panel classification rules: `docs/decisions/adr-0029-dashboard-metric-presentation.md`
- Alert-rules precedent (for guard structure): `scripts/guards/simple/validate-alert-rules.sh`
- Alert conventions precedent (for conventions-doc structure): `docs/observability/alert-conventions.md`

---

## Implementation Summary

Survey-first approach by implementer: enumerated violation landscape across all 11 existing dashboards (166 panels) BEFORE acting, reported to Lead, confirmed no domain-judgment items (everything mechanical), then proceeded. This is exactly the posture Lead requested ("mechanical is mechanical"). Total turnaround: ~1 hour including survey, implementation, two adjacent-guard updates, and review.

### New artifacts
- `scripts/guards/simple/validate-dashboard-panels.sh` — 5-rule guard (counter/gauge/histogram classification per ADR-0029, units declared, `$datasource` template var, `$__rate_interval` on non-SLO dashboards, canonical metric-name references).
- `scripts/guards/simple/fixtures/dashboard-panels/` — 12 fixtures (4 pass, 8 fail).
- `docs/observability/dashboard-conventions.md` — ~380 lines, mirrors `alert-conventions.md` shape + machine-vs-reviewer rule index.
- `infra/grafana/dashboards/_template-service-overview.json` — 13-panel starter covering every metric-type category.

### Migrations to existing dashboards (all mechanical)
- 11 dashboards: `$datasource` template var added (Prometheus or Loki per dashboard), 404 hardcoded UIDs rewritten to `$datasource`.
- 29 `[5m]` → `[$__rate_interval]` substitutions on non-SLO dashboards.
- 2 ADR-0029 counter-misuse fixes: `ac_audit_log_failures_total` → `sum(increase(...[$__range]))`; `mc_actor_panics_total` → same shape.

### Adjacent-guard updates
- `scripts/guards/simple/grafana-datasources.sh` Check 4: skip `$var` template-variable UIDs (they're resolved at render-time, not UID refs).
- `scripts/guards/simple/validate-kustomize.sh` R-20: exclude `_template-*.json` from configMapGenerator bidirectional check (templates are not shipping dashboards).

### ADR-0029 carve-outs honored
- SLO dashboards (`*-slos.json`): exempt from `$__rate_interval` rule (intentional fixed windows aligned with burn-rate alert shapes per ADR-0029 §Category C).
- Log panels (`type=logs`, datasource=Loki): exempt from metric-type classification (no Prometheus metric semantics apply).

---

## Files Modified

**New** (5):
- `scripts/guards/simple/validate-dashboard-panels.sh`
- `scripts/guards/simple/fixtures/dashboard-panels/` (12 files)
- `docs/observability/dashboard-conventions.md`
- `infra/grafana/dashboards/_template-service-overview.json`

**Modified** (13):
- 11 × `infra/grafana/dashboards/*.json` (mechanical migrations)
- `scripts/guards/simple/grafana-datasources.sh` (Check 4: skip `$var`)
- `scripts/guards/simple/validate-kustomize.sh` (R-20: exclude `_template-*.json`)

Net diff: +793/−472 lines across 13 modified files, plus 5 new files.

---

## Devloop Verification Steps

- L1 (cargo check): PASS
- L2 (cargo fmt --check): PASS
- L3 (guards): **17/17 PASS** — new `validate-dashboard-panels` included. Self-test 12/12 fixtures.
- L4/L5 (tests, clippy): trivial — no Rust changes.
- L6 (cargo audit): pre-existing vulnerabilities (not this devloop's concern).
- L7 (semantic): Lead-judgment SAFE — JSON + shell changes, no Rust/service surface, implementer's survey-first discipline eliminated domain-judgment risk.
- L8 (env-tests): skipped — no Rust/service changes; mechanical Grafana-config-only migration.

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0

Guard structure matches `validate-alert-rules.sh` precedent cleanly (argv-passing, quoted heredoc, `shopt -s nullglob`, regex safety). Dashboard-JSON-hygiene scan on the 784 added lines found zero non-mechanical content — no credentials, hostnames, PII, or IPs introduced.

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 0

**ADR Compliance**: ADR-0029 rules map 1:1 to guard checks (table in verdict). Category-A counter wrapping, Category-B derived-via-rate, Category-C SLO carve-out, `$__rate_interval`, histogram `_bucket` handling all enforced. Items intentionally NOT enforced (Y-axis labels, row structure) explicitly marked `[reviewer-only]` in conventions doc.

Shell/Python quality: `set -euo pipefail`, `bash -n` clean. Consistency with `validate-alert-rules.sh`: same structure, same fixture naming, same conventions-doc shape. Common.sh helpers reused; no reimplementation. Adjacent-guard edits minimal and mechanical.

---

## Rollback Procedure

1. Start commit: `48da61fb0c83ec4697014b8184f847e9e32ffdad`
2. Soft reset: `git reset --soft 48da61f`
3. No schema or deployment changes — simple git revert is sufficient.
