# GC Service Incident Response Runbook

**Service**: Global Controller (gc-service)
**Owner**: SRE Team
**On-Call Rotation**: PagerDuty - Dark Tower GC Team
**Last Updated**: 2026-08-03

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
   - [Scenario 8: Meeting Creation Limit Exhaustion](#scenario-8-meeting-creation-limit-exhaustion)
   - [Scenario 9: Meeting Code Collision](#scenario-9-meeting-code-collision)
   - [Scenario 10: Telemetry Proxy High Rejection Rate](#scenario-10-telemetry-proxy-high-rejection-rate)
   - [Scenario 11: Telemetry Ingest Silent](#scenario-11-telemetry-ingest-silent)
   - [Scenario 12: CORS Misconfiguration Post-Deploy](#scenario-12-cors-misconfiguration-post-deploy)
   - [Scenario 13: Telemetry Proxy Unreachable / Rate-Limit Storm](#scenario-13-telemetry-proxy-unreachable--rate-limit-storm)
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
| **Client/Web-App Team** | Telemetry silent or rejected after a web-app/SDK rollout, CORS origin misconfiguration, client-side join failures | #client-oncall, PagerDuty: Client-Team |
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
sum by(rejection_reason) (increase(gc_mc_assignments_total{status!="success"}[5m]))

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
sum by(status_code) (increase(gc_http_requests_total{status_code=~"[45].."}[5m]))

# Error breakdown by endpoint (now includes guest-token path)
sum by(endpoint, status_code) (increase(gc_http_requests_total{status_code=~"[45].."}[5m]))
# Note: /api/v1/meetings/{code}/guest-token contributes to gc_meeting_join_*
# time series alongside /api/v1/meetings/{code} (since ADR-0032 Step 5,
# 2026-04-27). To distinguish authenticated-vs-guest failures: filter by
# `participant=user|guest` on gc_meeting_join_failures_total, or by `endpoint`
# on gc_http_requests_total, then cross-reference error_type counts.
#
# Note: `error_type=guests_disabled` and `error_type=bad_request` are new label
# values introduced in ADR-0032 Step 5 (2026-04-27). Expect them to first appear
# on error_type-breakdown panels when guest-flow traffic hits production. A new
# time series with no historical baseline does NOT indicate an incident on its
# own — cross-reference with catalog (`docs/observability/metrics/gc-service.md`)
# for the bounded set.

# Error breakdown by participant (user-vs-guest triage)
sum by(participant, error_type) (increase(gc_meeting_join_failures_total[5m]))

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
sum by(status_code) (increase(gc_http_requests_total{status_code=~"[45].."}[5m]))

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
sum(increase(gc_http_requests_total[5m]))

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

### Scenario 8: Meeting Creation Limit Exhaustion

**Alert**: `GCMeetingCreationFailureRate`
**Severity**: Warning
**Runbook Section**: `#scenario-8-meeting-creation-limit-exhaustion`

**Symptoms**:
- 403 Forbidden responses on `POST /api/v1/meetings`
- Users unable to create new meetings despite valid authentication
- Metrics: `gc_meeting_creation_failures_total{error_type="forbidden"}` spiking
- Logs: `meeting creation forbidden: org concurrent meeting limit reached`

**Diagnosis**:

```bash
# 1. Check meeting creation failure metrics
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/metrics | grep gc_meeting_creation
kill %1

# 2. Check failure rate by error type in Prometheus
sum by(error_type) (increase(gc_meeting_creation_failures_total[5m]))

# 3. Identify the affected organization(s)
kubectl logs -n dark-tower -l app=gc-service --tail=200 | grep "meeting creation forbidden"

# 4. Check active and scheduled meeting counts for the org
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c \
  "SELECT org_id, COUNT(*) AS active_meetings
   FROM meetings
   WHERE status IN ('scheduled', 'active')
   GROUP BY org_id
   ORDER BY active_meetings DESC
   LIMIT 20;"

# 5. Check the org's concurrent meeting limit
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c \
  "SELECT org_id, display_name, max_concurrent_meetings
   FROM organizations
   WHERE org_id = '<ORG_ID>';"

# 6. Check for orphaned meetings stuck in 'scheduled' status
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c \
  "SELECT meeting_id, meeting_code, status, created_at, scheduled_start_time
   FROM meetings
   WHERE org_id = '<ORG_ID>'
     AND status = 'scheduled'
     AND created_at < NOW() - INTERVAL '24 hours'
   ORDER BY created_at ASC;"
```

**Common Root Causes**:

1. **Org Hitting Concurrent Meeting Limit**: Count of scheduled + active meetings >= max_concurrent_meetings
   - Check: Active meeting count vs org limit (queries above)
   - Fix: Clean up completed/orphaned meetings or raise org limit

2. **Orphaned Meetings Stuck in 'scheduled' Status**: Meetings created but never started or cancelled, remaining in 'scheduled' status indefinitely
   - Check: Query for old 'scheduled' meetings (>24h old)
   - Fix: Transition orphaned meetings to 'cancelled' status

3. **Org Limit Misconfigured**: max_concurrent_meetings set too low for org usage
   - Check: Compare org limit to typical concurrent meeting volume
   - Fix: Adjust org limit after business approval

4. **Intentional Resource Exhaustion**: A user or automated process repeatedly creating and abandoning meetings to consume the org's concurrent meeting slots, blocking legitimate users from creating meetings
   - Check: Look for a single user creating many 'scheduled' meetings that are never started — query `created_by_user_id` on recent orphaned meetings
   - Fix: Identify the offending user, clean up orphaned meetings, consider per-user meeting creation rate limits; escalate to Security Team if abuse is confirmed

**Remediation**:

```bash
# Option 1: Clean up orphaned meetings for the affected org
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c \
  "UPDATE meetings
   SET status = 'cancelled', updated_at = NOW()
   WHERE org_id = '<ORG_ID>'
     AND status = 'scheduled'
     AND created_at < NOW() - INTERVAL '24 hours';"

# Verify meetings freed up
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c \
  "SELECT COUNT(*) AS active_meetings
   FROM meetings
   WHERE org_id = '<ORG_ID>'
     AND status IN ('scheduled', 'active');"

# Expected: Count should now be below max_concurrent_meetings

# Option 2: Raise org concurrent meeting limit (requires business approval)
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c \
  "UPDATE organizations
   SET max_concurrent_meetings = <NEW_LIMIT>, updated_at = NOW()
   WHERE org_id = '<ORG_ID>';"

# Expected recovery time: Immediate after limit change or meeting cleanup

# Verify recovery - meeting creation should succeed
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/metrics | grep 'gc_meeting_creation_failures_total'
kill %1
# forbidden error_type rate should drop to 0
```

**Escalation**:
- If multiple orgs affected simultaneously, escalate to GC Team for systemic investigation
- If orphaned meetings are recurring, escalate to GC Team for meeting lifecycle bug investigation
- If limit changes require business approval, escalate to Product/Business team
- If intentional resource exhaustion suspected (single user creating many abandoned meetings), escalate to Security Team

---

### Scenario 9: Meeting Code Collision

**Alert**: `GCMeetingCreationFailureRate` (elevated error rate may indicate collisions)
**Severity**: Warning → Critical if persistent
**Runbook Section**: `#scenario-9-meeting-code-collision`

> **Important**: Meeting codes use 12 base62 characters with 72-bit CSPRNG entropy and a database unique constraint with 3 retry attempts. Code collisions are extremely unlikely under normal operation. If this scenario occurs, it warrants serious investigation — it may indicate a CSPRNG failure, database corruption, or an extraordinary creation volume anomaly.

**Symptoms**:
- 500 Internal Server Error responses on `POST /api/v1/meetings` after retry exhaustion
- Metrics: `gc_meeting_creation_failures_total{error_type="code_collision"}` incrementing
- Logs: `meeting code collision after 3 retries` or `unique constraint violation on meeting_code`
- May co-occur with elevated `gc_meeting_creation_duration_seconds` (retries add latency)

**Diagnosis**:

```bash
# 1. Check code collision metrics
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/metrics | grep gc_meeting_creation
kill %1

# 2. Check collision rate in Prometheus
sum(increase(gc_meeting_creation_failures_total{error_type="code_collision"}[5m]))

# 3. Check logs for collision details
kubectl logs -n dark-tower -l app=gc-service --tail=500 | grep -i "collision\|unique constraint"

# 4. Check total meeting count (high volume increases collision probability)
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c \
  "SELECT COUNT(*) AS total_meetings FROM meetings;"

# 5. Check meeting creation rate (is there an unusual spike?)
# In Prometheus:
sum(increase(gc_meeting_creation_total[5m]))

# 6. Verify database unique constraint is intact
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c \
  "SELECT indexname, indexdef
   FROM pg_indexes
   WHERE tablename = 'meetings' AND indexdef LIKE '%meeting_code%';"

# 7. Check for duplicate meeting codes within an org (should be impossible if constraint is intact)
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c \
  "SELECT org_id, meeting_code, COUNT(*) AS cnt
   FROM meetings
   GROUP BY org_id, meeting_code
   HAVING COUNT(*) > 1;"
```

**Common Root Causes**:

1. **CSPRNG Failure**: `ring::SystemRandom` not producing sufficient entropy
   - Check: Multiple collisions in rapid succession — statistically near-impossible with healthy CSPRNG
   - Fix: Restart pods to reinitialize CSPRNG; if persistent, investigate OS entropy source (`/dev/urandom`)
   - **Security**: Persistent CSPRNG failures should be treated as a potential security incident — a compromised node entropy source could affect all cryptographic operations (JWT signing, token generation, TLS). Escalate to Security Team immediately if collisions persist after pod restart

2. **Extremely High Creation Volume**: Massive spike in meeting creation increasing collision probability
   - Check: Meeting creation rate and total meeting count
   - Fix: Scale service horizontally; collisions should resolve as retries succeed

3. **Database Unique Constraint Corruption**: Constraint dropped or not enforced
   - Check: Verify unique index exists on `meeting_code` column
   - Fix: Recreate unique constraint (requires database team coordination)

4. **Code Generation Bug**: Code generation producing non-uniform or reduced-entropy output
   - Check: Review recent code changes to meeting code generation
   - Fix: Rollback deployment if recent change introduced bug

**Remediation**:

```bash
# Option 1: Restart pods to reinitialize CSPRNG
kubectl rollout restart deployment/gc-service -n dark-tower
kubectl rollout status deployment/gc-service -n dark-tower

# Expected recovery time: 2-3 minutes

# Option 2: Verify and recreate unique constraint (requires Database Team)
# Check if constraint exists
kubectl exec -it deployment/gc-service -n dark-tower -- psql $DATABASE_URL -c \
  "SELECT conname, contype
   FROM pg_constraint
   WHERE conrelid = 'meetings'::regclass AND contype = 'u';"

# If missing, recreate (Database Team should execute this)
kubectl exec -it -n dark-tower postgres-0 -- psql -c \
  "ALTER TABLE meetings ADD CONSTRAINT meetings_org_code_unique UNIQUE (org_id, meeting_code);"

# Expected recovery time: Immediate after constraint recreation

# Option 3: Rollback if recent deployment introduced code generation bug
kubectl rollout undo deployment/gc-service -n dark-tower
kubectl rollout status deployment/gc-service -n dark-tower

# Expected recovery time: 2-3 minutes

# Verify recovery
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/metrics | grep 'gc_meeting_creation_failures_total'
kill %1
# Rate should drop to 0
```

**Escalation**:
- **Any** code collision occurrence should be reported to GC Team for investigation — this is not expected under normal operation
- If CSPRNG failure suspected, escalate to Security Team immediately — persistent entropy failures may indicate a compromised node and affect all cryptographic operations across services on that node
- If database constraint issues, escalate to Database Team
- If collisions persist after pod restart, escalate to Engineering Lead and Security Team — may require emergency code length increase and node-level investigation

---

### Scenario 10: Telemetry Proxy High Rejection Rate

**Alert**: `GCTelemetryProxyHighRejectionRate`
**Severity**: Warning
**Runbook Section**: `#scenario-10-telemetry-proxy-high-rejection-rate`

**Symptoms**:
- Rejection rate above 10% of telemetry ingest for 10+ minutes
- Metrics: `gc_telemetry_ingest_total{status=~"rejected_.*|error"}` elevated relative to total ingest
- Client telemetry (browser metrics/traces) partially or fully missing from the collector
- Possible client-side console errors on `POST /api/v1/telemetry/v1/{metrics,traces}` (413/429/400/415/502/503)

**Diagnosis**:

```bash
# 1. FIRST: split fault direction on the telemetry counter's own status values
# In Prometheus:
sum by(status) (rate(gc_telemetry_ingest_total{status=~"rejected_.*|error"}[10m]))
#   rejected_size / rejected_rate dominating -> CLIENT fault (broken or abusive client) -> continue here
#   error dominating -> could be client fault (400/415) OR collector fault (502/503) -> step 2
#   If collector fault confirmed in step 2 -> treat as a server-side error condition;
#   follow Scenario 5 (High Error Rate) practices for the forwarding path

# 2. Split the aggregated `error` status via paired HTTP status codes.
# CAVEAT: telemetry routes currently normalize to endpoint="/other" on gc_http_* —
# this series conflates ALL unrecognized paths, so treat it as approximate and
# confirm via logs (see docs/TODO.md "Observability Debt" for the normalization fix).
sum by(status_code) (rate(gc_http_requests_total{endpoint="/other", status_code=~"400|413|415|429|502|503"}[10m]))
#   400/415 -> client sending malformed OTLP or wrong Content-Type (bad SDK rollout?)
#   502/503 -> collector unreachable or telemetry disabled -> collector path, step 5

# 3. Confirm via GC logs (authoritative, not conflated)
kubectl logs -n dark-tower -l app=gc-service --tail=500 | grep -i "telemetry"

# 4. If rejected_rate dominates: check rate-limit rejections by reason
# In Prometheus:
sum by(reason) (rate(gc_telemetry_rate_limited_total[10m]))
# per_user -> one or few users hammering (client retry loop? abuse?) — check logs for user context

# 5. If 502/503: check collector health and GC->collector connectivity
kubectl get pods -n dark-tower -l app=otel-collector
kubectl logs -n dark-tower -l app=otel-collector --tail=100

# 6. Check payload sizes if rejected_size dominates (oversize client batches)
# In Prometheus:
histogram_quantile(0.99, sum by(payload_kind, le) (rate(gc_telemetry_payload_bytes_bucket[10m])))
# NOTE: bodies above 2x the configured max are rejected PRE-handler (DoS backstop)
# and appear only as gc_http 413s (step 2), never on the telemetry counter.
```

**Common Root Causes**:

1. **Bad client/SDK rollout**: a web-app or SDK deploy started sending oversized batches, malformed OTLP, or wrong Content-Type
   - Check: correlate rejection onset with web-app/SDK deploy timeline; 400/415 in step 2
   - Fix: roll back the client release; escalate to Client/Web-App Team

2. **Client retry loop hitting the rate limit**: a client bug retrying failed sends without backoff, tripping `per_user` limits
   - Check: `gc_telemetry_rate_limited_total{reason="per_user"}` in step 4; GC logs for repeated users
   - Fix: client-side fix (backoff); consider temporarily raising `telemetry_proxy_rate_limit_per_minute` only if legitimate traffic is being dropped

3. **Collector path failing (502/503)**: OTel collector down, unreachable, or telemetry disabled in config
   - Check: step 5; `error` status + 502/503 in step 2
   - Fix: restore collector (see infra runbooks); if intentional (telemetry disabled), silence the alert for the maintenance window

4. **Abusive/oversize sender**: a single origin pushing large payloads (rejected_size) — possible probing
   - Check: step 6 payload p99 vs the 256 KiB bucket ceiling; GC logs for user context
   - Fix: rate-limit or block at ingress; escalate to Security Team if abuse suspected

**Remediation**:

```bash
# Option 1: Roll back a bad client release (Client/Web-App Team executes)
# Expected recovery time: minutes after rollout completes; rejection rate drops as clients reload

# Option 2: Restore the collector path
kubectl rollout restart deployment/otel-collector -n dark-tower
kubectl rollout status deployment/otel-collector -n dark-tower
# Expected recovery time: 1-2 minutes; error-status rate drops immediately after

# Verify recovery — rejection share should fall back under 10%
# In Prometheus:
sum(rate(gc_telemetry_ingest_total{status=~"rejected_.*|error"}[10m]))
  / sum(rate(gc_telemetry_ingest_total[10m]))
```

**Escalation**:
- If rejections correlate with a client release, escalate to Client/Web-App Team for rollback
- If collector infrastructure is down, escalate to Infrastructure/SRE
- If abuse suspected (rate-limit or oversize probing from few users), escalate to Security Team
- If rejection taxonomy looks wrong (e.g., legitimate payloads rejected as oversize), escalate to GC Team

---

### Scenario 11: Telemetry Ingest Silent

**Alert**: `GCTelemetryProxySilent`
**Severity**: Page
**Runbook Section**: `#scenario-11-telemetry-ingest-silent`

> **Why this pages**: total loss of client-side observability, and possibly the
> leading indicator of client-facing breakage (CORS, ingress, auth) that the
> server cannot otherwise see. Expected detection delay is ~20 minutes for the
> flat-counter shape (15m rate window + 5m for clause).

> **Auto-resolve is NOT recovery**: after ~1h15m of continuous silence the 1h
> baseline window drains and the alert resolves on its own while ingest may
> still be dead. Before closing any incident, verify
> `sum(rate(gc_telemetry_ingest_total[15m])) > 0`.

**Symptoms**:
- No `gc_telemetry_ingest_total` events for 15+ minutes despite traffic in the prior hour
- Telemetry Ingest dashboard row flat or empty
- Client dashboards fed by the collector stop updating

**Diagnosis**:

```bash
# 1. FIRST: is this user-impacting? Check user-facing traffic and error rate.
# In Prometheus:
sum(rate(gc_http_requests_total[5m]))
sum(rate(gc_http_requests_total{status_code=~"[45].."}[5m])) / sum(rate(gc_http_requests_total[5m]))
#   ALL GC traffic near-zero -> whole-service problem -> go to Scenario 4 (Complete Service Outage)
#   Traffic normal, only telemetry silent -> telemetry-scoped, continue here

# 2. Check CORS preflight outcomes — a CORS misconfiguration blocks browser
# telemetry (and possibly all browser API calls) before requests reach handlers
# In Prometheus:
sum by(origin_class, status) (rate(gc_cors_preflight_total[15m]))
#   denied/403 spiking -> CORS misconfig or unexpected origin -> check GC config
#   (denied-preflight warn logs carry the structured requested_origin field)

# 3. Which silence shape fired? Run the two alert branches separately.
# Flat shape (pod alive, traffic stopped):
sum(rate(gc_telemetry_ingest_total[15m])) == 0
# Absent shape (series gone — pod restarted, nothing arrived since):
absent_over_time(gc_telemetry_ingest_total[15m])
#   Absent shape -> check the deploy/restart timeline FIRST:
kubectl get pods -n dark-tower -l app=gc-service -o wide   # look at pod AGE
#   A rolling restart in a low-traffic environment is a known benign trigger —
#   the counter registers lazily on the first request after restart.

# 4. Check 401s — an auth regression rejects telemetry at the route layer,
# BEFORE the handler, so it produces silence on the telemetry counter (the
# declared rejected_auth status is never emitted; 401s land on gc_http only).
# Same endpoint="/other" conflation caveat as Scenario 10 step 2.
sum(rate(gc_http_requests_total{endpoint="/other", status_code="401"}[15m]))
# GC logs are the AUTHORITATIVE user-token 401 diagnosis (not conflated):
kubectl logs -n dark-tower -l app=gc-service --tail=500 | grep -i "unauthorized\|401"
# Corroboration ONLY — the following counter covers the gRPC SERVICE-token path
# exclusively (user/guest HTTP tokens are never recorded on it), so a failure
# spike here is meaningful when the root cause is shared (JWKS/AC-side key
# regression hits both paths), but a healthy series does NOT clear the HTTP
# user-token path that telemetry uses:
sum by(result, failure_reason) (rate(gc_jwt_validations_total[15m]))

# 5. Check for a client rollout that disabled or sampled-down telemetry
# (benign): recent web-app/SDK deploys, telemetry flag/sampling config changes

# 6. Natural traffic trough check (benign): if gc_http traffic is also near-zero
# AND the error rate is normal, this is likely just no users online — not an
# incident. NOTE: recurring trough pages are an Alertmanager time-of-day routing
# concern, not a rule-threshold change (GCMeetingCreationStopped precedent).
```

**Common Root Causes**:

1. **Whole-service or ingress outage**: GC itself is down or unreachable
   - Check: step 1
   - Fix: go to Scenario 4 (Complete Service Outage)

2. **CORS misconfiguration**: allowed-origins list regressed; browsers blocked at preflight. NOTE: the allowlist is fail-closed — an EMPTY `CORS_ALLOWED_ORIGINS` blocks ALL cross-origin browser requests, which is exactly the misconfig shape that produces total telemetry silence
   - Check: step 2 denied/403 spike; then the knob itself:
     `kubectl get configmap gc-service -n dark-tower -o yaml | grep CORS`
     (`CORS_ALLOWED_ORIGINS`, comma-separated explicit origins, never `*` —
     set in `infra/services/gc-service/configmap.yaml` + env overlays, read by
     `crates/gc-service/src/config.rs`)
   - Fix: correct `CORS_ALLOWED_ORIGINS` in the gc-service ConfigMap and restart; expected recovery 2-3 minutes

3. **Auth regression on the telemetry route**: clients getting 401s pre-handler
   - Check: step 4
   - Fix: identify the auth change (JWKS, token validation); see Scenario 7 for AC-side token issues

4. **Rolling restart in a low-traffic environment (benign)**: absent-series shape; counter registers lazily on first post-restart request
   - Check: step 3 pod AGE vs alert firing time
   - Fix: none needed if the first real telemetry request lands; verify with the recovery query

5. **Client rollout disabled/sampled-down telemetry (benign-ish)**: web-app deploy turned telemetry off or sampling to ~0
   - Check: step 5 deploy timeline
   - Fix: confirm intent with Client/Web-App Team; if unintended, roll back the client release

6. **Natural traffic trough (benign)**: nobody online
   - Check: step 6
   - Fix: none; close as non-incident after the recovery-verification query is understood

**Remediation**:

```bash
# Option 1: Fix CORS origin allowlist — edit CORS_ALLOWED_ORIGINS in the
# gc-service ConfigMap (infra/services/gc-service/configmap.yaml / env overlay;
# comma-separated explicit origins, never "*"; empty = fail-closed, ALL
# cross-origin browser requests blocked), then:
kubectl rollout restart deployment/gc-service -n dark-tower
kubectl rollout status deployment/gc-service -n dark-tower
# Expected recovery time: 2-3 minutes + client page reloads

# Option 2: Roll back a client release that broke/disabled telemetry (Client/Web-App Team)
# Expected recovery time: minutes after rollout; ingest resumes as clients reload

# Option 3: Whole-service outage -> Scenario 4 procedures

# Verify recovery (REQUIRED before closing — the alert can auto-resolve while still broken):
# In Prometheus:
sum(rate(gc_telemetry_ingest_total[15m])) > 0
```

**Escalation**:
- If whole-service outage, follow Scenario 4 escalation (Engineering Lead after 30 min)
- If CORS/auth regression from a GC config or code change, escalate to GC Team; auth-side to AC Team
- If a client release disabled or broke telemetry, escalate to Client/Web-App Team
- If silence persists with no identified cause, escalate to Observability Team (#observability) — collector-side ingestion may be masking a deeper issue

---

### Scenario 12: CORS Misconfiguration Post-Deploy

**Alert**: none dedicated — surfaces via `GCTelemetryProxySilent` (Scenario 11) when browser telemetry stops, or via user reports of blocked browser API calls
**Severity**: P2 (High) — browser clients broken while server health is normal
**Runbook Section**: `#scenario-12-cors-misconfiguration-post-deploy`
**Trigger context**: a gc-service config or image rollout that changed `CORS_ALLOWED_ORIGINS`

**Symptoms**:
- Spike in `gc_cors_preflight_total{origin_class="denied",status="403"}` immediately after a gc-service rollout
- Browser console errors on cross-origin `/api/v1/*` calls ("blocked by CORS policy", missing `Access-Control-Allow-Origin`)
- Frequently arrives first as Scenario 11 (Telemetry Ingest Silent) — browser telemetry POSTs are blocked at preflight before reaching the handler
- Server-side health (`gc_http_requests_total` overall rate + error rate, `/health`, `/ready`) is normal — the breakage is browser-only

A `denied`/`403` climb while `allowed` drops right after a deploy signals a CORS regression — run the `gc_cors_preflight_total` preflight query from **Scenario 11 step 2** (not restated here, single source).

**Diagnosis**:

```bash
# 1. Confirm the denial pattern and the requested origins. Denied-preflight warn
#    logs carry the structured requested_origin field (never string-interpolated).
kubectl logs -n dark-tower -l app=gc-service --tail=500 | grep -i "CORS preflight denied"

# 2. Inspect the live allowlist knob.
kubectl get configmap gc-service -n dark-tower -o yaml | grep CORS_ALLOWED_ORIGINS
#   Empty value            -> fail-closed: ALL cross-origin browser requests blocked
#   Missing the real origin -> that origin specifically blocked
#   Value "*"               -> does NOT widen: config keeps "*" verbatim, and "*" parses
#                              as a valid HeaderValue, so it reaches AllowOrigin::list,
#                              which PANICS on a wildcard (tower-http 0.5.2) -> gc-service
#                              crashes at startup (pod CrashLoopBackOff / rollout never
#                              Ready). Net effect: "*" never widens the allowlist, but via
#                              a hard startup crash, not a silent ignore.

# 3. Diff against last-good to pinpoint the regression. The ConfigMap + overlay are
#    the single source; crates/gc-service/src/config.rs parses CORS_ALLOWED_ORIGINS.
git log -p --oneline -- infra/services/gc-service/configmap.yaml \
  infra/kubernetes/overlays/*/services/gc-service/ | head -80
```

For the shared `gc_cors_preflight_total` allowlist query and the fail-closed semantics, cross-reference **Scenario 11 step 2** (do not re-run both paths). This scenario adds only the deploy-time entry point.

**Common Root Causes**:

1. **Allowlist emptied by a bad config merge**: an overlay/ConfigMap change dropped `CORS_ALLOWED_ORIGINS` to empty → fail-closed, every browser origin blocked
   - Check: step 2 (empty value)
   - Fix: restore the correct explicit origins
2. **New client origin not added**: the web-app moved to a new origin (host/port/scheme) never added to the allowlist
   - Check: step 1 (the blocked origin appears in the warn logs) + step 2
   - Fix: add that specific origin
3. **Someone "fixed" it with `*`**: a `*` in `CORS_ALLOWED_ORIGINS` does NOT allow-all — it panics `AllowOrigin::list` at router build (tower-http 0.5.2), so gc-service crashes at startup and the symptom flips from browser-only breakage to gc-service not coming up (CrashLoopBackOff / rollout stuck)
   - Check: step 2 shows `*` AND the pod is crashlooping / the rollout is stuck (not Ready)
   - Fix: replace `*` with the explicit origin(s)

**Remediation**:

```bash
# Add the SPECIFIC missing origin(s) to CORS_ALLOWED_ORIGINS — comma-separated,
# explicit, NEVER "*" and never allow-all (the layer is fail-closed by design and
# runs with allow_credentials(false); a "*" entry does NOT allow-all — it panics
# AllowOrigin::list and crashes gc-service at startup).
# Edit infra/services/gc-service/configmap.yaml (or the env overlay), then:
kubectl rollout restart deployment/gc-service -n dark-tower
kubectl rollout status deployment/gc-service -n dark-tower

# Alternatively, if the regression came from a specific deploy, roll that back:
kubectl rollout undo deployment/gc-service -n dark-tower
```

**Verify** with the deploy smoke stanzas (gc-deployment.md §6, "CORS + Telemetry Proxy Smoke Tests"): stanza (a) — the real client origin now returns 200 + `Access-Control-Allow-Origin`; stanza (b) — a disallowed origin still returns 403. Confirm `gc_cors_preflight_total{origin_class="denied"}` falls back to baseline. Expected recovery: 2-3 minutes + client page reloads.

**Escalation**:
- Regression from a GC config/image change → GC Team; client changed origin without coordination → Client/Web-App Team
- If total telemetry silence accompanied it, resolving CORS should clear Scenario 11 — re-verify `sum(rate(gc_telemetry_ingest_total[15m])) > 0`

---

### Scenario 13: Telemetry Proxy Unreachable / Rate-Limit Storm

**Alert**: `GCTelemetryProxyHighRejectionRate` (warning); may co-fire with `GCTelemetryProxySilent` (page) if accepted ingest also stops
**Severity**: P2 (High) — degraded client-side observability
**Runbook Section**: `#scenario-13-telemetry-proxy-unreachable--rate-limit-storm`
**Related**: focused companion to **Scenario 10** (Telemetry Proxy High Rejection Rate) and **Scenario 11** (Telemetry Ingest Silent). Use their diagnosis for the rejection-rate taxonomy and the silence shapes; this scenario adds only the two angles they do not fully cover: (i) distinguishing a broken GC→collector network path from a collector-pod outage, and (ii) a burst-shaped per-user rate-limit storm.

> **Do not restate 10/11.** For the `status`-value fault split (`rejected_size` / `rejected_rate` / `error`), the `error`→502/503 collector branch, and the `endpoint="/other"` conflation caveat, follow **Scenario 10 → Diagnosis**. For the silent-ingest branches (flat vs absent series, auth-401 pre-handler), follow **Scenario 11 → Diagnosis**. The alert PromQL/thresholds live in `docs/observability/alerts.md` and `infra/docker/prometheus/rules/gc-alerts.yaml` — not repeated here.

#### 13a. Collector unreachable (502 storm)

**Symptoms**:
- `GCTelemetryProxyHighRejectionRate` firing with the `error` status dominating (Scenario 10 step 1), paired with 502s on the telemetry path
- 502 body code `BAD_GATEWAY` ("Telemetry collector is unavailable"); GC logs a `gc.telemetry` warn "Telemetry collector unreachable"
- If accepted ingest degrades to zero, `GCTelemetryProxySilent` also fires (Scenario 11)

**Diagnosis** (added angle — separate the two 502 causes):

First confirm collector-pod health per **Scenario 10 step 5** (`kubectl get pods` / `logs -l app=otel-collector`) — not restated here. If the pod is healthy but GC still 502s, this scenario's added angle is a broken GC→collector **network path**:

```bash
# Path check — DNS + reachability + config from a GC pod to the configured endpoint.
# OTEL_COLLECTOR_ENDPOINT is the BARE base (scheme+host+port, no path); GC appends
# /v1/metrics|/v1/traces itself (crates/gc-service/src/config.rs).
kubectl get configmap gc-service -n dark-tower -o yaml | grep OTEL_COLLECTOR_ENDPOINT
kubectl exec -n dark-tower deployment/gc-service -- \
  sh -c 'getent hosts otel-collector.dark-tower.svc.cluster.local'
# Confirm no NetworkPolicy blocks GC egress -> collector
# (infra/services/gc-service/network-policy.yaml).
kubectl get networkpolicy -n dark-tower

# Distinguish 502 (unreachable) from 503 (disabled): an EMPTY OTEL_COLLECTOR_ENDPOINT
# disables the proxy and returns 503, not 502.
```

- Collector pod healthy but GC still 502s → **network-path** fault (DNS, netpol egress, wrong endpoint) — the angle beyond Scenario 10.
- Collector pod down/crashlooping → collector outage — hand off per Scenario 10 remediation.

**Remediation**:
- **Collector outage:** restore it per **Scenario 10 → Remediation Option 2** (`kubectl rollout restart deployment/otel-collector`) — not restated here.
- **Network-path fault (this scenario's angle):** fix the broken piece — correct `OTEL_COLLECTOR_ENDPOINT` to the bare base, restore the GC egress NetworkPolicy, or fix DNS — then restart GC if config changed:

```bash
kubectl rollout restart deployment/gc-service -n dark-tower
```

Verify with deploy smoke stanza (c): an authenticated OTLP POST returns 202 (not 502). Then confirm the error share falls under 10% (Scenario 10 recovery query).

#### 13b. Rate-limit storm (429 burst)

**Symptoms**:
- `GCTelemetryProxyHighRejectionRate` firing with `rejected_rate` dominating (Scenario 10 step 1)
- Sharp spike in `gc_telemetry_rate_limited_total{reason="per_user"}`
- Client console shows repeated 429 (`RATE_LIMIT_EXCEEDED`) on `POST /api/v1/telemetry/v1/{metrics,traces}`

**Diagnosis** (added angle — storm shape):

Run the per-user rate-limit query from **Scenario 10 step 4** (`gc_telemetry_rate_limited_total` by `reason`) — not restated here. Two additive notes for the storm shape:

- **Cardinality:** `reason` is the ONLY label — the `sub` is deliberately NOT a label (bounded cardinality), so the metric shows the storm but not which user. To identify the offending `sub`, correlate with ingress/gateway access logs or the client fleet.
- **Quota:** the limiter is per-JWT-`sub` GCRA at `TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE` (default 60/min, burst 60): one hot `sub` ⇒ a client retry loop without backoff; broad across many `sub`s ⇒ a fleet-wide client bug or a genuine surge past the quota.

**Remediation**:
- Single-sub retry loop → fix the client (backoff/jitter); escalate to Client/Web-App Team. Do NOT raise the limit to paper over a retry bug.
- Broad, legitimate surge that outgrew the quota → raise `TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE` in the gc-service ConfigMap (config over hardcoding; must be > 0), then `kubectl rollout restart deployment/gc-service -n dark-tower`.
- Never disable per-user rate limiting — it is the abuse/DoS guard on the browser-facing telemetry surface.

Verify with deploy smoke stanza (f): a fresh-`sub` 70-burst still shows the intended 202→429 transition (the limiter works), and legitimate single-request traffic returns 202.

**Escalation**:
- Collector infrastructure down or network-path broken beyond a config fix → Infrastructure/SRE
- Client retry-loop or fleet-wide client bug → Client/Web-App Team
- Suspected abuse (a single origin storming the limiter or oversize probing) → Security Team

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
- 2026-02-28: Added Scenario 8 (Meeting Creation Limit Exhaustion) and Scenario 9 (Meeting Code Collision)

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
