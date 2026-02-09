# Security Review: GC Observability Dashboards, Alarms, and Runbooks

**Reviewer**: Security Specialist
**Date**: 2026-02-08 (Updated from 2026-02-05)
**Task**: Create GC observability dashboards, alarms, and runbooks per ADR-0011
**Iterations Reviewed**: 1, 2, 3

## Files Reviewed

### Iteration 1 (Initial Implementation)
1. `infra/grafana/dashboards/gc-overview.json` - Grafana dashboard with 13 panels
2. `infra/grafana/dashboards/gc-slos.json` - SLO dashboard with 8 panels
3. `infra/docker/prometheus/rules/gc-alerts.yaml` - 13 Prometheus alerts
4. `docs/observability/dashboards.md` - Dashboard catalog
5. `docs/observability/alerts.md` - Alert catalog
6. `docs/observability/runbooks.md` - Runbook index

### Iteration 2 (Runbook Consolidation)
7. `docs/runbooks/gc-deployment.md` - Deployment runbook (~600 lines)
8. `docs/runbooks/gc-incident-response.md` - Incident response runbook with 7 scenarios (~1000 lines)
9. `infra/docker/prometheus/rules/gc-alerts.yaml` - Updated runbook_url annotations

### Iteration 3 (Test Fixes)
10. `crates/global-controller/tests/auth_tests.rs` - Fixed test for /health plain text
11. `crates/global-controller/tests/health_tests.rs` - Fixed 2 tests for /health plain text

---

## Security Findings

### Verdict: APPROVED

No security vulnerabilities identified. The implementation follows ADR-0011 privacy-by-default principles.

**Finding Summary**:
- BLOCKER: 0
- CRITICAL: 0
- MAJOR: 0
- MINOR: 0
- TECH_DEBT: 3

---

## Detailed Analysis

### 1. PII in Dashboard Queries

**Status**: PASS

Reviewed all PromQL queries in:
- `gc-overview.json` (13 panels)
- `gc-slos.json` (8 panels)

**Verification**:
- No user_id, meeting_id, email, IP address, or participant_id labels in queries
- Queries use only service-level metrics: `endpoint`, `status_code`, `status`, `operation`, `le`, `job`, `pod`
- Labels are bounded (endpoint names, status codes, operation types)
- Cardinality is controlled via `sum by(label)` aggregations

**Example of safe query patterns observed**:
```promql
# Request rate by endpoint (bounded label)
sum by(endpoint) (rate(gc_http_requests_total[5m]))

# Error rate calculation (no PII)
100 * sum(rate(gc_http_requests_total{status_code=~"[45].."}[5m])) / sum(rate(gc_http_requests_total[5m]))

# Memory usage by pod (infrastructure identifier, not PII)
container_memory_usage_bytes{pod=~"global-controller-.*"}
```

---

### 2. PII in Alert Annotations

**Status**: PASS

Reviewed all 13 alert annotations in `gc-alerts.yaml`:

**Verification**:
- `summary` and `description` annotations contain only:
  - Metric values (`{{ $value }}`)
  - Static text describing the condition
  - Prometheus-provided labels (`{{ $labels.pod }}` for pod names)
- No PII-capable labels are exposed (no user_id, meeting_id, IP, etc.)
- Pod names are infrastructure identifiers, not PII

**Alert examples reviewed**:
- `GCDown` - Only references job name
- `GCHighErrorRate` - References metric value only
- `GCHighMemory` - References pod name (infrastructure identifier)
- `GCTokenRefreshFailures` - References status and metric value only

---

### 3. Sensitive Data in Runbooks

**Status**: PASS

Reviewed both consolidated runbooks (Iteration 2):
- `gc-deployment.md` (~600 lines)
- `gc-incident-response.md` (~1000 lines)

**Database Credentials**:
- All database connections use environment variable references (`$DATABASE_URL`)
- Example pattern: `psql $DATABASE_URL -c "SELECT 1;"`
- No hardcoded production credentials

**Secret Handling**:
- Secret creation examples use placeholders:
  ```bash
  kubectl create secret generic gc-service-secrets \
    --from-literal=DATABASE_URL="${DATABASE_URL}" \
    --from-literal=GC_CLIENT_SECRET="${GC_CLIENT_SECRET}"
  ```
- No actual secret values in examples
- Password placeholders clearly marked as `<password>` or `REPLACE_PASSWORD`

**Token Handling**:
- Token endpoint testing uses environment variables for credentials
- Example: `-u "${GC_CLIENT_ID}:${GC_CLIENT_SECRET}"`
- No hardcoded tokens

---

### 4. Command Injection Risk Assessment

**Status**: PASS

Reviewed all shell commands in runbooks for command injection risks:

**Safe Patterns Observed**:
1. **kubectl commands**: All use fixed arguments or environment variables
   - `kubectl get pods -n dark-tower -l app=global-controller`
   - `kubectl exec -it deployment/global-controller -n dark-tower -- psql $DATABASE_URL`

2. **SQL commands**: All use parameterized or fixed queries
   - `SELECT 1` (fixed)
   - `SELECT pid, now() - query_start AS duration FROM pg_stat_activity` (fixed)
   - `SELECT pg_terminate_backend(<PID>)` - requires user to provide PID (safe)

3. **curl commands**: Fixed URLs and headers
   - `curl -i http://localhost:8080/health`
   - `curl http://localhost:8080/metrics`

**Potentially Dangerous Commands Properly Documented**:
- `kubectl delete pod` - Documented with recovery time expectations
- `pg_terminate_backend()` - Requires explicit PID identification first
- `kubectl rollout undo` - Rollback procedure clearly documented

---

### 5. Information Disclosure in Runbooks

**Status**: PASS

**No Sensitive Information Exposed**:
- No internal IP addresses
- No cloud provider account details
- No cluster names beyond namespace (`dark-tower`)
- No secret key values
- No API keys or tokens

**Infrastructure Details (Acceptable for Internal Runbooks)**:
- Service names: `global-controller`, `ac-service`, `meeting-controller`
- Namespace: `dark-tower`
- Port numbers: 8080, 8082, 9090 (standard service ports)
- PostgreSQL service: `postgres.dark-tower.svc.cluster.local`

These are appropriate for internal operational runbooks.

---

### 6. Runbook URL Security

**Status**: PASS

Alert annotations link to runbooks using anchor-based URLs:
```yaml
runbook_url: "https://github.com/yourorg/dark_tower/blob/main/docs/runbooks/gc-incident-response.md#scenario-1-database-connection-failures"
```

**Assessment**:
- Uses HTTPS (secure)
- Uses placeholder org name (`yourorg`) - requires deployment-time replacement
- Anchor links to specific sections (#scenario-N) is safe pattern
- No query parameters that could leak information

---

### 7. Test File Security (Iteration 3)

**Status**: PASS

Reviewed test fixes in:
- `auth_tests.rs`
- `health_tests.rs`

**Verification**:
- Test credentials are clearly test values (`test-gc-secret`, `test-client`)
- Server binds to localhost only (`127.0.0.1:0`)
- No production secrets referenced
- Test token generation uses deterministic test seeds
- PKCS#8 key generation is for testing only

**Security Test Coverage Observed**:
- Algorithm confusion attack tests (`alg:none`, `alg:HS256`)
- Token size boundary tests (8KB limit)
- Expired token rejection
- Future `iat` rejection
- Unknown `kid` rejection
- Malformed token rejection

---

### 8. Escalation Path Security

**Status**: PASS

Reviewed escalation paths in `gc-incident-response.md`:

**Safe Patterns**:
- PagerDuty integration references (no actual credentials)
- Slack channel names (public info)
- Team contact patterns (role-based, not personal info)

**Security-Specific Escalation**:
- Security Team escalation clearly documented for suspected breaches
- Marked as "CRITICAL ONLY" to prevent alert fatigue
- Separate channel (`#security-incidents`) from operational channels

---

### 9. Postmortem Template Security

**Status**: PASS

The postmortem template in `gc-incident-response.md`:
- Does not request PII collection
- Focuses on metrics and technical details
- Timeline captures UTC times (no location inference)
- Communication links reference internal channels only

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| PII in dashboard queries | PASS | No PII labels, bounded cardinality |
| PII in alert annotations | PASS | Only metric values and pod names |
| Credentials in runbooks | PASS | Uses env vars, placeholder examples |
| Command injection risk | PASS | All commands use fixed arguments or env vars |
| Information disclosure | PASS | No sensitive infra details |
| Runbook URLs | PASS | HTTPS, placeholder org, section anchors |
| Test file security | PASS | Localhost only, test creds only |
| Escalation paths | PASS | Role-based, no personal info |
| Postmortem template | PASS | No PII collection |

---

## Verdict

**APPROVED** - No security findings requiring changes.

The implementation correctly follows ADR-0011 privacy-by-default principles:
- No PII in metrics labels
- Bounded cardinality in queries
- Credentials referenced via environment variables
- Safe operational commands in runbooks
- Proper escalation for security incidents

---

## Tech Debt Notes

1. **TECH_DEBT: Grafana access control documentation**
   - Consider adding explicit documentation about folder-based access control for production Grafana deployments.
   - Dashboards have tags that can be used for RBAC.

2. **TECH_DEBT: Runbook org placeholder**
   - The `yourorg` placeholder in runbook URLs should be replaced during deployment automation.
   - Example: `https://github.com/yourorg/dark_tower/` needs actual org name.

3. **TECH_DEBT: Secret rotation documentation**
   - The deployment runbook covers secret rotation but could benefit from a dedicated security section about key/secret lifecycle management.

These are documentation improvements, not security vulnerabilities.

---

**Checkpoint Written**: 2026-02-08
**Reviewer**: Security Specialist
**Previous Review**: 2026-02-05

---

## Reflection

**Knowledge File Updates**:
- Added 1 new pattern to `docs/specialist-knowledge/security/patterns.md`
- Pattern: "Observability Asset Security Review" - covers dashboard query PII checks, alert annotation safety, runbook command injection prevention, and credential handling in operational documentation

**Why This Pattern Was Added**:
This is the first observability security review in the project. Future service observability (AC, MC, MH) will need similar reviews. A fresh security specialist would benefit from a checklist of what to verify: PromQL label safety, annotation content, runbook command patterns, and credential placeholder conventions.

**Not Added**:
- No gotchas added - no unexpected security pitfalls were discovered
- No integration notes added - existing integration patterns (credential handling, tracing safety) already cover the relevant cross-specialist concerns
