# Observability Specialist Checkpoint

**Last Updated**: 2026-02-06
**Iteration**: 3
**Status**: Fix complete - all verification layers PASS

---

## Iteration 1 Summary

Created comprehensive observability infrastructure for Global Controller:

1. **Grafana Dashboards** (2):
   - `gc-overview.json` - 13 panels (HTTP, MC assignment, DB, service health)
   - `gc-slos.json` - 8 panels (availability, latency, error budget)

2. **Prometheus Alerts** (13):
   - 6 critical: GCDown, GCHighErrorRate, GCHighLatency, GCMCAssignmentSlow, GCDatabaseDown, GCErrorBudgetBurnRateCritical
   - 7 warning: GCHighMemory, GCHighCPU, GCMCAssignmentFailures, GCDatabaseSlow, GCTokenRefreshFailures, GCErrorBudgetBurnRateWarning, GCPodRestartingFrequently

3. **Runbooks** (3 separate):
   - gc-high-latency.md
   - gc-mc-assignment-failures.md
   - gc-database-issues.md

4. **Documentation Catalogs** (3):
   - dashboards.md
   - alerts.md
   - runbooks.md

---

## Iteration 2 Fix: ADR-0011 Runbook Consolidation

### Issue Identified

Runbooks did not follow the updated ADR-0011 two-runbook pattern established by AC service reference implementation:
- AC service has 2 mega-runbooks: `ac-service-deployment.md` and `ac-service-incident-response.md`
- GC had 3 separate smaller runbooks

### Fix Applied

Consolidated GC runbooks into 2 mega-runbooks following AC pattern:

#### 1. gc-deployment.md (~600 lines)

Structure (matching AC deployment runbook):
- Overview
- Pre-Deployment Checklist (code quality, infrastructure, coordination)
- Deployment Steps (9 steps with verification)
- Rollback Procedure
- Configuration Reference (env vars, secrets, ConfigMaps, resource limits)
- Common Deployment Issues (5 scenarios)
- Smoke Tests (5 tests)
- Monitoring and Verification

#### 2. gc-incident-response.md (~1000 lines)

Structure (matching AC incident response runbook):
- Header (service, owner, on-call, last updated)
- Severity Classification (P1-P4 matrix)
- Escalation Paths
- Common Failure Scenarios (7 scenarios):
  1. Database Connection Failures
  2. High Latency / Slow Responses
  3. MC Assignment Failures
  4. Complete Service Outage
  5. High Error Rate
  6. Resource Pressure
  7. Token Refresh Failures
- Diagnostic Commands
- Recovery Procedures
- Postmortem Template
- Maintenance and Updates
- Additional Resources

### Alert Annotation Updates

Updated all 13 alert `runbook_url` annotations to use section anchors:

| Alert | URL |
|-------|-----|
| GCDown | `gc-incident-response.md#scenario-4-complete-service-outage` |
| GCHighErrorRate | `gc-incident-response.md#scenario-5-high-error-rate` |
| GCHighLatency | `gc-incident-response.md#scenario-2-high-latency--slow-responses` |
| GCMCAssignmentSlow | `gc-incident-response.md#scenario-3-mc-assignment-failures` |
| GCDatabaseDown | `gc-incident-response.md#scenario-1-database-connection-failures` |
| GCErrorBudgetBurnRateCritical | `gc-incident-response.md#scenario-5-high-error-rate` |
| GCHighMemory | `gc-incident-response.md#scenario-6-resource-pressure` |
| GCHighCPU | `gc-incident-response.md#scenario-6-resource-pressure` |
| GCMCAssignmentFailures | `gc-incident-response.md#scenario-3-mc-assignment-failures` |
| GCDatabaseSlow | `gc-incident-response.md#scenario-1-database-connection-failures` |
| GCTokenRefreshFailures | `gc-incident-response.md#scenario-7-token-refresh-failures` |
| GCErrorBudgetBurnRateWarning | `gc-incident-response.md#scenario-5-high-error-rate` |
| GCPodRestartingFrequently | `gc-incident-response.md#scenario-4-complete-service-outage` |

### Files Changed

**Created**:
- `/home/nathan/code/dark_tower/docs/runbooks/gc-deployment.md`
- `/home/nathan/code/dark_tower/docs/runbooks/gc-incident-response.md`

**Updated**:
- `/home/nathan/code/dark_tower/infra/docker/prometheus/rules/gc-alerts.yaml` - 13 runbook_url annotations
- `/home/nathan/code/dark_tower/docs/observability/runbooks.md` - Reflect 2-runbook pattern, alert mapping table

**Deleted**:
- `/home/nathan/code/dark_tower/docs/runbooks/gc-high-latency.md`
- `/home/nathan/code/dark_tower/docs/runbooks/gc-mc-assignment-failures.md`
- `/home/nathan/code/dark_tower/docs/runbooks/gc-database-issues.md`

---

## Verification Results (Iteration 2)

| Layer | Status | Notes |
|-------|--------|-------|
| 1. cargo check | PASS | All crates compiled |
| 2. cargo fmt | PASS | Formatted |
| 3. Guards | PASS | 9/9 passed |
| 4. Unit tests | PASS | 129 tests |
| 5. Integration tests | SKIP | PostgreSQL not running |
| 6. Clippy | PASS | No warnings |
| 7. Semantic | PASS | YAML/JSON validated |

**Note**: Integration test skip is infrastructure issue (missing PostgreSQL container), not related to observability file changes.

---

## ADR-0011 Compliance Checklist

### Dashboard Standards
- [x] SLO-aligned panels with threshold lines
- [x] Cardinality-safe queries (no unbounded labels)
- [x] Privacy-by-default (no PII)
- [x] Panel descriptions
- [x] Color-coded thresholds

### Alert Standards
- [x] Severity levels (critical/warning)
- [x] Component labels
- [x] Summary, description, impact annotations
- [x] Runbook URL with section anchor
- [x] Duration tuning (critical: 1-5min, warning: 5-10min)

### Runbook Standards (Two-Runbook Pattern)
- [x] Deployment runbook with 7 sections
- [x] Incident response runbook with 9 sections
- [x] At least 5 deployment scenarios
- [x] At least 7 failure scenarios
- [x] Section anchors for alert linking
- [x] Specific diagnostic commands
- [x] Expected output examples
- [x] Escalation paths
- [x] Postmortem template

---

## Iteration 3 Fix: Integration Test Failures

### Issue Identified

Integration tests in `crates/global-controller/tests/` expected JSON from `/health` endpoint, but endpoint returns plain text "OK" for Kubernetes liveness probes. This is the same issue fixed in `gc-test-utils/src/server_harness.rs` during Iteration 1, but in different test files.

**Failing Tests**:
1. `auth_tests.rs:448-463` - `test_health_endpoint_is_public`
2. `health_tests.rs:10-27` - `test_health_endpoint_returns_200`
3. `health_tests.rs:30-52` - `test_health_endpoint_returns_json`

### Fix Applied

Updated all 3 tests to expect plain text "OK" from `/health`:

**auth_tests.rs** (lines 459-460):
```rust
// Before:
let body: serde_json::Value = response.json().await?;
assert_eq!(body["status"], "healthy");

// After:
let body = response.text().await?;
assert_eq!(body, "OK");
```

**health_tests.rs**:
- `test_health_endpoint_returns_200`: Changed to expect plain text "OK"
- Renamed `test_health_endpoint_returns_json` to `test_ready_endpoint_returns_json` - now tests `/ready` endpoint which returns JSON with detailed health status

### Files Changed

**Updated**:
- `/home/nathan/code/dark_tower/crates/global-controller/tests/auth_tests.rs`
- `/home/nathan/code/dark_tower/crates/global-controller/tests/health_tests.rs`

---

## Verification Results (Iteration 3)

| Layer | Status | Notes |
|-------|--------|-------|
| 1. cargo check | PASS | All crates compiled |
| 2. cargo fmt | PASS | All files formatted |
| 3. Guards | PASS | 9/9 passed |
| 4. Unit tests | PASS | All unit tests passed |
| 5. Integration tests | PASS | 15 auth_tests + 3 health_tests passed |
| 6. Clippy | PASS | No warnings |
| 7. Semantic | PASS | No modified YAML/JSON |

**All 7 verification layers PASS.**

---

## Reflection Summary

### Key Learnings

1. **Two-Runbook Pattern**: Consolidating operational knowledge into deployment + incident response runbooks per service (following AC pattern) improves navigation and ensures all alerts have context. The alert-to-runbook anchoring with specific section IDs enables direct linking.

2. **Dashboard-Alert-Runbook Triangle**: These three artifacts must be designed together. Alert thresholds match dashboard SLO lines, runbooks reference dashboard panels for diagnosis, and alert annotations link to runbook sections.

3. **Health Endpoint Test Coverage**: When endpoint behavior changes (e.g., `/health` from JSON to plain text), tests in multiple locations break - test utilities, integration tests, and unit tests. Search comprehensively before declaring a fix complete.

4. **Cardinality Planning**: Using `sum by(label)` with bounded labels (endpoint, status_code) prevents cardinality explosion while maintaining debuggability. The 1,000 label combination limit from ADR-0011 is practical.

### Knowledge Files Created

Created `docs/specialist-knowledge/observability/`:
- `patterns.md` - 5 patterns (two-runbook structure, alert anchoring, cardinality-safe PromQL, SLO threshold lines, triangle design)
- `gotchas.md` - 5 gotchas (health endpoint tests, anchor format, datasource UID, alert duration, rule file config)
- `integration.md` - 6 integration notes (metric naming, histogram buckets, privacy labels, catalog locations)

---

## Status

Dev-loop complete. Knowledge files created for future observability work.
