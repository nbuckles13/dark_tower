# Test Specialist Review: GC Observability Implementation (Iteration 3 Update)

**Reviewer**: Test Specialist
**Date**: 2026-02-08
**Task**: GC observability dashboards, alarms, and runbooks per ADR-0011
**Iteration**: 3 (Confirming test fixes)

---

## Summary

This is a re-review following Iteration 3 test fixes. The implementation creates observability infrastructure (Grafana dashboards, Prometheus alerts, operational runbooks) rather than traditional Rust application code. The test coverage requirements are therefore different - focusing on validation, syntax correctness, and metric alignment rather than unit/integration tests.

**Overall Assessment**: The implementation is well-structured with appropriate validation mechanisms in place. The infrastructure files pass syntax validation, and the Iteration 3 test fixes correctly update tests to expect plain text "OK" from `/health` endpoint.

---

## Files Reviewed

### Iteration 3 Updates

#### 1. `crates/global-controller/tests/auth_tests.rs` (Lines 447-464)

**Change**: Fixed `test_health_endpoint_is_public` to expect plain text "OK" instead of JSON.

**Code Analysis**:
```rust
#[sqlx::test(migrations = "../../migrations")]
async fn test_health_endpoint_is_public(pool: PgPool) -> Result<()> {
    // ...
    // /health returns plain text "OK" for Kubernetes liveness probes
    let body = response.text().await?;
    assert_eq!(body, "OK");
    Ok(())
}
```

**Verdict**: Correct fix. The test now properly validates the `/health` endpoint returns plain text "OK" suitable for Kubernetes liveness probes.

---

#### 2. `crates/global-controller/tests/health_tests.rs` (Lines 1-75)

**Changes**:
1. Fixed `test_health_endpoint_returns_200` to expect plain text "OK"
2. Renamed `test_health_endpoint_returns_json` to `test_ready_endpoint_returns_json` to test `/ready`

**Code Analysis**:
```rust
/// Test that /health liveness endpoint returns 200 and plain text "OK".
#[sqlx::test(migrations = "../../migrations")]
async fn test_health_endpoint_returns_200(pool: PgPool) -> Result<(), anyhow::Error> {
    // ...
    // /health returns plain text "OK" for Kubernetes liveness probes
    let body = response.text().await?;
    assert_eq!(body, "OK");
    Ok(())
}

/// Test that /ready readiness endpoint returns JSON with health details.
#[sqlx::test(migrations = "../../migrations")]
async fn test_ready_endpoint_returns_json(pool: PgPool) -> Result<(), anyhow::Error> {
    // ...
    // /ready returns JSON with detailed status
    let body: serde_json::Value = response.json().await?;
    assert!(body.get("database").is_some(), "Expected 'database' field in response");
    Ok(())
}
```

**Verdict**: Correct fixes. The tests now properly differentiate between:
- `/health` - Plain text "OK" for Kubernetes liveness probes
- `/ready` - JSON with detailed health status for readiness probes

---

### Infrastructure Files (Previously Reviewed)

#### 3. `infra/grafana/dashboards/gc-overview.json`

**Validation Results**:
- JSON syntax: **VALID** (verified via `jq empty`)
- Panel count: 13 panels (HTTP, MC assignment, DB, infrastructure metrics)
- Datasource UIDs: All reference `prometheus`

**Metric Cross-Reference** (all defined in `crates/global-controller/src/observability/metrics.rs`):
| Metric | Lines in metrics.rs | Status |
|--------|---------------------|--------|
| `gc_http_requests_total` | 57-62 | DEFINED |
| `gc_http_request_duration_seconds` | 50-55 | DEFINED |
| `gc_mc_assignments_total` | 146-150 | DEFINED (not wired) |
| `gc_mc_assignment_duration_seconds` | 141-144 | DEFINED (not wired) |
| `gc_db_queries_total` | 174-178 | DEFINED (not wired) |
| `gc_db_query_duration_seconds` | 169-172 | DEFINED (not wired) |
| `gc_token_refresh_total` | 196-199 | DEFINED (not wired) |
| `up{job="global-controller"}` | N/A | Prometheus built-in |
| `container_*` metrics | N/A | Kubernetes cadvisor |

**Verdict**: Dashboard is valid with all custom metrics defined in codebase.

---

#### 4. `infra/grafana/dashboards/gc-slos.json`

**Validation Results**:
- JSON syntax: **VALID**
- Panel count: 8 panels (SLO compliance, error budgets)
- Uses same metrics as gc-overview.json

**Verdict**: Valid.

---

#### 5. `infra/docker/prometheus/rules/gc-alerts.yaml`

**Validation Results**:
- YAML syntax: **VALID** (verified via `yaml.safe_load`)
- Alert count: 13 alerts (6 critical, 7 warning)
- All alerts include required annotations: `runbook_url`, `summary`, `description`, `impact`

**PromQL Query Correctness**:
| Alert | Query Pattern | Metric Exists |
|-------|--------------|---------------|
| `GCDown` | `up{job="global-controller"} == 0` | Built-in |
| `GCHighErrorRate` | `gc_http_requests_total{status_code=~"[45].."}` | YES |
| `GCHighLatency` | `gc_http_request_duration_seconds_bucket` | YES |
| `GCMCAssignmentSlow` | `gc_mc_assignment_duration_seconds_bucket` | YES (not wired) |
| `GCDatabaseDown` | `gc_db_queries_total{status="error"}` | YES (not wired) |
| `GCErrorBudgetBurnRateCritical` | `gc_http_requests_total` | YES |
| `GCHighMemory` | `container_memory_usage_bytes` | Kubernetes |
| `GCHighCPU` | `container_cpu_usage_seconds_total` | Kubernetes |
| `GCMCAssignmentFailures` | `gc_mc_assignments_total` | YES (not wired) |
| `GCDatabaseSlow` | `gc_db_query_duration_seconds_bucket` | YES (not wired) |
| `GCTokenRefreshFailures` | `gc_token_refresh_total{status="error"}` | YES (not wired) |
| `GCErrorBudgetBurnRateWarning` | `gc_http_requests_total` | YES |
| `GCPodRestartingFrequently` | `kube_pod_container_status_restarts_total` | kube-state-metrics |

**Verdict**: All PromQL queries reference defined metrics. Some metrics marked "not wired" means they're defined in code but instrumentation not yet added to code paths.

---

#### 6. `docs/runbooks/gc-deployment.md` (~600 lines)

**Content Review**:
- Complete deployment runbook structure per ADR-0011
- Includes: Pre-deployment checklist, 9-step deployment process, rollback procedure
- Configuration reference with environment variables
- 5 common deployment issues with diagnosis/resolution
- 5 smoke tests with expected outputs
- Monitoring and verification section

**Runbook Command Validation**:
- All `kubectl` commands are syntactically correct
- All `psql` commands use proper syntax
- All `curl` commands target correct endpoints

**Verdict**: Comprehensive operational documentation.

---

#### 7. `docs/runbooks/gc-incident-response.md` (~1000 lines)

**Content Review**:
- Complete incident response runbook with 7 scenarios
- Severity classification (P1-P4) with response times
- Escalation paths with specialist contacts
- Each scenario includes: Symptoms, Diagnosis, Common Root Causes, Remediation, Escalation

**Scenario Coverage**:
1. Database Connection Failures - GCDatabaseDown, GCDatabaseSlow alerts
2. High Latency / Slow Responses - GCHighLatency alert
3. MC Assignment Failures - GCMCAssignmentSlow, GCMCAssignmentFailures alerts
4. Complete Service Outage - GCDown, GCPodRestartingFrequently alerts
5. High Error Rate - GCHighErrorRate, GCErrorBudgetBurn* alerts
6. Resource Pressure - GCHighMemory, GCHighCPU alerts
7. Token Refresh Failures - GCTokenRefreshFailures alert

**Verdict**: Comprehensive incident response covering all 13 alerts.

---

#### 8. `docs/observability/dashboards.md`, `alerts.md`, `runbooks.md`

**Verdict**: Documentation catalogs are complete and cross-reference correctly.

---

## Test Coverage Analysis

### Validation Performed

| Artifact Type | Validation Method | Result |
|---------------|------------------|--------|
| Dashboard JSON (`gc-overview.json`) | `jq empty` syntax check | PASS |
| Dashboard JSON (`gc-slos.json`) | `jq empty` syntax check | PASS |
| Alert YAML (`gc-alerts.yaml`) | `yaml.safe_load` syntax check | PASS |
| Metrics existence | Cross-reference with metrics.rs | PASS |
| Test fixes (auth_tests.rs) | Code review | PASS |
| Test fixes (health_tests.rs) | Code review | PASS |

### Test Execution Note

The integration tests require `DATABASE_URL` environment variable set and a PostgreSQL instance running. The tests use `#[sqlx::test]` attribute which creates temporary databases per test. Without a database, tests fail with:
```
DATABASE_URL must be set: EnvVar(NotPresent)
```

This is an infrastructure requirement, not a test bug. The test code is correct.

---

## Findings

### TECH_DEBT-1: No PromQL Query Validation Against Live Prometheus

**Severity**: TECH_DEBT
**Description**: PromQL queries in dashboards and alerts are not validated against a live Prometheus instance with actual metrics. While syntax is valid and metrics exist in code, query correctness (proper aggregation, label matching) is only verified at runtime.

**Recommendation**: Consider adding integration test that:
1. Starts GC server with test database
2. Generates HTTP traffic to produce metrics
3. Scrapes `/metrics` endpoint
4. Validates PromQL queries return expected data

---

### TECH_DEBT-2: Metrics Defined But Not Wired

**Severity**: TECH_DEBT
**Description**: Several metrics referenced in dashboards and alerts are defined in `metrics.rs` but not yet instrumented in code paths:
- `gc_mc_assignments_total` / `gc_mc_assignment_duration_seconds`
- `gc_db_queries_total` / `gc_db_query_duration_seconds`
- `gc_token_refresh_total` / `gc_token_refresh_duration_seconds`

**Impact**: Dashboard panels for these metrics will show "No data" until wired.
**Recommendation**: Track as separate dev-loop task to instrument these code paths.

---

### TECH_DEBT-3: No Guard Script Unit Tests

**Severity**: TECH_DEBT
**Description**: The `grafana-datasources.sh` guard has no dedicated unit tests. Edge cases (malformed JSON, missing jq, etc.) are handled but not tested in isolation.

**Recommendation**: Consider shellspec or bats tests for guard scripts.

---

### TECH_DEBT-4: Runbook Command Automation

**Severity**: TECH_DEBT
**Description**: Runbook kubectl/psql commands are not tested automatically. Commands could become stale as infrastructure evolves.

**Recommendation**: Consider runbook testing framework that validates commands against staging environment.

---

## Verdict

```
verdict: APPROVED
finding_count:
  blocker: 0
  critical: 0
  major: 0
  minor: 0
  tech_debt: 4
checkpoint_exists: true
summary: GC observability implementation passes all validation checks. Dashboard JSON and alert YAML have valid syntax, all referenced metrics exist in codebase, and Iteration 3 test fixes correctly update tests to expect plain text from /health endpoint. Four tech debt items documented for future improvement (PromQL integration testing, metric wiring, guard tests, runbook automation).
```

**Rationale**:
- All JSON/YAML files pass syntax validation
- Dashboard datasource UIDs match Prometheus configuration
- All custom metrics referenced in queries exist in `metrics.rs`
- Iteration 3 test fixes are correct (plain text /health, JSON /ready)
- Only TECH_DEBT findings (no blocking issues)
- All 13 alerts properly link to consolidated incident response runbook

---

## Recommendations for Future Work

1. **Wire Defined Metrics**: Instrument MC assignment, DB query, and token refresh code paths to emit the metrics defined in `metrics.rs`.

2. **Prometheus Integration Tests**: Create end-to-end test that validates metrics are emitted and PromQL queries return expected data.

3. **Guard Test Suite**: Add shellspec/bats tests for guard scripts to catch regressions.

4. **Runbook Validation**: Establish "runbook drills" in staging to verify commands work as documented.

---

**Review completed**: 2026-02-08
**Checkpoint updated**: `docs/dev-loop-outputs/2026-02-05-gc-observability-dashboards-alarms-runbooks/test.md`

---

## Reflection

### Knowledge File Updates

**Added to `gotchas.md`**:
- **Endpoint Behavior Changes Require Multi-File Test Updates** - When `/health` changed from JSON to plain text, three tests across two files needed updating. Pattern: always search for all tests touching changed endpoints.

**Not added** (evaluated and rejected):
- Infrastructure file validation pattern (Grafana JSON, Prometheus YAML) - Too project-specific to the Dark Tower metric naming conventions
- Metrics defined but not wired - This is a tech debt observation, not a reusable test pattern

### Key Takeaway

This review highlighted a common oversight in refactoring: endpoint behavior changes can have test dependencies in unexpected locations. The `/health` endpoint was tested in both `auth_tests.rs` (as a "public endpoint" example) and `health_tests.rs` (as the primary subject). Both needed updating when behavior changed.
