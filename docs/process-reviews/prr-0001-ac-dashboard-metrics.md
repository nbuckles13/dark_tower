# PRR-0001: AC Dashboard Metrics Mismatch

**Status**: Closed
**Date**: 2025-12-11
**Trigger**: Grafana AC service dashboard shows no data because it queries wrong metric names
**Participants**: Auth Controller, Infrastructure, Observability, Operations specialists

## Summary

The AC service Grafana dashboard was created with placeholder metric names (`http_requests_total`, `token_issued_total`) instead of the actual metrics defined in ADR-0011 and implemented in the AC service (`ac_token_issuance_total`, `ac_jwks_requests_total`). This resulted in empty dashboards when the local development environment was set up.

## Investigation

### What Happened

1. **ADR-0011 (Observability Framework)** specified the metric naming convention:
   - Format: `<service>_<subsystem>_<metric>_<unit>`
   - AC service prefix: `ac_`

2. **AC service implementation** (`crates/ac-service/src/observability/metrics.rs`) correctly implemented:
   - `ac_token_issuance_total` - Token issuance counter
   - `ac_token_issuance_duration_seconds` - Token issuance latency histogram
   - `ac_jwks_requests_total` - JWKS endpoint counter
   - `ac_errors_total` - Error counter
   - `ac_key_rotation_total` - Key rotation counter

3. **Dashboard created** (`infra/grafana/dashboards/ac-service.json`) used incorrect names:
   - `http_requests_total` (generic, not AC-specific)
   - `token_issued_total` (wrong name)
   - `auth_attempts_total` (doesn't exist)
   - `http_request_duration_seconds_bucket` (generic, not AC-specific)

### Expected Behavior

Dashboard queries should match the actual metrics exported by the service, as defined in ADR-0011 and implemented in the metrics module.

### Root Cause Analysis

1. **Why doesn't dashboard show data?**
   → Dashboard queries reference metric names that don't exist

2. **Why wrong metric names?**
   → Dashboard was created without referencing the AC service metrics implementation or ADR-0011

3. **Why wasn't ADR-0011 referenced?**
   → Infrastructure specialist created dashboard scaffolding before metrics were implemented, using placeholder names

4. **Why wasn't it validated after metrics were implemented?**
   → No cross-specialist review process for dashboard-to-metrics alignment

5. **Why no cross-specialist review?**
   → **Process gap**: Dashboard changes don't trigger Observability specialist review

### Process Gap Identified

**Gap**: Infrastructure/Operations work on dashboards doesn't include mandatory Observability specialist review to validate metric queries against service implementations.

The current code review workflow includes Observability specialist for every review, but:
- Dashboard JSON changes weren't being treated as observability artifacts requiring special attention
- There was no explicit checklist item to verify metric names match implementation
- Documentation ownership in ADR-0011 didn't specify that dashboards must be validated against metrics catalogs

## Specialist Perspectives

### Auth Controller Specialist

- **Understanding**: Responsible for implementing metrics per ADR-0011 naming conventions
- **Information available**: ADR-0011 specifications, metrics module implementation
- **Missing information**: Dashboard was created in parallel; no notification to validate metric names

### Infrastructure Specialist

- **Understanding**: Responsible for Grafana infrastructure and dashboard provisioning
- **Information available**: Standard dashboard patterns, Grafana provisioning
- **Missing information**: Final metric names from ADR-0011; used placeholder names assuming they'd be updated

### Observability Specialist

- **Understanding**: Owns metric naming conventions, dashboard design, metrics catalogs
- **Information available**: ADR-0011, owns `docs/observability/metrics/` documentation
- **Missing information**: Was not explicitly notified when dashboard was created to review metric queries

### Operations Specialist

- **Understanding**: Process owner, owns operational procedures
- **Information available**: Code review workflow, specialist coordination
- **Missing information**: Dashboard creation was not flagged for observability review

## Recommendations

### Immediate Fix

**Owner**: Observability specialist (dashboard content), Infrastructure specialist (deployment)
**Files**: `infra/grafana/dashboards/ac-service.json`

Update metric queries to match actual AC service metrics:

| Panel | Current Query | Correct Query |
|-------|--------------|---------------|
| Request Rate | `http_requests_total{job="ac-service"}` | `sum(rate(ac_token_issuance_total[5m])) + sum(rate(ac_jwks_requests_total[5m]))` |
| Error Rate | `http_requests_total{...status=~"5.."}` / total | `sum(rate(ac_errors_total[5m]))` / request_rate |
| p95 Latency | `http_request_duration_seconds_bucket{...}` | `histogram_quantile(0.95, sum(rate(ac_token_issuance_duration_seconds_bucket[5m])) by (le))` |
| Tokens Issued (1h) | `token_issued_total{...}` | `sum(increase(ac_token_issuance_total[1h]))` |
| Token Issuance Rate | `token_issued_total{...}` | `sum(rate(ac_token_issuance_total[5m])) by (grant_type)` |
| Auth Attempts | `auth_attempts_total{...}` | Remove or replace with `ac_token_issuance_total` by status |

**Note**: Use job label `ac-service-local` for local development Prometheus scrape target.

### Process Improvements

#### 1. Update Observability Specialist Definition

**Target**: `.claude/agents/observability.md`
**Change**: Add explicit responsibility for dashboard-to-metrics validation

Add to "Your Scope":
- Review and validate ALL dashboard metric queries against service implementations
- Ensure dashboard queries match metrics catalog in `docs/observability/metrics/`

Add to "Code Review Documentation Responsibility":
- Dashboard changes are observability artifacts requiring validation against metrics implementation
- Block dashboard PRs where metric names don't match service exports

#### 2. Update Infrastructure Specialist Definition

**Target**: `.claude/agents/infrastructure.md`
**Change**: Add requirement to reference ADRs and implementations when creating dashboards

Add to "You Coordinate With" section under Observability:
- When creating or modifying Grafana dashboards, ALWAYS reference the service's metrics implementation or `docs/observability/metrics/` catalog
- Require Observability specialist sign-off on dashboard metric queries

#### 3. Update Code Review Workflow

**Target**: `.claude/workflows/code-review.md`
**Change**: Add dashboard validation checklist

Add to "Observability Review Checklist":
- [ ] Dashboard metric queries match service metrics catalog (`docs/observability/metrics/`)
- [ ] Dashboard job labels match Prometheus scrape targets
- [ ] Dashboard works in both local development and cloud environments

#### 4. Create AC Metrics Catalog

**Target**: `docs/observability/metrics/ac-service.md`
**Change**: Single source of truth for AC metrics

Document all AC service metrics with:
- Metric name
- Type (counter, histogram, gauge)
- Labels
- Description
- Implementation location

## Implementation Status

- [x] PRR process documentation created (`.claude/workflows/process-review-record.md`)
- [x] PRR document created (`docs/process-reviews/prr-0001-ac-dashboard-metrics.md`)
- [x] Dashboard fixed (`infra/grafana/dashboards/ac-service.json`)
- [x] Observability specialist definition updated (`.claude/agents/observability.md`)
- [x] Infrastructure specialist definition updated (`.claude/agents/infrastructure.md`)
- [x] Code Review workflow updated (`.claude/workflows/code-review.md`)
- [x] AC metrics catalog created (`docs/observability/metrics/ac-service.md`)

## Files Changed

| File | Type | Description |
|------|------|-------------|
| `.claude/workflows/process-review-record.md` | NEW | PRR process documentation |
| `docs/process-reviews/prr-0001-ac-dashboard-metrics.md` | NEW | This PRR document |
| `infra/grafana/dashboards/ac-service.json` | FIX | Correct metric queries |
| `.claude/agents/observability.md` | PROCESS | Add dashboard validation responsibility |
| `.claude/agents/infrastructure.md` | PROCESS | Add ADR reference requirement |
| `.claude/workflows/code-review.md` | PROCESS | Add dashboard checklist |
| `docs/observability/metrics/ac-service.md` | NEW | AC metrics catalog |

## Follow-up

After implementing fixes:
1. Verify dashboard shows data in local development environment
2. Ensure process improvements prevent similar gaps in future dashboards (GC, MC, MH)
3. Consider automated validation of dashboard queries against metrics exports

### Additional Finding (2025-12-13): Datasource UID Mismatch

**Problem**: After fixing metric names, dashboard still showed no data.

**Root Cause**: Datasource UID mismatch between dashboard and Grafana configuration:
- Dashboard referenced `"uid": "prometheus"`
- Datasource config had no explicit UID, so Grafana auto-generated `PBFA97CFB590B2093`

**Fix**: Added explicit `uid: prometheus` to `infra/grafana/provisioning/datasources/datasources.yaml`

**Prevention**: Created CI guard (`scripts/guards/simple/grafana-datasources.sh`) that validates:
- All datasource UIDs referenced in dashboards exist in datasource provisioning configs
- Added to `.github/workflows/ci.yml` to run on every PR

**Files Changed**:
| File | Type | Description |
|------|------|-------------|
| `infra/grafana/provisioning/datasources/datasources.yaml` | FIX | Add explicit `uid: prometheus` and `uid: loki` |
| `scripts/guards/simple/grafana-datasources.sh` | NEW | CI guard for dashboard validation |
| `.github/workflows/ci.yml` | UPDATE | Add Grafana dashboard lint step |

**Process Improvement**: When creating datasource configs, always specify explicit UIDs to match dashboard references. The CI lint will now catch any mismatches before merge.
