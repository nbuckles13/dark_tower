# DRY Review: GC Observability Dashboards, Alarms, and Runbooks

**Reviewer**: DRY Reviewer
**Date**: 2026-02-08 (final review after 3 iterations)
**Implementation**: GC observability per ADR-0011
**Iterations**: 3 (Iteration 1: initial, Iteration 2: runbook consolidation, Iteration 3: test fixes)

---

## Summary

Reviewed GC observability implementation for cross-service duplication patterns across all 3 iterations. The implementation follows the AC service reference pattern closely, which is the intended approach per ADR-0011.

**Key observation**: Iteration 2 consolidated the separate runbooks (gc-high-latency.md, gc-mc-assignment-failures.md, gc-database-issues.md) into the comprehensive two-runbook pattern (gc-deployment.md, gc-incident-response.md) matching the AC service reference implementation. This consolidation REDUCED duplication compared to the original approach.

**Verdict**: APPROVED with TECH_DEBT documentation

---

## Files Reviewed

### Iteration 1 - Initial Implementation

| File | Type | Status |
|------|------|--------|
| `infra/grafana/dashboards/gc-overview.json` | Dashboard | New |
| `infra/grafana/dashboards/gc-slos.json` | Dashboard | New |
| `infra/docker/prometheus/rules/gc-alerts.yaml` | Alerts | New |
| `docs/observability/dashboards.md` | Documentation | New |
| `docs/observability/alerts.md` | Documentation | New |
| `docs/observability/runbooks.md` | Documentation | New |

### Iteration 2 - Runbook Consolidation

| File | Type | Status |
|------|------|--------|
| `docs/runbooks/gc-deployment.md` | Runbook | New (~1000 lines) |
| `docs/runbooks/gc-incident-response.md` | Runbook | New (~1250 lines) |

### Iteration 3 - Test Fixes

| File | Type | Status |
|------|------|--------|
| `crates/global-controller/tests/auth_tests.rs` | Test | Modified |
| `crates/global-controller/tests/health_tests.rs` | Test | Modified |

---

## Comparison with AC Service Reference Implementation

| Artifact | AC Service | GC Service | Pattern Adherence |
|----------|------------|------------|-------------------|
| Dashboard | `ac-service.json` | `gc-overview.json`, `gc-slos.json` | Good - GC adds SLO dashboard |
| Alerts | (planned) | `gc-alerts.yaml` | N/A - GC implements first |
| Deployment Runbook | `ac-service-deployment.md` | `gc-deployment.md` | Excellent - same structure |
| Incident Response Runbook | `ac-service-incident-response.md` | `gc-incident-response.md` | Excellent - same structure |

The GC implementation correctly follows the AC reference pattern established in ADR-0011, including:
- Pre-deployment checklist with identical sections
- 7+ failure scenarios in incident response
- Postmortem template
- Escalation paths and specialist contacts
- Diagnostic commands and recovery procedures

---

## Findings

### TECH_DEBT-1: Dashboard JSON Boilerplate Duplication

**Files**:
- `infra/grafana/dashboards/gc-overview.json`
- `infra/grafana/dashboards/gc-slos.json`
- `infra/grafana/dashboards/ac-service.json`

**Observation**: All dashboards share common JSON structure:
- Identical `annotations.list` configuration (lines 2-16 in each file)
- Identical `editable`, `fiscalYearStartMonth`, `graphTooltip`, `liveNow` settings
- Repeated panel configuration patterns:
  - Timeseries panels with identical `custom` field defaults
  - Gauge panels with identical `reduceOptions` configuration
  - Identical legend/tooltip configuration

**Estimated boilerplate per dashboard**: ~100 lines of repeated configuration

**Recommendation**: When creating MC/MH dashboards, consider:
1. Dashboard templating tool (Grafonnet, Jsonnet) for generating JSON
2. Shared "base dashboard" partial that defines common configuration
3. Panel library with common visualizations (latency percentiles, error rate gauge)

**Severity**: TECH_DEBT (Grafana JSON is inherently verbose; templating would add build complexity)

---

### TECH_DEBT-2: Prometheus Alert Patterns

**Files**:
- `infra/docker/prometheus/rules/gc-alerts.yaml`

**Observation**: Alert patterns that will be duplicated when AC, MC, MH alerts are created:
- `{Service}Down` pattern for availability
- `{Service}HighErrorRate` with 1% threshold
- `{Service}High{CPU,Memory}` resource alerts
- `{Service}ErrorBudgetBurnRate{Critical,Warning}` SLO alerts
- `{Service}PodRestartingFrequently` infrastructure alert

**Common infrastructure alerts** (CPU, memory, restarts) have identical thresholds across services.

**Recommendation**: When creating alerts for other services:
1. Extract common infrastructure alert templates
2. Create recording rules for shared metrics patterns
3. Document patterns in `docs/observability/alert-patterns.md`

**Severity**: TECH_DEBT (service-specific alerts are appropriate; infrastructure alerts could be templatized)

---

### TECH_DEBT-3: Runbook Section Duplication (Intentional Pattern)

**Files**:
- `docs/runbooks/gc-deployment.md`
- `docs/runbooks/gc-incident-response.md`
- `docs/runbooks/ac-service-deployment.md`
- `docs/runbooks/ac-service-incident-response.md`

**Observation**: Both deployment runbooks follow identical section structure:
- Pre-Deployment Checklist (identical subsections)
- Deployment Steps (9 steps, same structure)
- Rollback Procedure (identical format)
- Configuration Reference (table format)
- Common Deployment Issues (5+ scenarios)
- Smoke Tests (5 tests)
- Monitoring and Verification

Both incident response runbooks follow identical structure:
- Severity Classification (P1-P4 table)
- Escalation Paths (same format)
- 7 Common Failure Scenarios (each with Symptoms, Diagnosis, Root Causes, Remediation, Escalation)
- Diagnostic Commands (6 subsections)
- Recovery Procedures (same format)
- Postmortem Template (identical)
- Maintenance and Updates

**Assessment**: This is **intentional consistency**, not problematic duplication. The ADR-0011 pattern establishes that each service should have comprehensive, self-contained runbooks. The structure duplication ensures:
- Operators can find information in consistent locations
- New runbooks for MC/MH follow established patterns
- Training is simplified (learn one pattern, apply to all services)

**Severity**: TECH_DEBT (intentional pattern - template exists at `docs/runbooks/TEMPLATE.md`)

---

### TECH_DEBT-4: Observability Catalog Documentation Pattern

**Files**:
- `docs/observability/dashboards.md`
- `docs/observability/alerts.md`
- `docs/observability/runbooks.md`

**Observation**: All catalog documents follow similar structure:
- Organization section
- Status tables with planned items
- Standards section
- Ownership table
- Deployment/validation instructions

**Assessment**: Consistent pattern across observability documentation. The catalogs serve different purposes (dashboards, alerts, runbooks) but use the same organizational approach.

**Severity**: TECH_DEBT (consistent pattern is a positive; could consider consolidation after all services implemented)

---

## Non-Findings (Acceptable Differences)

### Service-Specific Content

These differences between AC and GC are appropriate and NOT duplication:

1. **Dashboard metrics**: GC uses `gc_*` metrics, AC uses `ac_*` metrics - correct service prefixes
2. **Alert thresholds**: GC has 200ms HTTP latency SLO, MC assignment 20ms SLO - service-specific SLOs
3. **Runbook scenarios**: GC has MC assignment failures, token refresh failures - service-specific failure modes
4. **Configuration**: GC references `AC_JWKS_URL`, `AC_TOKEN_URL` - dependency configuration

### Improvement Over AC Dashboard

The GC dashboards are more comprehensive than the AC dashboard:
- SLO threshold lines on latency panels
- Separate SLO dashboard with error budget tracking
- Resource usage panels (memory, CPU)
- Status gauges with color thresholds

This represents **evolution of the pattern**, not duplication. The AC dashboard could be enhanced to match.

---

## BLOCKER Analysis

Per ADR-0019, BLOCKER findings occur when:
> Code EXISTS in `common` but wasn't used

**Analysis of common crate usage**:

Checked `crates/common/` for existing utilities that should be reused:
- JWT utilities (`crates/common/src/jwt/`) - Not directly applicable to observability implementation
- No existing Grafana dashboard utilities in common
- No existing Prometheus alert utilities in common
- No existing runbook utilities in common

**Result**: No BLOCKER findings. The observability artifacts (dashboards, alerts, runbooks) are infrastructure configuration, not Rust code. There is no existing common code that was ignored.

---

## Verdict

**APPROVED**

Per ADR-0019 tiered severity model:
- **BLOCKER**: 0 (no existing common code ignored)
- **TECH_DEBT**: 4 (documented patterns for future extraction)

All findings are TECH_DEBT level. Per ADR-0019:
> Only BLOCKER blocks; others documented as tech debt

The implementation correctly follows the AC service reference pattern. The identified duplication patterns are either:
1. **Intentional consistency** (runbook structure, catalog format)
2. **Infrastructure boilerplate** (Grafana JSON, Prometheus YAML)
3. **Future extraction candidates** (when MC/MH observability is implemented)

---

## Tech Debt Documentation

### For `.claude/TODO.md`

```markdown
### Observability Tech Debt (from DRY review 2026-02-08)

- [ ] TD-OBS-1: Evaluate Grafonnet/Jsonnet for dashboard templating before MC/MH dashboards
- [ ] TD-OBS-2: Extract common infrastructure alert templates (CPU, memory, restarts) when creating other service alerts
- [ ] TD-OBS-3: Consider single consolidated observability catalog after all 4 services implemented
- [ ] TD-OBS-4: Enhance AC dashboard to match GC pattern (SLO lines, resource panels)
```

### Pattern Observations for Future Services

When implementing MC/MH observability:
1. Copy GC dashboard structure (gc-overview.json, gc-slos.json)
2. Copy GC alert structure (gc-alerts.yaml)
3. Use gc-deployment.md and gc-incident-response.md as templates
4. Service-specific content only (metrics, scenarios, configuration)
5. Register in catalogs (dashboards.md, alerts.md, runbooks.md)

---

## Return Value

```
verdict: APPROVED
finding_count:
  blocker: 0
  tech_debt: 4
checkpoint_exists: true
summary: GC observability follows AC reference pattern correctly. 4 TECH_DEBT findings documented for future dashboard templating, alert extraction, and catalog consolidation. No BLOCKER - no existing common code was ignored.
```

---

## Reflection

**Knowledge update**: Added entry to `gotchas.md` documenting that infrastructure artifacts (dashboards, alerts, runbooks) follow a reference pattern approach rather than code DRY principles. This insight will help future reviews of MC/MH observability avoid incorrectly flagging intentional consistency as duplication.

**Key learning**: The distinction between "following an established pattern" (positive) and "copy-paste code requiring extraction" (negative) is particularly important for infrastructure configuration where templating tools exist but add build complexity.

---

**Reviewed By**: DRY Reviewer
**Review Date**: 2026-02-08
**ADR Reference**: ADR-0019 (DRY Reviewer tiered severity model)
**Next Review**: When MC or MH observability is implemented
