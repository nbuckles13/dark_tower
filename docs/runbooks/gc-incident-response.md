# GC Service Incident Response Runbook

**Service**: Global Controller (gc-service)
**Owner**: SRE Team
**On-Call Rotation**: PagerDuty - Dark Tower GC Team
**Last Updated**: 2026-02-05

---

## Table of Contents

1. [Severity Classification](#severity-classification)
2. [Escalation Paths](#escalation-paths)
3. [Common Failure Scenarios](#common-failure-scenarios)
   - [Scenario 1: Database Connection Failures](#scenario-1-database-connection-failures)
   - [Scenario 2: High Latency / Slow Responses](#scenario-2-high-latency--slow-responses)
   - [Scenario 3: MC Assignment Failures](#scenario-3-mc-assignment-failures)
   - [Scenario 4: Complete Service Outage](#scenario-4-complete-service-outage)
   - [Scenario 5: High Error Rate](#scenario-5-high-error-rate)
   - [Scenario 6: Resource Pressure](#scenario-6-resource-pressure)
   - [Scenario 7: Token Refresh Failures](#scenario-7-token-refresh-failures)
4. [Diagnostic Commands](#diagnostic-commands)
5. [Recovery Procedures](#recovery-procedures)
6. [Postmortem Template](#postmortem-template)
7. [Maintenance and Updates](#maintenance-and-updates)
8. [Additional Resources](#additional-resources)

---

## Severity Classification

Use this table to classify incidents and determine response times:

| Severity | Description | Response Time | Examples | Escalation |
|----------|-------------|---------------|----------|------------|
| **P1 (Critical)** | Service down, complete meeting join failure | **15 minutes** | All meeting joins failing (>95% error rate), Database unreachable, All pods crash-looping, MC assignment completely broken | Immediate page, escalate to Engineering Lead after 30 min |
| **P2 (High)** | Degraded performance, partial failures | **1 hour** | High latency (p95 > 500ms), 10-50% error rate, Single pod failing, MC assignment slow (>100ms) | Page if persists > 15 min, escalate to Service Owner after 2 hours |
| **P3 (Medium)** | Non-critical issue, workaround available | **4 hours** | Single region affected, Metrics unavailable, Non-critical alerts firing, Token refresh intermittent | Slack notification, escalate if not resolved in 8 hours |
| **P4 (Low)** | Minor issue, no immediate impact | **24 hours** | Log noise, Cosmetic dashboard issues, Deprecated endpoint warnings | Normal ticket, review in next on-call handoff |

### Severity Upgrade Triggers

Automatically upgrade severity if:
- P2 persists for > 2 hours → Upgrade to P1
- P3 affects multiple regions → Upgrade to P2
- Any security breach suspected → Upgrade to P1 + notify Security Team immediately

---

## Escalation Paths

### Initial Response

**On-Call Engineer** (First Responder):
1. Acknowledge alert within 5 minutes
2. Assess severity using table above
3. Post incident notice in `#incidents` Slack channel
4. Begin investigation using diagnostic commands
5. Engage additional specialists as needed

### Escalation Chain

```
On-Call Engineer (0-15 min)
    ↓ (if not resolved in 30 min for P1, 2h for P2)
Service Owner / Tech Lead
    ↓ (if architectural decision needed)
Engineering Manager
    ↓ (if multi-service impact)
Infrastructure Team / SRE Lead
```

### Specialist Contacts

| Team | When to Engage | Contact |
|------|----------------|---------|
| **Database Team** | Database connectivity issues, migration failures, query performance | #database-oncall, PagerDuty: DB-Team |
| **Infrastructure/SRE** | Kubernetes issues, network problems, resource constraints | #infra-oncall, PagerDuty: SRE |
| **AC Team** | Token validation failures, JWKS issues, TokenManager problems | #ac-oncall, PagerDuty: AC-Team |
| **MC Team** | MC assignment failures, MC connectivity issues | #mc-oncall, PagerDuty: MC-Team |
| **Security Team** | Suspected breach, authentication bypass, audit log failures | #security-incidents (CRITICAL ONLY) |
| **Product/Business** | Customer impact assessment, external communications | Engineering Manager escalates |

### External Dependencies

- **PostgreSQL**: Managed by Database Team (see Database Team escalation)
- **Authentication Controller**: Managed by AC Team (JWKS, token validation)
- **Meeting Controller**: Managed by MC Team (meeting assignment)
- **Kubernetes**: Managed by Infrastructure Team
- **Prometheus/Grafana**: Managed by Observability Team (#observability)

---

## Common Failure Scenarios

### Scenario 1: Database Connection Failures

**Alert**: `GCDatabaseDown`
**Severity**: Critical
**Runbook Section**: `#scenario-1-database-connection-failures`

**Symptoms**:
- 503 Service Unavailable on `/ready` endpoint
- Error logs: `connection refused`, `too many connections`, `authentication failed`
- Metrics: `gc_db_queries_total{status="error"}` spiking
- All meeting join requests failing

**Diagnosis**:

```bash
# 1. Check readiness endpoint
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/ready
kill %1
# Expected if failing: {"status":"not_ready","database":"unhealthy","error":"..."}

# 2. Check pod status
kubectl get pods -n dark-tower -l app=gc-service

# 3. Check recent logs for DB errors
kubectl logs -n dark-tower -l app=gc-service --tail=100 | grep -i "database\|connection\|sqlx"

# 4. Check database connectivity from pod
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c "SELECT 1"

# 5. Check connection pool metrics
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/metrics | grep -E "gc_db_"
kill %1

# 6. Check database service status
kubectl get svc -n dark-tower postgresql
kubectl get endpoints -n dark-tower postgresql
```

**Common Root Causes**:

1. **Database Pod Down**: PostgreSQL pod crashed or evicted
   - Check: `kubectl get pods -n dark-tower -l app=postgresql`
   - Fix: Investigate database pod logs, check resource limits

2. **Connection Pool Exhausted**: Too many concurrent connections
   - Check: `gc_db_queries_total` - look for slow queries holding connections
   - Fix: Identify slow queries, increase pool size (if safe), or scale pods

3. **Network Partition**: Network policy blocking traffic
   - Check: `kubectl get networkpolicies -n dark-tower`
   - Fix: Verify network policies allow gc-service → postgresql traffic

4. **Database Credentials Rotated**: Secret updated but pods not restarted
   - Check: Compare secret with running pod env vars
   - Fix: Restart deployment to pick up new secrets

5. **Database Disk Full**: PostgreSQL out of disk space
   - Check: Database team escalation required
   - Fix: Database team handles disk expansion

6. **Replication Lag High**: Replica falling behind primary
   - Check: `kubectl exec -n dark-tower postgres-0 -- psql -c "SELECT client_addr, state, replay_lag FROM pg_stat_replication;"`
   - Fix: Escalate to Database Team

**Remediation**:

```bash
# Option 1: Restart pods to clear stuck connections (quick fix)
kubectl rollout restart deployment/gc-service -n dark-tower

# Expected recovery time: 30-60 seconds

# Option 2: Scale down and up to force new connections
kubectl scale deployment/gc-service -n dark-tower --replicas=0
sleep 5
kubectl scale deployment/gc-service -n dark-tower --replicas=3

# Expected recovery time: 60-90 seconds

# Option 3: Kill slow database queries
kubectl exec -it -n dark-tower postgres-0 -- psql -c "SELECT pid, now() - query_start AS duration, query FROM pg_stat_activity WHERE state = 'active' AND now() - query_start > interval '100 milliseconds' ORDER BY duration DESC;"

# Kill specific query (get PID from above)
kubectl exec -it -n dark-tower postgres-0 -- psql -c "SELECT pg_terminate_backend(<PID>);"

# Expected recovery time: Immediate

# After remediation, verify recovery
kubectl get pods -n dark-tower -l app=gc-service
curl http://localhost:8080/ready
curl http://localhost:8080/metrics | grep gc_db_queries_total
```

**Escalation**: If database is unresponsive for >5 minutes, page Database Team immediately.

---

### Scenario 2: High Latency / Slow Responses

**Alert**: `GCHighLatency`
**Severity**: Critical
**Runbook Section**: `#scenario-2-high-latency--slow-responses`

**Symptoms**:
- Alert: p95 HTTP latency >200ms for >5 minutes
- Timeouts (30s timeout per ADR-0012)
- Slow meeting joins
- Metrics: `gc_http_request_duration_seconds` histogram skewed right

**Diagnosis**:

```bash
# 1. Check current latency metrics
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/metrics | grep gc_http_request_duration_seconds
kill %1

# 2. Check latency by endpoint (identify slow endpoints)
# In Prometheus:
histogram_quantile(0.95, sum by(endpoint, le) (rate(gc_http_request_duration_seconds_bucket[5m])))

# 3. Check database query performance
histogram_quantile(0.95, sum by(operation, le) (rate(gc_db_query_duration_seconds_bucket[5m])))

# 4. Check MC assignment latency (is MC assignment the bottleneck?)
histogram_quantile(0.95, sum by(le) (rate(gc_mc_assignment_duration_seconds_bucket[5m])))

# 5. Check token refresh latency (is AC the bottleneck?)
histogram_quantile(0.95, sum by(le) (rate(gc_token_refresh_duration_seconds_bucket[5m])))

# 6. Check pod resource utilization
kubectl top pods -n dark-tower -l app=gc-service

# 7. Check for slow queries in logs
kubectl logs -n dark-tower -l app=gc-service --tail=1000 | grep -E "duration_ms|slow"
```

**Common Root Causes**:

1. **Database Query Slow**: Unoptimized queries, missing indexes
   - Check: `gc_db_query_duration_seconds{operation="select"}` p99
   - Fix: Database team investigates slow queries, adds indexes

2. **MC Assignment Slow**: MC selection or gRPC calls slow
   - Check: `gc_mc_assignment_duration_seconds` p95
   - Fix: Scale MC pods, optimize MC selection query

3. **Resource Contention**: Insufficient CPU/memory
   - Check: `kubectl top pods` - CPU/memory at limits
   - Fix: Increase resource requests/limits, scale horizontally

4. **High Request Volume**: Unexpected traffic spike
   - Check: `gc_http_requests_total` rate
   - Fix: Scale horizontally, verify not a DDoS attack

5. **Network Latency**: Pod-to-database or pod-to-MC network slow
   - Check: Ping database/MC from pod, check network metrics
   - Fix: Infrastructure team investigates CNI issues

6. **Token Refresh Slow**: AC responding slowly
   - Check: `gc_token_refresh_duration_seconds` p95
   - Fix: Scale AC service, check AC performance

**Remediation**:

```bash
# Scenario A: CPU Bound (CPU >80%)
# Scale horizontally to distribute load
kubectl scale deployment/gc-service -n dark-tower --replicas=5

# Expected recovery time: 30-60 seconds

# Scenario B: Database Slow Queries
# Identify and kill long-running queries
kubectl exec -it -n dark-tower postgres-0 -- psql -c "SELECT pid, now() - query_start AS duration, query FROM pg_stat_activity WHERE state = 'active' ORDER BY duration DESC LIMIT 10;"

# Kill blocking query
kubectl exec -it -n dark-tower postgres-0 -- psql -c "SELECT pg_terminate_backend(<PID>);"

# Expected recovery time: Immediate

# Scenario C: MC Assignment Slow
# Check MC pod health and scale if needed
kubectl get pods -n dark-tower -l app=mc-service
kubectl scale deployment/mc-service -n dark-tower --replicas=5

# Expected recovery time: 30-60 seconds

# Scenario D: Token Refresh Slow
# Check AC service health
kubectl get pods -n dark-tower -l app=ac-service
kubectl rollout restart deployment/ac-service -n dark-tower

# Expected recovery time: 1-2 minutes

# Scenario E: Memory Pressure (temporary mitigation)
kubectl delete pod <POD_NAME> -n dark-tower
# Pod will be recreated by deployment

# Expected recovery time: 30 seconds

# Verify recovery
histogram_quantile(0.95, sum by(le) (rate(gc_http_request_duration_seconds_bucket[5m])))
# Should return value < 0.200
```

**Escalation**:
- If database queries are slow (>100ms p99), escalate to Database Team
- If CPU/memory issues persist after scaling, escalate to Infrastructure Team
- If appears to be attack, escalate to Security Team

---

### Scenario 3: MC Assignment Failures

**Alert**: `GCMCAssignmentFailures`, `GCMCAssignmentSlow`
**Severity**: Critical (failures >5%) / Warning (latency >20ms)
**Runbook Section**: `#scenario-3-mc-assignment-failures`

**Symptoms**:
- MC assignment failure rate >5%
- MC assignment p95 latency >20ms
- Users unable to join meetings (stuck on "Joining..." screen)
- Logs: `MC assignment failed`, `No healthy MCs available`, `MC rejected assignment`

**Diagnosis**:

```bash
# 1. Check MC assignment metrics
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/metrics | grep gc_mc_assignment
kill %1

# 2. Identify assignment failure reasons
# In Prometheus:
sum by(rejection_reason) (rate(gc_mc_assignments_total{status!="success"}[5m]))

# 3. Check MC pod availability
kubectl get pods -n dark-tower -l app=mc-service

# 4. Check MC registrations in database
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c \
  "SELECT id, region, capacity, current_sessions, last_heartbeat FROM meeting_controllers WHERE last_heartbeat > NOW() - INTERVAL '30 seconds' ORDER BY last_heartbeat DESC;"

# 5. Check GC→MC gRPC connectivity
kubectl exec -it deployment/gc-service -n dark-tower -- grpcurl -plaintext mc-service.dark-tower.svc.cluster.local:9090 list

# 6. Check GC logs for assignment errors
kubectl logs -n dark-tower -l app=gc-service --tail=100 | grep "mc_assignment"

# 7. Check MC logs for rejections
kubectl logs -n dark-tower -l app=mc-service --tail=100 | grep -i "reject\|capacity\|assignment"
```

**Common Rejection Reasons** (from ADR-0010):
- `at_capacity`: MC has reached max concurrent sessions
- `draining`: MC is in graceful shutdown
- `unhealthy`: MC failed health check
- `rpc_failed`: gRPC call to MC failed (network/timeout)

**Remediation**:

```bash
# Scenario A: No Healthy MCs (all at_capacity or unhealthy)
# Scale up MC pods
kubectl scale deployment/mc-service -n dark-tower --replicas=5

# Expected recovery time: 30-60 seconds

# Scenario B: MC Pods Down or CrashLoopBackOff
# Check crash reason
kubectl describe pod <MC_POD_NAME> -n dark-tower
kubectl logs <MC_POD_NAME> -n dark-tower --previous

# Force restart
kubectl delete pod <MC_POD_NAME> -n dark-tower

# Expected recovery time: 30-60 seconds

# Scenario C: MC Heartbeats Failing (stale last_heartbeat)
# Test database connection from MC pod
kubectl exec -it deployment/mc-service -n dark-tower -- psql $DATABASE_URL -c "SELECT NOW();"

# Check MC logs for heartbeat errors
kubectl logs -n dark-tower -l app=mc-service --tail=100 | grep -i "heartbeat"

# Restart MC to force re-registration
kubectl rollout restart deployment/mc-service -n dark-tower

# Expected recovery time: 60 seconds

# Scenario D: gRPC Call Failures (rpc_failed)
# Check NetworkPolicy allows GC→MC traffic
kubectl get networkpolicy -n dark-tower -o yaml | grep -A20 "mc-service"

# Verify MC service endpoints are populated
kubectl get endpoints mc-service -n dark-tower

# Expected recovery time: Immediate if config fix

# Scenario E: Database Query Slow (MC selection timeout)
# Check for missing index
kubectl exec -it -n dark-tower postgres-0 -- psql -c "EXPLAIN ANALYZE SELECT * FROM meeting_controllers WHERE last_heartbeat > NOW() - INTERVAL '30 seconds';"

# Add index if missing
kubectl exec -it -n dark-tower postgres-0 -- psql -c "CREATE INDEX CONCURRENTLY idx_mc_last_heartbeat ON meeting_controllers (last_heartbeat);"

# Expected recovery time: Immediate after index creation

# Verify recovery
sum(rate(gc_mc_assignments_total{status="success"}[5m])) / sum(rate(gc_mc_assignments_total[5m]))
# Should return value > 0.99
```

**Escalation**:
- If MC pods crashing repeatedly, escalate to MC Team
- If database connection failures from MC, escalate to Database Team
- If NetworkPolicy issues, escalate to Infrastructure Team

---

### Scenario 4: Complete Service Outage

**Alert**: `GCDown`, `GCPodRestartingFrequently`
**Severity**: Critical
**Runbook Section**: `#scenario-4-complete-service-outage`

**Symptoms**:
- All GC pods in CrashLoopBackOff or Pending state
- 503 Service Unavailable on all endpoints
- No healthy pods in `kubectl get pods -l app=gc-service`
- Alert: `GCDown` firing

**Diagnosis**:

```bash
# 1. Check pod status
kubectl get pods -n dark-tower -l app=gc-service

# 2. Check pod events
kubectl describe pods -n dark-tower -l app=gc-service

# 3. Check recent logs before crash
kubectl logs -n dark-tower -l app=gc-service --previous --tail=100

# 4. Check deployment status
kubectl describe deployment gc-service -n dark-tower

# 5. Check resource quotas
kubectl describe resourcequota -n dark-tower

# 6. Check node status
kubectl get nodes
kubectl describe node <node-name>

# 7. Check for recent deployments
kubectl rollout history deployment/gc-service -n dark-tower
```

**Common Root Causes**:

1. **Bad Deployment**: Recent deployment introduced panic/crash
   - Check: Deployment history, recent changes
   - Fix: Rollback to previous version

2. **Out of Memory**: Pods OOMKilled due to memory limits
   - Check: Pod events show "OOMKilled"
   - Fix: Increase memory limits, investigate memory leak

3. **Missing Secret**: Required secret deleted or corrupted
   - Check: `kubectl get secret -n dark-tower gc-service-secrets`
   - Fix: Restore secret

4. **Missing ConfigMap**: Required ConfigMap deleted
   - Check: `kubectl get configmap -n dark-tower gc-service-config`
   - Fix: Restore ConfigMap

5. **ImagePullBackOff**: Cannot pull container image
   - Check: Pod events show image pull errors
   - Fix: Verify image registry credentials, image exists

6. **Node Failure**: All nodes where pods were scheduled failed
   - Check: `kubectl get nodes` - nodes NotReady
   - Fix: Infrastructure team handles node recovery

7. **Database Unavailable**: Cannot connect to database at startup
   - Check: Database connectivity
   - Fix: See Scenario 1

8. **AC Service Unavailable**: Cannot fetch JWKS at startup
   - Check: AC service health
   - Fix: See Scenario 7

**Remediation**:

```bash
# Option 1: Rollback deployment to last known good version
kubectl rollout undo deployment/gc-service -n dark-tower
kubectl rollout status deployment/gc-service -n dark-tower

# Expected recovery time: 2-3 minutes

# Option 2: Force reschedule pods
kubectl delete pods -n dark-tower -l app=gc-service
# Deployment will recreate them

# Expected recovery time: 30-60 seconds

# Option 3: Manually scale up from zero (if scaled down accidentally)
kubectl scale deployment/gc-service -n dark-tower --replicas=3

# Expected recovery time: 30-60 seconds

# Option 4: Check and restore missing secrets
kubectl get secret -n dark-tower gc-service-secrets
# If missing, recreate from secure backup

# Option 5: Bypass resource limits (emergency only)
kubectl patch deployment/gc-service -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"gc-service","resources":{"limits":{"memory":"1Gi"}}}]}}}}'

# Expected recovery time: 2-3 minutes

# Verify recovery
kubectl get pods -n dark-tower -l app=gc-service
kubectl logs -n dark-tower -l app=gc-service --tail=50
curl http://gc-service.dark-tower.svc.cluster.local:8080/ready
```

**Escalation**:
- If rollback fails, escalate to Engineering Lead immediately
- If node issues, escalate to Infrastructure Team
- If secret/config issues, escalate to Operations Team

---

### Scenario 5: High Error Rate

**Alert**: `GCHighErrorRate`, `GCErrorBudgetBurnRateCritical`, `GCErrorBudgetBurnRateWarning`
**Severity**: Critical
**Runbook Section**: `#scenario-5-high-error-rate`

**Symptoms**:
- Error rate >1% for >5 minutes
- Error budget burning at >10x sustainable rate
- Mix of 4xx and 5xx errors
- Metrics: `gc_http_requests_total{status_code=~"[45].."}` increasing

**Diagnosis**:

```bash
# 1. Check error rate and breakdown by status code
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/metrics | grep gc_http_requests_total
kill %1

# In Prometheus:
# Overall error rate
sum(rate(gc_http_requests_total{status_code=~"[45].."}[5m])) / sum(rate(gc_http_requests_total[5m]))

# Error breakdown by status code
sum by(status_code) (rate(gc_http_requests_total{status_code=~"[45].."}[5m]))

# Error breakdown by endpoint
sum by(endpoint, status_code) (rate(gc_http_requests_total{status_code=~"[45].."}[5m]))

# 2. Check error types in metrics
curl http://localhost:8080/metrics | grep gc_errors_total

# 3. Check logs for error patterns
kubectl logs -n dark-tower -l app=gc-service --tail=200 | grep -i "error\|failed\|panic"

# 4. Check recent deployments
kubectl rollout history deployment/gc-service -n dark-tower

# 5. Check dependency health
curl http://ac-service.dark-tower.svc.cluster.local:8082/ready
curl http://mc-service.dark-tower.svc.cluster.local:8080/ready
```

**Common Root Causes**:

1. **Bad Deployment**: Code regression in recent deployment
   - Check: Deployment history, error logs for stack traces
   - Fix: Rollback deployment

2. **Database Failures**: Intermittent database connectivity
   - Check: `gc_db_queries_total{status="error"}` rate
   - Fix: See Scenario 1

3. **AC Service Failures**: Token validation or JWKS failures
   - Check: 401 errors on authenticated endpoints
   - Fix: See Scenario 7

4. **MC Assignment Failures**: MC assignment returning errors
   - Check: 503 errors on meeting join endpoints
   - Fix: See Scenario 3

5. **Invalid Requests**: Clients sending malformed requests
   - Check: High 400 error rate, request validation errors in logs
   - Fix: Client-side fix required, document issue

6. **Rate Limiting**: Legitimate traffic triggering rate limits
   - Check: 429 error rate, rate limit metrics
   - Fix: Adjust rate limits or scale service

**Remediation**:

```bash
# Step 1: Identify if it's 4xx or 5xx errors
sum by(status_code) (rate(gc_http_requests_total{status_code=~"[45].."}[5m]))

# If mostly 5xx: Service-side issue
# If mostly 4xx: Likely client-side or invalid requests

# Step 2: For 5xx errors - check for recent deployments
kubectl rollout history deployment/gc-service -n dark-tower

# If recent deployment correlates with error spike:
kubectl rollout undo deployment/gc-service -n dark-tower

# Expected recovery time: 2-3 minutes

# Step 3: For 5xx errors - check dependency health
# Database
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c "SELECT 1"

# AC Service
curl http://ac-service.dark-tower.svc.cluster.local:8082/ready

# MC Service
curl http://mc-service.dark-tower.svc.cluster.local:8080/ready

# Step 4: For 4xx errors - analyze request patterns
# Check logs for validation errors
kubectl logs -n dark-tower -l app=gc-service --tail=200 | grep "400\|validation\|invalid"

# Step 5: Scale up if under load
kubectl scale deployment/gc-service -n dark-tower --replicas=5

# Verify recovery
sum(rate(gc_http_requests_total{status_code=~"[45].."}[5m])) / sum(rate(gc_http_requests_total[5m]))
# Should return value < 0.01
```

**Escalation**:
- If recent deployment is cause, escalate to GC Team for root cause analysis
- If database errors, escalate to Database Team
- If AC/MC errors, escalate to respective teams
- If appears to be attack, escalate to Security Team

---

### Scenario 6: Resource Pressure

**Alert**: `GCHighMemory`, `GCHighCPU`
**Severity**: Warning
**Runbook Section**: `#scenario-6-resource-pressure`

**Symptoms**:
- Memory usage >85% for >10 minutes
- CPU usage >80% for >5 minutes
- Increased latency (secondary symptom)
- Pod OOMKilled events (if limit reached)

**Diagnosis**:

```bash
# 1. Check current resource usage
kubectl top pods -n dark-tower -l app=gc-service

# 2. Check resource limits
kubectl describe deployment gc-service -n dark-tower | grep -A 10 "Limits:"

# 3. Check for OOMKilled events
kubectl get events -n dark-tower --field-selector involvedObject.kind=Pod | grep -i "oom\|killed"

# 4. Check memory usage trend in Prometheus
container_memory_working_set_bytes{pod=~"gc-service-.*"}
container_spec_memory_limit_bytes{pod=~"gc-service-.*"}

# 5. Check CPU usage trend
rate(container_cpu_usage_seconds_total{pod=~"gc-service-.*"}[5m])

# 6. Check request rate (is load increasing?)
sum(rate(gc_http_requests_total[5m]))

# 7. Check for memory leaks (memory continuously growing)
# Look at memory over last 24h - if steadily increasing, may be leak
```

**Common Root Causes**:

1. **Traffic Spike**: Legitimate traffic increase
   - Check: Request rate increasing
   - Fix: Scale horizontally

2. **Memory Leak**: Memory continuously growing (rare in Rust)
   - Check: Memory usage over time, never decreasing
   - Fix: Investigate with profiling, restart pods as temporary fix

3. **Insufficient Limits**: Resource limits too low for workload
   - Check: Consistent high utilization even at normal load
   - Fix: Increase resource limits

4. **Slow Queries**: Database queries holding connections
   - Check: High connection pool usage, slow queries
   - Fix: Optimize queries, kill long-running queries

5. **Goroutine/Task Leak**: Async tasks not completing
   - Check: Increasing number of active tasks (if metric available)
   - Fix: Investigate task lifecycle, restart pods

**Remediation**:

```bash
# Option 1: Scale horizontally to distribute load
kubectl scale deployment/gc-service -n dark-tower --replicas=5

# Expected recovery time: 30-60 seconds

# Option 2: Increase resource limits
kubectl patch deployment/gc-service -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"gc-service","resources":{"limits":{"cpu":"2000m","memory":"1Gi"},"requests":{"cpu":"500m","memory":"512Mi"}}}]}}}}'

# Expected recovery time: 2-3 minutes (rolling update)

# Option 3: Restart pods (temporary fix for memory issues)
kubectl rollout restart deployment/gc-service -n dark-tower

# Expected recovery time: 2-3 minutes

# Option 4: Delete specific high-memory pod (if one pod affected)
kubectl delete pod <POD_NAME> -n dark-tower

# Expected recovery time: 30 seconds

# Verify recovery
kubectl top pods -n dark-tower -l app=gc-service
# CPU should be <70%, memory should be <70%
```

**Escalation**:
- If memory leak suspected, escalate to GC Team for profiling
- If infrastructure resource constraints, escalate to Infrastructure Team
- If traffic spike is attack, escalate to Security Team

---

### Scenario 7: Token Refresh Failures

**Alert**: `GCTokenRefreshFailures`
**Severity**: Warning
**Runbook Section**: `#scenario-7-token-refresh-failures`

**Symptoms**:
- Token refresh failure rate >10% for >5 minutes
- GC → MC/MH calls failing with 401
- Logs: `TokenManager: Failed to refresh token`
- Metrics: `gc_token_refresh_total{status="error"}` increasing

**Diagnosis**:

```bash
# 1. Check token refresh metrics
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/metrics | grep gc_token_refresh
kill %1

# 2. Check TokenManager logs
kubectl logs deployment/gc-service -n dark-tower --tail=100 | grep -i "token"

# 3. Check AC service health
kubectl get pods -n dark-tower -l app=ac-service
curl http://ac-service.dark-tower.svc.cluster.local:8082/ready

# 4. Test token endpoint directly from GC pod
kubectl exec -it deployment/gc-service -n dark-tower -- curl -X POST $AC_TOKEN_URL \
  -u "${GC_CLIENT_ID}:${GC_CLIENT_SECRET}" \
  -d "grant_type=client_credentials"

# 5. Check AC logs for rejection reasons
kubectl logs deployment/ac-service -n dark-tower --tail=100 | grep -i "rejected\|invalid\|failed"

# 6. Verify GC client credentials
kubectl get secret gc-service-secrets -n dark-tower -o jsonpath='{.data.GC_CLIENT_SECRET}' | base64 -d
# Compare with what's registered in AC database
```

**Common Root Causes**:

1. **AC Service Down**: AC not running or not healthy
   - Check: `kubectl get pods -l app=ac-service`
   - Fix: Scale/restart AC service

2. **Invalid Credentials**: GC client credentials incorrect or revoked
   - Check: Token request returns 401
   - Fix: Verify credentials in Secret match AC database

3. **Network Connectivity**: Cannot reach AC token endpoint
   - Check: NetworkPolicy, service endpoints
   - Fix: Adjust NetworkPolicy

4. **AC Rate Limiting**: AC rate limiting GC token requests
   - Check: AC logs for rate limit rejections
   - Fix: Adjust rate limits or reduce refresh frequency

5. **Clock Skew**: JWT validation failing due to time mismatch
   - Check: Compare times on GC and AC pods
   - Fix: Verify NTP, restart pods

**Remediation**:

```bash
# Option 1: Restart AC service if unhealthy
kubectl get pods -n dark-tower -l app=ac-service
kubectl rollout restart deployment/ac-service -n dark-tower

# Expected recovery time: 1-2 minutes

# Option 2: Fix GC credentials if incorrect
# Get correct secret from secure storage
kubectl create secret generic gc-service-secrets \
  --from-literal=GC_CLIENT_SECRET="${CORRECT_SECRET}" \
  --namespace dark-tower \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl rollout restart deployment/gc-service -n dark-tower

# Expected recovery time: 2-3 minutes

# Option 3: Verify AC client registration
kubectl exec -it deployment/ac-service -n dark-tower -- psql $DATABASE_URL -c \
  "SELECT client_id, is_active FROM service_credentials WHERE client_id = 'global-controller';"

# If not found or inactive, create/activate (requires admin access)

# Option 4: Check and fix NetworkPolicy
kubectl get networkpolicy -n dark-tower
kubectl describe networkpolicy gc-service -n dark-tower

# Ensure egress to ac-service:8082 is allowed

# Verify recovery
curl http://localhost:8080/metrics | grep 'gc_token_refresh_total{status="success"}'
# Should be incrementing
```

**Escalation**:
- If AC service issue, escalate to AC Team
- If credentials compromised, escalate to Security Team
- If NetworkPolicy issue, escalate to Infrastructure Team

---

## Diagnostic Commands

### Quick Health Check

```bash
# Check service health
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/health      # Liveness (should always return "OK")
curl http://localhost:8080/ready       # Readiness (checks DB + JWKS)
kill %1

# Check pod status
kubectl get pods -n dark-tower -l app=gc-service

# Check recent errors in logs
kubectl logs -n dark-tower -l app=gc-service --tail=100 | grep -i error
```

### Metrics Analysis

```bash
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &

# Get all metrics
curl http://localhost:8080/metrics

# HTTP request metrics
curl http://localhost:8080/metrics | grep gc_http_request

# MC assignment metrics
curl http://localhost:8080/metrics | grep gc_mc_assignment

# Database metrics
curl http://localhost:8080/metrics | grep gc_db_

# Token refresh metrics
curl http://localhost:8080/metrics | grep gc_token_refresh

# Error metrics
curl http://localhost:8080/metrics | grep gc_errors_total

kill %1
```

### Database Queries

```bash
# Connect to database from pod
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL

# Check healthy MC registrations
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c \
  "SELECT id, region, capacity, current_sessions, last_heartbeat FROM meeting_controllers WHERE last_heartbeat > NOW() - INTERVAL '30 seconds' ORDER BY last_heartbeat DESC;"

# Check active meetings (if applicable)
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c \
  "SELECT COUNT(*) FROM meetings WHERE status = 'active';"

# Check for slow queries (requires pg_stat_statements)
# Escalate to Database Team for this analysis
```

### Log Analysis

```bash
# Stream logs in real-time
kubectl logs -n dark-tower -l app=gc-service -f

# Get logs from all pods
kubectl logs -n dark-tower -l app=gc-service --all-containers --tail=200

# Get logs from previous pod instance (after crash)
kubectl logs -n dark-tower <pod-name> --previous

# Search for specific errors
kubectl logs -n dark-tower -l app=gc-service --tail=1000 | grep -E "error|panic|fatal"

# Search for HTTP errors
kubectl logs -n dark-tower -l app=gc-service --tail=1000 | grep -E "status.*[45][0-9][0-9]"

# Search for database errors
kubectl logs -n dark-tower -l app=gc-service --tail=1000 | grep -i "database\|sqlx"

# Search for MC assignment errors
kubectl logs -n dark-tower -l app=gc-service --tail=1000 | grep -i "mc_assignment\|meeting_controller"
```

### Resource Utilization

```bash
# Check CPU and memory usage
kubectl top pods -n dark-tower -l app=gc-service

# Check node resources
kubectl top nodes

# Check resource limits
kubectl describe deployment gc-service -n dark-tower | grep -A 5 "Limits:"

# Check events for resource issues
kubectl get events -n dark-tower --field-selector involvedObject.name=gc-service --sort-by='.lastTimestamp'
```

### Network Debugging

```bash
# Test service connectivity
kubectl run -it --rm debug --image=nicolaka/netshoot --restart=Never -- /bin/bash
# From debug pod:
curl http://gc-service.dark-tower.svc.cluster.local:8080/health
nslookup gc-service.dark-tower.svc.cluster.local
ping gc-service.dark-tower.svc.cluster.local

# Check service endpoints
kubectl get endpoints -n dark-tower gc-service

# Check network policies
kubectl get networkpolicies -n dark-tower

# Test database connectivity
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c "SELECT 1"

# Test AC connectivity
kubectl exec -it deployment/gc-service -n dark-tower -- curl -i $AC_JWKS_URL

# Test MC connectivity
kubectl exec -it deployment/gc-service -n dark-tower -- grpcurl -plaintext mc-service.dark-tower.svc.cluster.local:9090 list
```

---

## Recovery Procedures

### Service Restart Procedure

**When to use**: Minor issues, stuck connections, state corruption

```bash
# 1. Verify current state
kubectl get pods -n dark-tower -l app=gc-service
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/metrics | grep gc_http_requests_total
kill %1

# 2. Perform rolling restart (zero downtime)
kubectl rollout restart deployment/gc-service -n dark-tower

# 3. Monitor rollout
kubectl rollout status deployment/gc-service -n dark-tower

# 4. Verify recovery
kubectl get pods -n dark-tower -l app=gc-service
curl http://gc-service.dark-tower.svc.cluster.local:8080/ready

# 5. Check logs for startup errors
kubectl logs -n dark-tower -l app=gc-service --tail=50
```

**Rollback on failure**:
```bash
kubectl rollout undo deployment/gc-service -n dark-tower
```

---

### Database Failover Procedure

**When to use**: Primary database failure, planned maintenance

**WARNING**: This procedure requires coordination with Database Team. Do NOT execute without Database Team approval.

```bash
# 1. Verify database status
# Escalate to Database Team - they handle failover

# 2. After Database Team completes failover, verify new connection
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c "SELECT 1"

# 3. If pods have stale connections, restart them
kubectl rollout restart deployment/gc-service -n dark-tower

# 4. Verify recovery
curl http://gc-service.dark-tower.svc.cluster.local:8080/ready
# Should show database: "healthy"

# 5. Monitor metrics for 15 minutes
watch -n 10 'kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 & curl -s http://localhost:8080/metrics | grep -E "gc_http_request|gc_db_query|gc_errors"; kill %1'
```

---

### Load Shedding / Traffic Control

**When to use**: Overwhelming traffic, DDoS attack, protecting service from cascading failure

```bash
# Option 1: Scale horizontally (handles legitimate traffic increases)
kubectl scale deployment/gc-service -n dark-tower --replicas=10

# Option 2: Increase resource limits (if CPU/memory constrained)
kubectl patch deployment/gc-service -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"gc-service","resources":{"limits":{"cpu":"2000m","memory":"1Gi"}}}]}}}}'

# Option 3: Emergency IP blocking (Infrastructure Team)
# If under attack, provide attacker IPs to Infrastructure Team
# They update ingress rules or network policies

# Verify traffic levels
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/metrics | grep gc_http_requests_total
kill %1
```

---

## Postmortem Template

Use this template for all P1 and P2 incidents:

```markdown
# Postmortem: [Incident Title]

**Date**: YYYY-MM-DD
**Severity**: P1/P2/P3
**Duration**: [Start time] - [End time] (Total: X hours Y minutes)
**Status**: Resolved / Mitigated / Investigating
**Author**: [On-call engineer name]
**Reviewers**: [Tech Lead, Engineering Manager]

---

## Executive Summary

[1-2 sentences describing what happened and impact]

---

## Impact

**User Impact**:
- Number of affected meeting joins: [metric]
- Error rate: X% (normal: Y%)
- Affected users: [estimate or "all users"]
- Duration of impact: [X minutes/hours]

**Business Impact**:
- Revenue impact: $[estimate] or N/A
- Reputation impact: [description]
- SLA breach: Yes/No - [details]

**Metrics**:
- Peak error rate: [from gc_http_requests_total]
- Peak latency: [from gc_http_request_duration_seconds]
- Total failed requests: [from metrics]

---

## Timeline

All times in UTC. Link to relevant Slack threads, PagerDuty incidents, and dashboards.

| Time (UTC) | Event |
|------------|-------|
| HH:MM | [First alert fired / User report received] |
| HH:MM | [On-call engineer acknowledged] |
| HH:MM | [Investigation began - diagnostic commands run] |
| HH:MM | [Root cause identified: ...] |
| HH:MM | [Remediation started: ...] |
| HH:MM | [Service recovered to baseline] |
| HH:MM | [Incident declared resolved] |
| HH:MM | [Postmortem review completed] |

---

## Root Cause

[Detailed explanation of what caused the incident]

**Technical Details**:
- Component: [e.g., gc-service, database, mc-service]
- Failure mode: [e.g., connection pool exhaustion, OOMKill, MC assignment stuck]
- Why it happened: [e.g., traffic spike exceeded capacity, bug in X, config change Y]

**Contributing Factors**:
- [Factor 1: e.g., Insufficient monitoring of X]
- [Factor 2: e.g., Lack of rate limiting]
- [Factor 3: e.g., Missing alert for Y]

---

## Detection

**How was the incident detected?**
- [ ] Automated alert (name: [alert name])
- [ ] Customer report
- [ ] Manual monitoring
- [ ] Other: [describe]

**Time to detect**: [X minutes from start of issue to detection]

**What went well**:
- [e.g., Alert fired within SLA (15 min for P1)]

**What could be improved**:
- [e.g., Alert threshold too high]

---

## Response

**What went well**:
- [e.g., Diagnostic commands in runbook were accurate]
- [e.g., Rollback completed quickly]

**What could be improved**:
- [e.g., Escalation was delayed]
- [e.g., Runbook missing steps for X scenario]

**Lessons Learned**:
- [Lesson 1]
- [Lesson 2]

---

## Action Items

| Action | Owner | Due Date | Priority | Status |
|--------|-------|----------|----------|--------|
| [Fix root cause: ...] | [Name] | YYYY-MM-DD | P0 | Open |
| [Update runbook: ...] | [Name] | YYYY-MM-DD | P1 | Open |
| [Add alert: ...] | [Name] | YYYY-MM-DD | P1 | Open |

---

## Supporting Information

**Dashboards**:
- [Link to GC Overview dashboard during incident timeframe]
- [Link to GC SLOs dashboard]

**Logs**:
- [Link to log aggregator with relevant query]

**Metrics**:
- [Prometheus query showing incident impact]

**Communication**:
- [Slack #incidents thread]
- [PagerDuty incident link]
```

---

## Maintenance and Updates

**Runbook Ownership**:
- **Primary**: Observability Specialist
- **Reviewers**: GC Service Owner, On-call rotation members

**Review Schedule**:
- After every P1/P2 incident (update within 24 hours)
- Monthly review during on-call handoff
- Quarterly comprehensive review

**Change Process**:
1. Create pull request with runbook updates
2. Review by on-call rotation members
3. Test new diagnostic commands in staging
4. Merge and notify team in #dark-tower-ops channel

**Version History**:
- 2026-02-05: Initial version (consolidated from gc-high-latency.md, gc-mc-assignment-failures.md, gc-database-issues.md)
- [Future updates tracked here]

---

## Additional Resources

- **ADR-0010**: Global Controller Architecture
- **ADR-0011**: Observability Framework
- **ADR-0012**: Infrastructure Architecture
- **Metrics Catalog**: `docs/observability/metrics/gc-service.md` (to be created)
- **SLO Definitions**: `docs/observability/slos.md` (to be created)
- **GC Service Architecture**: `docs/ARCHITECTURE.md` (GC section)
- **Database Schema**: `docs/DATABASE_SCHEMA.md`
- **On-call Rotation**: PagerDuty schedule "Dark Tower GC Team"
- **Slack Channels**:
  - `#incidents` - Active incident coordination
  - `#dark-tower-ops` - Operational discussions
  - `#gc-service` - Service-specific channel
  - `#database-oncall` - Database team escalation
  - `#infra-oncall` - Infrastructure team escalation
  - `#ac-oncall` - AC team escalation
  - `#mc-oncall` - MC team escalation
  - `#security-incidents` - Security team (CRITICAL ONLY)

---

**Remember**: When in doubt, escalate. It's better to involve specialists early than to struggle alone during an incident.
