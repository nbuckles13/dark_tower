# MC Service Incident Response Runbook

**Service**: Meeting Controller (mc-service)
**Owner**: SRE Team
**On-Call Rotation**: PagerDuty - Dark Tower MC Team
**Last Updated**: 2026-05-01

---

## Table of Contents

1. [Severity Classification](#severity-classification)
2. [Escalation Paths](#escalation-paths)
3. [Common Failure Scenarios](#common-failure-scenarios)
   - [Scenario 1: High Mailbox Depth](#scenario-1-high-mailbox-depth)
   - [Scenario 2: Actor Panics](#scenario-2-actor-panics)
   - [Scenario 3: Meeting Lifecycle Issues](#scenario-3-meeting-lifecycle-issues)
   - [Scenario 4: Complete Service Outage](#scenario-4-complete-service-outage)
   - [Scenario 5: High Latency](#scenario-5-high-latency)
   - [Scenario 6: GC Integration Failures](#scenario-6-gc-integration-failures)
   - [Scenario 7: Resource Pressure](#scenario-7-resource-pressure)
   - [Scenario 8: Join Failures](#scenario-8-join-failures)
   - [Scenario 9: WebTransport Rejections](#scenario-9-webtransport-rejections)
   - [Scenario 10: JWT Validation Failures](#scenario-10-jwt-validation-failures)
   - [Scenario 11: Media Connection Failures](#scenario-11-media-connection-failures)
   - [Scenario 12: RegisterMeeting Coordination Failures](#scenario-12-registermeeting-coordination-failures)
   - [Scenario 13: Unexpected MH Notifications](#scenario-13-unexpected-mh-notifications)
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
| **P1 (Critical)** | Service down, active meetings disrupted | **15 minutes** | All pods crash-looping, Actor panics affecting multiple meetings, Complete GC registration failure, >1% message drop rate | Immediate page, escalate to Engineering Lead after 30 min |
| **P2 (High)** | Degraded performance, some meetings affected | **1 hour** | High latency (p95 > 1s), Mailbox depth critical (>500), Single pod failing, GC heartbeat intermittent | Page if persists > 15 min, escalate to Service Owner after 2 hours |
| **P3 (Medium)** | Non-critical issue, workaround available | **4 hours** | Warning-level mailbox depth, Single meeting stuck, Metrics unavailable, High CPU (non-critical) | Slack notification, escalate if not resolved in 8 hours |
| **P4 (Low)** | Minor issue, no immediate impact | **24 hours** | Log noise, Cosmetic dashboard issues, Non-critical warnings | Normal ticket, review in next on-call handoff |

### Severity Upgrade Triggers

Automatically upgrade severity if:
- P2 persists for > 2 hours -> Upgrade to P1
- P3 affects multiple meetings -> Upgrade to P2
- Any actor panic detected -> Upgrade to P1
- Any security breach suspected -> Upgrade to P1 + notify Security Team immediately
- `rate(mc_register_meeting_total{status="error"}[5m]) / rate(mc_register_meeting_total[5m]) > 0.10` sustained for > 15m -> Upgrade to P2 (new meetings losing media) per Scenario 12
- `rate(mc_media_connection_failures_total{all_failed="true"}[5m]) > 0` sustained for > 15m -> Upgrade to P1 (active participants have no media) per Scenario 11
- Steady-rate `mc_mh_notifications_received_total` from a single MH service identity referencing meeting_ids absent from `mc_meetings_active` -> Treat as authenticated-MH-misbehavior; notify Security Team immediately per Scenario 13

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
    | (if not resolved in 30 min for P1, 2h for P2)
Service Owner / Tech Lead
    | (if architectural decision needed)
Engineering Manager
    | (if multi-service impact)
Infrastructure Team / SRE Lead
```

### Specialist Contacts

| Team | When to Engage | Contact |
|------|----------------|---------|
| **GC Team** | Registration failures, heartbeat issues, meeting assignment problems | #gc-oncall, PagerDuty: GC-Team |
| **Media Handler Team** | MC -> MH connectivity, media routing issues | #mh-oncall, PagerDuty: MH-Team |
| **Infrastructure/SRE** | Kubernetes issues, network problems, resource constraints | #infra-oncall, PagerDuty: SRE |
| **Security Team** | Suspected breach, authentication bypass, audit log failures | #security-incidents (CRITICAL ONLY) |
| **Product/Business** | Customer impact assessment, external communications | Engineering Manager escalates |

### External Dependencies

- **Global Controller**: MC registration, meeting assignment, capacity management
- **Media Handler**: Media routing (MC coordinates with MH for media streams)
- **Kubernetes**: Pod scheduling, networking, resource allocation
- **Prometheus/Grafana**: Managed by Observability Team (#observability)

---

## Common Failure Scenarios

### Scenario 1: High Mailbox Depth

**Alert**: `MCHighMailboxDepthWarning`, `MCHighMailboxDepthCritical`
**Severity**: Warning (>100) / Critical (>500)
**Runbook Section**: `#scenario-1-high-mailbox-depth`

**Symptoms**:
- Mailbox depth metric elevated (>100 warning, >500 critical)
- Message processing latency increasing
- Possible message drops
- Meeting participants experiencing delays

**Diagnosis**:

```bash
# 1. Check mailbox depth by actor type
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep mc_actor_mailbox_depth
kill %1

# 2. Identify which actor type is backlogged
# In Prometheus:
sum by(actor_type) (mc_actor_mailbox_depth)

# 3. Check active meetings and connections
curl http://localhost:8080/metrics | grep -E "mc_meetings_active|mc_connections_active"

# 4. Check pod resource usage
kubectl top pods -n dark-tower -l app=mc-service

# 5. Check for message drops
curl http://localhost:8080/metrics | grep mc_messages_dropped_total
```

**Common Root Causes**:

1. **Slow Message Processing**: Message handler taking too long
   - Check: mailbox depth growth by actor type, Redis p99 latency, session join p95
   - Fix: Investigate specific actor logic, optimize, or scale

2. **Message Storm**: Burst of messages from clients
   - Check: Connection count spike, message rate spike
   - Fix: Implement rate limiting, scale horizontally

3. **Blocking Operations**: Actor performing blocking I/O
   - Check: Logs for slow operations, trace spans
   - Fix: Make blocking operations async, use dedicated executor

4. **Resource Contention**: CPU/memory pressure
   - Check: `kubectl top pods`
   - Fix: Scale horizontally, increase resource limits

5. **GC Integration Slow**: Slow responses from GC
   - Check: GC heartbeat latency
   - Fix: Investigate GC health, see Scenario 6

**Remediation**:

```bash
# Option 1: Scale horizontally to distribute load
kubectl scale deployment/mc-service -n dark-tower --replicas=5

# Expected recovery time: 30-60 seconds

# Option 2: Restart affected pods (clears mailbox but may drop messages)
# CAUTION: This will disconnect active meetings on this pod
kubectl delete pod <MC_POD_NAME> -n dark-tower

# Expected recovery time: 30 seconds
# WARNING: Active meetings on this pod will be affected

# Option 3: Identify and kill problematic meetings (if specific meeting causing issues)
# Requires admin API or database intervention
# Escalate to Service Owner

# Option 4: Increase mailbox capacity (temporary, requires config change)
# Update ACTOR_MAILBOX_SIZE in ConfigMap and restart pods
# Not recommended as first action - address root cause first

# Verify recovery
curl http://localhost:8080/metrics | grep mc_actor_mailbox_depth
# Should be decreasing
```

**Escalation**:
- If mailbox stays critical (>500) for >5 minutes, escalate to Service Owner
- If messages being dropped, escalate immediately

---

### Scenario 2: Actor Panics

**Alert**: `MCActorPanic`
**Severity**: Critical
**Runbook Section**: `#scenario-2-actor-panics`

**Symptoms**:
- Alert: Actor panic detected
- Metrics: `mc_actor_panics_total` incrementing
- Possible meeting disruption
- Logs: Panic stack trace

**Diagnosis**:

```bash
# 1. Check panic metrics
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep mc_actor_panics_total
kill %1

# 2. Identify affected actor type
# In Prometheus:
sum by(actor_type) (increase(mc_actor_panics_total[5m]))

# 3. Find panic in logs (look for stack trace)
kubectl logs -n dark-tower -l app=mc-service --tail=500 | grep -A 50 "panic\|PANIC"

# 4. Find correlation with meetings
# Look for meeting-related context in panic logs
kubectl logs -n dark-tower -l app=mc-service --tail=500 | grep -B 10 "panic" | grep -i "meeting\|session"

# 5. Check if panic is recurring
# Watch panic counter
watch -n 5 'kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 2>/dev/null & sleep 1; curl -s http://localhost:8080/metrics | grep mc_actor_panics_total; kill %1 2>/dev/null'
```

**Common Root Causes**:

1. **Null Pointer / Unwrap**: Code calling unwrap() on None
   - Check: Stack trace for "unwrap" or "expect"
   - Fix: Code fix required, rollback if recent deployment

2. **Invalid Message Format**: Malformed message from client
   - Check: Stack trace for parsing/deserialization errors
   - Fix: Add validation, may need client-side fix

3. **Invariant Violation**: Unexpected state in actor
   - Check: Stack trace for assertion failures
   - Fix: Debug state management, add defensive checks

4. **Resource Exhaustion**: Out of memory or file descriptors
   - Check: Pod resource usage, OOMKilled events
   - Fix: Increase limits, investigate leak

5. **External Dependency Failure**: Unexpected response from GC/MH
   - Check: Stack trace for network/response parsing errors
   - Fix: Add error handling, implement retries

**Remediation**:

```bash
# Step 1: Assess impact
# Check if panic is isolated or widespread
sum by(actor_type) (increase(mc_actor_panics_total[5m]))

# Step 2: If panic is in critical actor and recurring, consider rollback
# Check if recent deployment
kubectl rollout history deployment/mc-service -n dark-tower

# If panic started after deployment:
kubectl rollout undo deployment/mc-service -n dark-tower

# Expected recovery time: 2-3 minutes

# Step 3: If panic is isolated, restart affected pod
kubectl delete pod <MC_POD_NAME> -n dark-tower

# Expected recovery time: 30 seconds

# Step 4: Monitor for recurrence
watch -n 10 'kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 2>/dev/null & sleep 1; curl -s http://localhost:8080/metrics | grep mc_actor_panics_total; kill %1 2>/dev/null'

# Verify recovery
# Panic count should stop incrementing
curl http://localhost:8080/metrics | grep mc_actor_panics_total
```

**Escalation**:
- Any actor panic is critical - page immediately
- Escalate to MC Team for root cause analysis
- If rollback needed, inform GC Team (may affect registration)

---

### Scenario 3: Meeting Lifecycle Issues

**Alert**: `MCMeetingStale`, `MCLowConnectionCount`
**Severity**: Warning
**Runbook Section**: `#scenario-3-meeting-lifecycle-issues`

**Symptoms**:
- Active meetings with no connections
- Meetings not processing messages
- Stuck meetings that won't end
- Orphaned meeting sessions

**Diagnosis**:

```bash
# 1. Check meeting and connection counts
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep -E "mc_meetings_active|mc_connections_active"
kill %1

# 2. Check session join activity and outcomes
curl http://localhost:8080/metrics | grep -E "mc_session_joins_total|mc_session_join_duration_seconds"

# 3. Look for meeting-related errors in logs
kubectl logs -n dark-tower -l app=mc-service --tail=500 | grep -i "meeting\|session\|lifecycle"

# 4. Check for connection issues
kubectl logs -n dark-tower -l app=mc-service --tail=500 | grep -i "connection\|disconnect\|webtransport"

# 5. Check GC perspective on meetings
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "SELECT meeting_id, mc_id, status, participant_count, created_at FROM meetings WHERE status = 'active' ORDER BY created_at DESC LIMIT 10;"
```

**Common Root Causes**:

1. **Client Disconnect Without Cleanup**: Clients disconnected abruptly
   - Check: Connection close events in logs
   - Fix: Implement meeting timeout/cleanup logic

2. **Meeting Actor Stuck**: Meeting actor not processing messages
   - Check: Mailbox depth for MeetingActor
   - Fix: Restart pod or implement health check

3. **GC-MC State Mismatch**: GC thinks meeting is active but MC doesn't
   - Check: Compare GC database with MC metrics
   - Fix: Reconciliation required, may need manual cleanup

4. **WebTransport Connection Issues**: Connections failing to establish
   - Check: Connection error logs
   - Fix: TLS issues, network policy issues

5. **Meeting End Message Lost**: End meeting message not processed
   - Check: Message drop metrics
   - Fix: Implement reliable message delivery or timeout

**Remediation**:

```bash
# Option 1: Force cleanup of stale meetings (if MC has admin API)
# TODO: Implement admin API for meeting cleanup

# Option 2: Restart MC pod (will end all meetings on this pod)
# CAUTION: Active participants will be disconnected
kubectl delete pod <MC_POD_NAME> -n dark-tower

# Expected recovery time: 30 seconds
# Impact: All meetings on this pod will end

# Option 3: Update GC to mark meetings as ended
# Requires database intervention
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "UPDATE meetings SET status = 'ended' WHERE mc_id = '<MC_ID>' AND status = 'active' AND updated_at < NOW() - INTERVAL '1 hour';"

# This cleans up stale meetings in GC
# MC will naturally clean up after participants disconnect

# Verify recovery
curl http://localhost:8080/metrics | grep mc_meetings_active
# Should decrease after cleanup
```

**Escalation**:
- If stuck meetings affecting multiple users, escalate to Service Owner
- If GC-MC state mismatch, escalate to GC Team for coordination

---

### Scenario 4: Complete Service Outage

**Alert**: `MCDown`, `MCPodRestartingFrequently`
**Severity**: Critical
**Runbook Section**: `#scenario-4-complete-service-outage`

**Symptoms**:
- All MC pods in CrashLoopBackOff or Pending state
- No healthy pods in `kubectl get pods -l app=mc-service`
- Alert: `MCDown` firing
- Active meetings disrupted, users disconnected

**Diagnosis**:

```bash
# 1. Check pod status
kubectl get pods -n dark-tower -l app=mc-service

# 2. Check pod events
kubectl describe pods -n dark-tower -l app=mc-service

# 3. Check recent logs before crash
kubectl logs -n dark-tower -l app=mc-service --previous --tail=100

# 4. Check deployment status
kubectl describe deployment mc-service -n dark-tower

# 5. Check resource quotas
kubectl describe resourcequota -n dark-tower

# 6. Check node status
kubectl get nodes
kubectl describe node <node-name>

# 7. Check for recent deployments
kubectl rollout history deployment/mc-service -n dark-tower
```

**Common Root Causes**:

1. **Bad Deployment**: Recent deployment introduced crash
   - Check: Deployment history, crash logs
   - Fix: Rollback to previous version

2. **Out of Memory**: Pods OOMKilled due to memory limits
   - Check: Pod events show "OOMKilled"
   - Fix: Increase memory limits, investigate memory leak

3. **GC Registration Failure**: Cannot register with GC
   - Check: Logs for GC connection errors
   - Fix: Check GC health, network connectivity

4. **Missing Secret**: TLS certs or other secrets missing
   - Check: `kubectl get secret -n dark-tower mc-service-secrets`
   - Fix: Restore secret

5. **Missing ConfigMap**: Required ConfigMap deleted
   - Check: `kubectl get configmap -n dark-tower mc-service-config`
   - Fix: Restore ConfigMap

6. **Actor System Initialization Failure**: Actor system cannot start
   - Check: Logs for actor initialization errors
   - Fix: Check configuration, resource limits

**Remediation**:

```bash
# Option 1: Rollback deployment to last known good version
kubectl rollout undo deployment/mc-service -n dark-tower
kubectl rollout status deployment/mc-service -n dark-tower

# Expected recovery time: 2-3 minutes

# Option 2: Force reschedule pods
kubectl delete pods -n dark-tower -l app=mc-service
# Deployment will recreate them

# Expected recovery time: 30-60 seconds

# Option 3: Check and restore missing secrets/configmaps
kubectl get secret -n dark-tower mc-service-secrets
kubectl get configmap -n dark-tower mc-service-config
# If missing, recreate from secure backup

# Option 4: Increase resource limits (if OOMKilled)
kubectl patch deployment/mc-service -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"mc-service","resources":{"limits":{"memory":"2Gi"}}}]}}}}'

# Expected recovery time: 2-3 minutes

# Verify recovery
kubectl get pods -n dark-tower -l app=mc-service
kubectl logs -n dark-tower -l app=mc-service --tail=50
```

**Escalation**:
- If rollback fails, escalate to Engineering Lead immediately
- If node issues, escalate to Infrastructure Team
- Inform GC Team so they can route new meetings to other MCs

---

### Scenario 5: High Latency

**Alert**: `MCHighJoinLatency` (info: session-join p95 >2s for 5m), Redis SLO breach
**Severity**: Info / Warning (depending on SLI breached)
**Runbook Section**: `#scenario-5-high-latency`

**Symptoms**:
- Session join p95 exceeding 2s SLO
- Redis p99 exceeding 10ms SLO
- Meeting participants experiencing slow joins
- Possible timeout errors in clients

**Diagnosis**:

```bash
# 1. Check session join and Redis latency metrics
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep -E "mc_session_join_duration_seconds|mc_redis_latency_seconds"
kill %1

# 2. Session join p95 by status (Prometheus)
histogram_quantile(0.95, sum by(status, le) (rate(mc_session_join_duration_seconds_bucket[5m])))

# 3. Check mailbox depth (backpressure causes latency)
sum by(actor_type) (mc_actor_mailbox_depth)

# 4. Check pod resource utilization
kubectl top pods -n dark-tower -l app=mc-service

# 5. Check for GC heartbeat latency (slow GC can cause delays)
histogram_quantile(0.95, sum by(le) (rate(mc_gc_heartbeat_latency_seconds_bucket[5m])))

# 6. Check for garbage collection pauses (if applicable)
kubectl logs -n dark-tower -l app=mc-service --tail=500 | grep -i "gc\|pause"

# 7. Check network latency between pods
kubectl exec -it deployment/mc-service -n dark-tower -- ping gc-service.dark-tower.svc.cluster.local
```

**Common Root Causes**:

1. **Mailbox Backpressure**: Messages queued due to slow processing
   - Check: Mailbox depth metrics
   - Fix: See Scenario 1

2. **CPU Contention**: High CPU usage causing processing delays
   - Check: `kubectl top pods`
   - Fix: Scale horizontally, investigate CPU-intensive operations

3. **Blocking Operations**: Sync calls blocking actor processing
   - Check: Logs for slow operations, trace spans
   - Fix: Make operations async

4. **GC Integration Slow**: Slow responses from GC
   - Check: GC heartbeat latency
   - Fix: See Scenario 6

5. **Network Latency**: Slow network between components
   - Check: Ping times, network metrics
   - Fix: Infrastructure team investigation

6. **Memory Pressure**: GC pauses due to memory pressure
   - Check: Memory usage, GC logs
   - Fix: Increase memory, investigate allocations

**Remediation**:

```bash
# Scenario A: CPU Bound (CPU >80%)
kubectl scale deployment/mc-service -n dark-tower --replicas=5

# Expected recovery time: 30-60 seconds

# Scenario B: Mailbox Backpressure
# See Scenario 1 remediation

# Scenario C: Memory Pressure
kubectl patch deployment/mc-service -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"mc-service","resources":{"limits":{"memory":"2Gi"}}}]}}}}'

# Expected recovery time: 2-3 minutes

# Scenario D: Pod restart (clears accumulated state)
kubectl delete pod <POD_NAME> -n dark-tower

# Expected recovery time: 30 seconds
# WARNING: Active meetings affected

# Verify recovery
histogram_quantile(0.95, sum by(le) (rate(mc_session_join_duration_seconds_bucket{status="success"}[5m])))
# Should return value < 2.000
```

**Escalation**:
- If latency persists after scaling, escalate to Service Owner
- If GC is the bottleneck, escalate to GC Team
- If network issues, escalate to Infrastructure Team

---

### Scenario 6: GC Integration Failures

**Alert**: `MCGCHeartbeatWarning`
**Severity**: Warning (>10% heartbeat failure rate for 5m)
**Runbook Section**: `#scenario-6-gc-integration-failures`

**Symptoms**:
- GC heartbeat failures increasing
- MC not receiving new meeting assignments
- GC may mark MC as unhealthy
- New meetings not being routed to this MC

**Diagnosis**:

```bash
# 1. Check heartbeat metrics
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep mc_gc_heartbeat
kill %1

# 2. Check GC service health
kubectl get pods -n dark-tower -l app=gc-service

# 3. Check MC registration status in GC
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "SELECT id, region, capacity, current_sessions, last_heartbeat, status FROM meeting_controllers ORDER BY last_heartbeat DESC LIMIT 10;"

# 4. Test GC connectivity from MC pod
kubectl exec -it deployment/mc-service -n dark-tower -- \
  curl -i http://gc-service.dark-tower.svc.cluster.local:8080/health

# 5. Check MC logs for GC errors
kubectl logs -n dark-tower -l app=mc-service --tail=100 | grep -i "gc\|heartbeat\|register"

# 6. Check network policy
kubectl get networkpolicy -n dark-tower
kubectl describe networkpolicy mc-service -n dark-tower
```

**Common Root Causes**:

1. **GC Service Down**: GC not running or unhealthy
   - Check: `kubectl get pods -l app=gc-service`
   - Fix: Escalate to GC Team

2. **Network Connectivity**: NetworkPolicy blocking MC -> GC
   - Check: NetworkPolicy configuration
   - Fix: Adjust NetworkPolicy

3. **GC Overloaded**: GC not responding to heartbeats
   - Check: GC latency metrics, pod resources
   - Fix: Scale GC, escalate to GC Team

4. **Invalid Credentials**: MC credentials rejected by GC
   - Check: GC logs for authentication errors
   - Fix: Verify MC credentials, re-register

5. **DNS Issues**: Cannot resolve GC service name
   - Check: DNS resolution from MC pod
   - Fix: Check CoreDNS, Infrastructure Team

**Remediation**:

```bash
# Option 1: Restart MC to force re-registration
kubectl rollout restart deployment/mc-service -n dark-tower

# Expected recovery time: 2-3 minutes

# Option 2: Check and restart GC if unhealthy
kubectl get pods -n dark-tower -l app=gc-service
kubectl rollout restart deployment/gc-service -n dark-tower

# Expected recovery time: 2-3 minutes
# Escalate to GC Team before restarting GC

# Option 3: Verify NetworkPolicy
kubectl get networkpolicy mc-service -n dark-tower -o yaml
# Ensure egress to gc-service:8080 is allowed

# Option 4: Manual re-registration (if MC has admin API)
# TODO: Implement admin API for re-registration

# Verify recovery
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "SELECT id, last_heartbeat, status FROM meeting_controllers WHERE last_heartbeat > NOW() - INTERVAL '30 seconds';"
# MC should appear with recent heartbeat and 'active' status
```

**Escalation**:
- If GC is down or overloaded, escalate to GC Team immediately
- If NetworkPolicy issues, escalate to Infrastructure Team
- If MC cannot re-register after restart, escalate to Service Owner

---

### Scenario 7: Resource Pressure

**Alert**: `MCHighMemory`, `MCHighCPU`, `MCCapacityWarning`
**Severity**: Warning
**Runbook Section**: `#scenario-7-resource-pressure`

**Symptoms**:
- Memory usage >85% for >10 minutes
- CPU usage >80% for >5 minutes
- Approaching meeting capacity
- Increased latency (secondary symptom)
- Pod OOMKilled events (if limit reached)

**Diagnosis**:

```bash
# 1. Check current resource usage
kubectl top pods -n dark-tower -l app=mc-service

# 2. Check resource limits
kubectl describe deployment mc-service -n dark-tower | grep -A 10 "Limits:"

# 3. Check for OOMKilled events
kubectl get events -n dark-tower --field-selector involvedObject.kind=Pod | grep -i "oom\|killed"

# 4. Check memory usage trend in Prometheus
container_memory_working_set_bytes{pod=~"mc-service-.*"}
container_spec_memory_limit_bytes{pod=~"mc-service-.*"}

# 5. Check CPU usage trend
rate(container_cpu_usage_seconds_total{pod=~"mc-service-.*"}[5m])

# 6. Check meeting/connection load
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep -E "mc_meetings_active|mc_connections_active"
kill %1

# 7. Check mailbox depth (held messages consume memory)
curl http://localhost:8080/metrics | grep mc_actor_mailbox_depth
```

**Common Root Causes**:

1. **High Meeting Load**: Too many meetings on one MC
   - Check: mc_meetings_active metric
   - Fix: Scale horizontally, GC should distribute

2. **Connection Surge**: Spike in WebTransport connections
   - Check: mc_connections_active metric
   - Fix: Scale horizontally, implement rate limiting

3. **Mailbox Accumulation**: Messages queued in mailboxes
   - Check: mc_actor_mailbox_depth
   - Fix: Address backpressure (see Scenario 1)

4. **Memory Leak**: Memory continuously growing
   - Check: Memory trend over hours (never decreasing)
   - Fix: Restart pods, investigate with profiling

5. **CPU-Intensive Operations**: Heavy message processing
   - Check: Message processing latency by type
   - Fix: Optimize processing, scale horizontally

**Remediation**:

```bash
# Option 1: Scale horizontally to distribute load
kubectl scale deployment/mc-service -n dark-tower --replicas=5

# Expected recovery time: 30-60 seconds

# Option 2: Increase resource limits
kubectl patch deployment/mc-service -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"mc-service","resources":{"limits":{"cpu":"4000m","memory":"2Gi"},"requests":{"cpu":"1000m","memory":"1Gi"}}}]}}}}'

# Expected recovery time: 2-3 minutes (rolling update)

# Option 3: Restart pods (temporary fix for memory issues)
kubectl rollout restart deployment/mc-service -n dark-tower

# Expected recovery time: 2-3 minutes

# Option 4: Mark MC as draining (stop new assignments)
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "UPDATE meeting_controllers SET status = 'draining' WHERE id = '<MC_ID>';"
# This stops new meetings from being assigned while allowing current ones to finish

# Verify recovery
kubectl top pods -n dark-tower -l app=mc-service
# CPU should be <70%, memory should be <70%
```

**Escalation**:
- If memory leak suspected, escalate to MC Team for profiling
- If infrastructure resource constraints, escalate to Infrastructure Team
- If load is legitimately high, discuss capacity planning with Product

---

### Scenario 8: Join Failures

**Alert**: `MCHighJoinFailureRate`
**Severity**: Warning
**Runbook Section**: `#scenario-8-join-failures`

**Symptoms**:
- Session join failure rate >5% for 5 minutes
- Users unable to join meetings
- `mc_session_join_failures_total` incrementing by `error_type`
- "Session Join Failures by Type" dashboard panel showing elevated counts

**Diagnosis**:

```bash
# 1. Check overall join success/failure rate
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep mc_session_joins_total
kill %1

# 2. Break down failures by error type (most important diagnostic step)
# In Prometheus:
sum by(error_type) (increase(mc_session_join_failures_total[5m]))

# 3. Check join latency (slow joins may indicate upstream issues)
histogram_quantile(0.95, sum by(le) (rate(mc_session_join_duration_seconds_bucket{status="success"}[5m])))

# 4. Check join failure rate
(
  sum(rate(mc_session_joins_total{status="failure"}[5m]))
  /
  sum(increase(mc_session_joins_total[5m]))
)

# 5. Check active meetings and capacity
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep -E "mc_meetings_active|mc_connections_active"
kill %1

# 6. Check MC logs for join errors
kubectl logs -n dark-tower -l app=mc-service --tail=500 | grep -i "join\|JoinRequest\|session"

# 7. Check Redis health (session state depends on Redis)
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep mc_redis_latency_seconds
kill %1
```

**Common Root Causes**:

Triage by the `error_type` label on `mc_session_join_failures_total`:

1. **`jwt_validation`**: Token validation failed during join
   - Check: See [Scenario 10](#scenario-10-jwt-validation-failures) for full diagnosis
   - Fix: Resolve JWT/JWKS issues per Scenario 10

2. **`meeting_not_found`**: Client requested a meeting that does not exist on this MC
   - Check: Verify meeting assignment in GC database, check if meeting ended
   - Fix: Client may have stale meeting assignment; check GC routing

3. **`mc_capacity_exceeded`**: MC instance at maximum meeting capacity
   - Check: `mc_meetings_active` metric vs configured capacity limit
   - Fix: Scale horizontally, check GC load balancing

4. **`meeting_capacity_exceeded`**: Individual meeting at participant limit
   - Check: Meeting participant count in logs
   - Fix: Expected behavior if meeting is full; inform user

5. **`redis`**: Redis operation failed during join flow (session binding, fencing)
   - Check: Redis health, `mc_redis_latency_seconds` metric, connection pool metrics
   - Fix: Check Redis pod health, connection pool exhaustion, network connectivity

6. **`session_binding`**: Session binding token validation failed
   - Check: Binding token expiry, secret mismatch between MC instances
   - Fix: Verify `MC_BINDING_TOKEN_SECRET` is consistent across MC replicas

7. **`fenced_out`**: Stale fencing generation (split-brain protection triggered)
   - Check: `mc_fenced_out_total` metric, Redis fencing generation
   - Fix: MC may need restart to acquire fresh generation

8. **`draining` / `migrating`**: MC is draining or migrating meetings away
   - Check: MC status in GC database
   - Fix: Expected during maintenance; joins will succeed on another MC

9. **`internal`**: Unexpected internal error (stream failures, decode errors)
   - Check: MC logs for stack traces or error details
   - Fix: Investigate logs, may require code fix or rollback

**Remediation**:

```bash
# Step 1: Identify dominant error_type from dashboard or PromQL
sum by(error_type) (increase(mc_session_join_failures_total[5m]))

# Step 2: Apply targeted fix based on error_type (see root causes above)

# Step 3: If redis errors — check Redis health
kubectl get pods -n dark-tower -l app=redis
kubectl exec -it deployment/redis -n dark-tower -- redis-cli ping
# Expected: PONG

# Step 4: If capacity exceeded — scale MC
kubectl scale deployment/mc-service -n dark-tower --replicas=5

# Expected recovery time: 30-60 seconds

# Step 5: If internal errors persist — restart as last resort
# See Recovery Procedures: #service-restart-procedure
# WARNING: Active meetings on restarted pods will be affected

# Verify recovery
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep mc_session_joins_total
kill %1
# Failure rate should be decreasing
```

**Escalation**:
- If `jwt_validation` errors dominate, escalate to AC Team (JWKS endpoint issue)
- If `redis` errors dominate, escalate to Infrastructure Team
- If `internal` errors persist after restart, escalate to MC Team for root cause
- If failure rate stays >5% for >15 minutes, upgrade to P2

---

### Scenario 9: WebTransport Rejections

**Alert**: `MCHighWebTransportRejections`
**Severity**: Warning
**Runbook Section**: `#scenario-9-webtransport-rejections`

**Symptoms**:
- WebTransport connection rejection rate >10% for 5 minutes
- Users unable to establish WebTransport sessions
- `mc_webtransport_connections_total{status="rejected"}` or `{status="error"}` elevated
- "WebTransport Connections by Status" dashboard panel showing rejected/error spike

**Diagnosis**:

```bash
# 1. Check WebTransport connection counts by status
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep mc_webtransport_connections_total
kill %1

# 2. Break down by status (accepted vs rejected vs error)
# In Prometheus:
sum by(status) (increase(mc_webtransport_connections_total[5m]))

# 3. Calculate rejection rate
(
  sum(rate(mc_webtransport_connections_total{status="rejected"}[5m]))
  /
  sum(increase(mc_webtransport_connections_total[5m]))
)

# 4. Check TLS certificate validity
kubectl exec -it deployment/mc-service -n dark-tower -- \
  openssl x509 -in /certs/tls.crt -noout -dates -subject
# Verify: notAfter is in the future

# 5. Check MC logs for TLS/QUIC errors
kubectl logs -n dark-tower -l app=mc-service --tail=500 | grep -iE "tls|quic|certificate|handshake|reject"

# 6. Check UDP port connectivity (WebTransport uses QUIC/UDP)
kubectl get svc -n dark-tower mc-service -o yaml | grep -A5 "port:"
# Verify UDP port 4433 is exposed

# 7. Check network policies for UDP traffic
kubectl get networkpolicy -n dark-tower
kubectl describe networkpolicy mc-service -n dark-tower

# 8. Check MC capacity (rejections may be due to connection limits)
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep -E "mc_connections_active|mc_meetings_active"
kill %1

# 9. Check pod resource pressure (may cause accept loop failures)
kubectl top pods -n dark-tower -l app=mc-service
```

**Common Root Causes**:

1. **TLS Certificate Expired or Missing**: QUIC handshake fails before WebTransport session
   - Check: Certificate dates via `openssl x509 -noout -dates`
   - Fix: Rotate certificate, verify cert-manager is running

2. **UDP Port Blocked**: Network policy or cloud firewall blocking QUIC/UDP
   - Check: NetworkPolicy, cloud security groups, `kubectl get svc` for port config
   - Fix: Update network policy to allow UDP on port 4433

3. **QUIC Listener Crash**: WebTransport accept loop panicked or stopped
   - Check: MC logs for panic traces, `mc_actor_panics_total` metric
   - Fix: Restart MC pod; if recurring, escalate for code fix

4. **Connection Capacity Exceeded**: Too many concurrent WebTransport connections
   - Check: `mc_connections_active` metric vs configured limit
   - Fix: Scale horizontally to distribute connections

5. **TLS Certificate Mismatch**: Client connecting with wrong SNI or hostname
   - Check: MC logs for TLS handshake errors mentioning SNI
   - Fix: Verify DNS resolution and client connection URL

6. **Transport-Level Errors** (`status="error"`): Network interruptions, malformed QUIC packets
   - Check: `mc_webtransport_connections_total{status="error"}` rate
   - Fix: Investigate network path; may indicate DDoS or network instability

**Remediation**:

```bash
# Option 1: Rotate TLS certificate (if expired)
# Check cert-manager status
kubectl get certificate -n dark-tower
kubectl describe certificate mc-service-tls -n dark-tower
# If cert-manager is not renewing, manually trigger:
kubectl delete secret mc-service-tls -n dark-tower
# cert-manager will recreate it

# Expected recovery time: 1-2 minutes (cert issuance + pod restart)

# Option 2: Fix network policy (if UDP blocked)
kubectl get networkpolicy mc-service -n dark-tower -o yaml
# Verify ingress allows UDP on port 4433
# Edit if needed:
kubectl edit networkpolicy mc-service -n dark-tower

# Expected recovery time: immediate after policy update

# Option 3: Scale horizontally (if capacity exceeded)
kubectl scale deployment/mc-service -n dark-tower --replicas=5

# Expected recovery time: 30-60 seconds

# Option 4: Restart MC (if QUIC listener crashed)
# See Recovery Procedures: #service-restart-procedure

# Verify recovery
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep mc_webtransport_connections_total
kill %1
# Rejection rate should be decreasing, accepted rate increasing
```

**Escalation**:
- If TLS cert cannot be renewed, escalate to Infrastructure Team
- If network policy changes needed, escalate to Infrastructure Team
- If QUIC listener crashes repeatedly, escalate to MC Team
- If rejection rate stays >10% for >15 minutes, upgrade to P2

---

### Scenario 10: JWT Validation Failures

**Alert**: `MCHighJwtValidationFailures`
**Severity**: Warning
**Runbook Section**: `#scenario-10-jwt-validation-failures`

**Symptoms**:
- JWT validation failure rate >10% for 5 minutes
- Users unable to authenticate for meeting join
- `mc_jwt_validations_total{result="failure"}` elevated
- "JWT Validations by Result & Type" dashboard panel showing failure spike
- MC logs: "JWT validation failed" (actual failure reason logged at debug level; client receives generic "The access token is invalid or expired" by design)

**Diagnosis**:

```bash
# 1. Check JWT validation success/failure counts
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep mc_jwt_validations_total
kill %1

# 2. Break down failures by token_type (meeting vs guest)
# In Prometheus:
sum by(token_type) (increase(mc_jwt_validations_total{result="failure"}[5m]))
# If "meeting" tokens failing: likely AC JWKS issue
# If "guest" tokens failing: likely GC token issue

# 3. Calculate failure rate
(
  sum(rate(mc_jwt_validations_total{result="failure"}[5m]))
  /
  sum(increase(mc_jwt_validations_total[5m]))
)

# 4. Check AC service health (JWKS source)
kubectl get pods -n dark-tower -l app=ac-service
kubectl exec -it deployment/mc-service -n dark-tower -- \
  curl -s http://ac-service.dark-tower.svc.cluster.local:8080/.well-known/jwks.json | head -c 500
# Verify: returns JSON with "keys" array containing at least one key

# 5. Check JWKS endpoint returns expected key IDs
kubectl exec -it deployment/mc-service -n dark-tower -- \
  curl -s http://ac-service.dark-tower.svc.cluster.local:8080/.well-known/jwks.json | grep '"kid"'
# Expected format: kid values like "auth-prod-2026-01"
# During key rotation: should see BOTH old and new kid values (overlap period)

# 6. Check clock skew between MC and AC pods
kubectl exec -it deployment/mc-service -n dark-tower -- date -u
kubectl exec -it deployment/ac-service -n dark-tower -- date -u
# Compare timestamps — drift >5s may cause validation failures
# (MC allows DEFAULT_CLOCK_SKEW_SECONDS = 5 for binding tokens;
#  common JWT layer allows 300s for standard tokens per NIST SP 800-63B)

# 7. Check NTP sync on nodes
kubectl get pods -n dark-tower -l app=mc-service -o wide
# Note the node, then check NTP on that node

# 8. Check MC logs for JWT failure details (debug level)
kubectl logs -n dark-tower -l app=mc-service --tail=500 | grep -i "jwt\|jwks\|token\|validation"

# 9. Check if this correlates with join failures
sum by(error_type) (increase(mc_session_join_failures_total[5m]))
# If error_type="jwt_validation" dominates, this scenario is the root cause
```

**Common Root Causes**:

1. **AC JWKS Endpoint Down**: MC cannot fetch public keys for validation
   - Check: AC pod health, JWKS endpoint response
   - Fix: Restart AC service, check AC logs
   - Note: MC caches JWKS keys with a 5-minute TTL (`JwksClient` default 300s). If AC goes down briefly, MC continues validating with cached keys. Failures start after cache expires.

2. **Clock Skew**: MC and AC system clocks diverged beyond tolerance
   - Check: Compare `date -u` output from MC and AC pods
   - Fix: Verify NTP sync on underlying nodes; restart chrony/ntpd if needed

3. **Key Rotation In Progress**: AC rotated signing keys but MC has not yet refreshed its JWKS cache
   - Check: JWKS endpoint should return both old and new `kid` values during rotation overlap period. If only the new key is present, tokens signed with the old key will fail.
   - Fix: Wait up to 5 minutes for MC JWKS cache to refresh. If AC removed the old key too early, the rotation was misconfigured — escalate to AC Team.

4. **Token Forging / Tampering Attempts**: Invalid signatures from unauthorized tokens
   - Check: Failure rate pattern — steady low rate suggests probing; sudden spike suggests legitimate issue. Check MC logs at debug level for signature verification vs expiry vs type mismatch failures.
   - Fix: If confirmed tampering, escalate to Security Team. MC intentionally returns generic error messages to prevent information leakage.

5. **Token Type Mismatch**: Client sending wrong token type (e.g., guest token where meeting token expected)
   - Check: MC logs for token type validation errors
   - Fix: Client-side bug — escalate to Client Team

**Remediation**:

```bash
# Step 1: Verify AC JWKS endpoint is healthy
kubectl exec -it deployment/mc-service -n dark-tower -- \
  curl -s -o /dev/null -w "%{http_code}" http://ac-service.dark-tower.svc.cluster.local:8080/.well-known/jwks.json
# Expected: 200

# Step 2: If AC is down, restart AC
kubectl get pods -n dark-tower -l app=ac-service
kubectl rollout restart deployment/ac-service -n dark-tower

# Expected recovery time: 2-3 minutes (AC restart + up to 5 min MC JWKS cache refresh)

# Step 3: If clock skew, fix NTP on affected nodes
# Identify node:
kubectl get pods -n dark-tower -l app=mc-service -o wide
# On the node, restart NTP:
# systemctl restart chronyd  (or ntpd)

# Expected recovery time: 1-2 minutes after NTP sync

# Step 4: If key rotation issue, wait for JWKS cache refresh
# MC refreshes JWKS cache every 5 minutes (300s TTL)
# Monitor validation failures — should resolve within 5 minutes after
# AC JWKS endpoint serves the correct keys

# Step 5: If tampering suspected, escalate to Security Team immediately
# Do NOT restart services — preserve logs for forensic analysis

# Verify recovery
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep mc_jwt_validations_total
kill %1
# Failure rate should be decreasing
```

**Escalation**:
- If AC is down or JWKS endpoint returns errors, escalate to AC Team
- If clock skew on nodes, escalate to Infrastructure Team
- If key rotation misconfigured (old key removed too early), escalate to AC Team
- If tampering suspected, escalate to Security Team immediately — preserve logs
- If failure rate stays >10% for >15 minutes, upgrade to P2

---

### Scenario 11: Media Connection Failures

**Alert**: `MCMediaConnectionAllFailed`
**Severity**: warning
**Runbook Section**: `#scenario-11-media-connection-failures`

**Symptoms**:
- `mc_media_connection_failures_total{all_failed="true"}` incrementing — clients are reporting via the `MediaConnectionFailed` signaling message that **every** assigned MH failed for them.
- `mc_media_connection_failures_total{all_failed="false"}` may also be elevated; this is per-MH failure noise without an immediate user impact (clients fall back to remaining MHs).
- Affected participants have signaling (MC) connectivity but no working media path.

**Impact**: Affected participants cannot send or receive media. Signaling connectivity is intact (the report itself arrived via MC WebTransport), so users see other participants in the roster but no audio/video. **MC takes no automatic remediation action** for these reports per R-20 — the metric is observability-only; reallocation is deferred to a future story.

**Treat the `error_reason` and `media_handler_url` fields in the signaling message as untrusted client input.** They reflect what the browser observed and are not authenticated as truthful (the message is authenticated as "from this session", but field content is self-reported). Always corroborate against MH-side metrics and logs before concluding a specific MH is at fault.

> **Note**: `mc_media_connection_failures_total` starts at zero in production. The `all_failed="true"` label value first appears the first time a client reports total failure; a brand-new time series with no historical baseline is not itself an incident — `rate(...{all_failed="true"}[5m])` is the actionable signal (mirrors the GC Sc 5 first-emission pattern).

**Diagnosis**:

```bash
# 1. Confirm scope: how many distinct clients reported all-failed in the window?
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep mc_media_connection_failures_total
kill %1

# 2. Rate of all-failed reports vs partial failures (delta-over-window framing)
sum(rate(mc_media_connection_failures_total{all_failed="true"}[5m]))
sum(rate(mc_media_connection_failures_total{all_failed="false"}[5m]))

# 3. Compare against active meeting count — a single all-failed report against
#    a 100-participant meeting is very different from 10 reports against a 2p meeting
sum(mc_meetings_active)
sum(mc_connections_active)

# 4. CORROBORATE before trusting the client-reported reason.
#    Cross-check MH-side handshake health (do not trust MediaConnectionFailed.error_reason alone):
sum by(status) (rate(mh_webtransport_connections_total[5m]))
histogram_quantile(0.95,
  sum by(le) (rate(mh_webtransport_handshake_duration_seconds_bucket[5m]))
)

# 5. Check MH JWT validation health — JWT failures at MH look like "media not working" to clients
sum by(failure_reason) (rate(mh_jwt_validations_total{result="failure"}[5m]))

# 6. MC logs for the WebTransport connection handler (target mc.webtransport.connection)
kubectl logs -n dark-tower -l app=mc-service --tail=500 \
  | grep -iE "MediaConnectionFailed|all_handlers_failed"

# 7. MH pod health (per-pod failures correlate with this report)
kubectl get pods -n dark-tower -l app=mh-service
```

**Common Root Causes**:

Client reports a failure; the underlying cause is almost always upstream of MC. Triage by corroborating evidence.

> **Strong-signal short-circuit**: if the all-failed reports correlate with elevated `mh_register_meeting_timeouts_total` (MH Sc 13) AND/OR elevated `mc_register_meeting_total{status="error"}` (MC Sc 12) in the same window, treat the RegisterMeeting coordination break as the upstream root cause. Clients connect to MH, get JWT-validated, and are then provisional-kicked because the meeting was never registered — this surfaces to clients as "all MHs failed." Fix the coordination, the all-failed reports stop. Skip to root cause #3 below.

1. **MH WebTransport rejections fleet-wide** — clients connect to MH, get rejected during handshake. Check `mh_webtransport_connections_total{status="rejected"}` rate. See [MH Sc 5: WebTransport Rejections](mh-incident-response.md#scenario-5-webtransport-rejections).
2. **MH JWT validation failures** — clients have valid JWTs at MC but MH rejects them (JWKS skew, key rotation). See [MH Sc 2: JWT Validation Failures](mh-incident-response.md#scenario-2-jwt-validation-failures).
3. **MH RegisterMeeting timeouts** — clients arriving at MH before MC has registered the meeting; MH provisional-kicks them. See [MH Sc 13: RegisterMeeting Timeout](mh-incident-response.md#scenario-13-registermeeting-timeout--clients-kicked) and [MC Sc 12](#scenario-12-registermeeting-coordination-failures).
4. **MH down or unreachable** — full MH outage or NetworkPolicy regression between client and MH. Check MH pod health and UDP/4434 reachability per MH Sc 1.
5. **TLS / certificate issues at MH** — clients fail QUIC handshake. **Do not conclude this from `error_reason="tls"` alone** — verify with `openssl x509` against the actual MH cert and with MH-side handshake metrics.
6. **Network path / cloud firewall** — UDP egress from client → MH blocked. Diffuse pattern across many `media_handler_url` values from many clients points here.
7. **Capacity exhaustion at MH** — `mh_active_connections` at cap, new clients rejected. See MH Sc 5 + Sc 8.

**Remediation**:

```bash
# Step 1: Identify the dominant upstream cause from the corroborating MH metrics in
#         Diagnosis steps 4–6. The MC-side runbook does not "fix" this scenario directly —
#         remediation is on the MH path the clients are reporting against.

# Step 2: If MH WebTransport rejections / capacity:
#   See MH Sc 5 — scale MH or rotate TLS as appropriate.

# Step 3: If MH RegisterMeeting timeouts (clients kicked before media establishes):
#   See MC Sc 12 below — fix MC→MH RegisterMeeting delivery.

# Step 4: If MH down:
#   See MH Sc 1 — restore MH service.

# Step 5: Monitor for resolution. The metric is a leading indicator of user impact;
#         all_failed=true rate dropping to zero across a 5-minute window is the
#         recovery signal.
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
watch -n 30 'curl -s http://localhost:8080/metrics | grep mc_media_connection_failures_total'
kill %1
```

Expected recovery time: bounded by upstream MH/network fix; once the underlying MH/network issue is resolved, in-flight `MediaConnectionFailed` reports stop within 30-60s and the rate decays to baseline over the next 5m window. New clients establish media within their normal connect timeout (~5s).

**Escalation**:
- If MH path is healthy by every MH-side metric and the all-failed rate persists, escalate to Infrastructure Team — likely a network path issue between clients and MH that MH itself cannot observe.
- If `all_failed="true"` rate stays elevated for >15 minutes, upgrade to P1 — affected participants have no usable media.
- If client reports point at TLS/cert issues but MH-side cert is verifiably valid, escalate to Client Team (possible client trust-store misconfiguration).

**Dashboards**: MC Overview → "Media Connection Failures" panel (if present); MH Overview → WebTransport handshake status panel + JWT validation panel for corroborating evidence.

---

### Scenario 12: RegisterMeeting Coordination Failures

**Alert**: No alert today; surfaces in `mc_register_meeting_total{status="error"}` rate, `mc_register_meeting_duration_seconds` p95, and `RegisterMeeting retries exhausted` error logs at target `mc.register_meeting.trigger`. May also co-fire MH-side [Scenario 13: RegisterMeeting Timeout — Clients Kicked](mh-incident-response.md#scenario-13-registermeeting-timeout--clients-kicked).
**Severity**: warning
**Runbook Section**: `#scenario-12-registermeeting-coordination-failures`

<!-- TODO: alert MCRegisterMeetingFailureRate — no alert rule exists today; this scenario is metric-driven triage. When observability adds a rule, link it here and remove this TODO. -->

> **Note**: `mc_register_meeting_total{status="error"}` and `mc_register_meeting_duration_seconds` start at zero in production and only emit on the first first-participant join. A brand-new series is not itself an incident; `rate(...{status="error"}[5m]) > 0` against a non-trivial total rate is the actionable signal.

**Symptoms**:
- `mc_register_meeting_total{status="error"}` non-zero or rising vs baseline. MC retries each MH up to 3 attempts with 1s/2s backoffs (see `register_meeting_with_handlers` in `crates/mc-service/src/webtransport/connection.rs`); a steady error rate means retries are being exhausted.
- `mc_register_meeting_duration_seconds` p95 climbing — RegisterMeeting RPCs are succeeding but slowly, eating into the MH-side 15s timeout budget.
- MC error log: `"RegisterMeeting retries exhausted"` (target `mc.register_meeting.trigger`) with `mh_grpc_endpoint` and the underlying error.
- Concurrent MH-side `mh_register_meeting_timeouts_total` rising — same incident from the receiver's vantage.
- New meetings fail to ever produce media; clients connect to MH, get JWT-validated, then disconnected ~15s later. From a user perspective: "I joined the meeting but media never came up."

**Impact**: New meetings on this MC do not get registered with their assigned MHs, so first-participant clients are kicked by the MH provisional-accept timeout. **Existing already-registered meetings are unaffected** — RegisterMeeting fires only on first-participant join (R-12). Subset of new meetings affected; severity is warning because clients still have signaling to MC and the active/active topology means a single MH coordination failure does not block the meeting if other assigned MHs registered successfully.

> **Rollback awareness**: During a deliberate rollback to a pre-RegisterMeeting MH version, MC will see exactly these symptoms — old MH does not implement the RPC, so MC retries then exhausts. **This is expected during the rollback window**; clients still establish via the JWT path. Confirm by checking `kubectl rollout history deployment/mh-service -n dark-tower` for an in-progress rollback before treating as an incident. Coordinate with the deployer.

**Diagnosis**:

```bash
# 1. RegisterMeeting outcome rate by status
sum by(status) (rate(mc_register_meeting_total[5m]))

# 2. Latency p95 (delta-over-window; uses the MC SLO histogram shape)
histogram_quantile(0.95,
  sum by(le) (rate(mc_register_meeting_duration_seconds_bucket[5m]))
)

# 3. Error rate vs 1h baseline (distinguish incident from background sequencing race)
sum(rate(mc_register_meeting_total{status="error"}[5m]))
/
clamp_min(sum(rate(mc_register_meeting_total[1h])), 0.001)

# 4. MC error logs — error message, mh_grpc_endpoint, and total_attempts are logged
kubectl logs -n dark-tower -l app=mc-service --tail=500 \
  | grep -iE "RegisterMeeting retries exhausted|RegisterMeeting attempt failed"

# 5. MH-side correlate (timeouts on the receiving MH)
sum(rate(mh_register_meeting_timeouts_total[5m]))

# 6. MC→MH gRPC reachability for the failing endpoint(s) from a fresh shell
kubectl exec -it deployment/mc-service -n dark-tower -- \
  grpcurl -plaintext mh-service.dark-tower.svc.cluster.local:50051 list

# 7. Check if MC mailbox depth is queueing the trigger task (CPU/backpressure cause)
sum by(actor_type) (mc_actor_mailbox_depth)

# 8. NetworkPolicy mc → mh
kubectl describe networkpolicy mc-service -n dark-tower
```

**Common Root Causes**:

1. **MH down or unreachable** — gRPC connect fails to all MHs in the assignment. Check MH pod health (`kubectl get pods -l app=mh-service`). See [MH Sc 1: Complete Service Outage](mh-incident-response.md#scenario-1-complete-service-outage).
2. **NetworkPolicy regression** — MC egress to MH gRPC port blocked. Infrastructure Team. Recently-deployed network policy is the most common trigger; check `kubectl rollout history`.
3. **MH JWKS unable to validate MC's service token** — MC presents a token that MH rejects at Layer 1 auth. Cross-check `mh_jwt_validations_total{token_type="service",result="failure"}`. See [MH Sc 2: JWT Validation Failures](mh-incident-response.md#scenario-2-jwt-validation-failures).
4. **MH overloaded** — RPC succeeds but slowly; p95 climbs into the timeout budget. Scale MH.
5. **MC overloaded / mailbox backpressure** — first-participant trigger task is queued behind other work; the 1+2s retry budget elapses before the network path even matters. See [Scenario 1: High Mailbox Depth](#scenario-1-high-mailbox-depth).
6. **Stale Redis MH assignment data** — assignment points at MH endpoints that no longer exist (drained / scaled-down). MC will exhaust retries against ghost endpoints. Check `MhAssignmentStore` freshness vs current MH pod IPs.

**Remediation**:

```bash
# Step 1: Confirm the dominant root cause from the metric breakdown above.

# Step 2: If MH down — restore MH (MH Sc 1).
# Step 3: If NetworkPolicy regression — Infrastructure Team. To verify before rollback:
kubectl get networkpolicy -n dark-tower -o yaml | grep -A5 mh
kubectl rollout undo deployment/mc-service -n dark-tower
# (Or revert the NetworkPolicy change — coordinate with Infrastructure.)

# Step 4: If MH JWKS rejection — escalate AC Team (see MH Sc 2 + MC Sc 10);
#          MC service token may need rotation if auth_rejected dominates.

# Step 5: If MC mailbox backpressure — scale MC horizontally
kubectl scale deployment/mc-service -n dark-tower --replicas=5

# Step 6: If stale Redis MH assignment data — escalate to MC Team to investigate
#         MhAssignmentStore TTL / refresh logic. Affected meetings will recover
#         on next first-participant join after the assignment is refreshed.

# Verify recovery
sum by(status) (rate(mc_register_meeting_total[5m]))
# status="success" should dominate; status="error" rate should drop to baseline.
```

Expected recovery time: 30-60s for horizontal MC scale; 1-2 minutes for NetworkPolicy revert; 2-3 minutes for AC service-token rotation + MC restart. Stale Redis assignment data: bounded by Redis TTL refresh (consult MhAssignmentStore config) — affected meetings recover on next first-participant-join after refresh.

**Rollback nuance**: `RegisterMeeting` is a new RPC. If MH is rolled back to a pre-RegisterMeeting build, MC will keep sending the RPC, exhaust retries, and log `"RegisterMeeting retries exhausted"` for each affected meeting. **Clients are NOT stranded by this** — they still establish WebTransport sessions to the rolled-back MH via JWT, and the active/active topology covers any meeting where some assigned MHs are on the new build. If you must roll MH back during a coordinated incident, expect sustained `mc_register_meeting_total{status="error"}` for the duration of the rollback window; suppress alerts for that period rather than blocking the rollback.

**Alert candidate** (breadcrumb for future observability work; out of scope here): a candidate threshold for an `MCRegisterMeetingFailureRate` alert would be `(sum(rate(mc_register_meeting_total{status="error"}[5m])) / sum(rate(mc_register_meeting_total[5m]))) > 0.10 and sum(rate(mc_register_meeting_total[5m])) > 0` for 5m at `severity: warning`, with `runbook_url` pointing at this scenario. This mirrors the shape of `MCHighJoinFailureRate`. See `<!-- TODO: alert ... -->` comment near the top of this scenario.

**Do NOT recommend tuning `MH_REGISTER_MEETING_TIMEOUT_SECONDS` (default 15s) as a mitigation.** That timeout is the security boundary that bounds stolen-JWT-against-unregistered-meeting exposure on the MH side. Sustained timeouts mean the coordination path is broken — fix the path, do not widen the window. If you believe the timeout itself is wrong, escalate to Security Team for review.

**Escalation**:
- If error rate >10% for >15 minutes, upgrade to P2 — new meetings are losing media.
- If MH is healthy by every MH-side metric but MC still cannot reach it, escalate to Infrastructure Team.
- If retries-exhausted logs span many distinct `mh_grpc_endpoint` values in a short window, this is fleet-wide MH coordination failure — page MH Team and consider whether GC has stale capacity records.

**Related Alerts**: MH-side `mh_register_meeting_timeouts_total` (downstream effect on receiving MH), `MCHighMailboxDepthWarning` (upstream cause when trigger task is queued), `MCMediaConnectionAllFailed` (downstream user-visible effect when first-participant clients are kicked from every assigned MH).

**Dashboards**: MC Overview → "RegisterMeeting RPC Rate by Status" + "RegisterMeeting RPC Latency (P50/P95/P99)" (the headline panels for this scenario, both in the MH Coordination row); MH Overview → "RegisterMeeting Receipts by Status" + "RegisterMeeting Timeouts (R-26)" (receiver-side correlates).

---

### Scenario 13: Unexpected MH Notifications

**Alert**: No alert today; diagnostic-only signal in `mc_mh_notifications_received_total` and warn-level log `"Connection registry limit reached for meeting"` at target `mc.grpc.media_coordination` (the gRPC handler emits this when the registry's `add_connection` returns `false`). The registry-internal log `"Meeting connection limit reached, rejecting new connection"` at target `mc.mh_registry` is the lower-level companion emitted from the same code path.
**Severity**: info (operational drift) / page (if security branch — see Common Root Causes)

> **Note on metric asymmetry**: MC has **no failure metric for inbound MH notifications by design** — `mc_mh_notifications_received_total` only carries an `event_type` label, no `status`. Failures originate and are counted on the MH sender side as `mh_mc_notifications_total{status="error"}`; see [MH Scenario 10: MH→MC Notification Failures](mh-incident-response.md#scenario-10-mhmc-notification-failures). This scenario is the complement: notifications that *successfully reached MC* but reference state MC does not expect.
>
> `mc_mh_notifications_received_total` starts at zero in production and emits on the first MH notification; a brand-new series is not itself an incident. The actionable signal is rate-relative-to-expected (see Diagnosis step 2) and the warn-log on registry-cap hits.

**Symptoms**:
- `mc_mh_notifications_received_total{event_type}` rate higher than expected for the active-meeting count.
- MC `debug` logs: `"Connection was not in registry (may have already been removed)"` (target `mc.grpc.media_coordination`) — disconnect notifications arriving for connections MC does not have a record of. Routine occurrence in small numbers; sustained volume is the signal.
- MC `warn` logs: `"Connection registry limit reached for meeting"` — `MAX_CONNECTIONS_PER_MEETING` (1000) cap hit, new connect notifications silently dropped. This is unusual for normal meeting load and warrants investigation.
- gRPC access logs: notifications arriving with `meeting_id` values that do not appear in `mc_meetings_active` for this MC.

**Impact**: Operationally, drift between MC's MhConnectionRegistry and reality. Currently the registry is observability-only (read by future media routing per R-18); a small amount of drift is tolerated by design. **The signal matters for two reasons**:
1. **Operational drift branch** — diffuse pattern across many `meeting_id` values from many MH source identities suggests GC↔MC↔MH routing has lost coherence (e.g., meeting reassigned but stale MH still notifying). Self-heals as participants reconnect.
2. **Authenticated-misbehavior branch** — steady-rate stream of unknown-meeting notifications from a single MH service identity suggests a compromised or misconfigured MH that has passed Layer 1 (JWKS) and Layer 2 (caller-type) auth but is sending notifications it should not be. Treat as a potential security incident.

**Diagnosis**:

```bash
# 1. Volume of notifications received
sum by(event_type) (rate(mc_mh_notifications_received_total[5m]))

# 2. Compare against expected: 1 connect notification per (participant × MH)
#    on first connect, 1 disconnect on departure. Active-conn baseline:
sum(mc_connections_active)

# 3. Stale-disconnect rate (debug-level log; tail with care)
kubectl logs -n dark-tower -l app=mc-service --tail=2000 \
  | grep -c "Connection was not in registry" \
  || true

# 4. Registry-cap hits (warn-level)
kubectl logs -n dark-tower -l app=mc-service --tail=2000 \
  | grep -i "Connection registry limit reached"

# 5. SHAPE the signal — diffuse vs concentrated:
#    Pull MH source identity from gRPC handler logs (target mc.grpc.media_coordination).
#    The auth interceptor logs the calling service identity at debug level on each request.
#    `mc_mh_notifications_received_total` itself currently has only `event_type` —
#    there is no `source_id` label, so attribution must come from logs / traces.
kubectl logs -n dark-tower -l app=mc-service --tail=2000 \
  | grep -iE "media_coordination.*meeting_id"
# Look for: are notifications arriving for the same meeting_id repeatedly from one MH,
# or scattered across many meetings + many MHs?

# 6. Cross-check with GC's view of meeting assignments (operational-drift hypothesis)
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c \
  "SELECT meeting_id, mc_id, status FROM meetings WHERE updated_at > NOW() - INTERVAL '1 hour' ORDER BY updated_at DESC LIMIT 50;"
```

**Common Root Causes**:

Triage by signal shape from Diagnosis step 5:

1. **Diffuse, many-MH, many-meeting → Operational drift.**
   - GC reassigned meetings but old MH continued to notify briefly.
   - MC was restarted and lost its in-memory registry; in-flight disconnects from MHs arrive for meetings the new MC instance never registered.
   - MhConnectionRegistry cleanup race in `controller.rs::remove_meeting()`.
   - Self-heals; no immediate action. Investigate MhConnectionRegistry behavior for tracking debt.

2. **Concentrated, single-MH source identity, many unknown meetings → Authenticated-MH misbehavior.**
   - Compromised MH credentials being used by an attacker who passed Layer 1+2 auth and is probing or fuzzing the MediaCoordinationService.
   - Misconfigured MH instance running with the wrong meeting-routing config and broadcasting notifications to the wrong MC.
   - **Treat as a security incident**: preserve logs (gRPC access log + handler log + the auth interceptor's caller-identity emission), snapshot `mc_mh_notifications_received_total`, do **NOT** restart MC, escalate to Security Team. Do not remediate operationally until Security has triaged.

3. **Registry cap hit (`MAX_CONNECTIONS_PER_MEETING=1000` reached).**
   - Legitimate giant-meeting scenario or a runaway MH spamming the same `meeting_id`.
   - If the meeting's `mc_connections_active` corroborates ~1000 participants, this is a capacity-planning signal — discuss with MC Team.
   - If `mc_connections_active` is small but the registry is full for the meeting, treat as the authenticated-misbehavior branch (#2 above).

**Remediation**:

```bash
# Operational-drift branch (root cause #1):
# No action — monitor. Drift resolves as participants reconnect or meetings end.

# Authenticated-misbehavior branch (root cause #2):
# 1. PRESERVE logs first — capture before any restart.
kubectl logs -n dark-tower -l app=mc-service --tail=5000 \
  > /tmp/mc-incident-$(date -u +%Y%m%dT%H%M%SZ).log
# 2. Snapshot the metric for forensic baseline.
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl -s http://localhost:8080/metrics | grep mc_mh_notifications_received_total \
  > /tmp/mc-metrics-$(date -u +%Y%m%dT%H%M%SZ).txt
kill %1
# 3. Do NOT restart MC. Do NOT rotate the MH service token (yet) — Security needs evidence.
# 4. Escalate to Security Team via #security-incidents.

# Capacity-cap branch (root cause #3):
# Confirm legitimate giant meeting via mc_connections_active for the affected meeting_id.
# If legitimate, file capacity-planning ticket; do not remediate at runtime.
# If not legitimate, treat as the authenticated-misbehavior branch above.
```

Expected recovery time: branch-dependent. Operational drift self-heals over the next 5-15m as participants reconnect or meetings end (no runtime action). Authenticated-misbehavior branch: bounded by Security investigation timeline (do not auto-recover). Capacity-cap branch on a legitimate giant meeting: persists until the meeting ends or capacity-planning lifts the cap; not a runtime fix.

**What this scenario tells you**: this is a diagnostic-appendix scenario, not an alerting one. Use it when you are ALREADY investigating a different signal (e.g. an MC actor panic, a security review of MH→MC RPCs, or an operational sweep) and you notice elevated `mc_mh_notifications_received_total` rate or `"Connection registry limit reached"` warn-logs that don't fit the surrounding incident. The scenario routes you to the right triage branch without re-deriving "what is `MhConnectionRegistry` and why does it have a registry-cap." If you found this scenario via an alert, the alert is wrong — file an issue.

**Escalation**:
- Authenticated-misbehavior branch: Security Team immediately. Do not restart, do not rotate tokens until Security has captured evidence.
- Operational-drift branch: no escalation; track for trend.
- Registry-cap hit on a small meeting: MC Team for investigation.

**Related Alerts**: `MCActorPanic` (if a MeetingActor crashed and lost registry state), MH-side `MHCallerTypeRejected` (Layer 2 caller-type rejections — would indicate misbehaving services that did NOT clear Layer 2; this scenario is the complementary branch where Layer 2 was passed); MH-side [Scenario 10: MH→MC Notification Failures](mh-incident-response.md#scenario-10-mhmc-notification-failures) (the *sender-side* view of the same RPC pair).

**Dashboards**: MC Overview → MH-coordination row notification panels (if present); rely on log-based triage (target `mc.grpc.media_coordination`) for source-identity attribution since the metric has no `source_id` label today.

---

## Diagnostic Commands

### Quick Health Check

```bash
# Check service health
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/health      # Liveness
curl http://localhost:8080/ready       # Readiness
kill %1

# Check pod status
kubectl get pods -n dark-tower -l app=mc-service

# Check recent errors in logs
kubectl logs -n dark-tower -l app=mc-service --tail=100 | grep -i error
```

### Metrics Analysis

```bash
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &

# Get all metrics
curl http://localhost:8080/metrics

# Actor system metrics
curl http://localhost:8080/metrics | grep mc_actor

# Meeting metrics
curl http://localhost:8080/metrics | grep mc_meetings

# Connection metrics
curl http://localhost:8080/metrics | grep mc_connections

# Session join metrics (rate, duration, failures)
curl http://localhost:8080/metrics | grep mc_session_join

# Redis op latency
curl http://localhost:8080/metrics | grep mc_redis_latency

# GC integration metrics
curl http://localhost:8080/metrics | grep mc_gc

kill %1
```

### Log Analysis

```bash
# Stream logs in real-time
kubectl logs -n dark-tower -l app=mc-service -f

# Get logs from all pods
kubectl logs -n dark-tower -l app=mc-service --all-containers --tail=200

# Get logs from previous pod instance (after crash)
kubectl logs -n dark-tower <pod-name> --previous

# Search for specific errors
kubectl logs -n dark-tower -l app=mc-service --tail=1000 | grep -E "error|panic|fatal"

# Search for actor panics
kubectl logs -n dark-tower -l app=mc-service --tail=1000 | grep -A 30 "panic\|PANIC"

# Search for GC integration issues
kubectl logs -n dark-tower -l app=mc-service --tail=1000 | grep -i "gc\|heartbeat\|register"

# Search for meeting lifecycle events
kubectl logs -n dark-tower -l app=mc-service --tail=1000 | grep -i "meeting\|session\|participant"
```

### Resource Utilization

```bash
# Check CPU and memory usage
kubectl top pods -n dark-tower -l app=mc-service

# Check node resources
kubectl top nodes

# Check resource limits
kubectl describe deployment mc-service -n dark-tower | grep -A 5 "Limits:"

# Check events for resource issues
kubectl get events -n dark-tower --field-selector involvedObject.name=mc-service --sort-by='.lastTimestamp'
```

### Network Debugging

```bash
# Test service connectivity
kubectl run -it --rm debug --image=nicolaka/netshoot --restart=Never -- /bin/bash
# From debug pod:
curl http://mc-service.dark-tower.svc.cluster.local:8080/health
nslookup mc-service.dark-tower.svc.cluster.local

# Check service endpoints
kubectl get endpoints -n dark-tower mc-service

# Check network policies
kubectl get networkpolicies -n dark-tower

# Test GC connectivity
kubectl exec -it deployment/mc-service -n dark-tower -- \
  curl -i http://gc-service.dark-tower.svc.cluster.local:8080/health
```

---

## Recovery Procedures

### Service Restart Procedure

**When to use**: Minor issues, stuck state, memory pressure

```bash
# 1. Verify current state
kubectl get pods -n dark-tower -l app=mc-service

# 2. Check active meetings (will be affected)
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl http://localhost:8080/metrics | grep mc_meetings_active
kill %1

# 3. Perform rolling restart (zero-downtime if multiple pods)
kubectl rollout restart deployment/mc-service -n dark-tower

# 4. Monitor rollout
kubectl rollout status deployment/mc-service -n dark-tower

# 5. Verify recovery
kubectl get pods -n dark-tower -l app=mc-service
curl http://mc-service.dark-tower.svc.cluster.local:8080/ready

# 6. Check logs for startup errors
kubectl logs -n dark-tower -l app=mc-service --tail=50
```

**Rollback on failure**:
```bash
kubectl rollout undo deployment/mc-service -n dark-tower
```

---

### Graceful Drain Procedure

**When to use**: Planned maintenance, pre-deployment

```bash
# 1. Mark MC as draining in GC
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "UPDATE meeting_controllers SET status = 'draining' WHERE id = '<MC_ID>';"

# 2. Wait for active meetings to complete (monitor metric)
watch -n 30 'kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 2>/dev/null & sleep 1; curl -s http://localhost:8080/metrics | grep mc_meetings_active; kill %1 2>/dev/null'

# 3. When meetings are zero, proceed with maintenance

# 4. After maintenance, re-enable
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "UPDATE meeting_controllers SET status = 'active' WHERE id = '<MC_ID>';"
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
- Number of affected meetings: [metric]
- Number of affected participants: [estimate]
- Duration of impact: [X minutes/hours]

**Business Impact**:
- Meeting minutes lost: [estimate]
- Customer complaints: [number]
- SLA breach: Yes/No - [details]

**Metrics**:
- Peak message drop rate: [from mc_messages_dropped_total]
- Peak mailbox depth: [from mc_actor_mailbox_depth]
- Actor panics: [from mc_actor_panics_total]

---

## Timeline

All times in UTC.

| Time (UTC) | Event |
|------------|-------|
| HH:MM | [First alert fired] |
| HH:MM | [On-call engineer acknowledged] |
| HH:MM | [Investigation began] |
| HH:MM | [Root cause identified] |
| HH:MM | [Remediation started] |
| HH:MM | [Service recovered] |
| HH:MM | [Incident declared resolved] |

---

## Root Cause

[Detailed explanation of what caused the incident]

---

## Action Items

| Action | Owner | Due Date | Priority | Status |
|--------|-------|----------|----------|--------|
| [Fix root cause] | [Name] | YYYY-MM-DD | P0 | Open |
| [Update runbook] | [Name] | YYYY-MM-DD | P1 | Open |
| [Add alert] | [Name] | YYYY-MM-DD | P1 | Open |
```

---

## Maintenance and Updates

**Runbook Ownership**:
- **Primary**: Operations Specialist
- **Reviewers**: MC Service Owner, On-call rotation members

**Review Schedule**:
- After every P1/P2 incident (update within 24 hours)
- Monthly review during on-call handoff
- Quarterly comprehensive review

**Version History**:
- 2026-05-01: Add Scenarios 11-13 (MediaConnectionFailed reports, RegisterMeeting coordination failures, unexpected MH notifications) — covers MC↔MH coordination failure modes for the client→MH QUIC connection story. New scenarios use ADR-0031 canonical lowercase severity vocabulary (`page` / `warning` / `info`) deliberately; existing Sc 1-10 retain inherited Title Case (`Warning` / `Critical` / `Info`) — do NOT normalize one to the other without an ADR follow-up.
- 2026-03-27: Add Scenarios 8-10 (join failures, WebTransport rejections, JWT validation failures); fix 7 stale metric references
- 2026-02-09: Initial version

---

## Additional Resources

- **ADR-0010**: Global Controller Architecture (MC registration)
- **ADR-0011**: Observability Framework
- **ADR-0012**: Infrastructure Architecture
- **MC Service Architecture**: `docs/ARCHITECTURE.md` (MC section)
- **On-call Rotation**: PagerDuty schedule "Dark Tower MC Team"
- **Slack Channels**:
  - `#incidents` - Active incident coordination
  - `#dark-tower-ops` - Operational discussions
  - `#mc-service` - Service-specific channel
  - `#gc-oncall` - GC team escalation
  - `#mh-oncall` - MH team escalation
  - `#infra-oncall` - Infrastructure team escalation

---

**Remember**: When in doubt, escalate. It's better to involve specialists early than to struggle alone during an incident.
