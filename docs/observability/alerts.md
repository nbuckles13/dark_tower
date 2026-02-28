# Prometheus Alerts Catalog

This document catalogs all Prometheus alerting rules for Dark Tower services.

## Alert Organization

Alerts are organized by:
- **Severity**: Critical (page immediately), Warning (notify), Info (log only)
- **Service**: Per-service alert groups (AC, GC, MC, MH)
- **Component**: Infrastructure, application logic, database, etc.

All alert rules are stored in `infra/docker/prometheus/rules/` and loaded by Prometheus server.

---

## Alert Severity Levels

### Critical

**Action**: Page on-call engineer immediately
**Response Time**: <15 minutes
**Channels**: PagerDuty + Slack #incidents
**Escalation**: 15min → on-call lead

**Criteria**:
- Service outage or severe degradation
- SLO violation (availability <99.9%, latency above threshold)
- Data loss risk
- Security incident

### Warning

**Action**: Notify team, investigate during business hours
**Response Time**: <1 hour
**Channels**: Slack #alerts
**Escalation**: 1h → service owner

**Criteria**:
- Performance degradation (not yet SLO violation)
- Resource saturation approaching limits
- Non-critical failures (elevated error rate, slow queries)

### Info

**Action**: Log only, no notification
**Response Time**: Best effort
**Channels**: Prometheus logs only
**Escalation**: None

**Criteria**:
- Informational events (deployments, config changes)
- Trend analysis (slow growth patterns)

---

## Global Controller Alerts

**File**: `infra/docker/prometheus/rules/gc-alerts.yaml`

### Critical Alerts

#### GCDown

**Severity**: Critical
**Condition**: No GC pods running for >1 minute
**Impact**: Complete service outage, users cannot join meetings
**Runbook**: [docs/runbooks/gc-down.md](../runbooks/gc-down.md) (to be created)

**PromQL**:
```promql
up{job="gc-service"} == 0
for: 1m
```

**Response**:
1. Check GC pod status (`kubectl get pods`)
2. Check deployment status (`kubectl describe deployment gc-service`)
3. Review logs from crashed pods
4. Escalate to platform team if Kubernetes issue

---

#### GCHighErrorRate

**Severity**: Critical
**Condition**: Error rate >1% for >5 minutes
**Impact**: Availability SLO violation (99.9% target), error budget consumption
**Runbook**: [docs/runbooks/gc-high-error-rate.md](../runbooks/gc-high-error-rate.md) (to be created)

**PromQL**:
```promql
(
  sum(rate(gc_http_requests_total{status_code=~"[45].."}[5m]))
  /
  sum(rate(gc_http_requests_total[5m]))
) > 0.01
for: 5m
```

**Response**:
1. Identify failing endpoints (dashboard or Prometheus)
2. Check for recent deployments (rollback if needed)
3. Check dependency health (database, AC, MC)
4. Scale horizontally if capacity issue

---

#### GCHighLatency

**Severity**: Critical
**Condition**: p95 HTTP latency >200ms for >5 minutes
**Impact**: Latency SLO violation, poor user experience
**Runbook**: [docs/runbooks/gc-high-latency.md](../runbooks/gc-high-latency.md)

**PromQL**:
```promql
histogram_quantile(0.95,
  sum by(le) (rate(gc_http_request_duration_seconds_bucket[5m]))
) > 0.200
for: 5m
```

**Response**:
1. Check latency source (database, MC assignment, token refresh)
2. Check resource utilization (CPU, memory)
3. Investigate slow queries
4. Scale horizontally if CPU bound

---

#### GCMCAssignmentSlow

**Severity**: Critical
**Condition**: MC assignment p95 latency >20ms for >5 minutes
**Impact**: Slow meeting join, critical path degradation
**Runbook**: [docs/runbooks/gc-mc-assignment-failures.md](../runbooks/gc-mc-assignment-failures.md)

**PromQL**:
```promql
histogram_quantile(0.95,
  sum by(le) (rate(gc_mc_assignment_duration_seconds_bucket[5m]))
) > 0.020
for: 5m
```

**Response**:
1. Check MC pod availability
2. Check database query latency (MC selection)
3. Check gRPC connectivity to MC
4. Scale MC if capacity issue

---

#### GCDatabaseDown

**Severity**: Critical
**Condition**: Database query error rate >50% for >1 minute
**Impact**: Complete GC outage, all operations fail
**Runbook**: [docs/runbooks/gc-database-issues.md](../runbooks/gc-database-issues.md)

**PromQL**:
```promql
(
  sum(rate(gc_db_queries_total{status="error"}[1m]))
  /
  sum(rate(gc_db_queries_total[1m]))
) > 0.5
for: 1m
```

**Response**:
1. Check PostgreSQL pod status
2. Test database connectivity from GC
3. Check NetworkPolicy allows GC→DB traffic
4. Escalate to DBA if database corruption or replication issues

---

#### GCErrorBudgetBurnRateCritical

**Severity**: Critical
**Condition**: Error budget burning at >10x sustainable rate for >1 hour
**Impact**: 30-day error budget will be exhausted in <3 days
**Runbook**: [docs/runbooks/gc-high-error-rate.md](../runbooks/gc-high-error-rate.md) (to be created)

**PromQL**:
```promql
(
  sum(rate(gc_http_requests_total{status_code=~"[45].."}[1h]))
  /
  sum(rate(gc_http_requests_total[1h]))
) / 0.001 > 10
for: 1h
```

**Response**:
1. Identify root cause of elevated error rate
2. Check recent deployments (rollback if regression)
3. Check dependency failures
4. Implement immediate mitigation to stop burn rate

---

#### GCMeetingCreationStopped

**Severity**: Critical
**Condition**: Zero meeting creation traffic for >15 minutes, with traffic in the prior hour
**Detection Delay**: ~30 minutes (15m rate window + 15m `for` clause)
**Impact**: Users cannot create meetings, possible service outage
**Runbook**: [docs/runbooks/gc-incident-response.md#scenario-4](../runbooks/gc-incident-response.md#scenario-4-complete-service-outage)

**PromQL**:
```promql
sum(rate(gc_meeting_creation_total[15m])) == 0
and
sum(rate(gc_meeting_creation_total[15m] offset 1h)) > 0
```
`for: 15m`

**Response**:
1. Check GC pod health and logs
2. Verify routing to `/api/v1/meetings` endpoint
3. Check upstream dependencies (database, auth)
4. Check for recent deployments or config changes

---

### Warning Alerts

#### GCHighMemory

**Severity**: Warning
**Condition**: Memory usage >85% for >10 minutes
**Impact**: Risk of OOM kill, pod restart
**Runbook**: [docs/runbooks/gc-high-memory.md](../runbooks/gc-high-memory.md) (to be created)

**PromQL**:
```promql
(
  container_memory_usage_bytes{pod=~"gc-service-.*"}
  /
  container_spec_memory_limit_bytes{pod=~"gc-service-.*"}
) > 0.85
for: 10m
```

**Response**:
1. Check for memory leak (heap profiling)
2. Increase memory limits if needed
3. Restart pod as temporary mitigation
4. Investigate leak in code

---

#### GCHighCPU

**Severity**: Warning
**Condition**: CPU usage >80% for >5 minutes
**Impact**: High CPU may increase latency
**Runbook**: [docs/runbooks/gc-high-cpu.md](../runbooks/gc-high-cpu.md) (to be created)

**PromQL**:
```promql
rate(container_cpu_usage_seconds_total{pod=~"gc-service-.*"}[5m]) > 0.80
for: 5m
```

**Response**:
1. Check request rate (traffic spike?)
2. Profile CPU usage (identify hot paths)
3. Scale horizontally
4. Optimize hot paths in code

---

#### GCMCAssignmentFailures

**Severity**: Warning
**Condition**: MC assignment failure rate >5% for >5 minutes
**Impact**: Some users unable to join meetings
**Runbook**: [docs/runbooks/gc-mc-assignment-failures.md](../runbooks/gc-mc-assignment-failures.md)

**PromQL**:
```promql
(
  sum(rate(gc_mc_assignments_total{status!="success"}[5m]))
  /
  sum(rate(gc_mc_assignments_total[5m]))
) > 0.05
for: 5m
```

**Response**:
1. Check MC pod health
2. Investigate rejection reasons (at_capacity, draining, unhealthy)
3. Scale MC if capacity issue
4. Check MC heartbeat in database

---

#### GCDatabaseSlow

**Severity**: Warning
**Condition**: Database query p99 latency >50ms for >5 minutes
**Impact**: Slow queries may cause HTTP latency SLO violations
**Runbook**: [docs/runbooks/gc-database-issues.md](../runbooks/gc-database-issues.md)

**PromQL**:
```promql
histogram_quantile(0.99,
  sum by(le) (rate(gc_db_query_duration_seconds_bucket[5m]))
) > 0.050
for: 5m
```

**Response**:
1. Identify slow queries (pg_stat_activity)
2. Check for missing indexes
3. Check database resource usage
4. Optimize queries or add indexes

---

#### GCTokenRefreshFailures

**Severity**: Warning
**Condition**: Token refresh failure rate >10% for >5 minutes
**Impact**: Risk of authentication failures for GC→MC/MH calls
**Runbook**: [docs/runbooks/gc-token-refresh-failures.md](../runbooks/gc-token-refresh-failures.md) (to be created)

**PromQL**:
```promql
(
  sum(rate(gc_token_refresh_total{status="error"}[5m]))
  /
  sum(rate(gc_token_refresh_total[5m]))
) > 0.10
for: 5m
```

**Response**:
1. Check AC service health
2. Check network connectivity to AC
3. Check token expiration configuration
4. Review AC logs for rejection reasons

---

#### GCErrorBudgetBurnRateWarning

**Severity**: Warning
**Condition**: Error budget burning at >5x sustainable rate for >6 hours
**Impact**: 30-day error budget will be exhausted in <6 days
**Runbook**: [docs/runbooks/gc-high-error-rate.md](../runbooks/gc-high-error-rate.md) (to be created)

**PromQL**:
```promql
(
  sum(rate(gc_http_requests_total{status_code=~"[45].."}[6h]))
  /
  sum(rate(gc_http_requests_total[6h]))
) / 0.001 > 5
for: 6h
```

**Response**:
1. Investigate error rate trend
2. Identify error sources
3. Plan mitigation before reaching critical burn rate

---

#### GCPodRestartingFrequently

**Severity**: Warning
**Condition**: Pod restart rate >1 per hour for >5 minutes
**Impact**: Service instability, potential crash loop
**Runbook**: [docs/runbooks/gc-pod-crashes.md](../runbooks/gc-pod-crashes.md) (to be created)

**PromQL**:
```promql
rate(kube_pod_container_status_restarts_total{pod=~"gc-service-.*"}[1h]) > 0.016
for: 5m
```

**Response**:
1. Check logs from crashed pods
2. Check for OOM kills (memory limits)
3. Check liveness probe configuration
4. Investigate panic/crash causes in code

---

#### GCMeetingCreationFailureRate

**Severity**: Warning
**Condition**: Meeting creation failure rate >5% for >5 minutes
**Impact**: Some users unable to create meetings
**Runbook**: [docs/runbooks/gc-incident-response.md#scenario-5](../runbooks/gc-incident-response.md#scenario-5-high-error-rate)

**PromQL**:
```promql
(
  sum(rate(gc_meeting_creation_total{status="error"}[5m]))
  /
  sum(rate(gc_meeting_creation_total[5m]))
) > 0.05
and
sum(rate(gc_meeting_creation_total[5m])) > 0
```
`for: 5m`

**Response**:
1. Check "Meeting Creation Failures by Type" dashboard panel for error breakdown
2. Investigate top error types (db_error, code_collision, forbidden/limit exhaustion)
3. Check database health and query latency
4. Check org concurrent meeting limits if `forbidden` errors dominate

---

#### GCMeetingCreationLatencyHigh

**Severity**: Warning
**Condition**: Meeting creation p95 latency >500ms for >5 minutes
**Threshold Rationale**: 500ms is higher than the 200ms aggregate HTTP SLO because meeting creation involves DB writes, CSPRNG code generation, and atomic limit-check CTE. The aggregate `GCHighLatency` alert covers SLO violations at 200ms.
**Impact**: Slow meeting creation experience
**Runbook**: [docs/runbooks/gc-incident-response.md#scenario-2](../runbooks/gc-incident-response.md#scenario-2-high-latency--slow-responses)

**PromQL**:
```promql
histogram_quantile(0.95,
  sum by(le) (rate(gc_meeting_creation_duration_seconds_bucket[5m]))
) > 0.500
```
`for: 5m`

**Response**:
1. Check "Meeting Creation Latency" dashboard panel for latency trend
2. Check database query latency (create_meeting operation)
3. Investigate code generation performance (collision retries)
4. Check resource utilization (CPU, memory)

---

## Authentication Controller Alerts

**Status**: 🚧 To be created
**File**: `infra/docker/prometheus/rules/ac-alerts.yaml` (planned)

**Planned Critical Alerts**:
- `ACDown` - No AC pods running
- `ACHighTokenIssuanceLatency` - Token issuance p99 >350ms
- `ACHighTokenValidationErrorRate` - Validation errors >1%
- `ACKeyRotationFailed` - Key rotation failed

**Planned Warning Alerts**:
- `ACHighCPU` - CPU >80%
- `ACHighMemory` - Memory >85%
- `ACJWKSCacheMissRate` - JWKS cache miss rate >10%

---

## Meeting Controller Alerts

**Status**: 🚧 To be created
**File**: `infra/docker/prometheus/rules/mc-alerts.yaml` (planned)

**Planned Critical Alerts**:
- `MCDown` - No MC pods running
- `MCHighSessionJoinLatency` - Session join p99 >500ms
- `MCSessionJoinFailureRate` - Join failures >5%

**Planned Warning Alerts**:
- `MCHighSessionCount` - Sessions approaching capacity
- `MCHighCPU` - CPU >80%
- `MCWebTransportConnectionFailures` - Connection failures >5%

---

## Media Handler Alerts

**Status**: 🚧 To be created
**File**: `infra/docker/prometheus/rules/mh-alerts.yaml` (planned)

**Planned Critical Alerts**:
- `MHDown` - No MH pods running
- `MHHighAudioLatency` - Audio forwarding p99 >30ms
- `MHHighPacketLoss` - Packet loss >1%

**Planned Warning Alerts**:
- `MHHighJitter` - Jitter p99 >20ms
- `MHHighCPU` - CPU >80%
- `MHForwardingQueueBacklog` - Queue depth high

---

## Alert Configuration Standards

All alerts must follow these standards (per ADR-0011):

### 1. Alert Naming Convention

**Format**: `{SERVICE}{COMPONENT}{CONDITION}`

**Examples**:
- `GCDown` (Global Controller Down)
- `GCHighLatency` (Global Controller High Latency)
- `ACKeyRotationFailed` (Auth Controller Key Rotation Failed)

### 2. Required Fields

Every alert must include:
- ✅ `alert`: Alert name (follows naming convention)
- ✅ `expr`: PromQL expression (cardinality-safe)
- ✅ `for`: Duration threshold (prevents flapping)
- ✅ `labels.severity`: critical, warning, or info
- ✅ `labels.service`: Service name
- ✅ `labels.component`: Component affected
- ✅ `annotations.summary`: One-line description with value
- ✅ `annotations.description`: Detailed explanation
- ✅ `annotations.impact`: User/business impact
- ✅ `annotations.runbook_url`: Link to runbook

### 3. Threshold Selection

- **Critical**: SLO violations, service outages
- **Warning**: Approaching limits, non-critical degradation
- **Info**: Informational, trend tracking

### 4. Duration (`for` clause)

- **Critical**: 1-5 minutes (fast detection)
- **Warning**: 5-10 minutes (avoid flapping)
- **Info**: 10-30 minutes (trend detection)

### 5. Runbook Requirement

Every alert MUST link to a runbook with:
- Symptom description
- Impact assessment
- Diagnosis steps (PromQL queries, kubectl commands)
- Mitigation actions (immediate + long-term)
- Escalation paths

---

## Alert Routing

**Configuration**: Prometheus Alertmanager (`infra/docker/prometheus/alertmanager.yml`)

### Critical Alerts

```yaml
route:
  routes:
    - match:
        severity: critical
      receiver: pagerduty-critical
      group_wait: 10s
      group_interval: 5m
      repeat_interval: 1h
      continue: true
    - match:
        severity: critical
      receiver: slack-incidents
```

**Channels**:
- PagerDuty (immediate page)
- Slack #incidents

**Escalation**: 15min → on-call lead

### Warning Alerts

```yaml
route:
  routes:
    - match:
        severity: warning
      receiver: slack-alerts
      group_wait: 30s
      group_interval: 10m
      repeat_interval: 4h
```

**Channels**:
- Slack #alerts

**Escalation**: 1h → service owner

---

## Alert Fatigue Prevention

Per ADR-0011, the following controls prevent alert fatigue:

### 1. Deduplication

- Group similar alerts (same service, same issue)
- Deduplication window: 5 minutes

### 2. Severity Bumping

- Warning → Critical if firing >30 minutes
- Implemented in Alertmanager routing

### 3. Volume Limiting

- Max 20 alerts/hour per service
- If exceeded: Suppress and page SRE lead
- Indicates widespread issue requiring urgent attention

### 4. Alert Quality Metrics

Track alert quality:
```promql
# Alert precision (true positive rate)
alerts_fired_total{resolution="true_positive"} / alerts_fired_total

# Time to resolution
histogram_quantile(0.95, alert_resolution_duration_seconds)
```

---

## Alert Testing

Before deploying alerts, test:

1. **PromQL Validation**: Test query returns expected results
   ```bash
   # Test query against Prometheus
   curl -G 'http://localhost:9090/api/v1/query' \
     --data-urlencode 'query=up{job="gc-service"} == 0'
   ```

2. **Alert Simulation**: Use `amtool` to simulate alerts
   ```bash
   amtool alert add alertname=GCDown \
     severity=critical \
     service=gc-service
   ```

3. **Runbook Validation**: Verify runbook exists and is accessible
   ```bash
   curl -I https://github.com/yourorg/dark_tower/blob/main/docs/runbooks/gc-high-latency.md
   ```

4. **Cardinality Check**: Ensure labels don't explode cardinality
   ```promql
   # Count unique label combinations
   count by(__name__, job, severity) (ALERTS)
   ```

---

## Alert Ownership

| Alert Group | Owner | Reviewer | Last Updated |
|-------------|-------|----------|--------------|
| GC Critical | Observability | GC Team + Operations | 2026-02-28 |
| GC Warning | Observability | GC Team | 2026-02-28 |
| AC Critical | Observability | AC Team + Operations | TBD |
| MC Critical | Observability | MC Team + Operations | TBD |
| MH Critical | Observability | MH Team + Operations | TBD |

**Update Frequency**: Review quarterly or after major SLO changes.

---

**Last Updated**: 2026-02-28
**Maintained By**: Observability Specialist + Operations Team
**Related Documents**:
- [ADR-0011: Observability Framework](../decisions/adr-0011-observability-framework.md)
- [Runbook Index](./runbooks.md)
- [Dashboard Catalog](./dashboards.md)
- [SLO Definitions](./slos.md) (to be created)
