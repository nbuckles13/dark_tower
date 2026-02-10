# MC Service Incident Response Runbook

**Service**: Meeting Controller (mc-service)
**Owner**: SRE Team
**On-Call Rotation**: PagerDuty - Dark Tower MC Team
**Last Updated**: 2026-02-09

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
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &
curl http://localhost:8080/metrics | grep mc_actor_mailbox_depth
kill %1

# 2. Identify which actor type is backlogged
# In Prometheus:
sum by(actor_type) (mc_actor_mailbox_depth)

# 3. Check message processing rate vs incoming rate
# Processing rate:
sum by(actor_type) (rate(mc_message_processing_duration_seconds_count[5m]))
# If processing rate is lower than mailbox growth, actor is overwhelmed

# 4. Check for slow message processing
histogram_quantile(0.99, sum by(le, actor_type) (rate(mc_message_processing_duration_seconds_bucket[5m])))

# 5. Check active meetings and connections
curl http://localhost:8080/metrics | grep -E "mc_meetings_active|mc_connections_active"

# 6. Check pod resource usage
kubectl top pods -n dark-tower -l app=meeting-controller

# 7. Check for message drops
curl http://localhost:8080/metrics | grep mc_messages_dropped_total
```

**Common Root Causes**:

1. **Slow Message Processing**: Message handler taking too long
   - Check: p99 processing latency by actor type
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
kubectl scale deployment/meeting-controller -n dark-tower --replicas=5

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
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &
curl http://localhost:8080/metrics | grep mc_actor_panics_total
kill %1

# 2. Identify affected actor type
# In Prometheus:
sum by(actor_type) (increase(mc_actor_panics_total[5m]))

# 3. Find panic in logs (look for stack trace)
kubectl logs -n dark-tower -l app=meeting-controller --tail=500 | grep -A 50 "panic\|PANIC"

# 4. Find correlation with meetings
# Look for meeting-related context in panic logs
kubectl logs -n dark-tower -l app=meeting-controller --tail=500 | grep -B 10 "panic" | grep -i "meeting\|session"

# 5. Check if panic is recurring
# Watch panic counter
watch -n 5 'kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 2>/dev/null & sleep 1; curl -s http://localhost:8080/metrics | grep mc_actor_panics_total; kill %1 2>/dev/null'
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
kubectl rollout history deployment/meeting-controller -n dark-tower

# If panic started after deployment:
kubectl rollout undo deployment/meeting-controller -n dark-tower

# Expected recovery time: 2-3 minutes

# Step 3: If panic is isolated, restart affected pod
kubectl delete pod <MC_POD_NAME> -n dark-tower

# Expected recovery time: 30 seconds

# Step 4: Monitor for recurrence
watch -n 10 'kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 2>/dev/null & sleep 1; curl -s http://localhost:8080/metrics | grep mc_actor_panics_total; kill %1 2>/dev/null'

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
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &
curl http://localhost:8080/metrics | grep -E "mc_meetings_active|mc_connections_active"
kill %1

# 2. Check message processing activity
curl http://localhost:8080/metrics | grep mc_message_processing_duration_seconds

# 3. Look for meeting-related errors in logs
kubectl logs -n dark-tower -l app=meeting-controller --tail=500 | grep -i "meeting\|session\|lifecycle"

# 4. Check for connection issues
kubectl logs -n dark-tower -l app=meeting-controller --tail=500 | grep -i "connection\|disconnect\|webtransport"

# 5. Check GC perspective on meetings
kubectl exec -it deployment/global-controller -n dark-tower -- \
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
kubectl exec -it deployment/global-controller -n dark-tower -- \
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
- No healthy pods in `kubectl get pods -l app=meeting-controller`
- Alert: `MCDown` firing
- Active meetings disrupted, users disconnected

**Diagnosis**:

```bash
# 1. Check pod status
kubectl get pods -n dark-tower -l app=meeting-controller

# 2. Check pod events
kubectl describe pods -n dark-tower -l app=meeting-controller

# 3. Check recent logs before crash
kubectl logs -n dark-tower -l app=meeting-controller --previous --tail=100

# 4. Check deployment status
kubectl describe deployment meeting-controller -n dark-tower

# 5. Check resource quotas
kubectl describe resourcequota -n dark-tower

# 6. Check node status
kubectl get nodes
kubectl describe node <node-name>

# 7. Check for recent deployments
kubectl rollout history deployment/meeting-controller -n dark-tower
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
kubectl rollout undo deployment/meeting-controller -n dark-tower
kubectl rollout status deployment/meeting-controller -n dark-tower

# Expected recovery time: 2-3 minutes

# Option 2: Force reschedule pods
kubectl delete pods -n dark-tower -l app=meeting-controller
# Deployment will recreate them

# Expected recovery time: 30-60 seconds

# Option 3: Check and restore missing secrets/configmaps
kubectl get secret -n dark-tower mc-service-secrets
kubectl get configmap -n dark-tower mc-service-config
# If missing, recreate from secure backup

# Option 4: Increase resource limits (if OOMKilled)
kubectl patch deployment/meeting-controller -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"meeting-controller","resources":{"limits":{"memory":"2Gi"}}}]}}}}'

# Expected recovery time: 2-3 minutes

# Verify recovery
kubectl get pods -n dark-tower -l app=meeting-controller
kubectl logs -n dark-tower -l app=meeting-controller --tail=50
```

**Escalation**:
- If rollback fails, escalate to Engineering Lead immediately
- If node issues, escalate to Infrastructure Team
- Inform GC Team so they can route new meetings to other MCs

---

### Scenario 5: High Latency

**Alert**: `MCHighLatency`
**Severity**: Critical
**Runbook Section**: `#scenario-5-high-latency`

**Symptoms**:
- Alert: p95 message processing latency >500ms
- Meeting participants experiencing delays
- Sluggish real-time communication
- Possible timeout errors in clients

**Diagnosis**:

```bash
# 1. Check current latency metrics
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &
curl http://localhost:8080/metrics | grep mc_message_processing_duration_seconds
kill %1

# 2. Check latency by actor type
# In Prometheus:
histogram_quantile(0.95, sum by(actor_type, le) (rate(mc_message_processing_duration_seconds_bucket[5m])))

# 3. Check mailbox depth (backpressure causes latency)
sum by(actor_type) (mc_actor_mailbox_depth)

# 4. Check pod resource utilization
kubectl top pods -n dark-tower -l app=meeting-controller

# 5. Check for GC heartbeat latency (slow GC can cause delays)
histogram_quantile(0.95, sum by(le) (rate(mc_gc_heartbeat_duration_seconds_bucket[5m])))

# 6. Check for garbage collection pauses (if applicable)
kubectl logs -n dark-tower -l app=meeting-controller --tail=500 | grep -i "gc\|pause"

# 7. Check network latency between pods
kubectl exec -it deployment/meeting-controller -n dark-tower -- ping global-controller.dark-tower.svc.cluster.local
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
kubectl scale deployment/meeting-controller -n dark-tower --replicas=5

# Expected recovery time: 30-60 seconds

# Scenario B: Mailbox Backpressure
# See Scenario 1 remediation

# Scenario C: Memory Pressure
kubectl patch deployment/meeting-controller -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"meeting-controller","resources":{"limits":{"memory":"2Gi"}}}]}}}}'

# Expected recovery time: 2-3 minutes

# Scenario D: Pod restart (clears accumulated state)
kubectl delete pod <POD_NAME> -n dark-tower

# Expected recovery time: 30 seconds
# WARNING: Active meetings affected

# Verify recovery
histogram_quantile(0.95, sum by(le) (rate(mc_message_processing_duration_seconds_bucket[5m])))
# Should return value < 0.500
```

**Escalation**:
- If latency persists after scaling, escalate to Service Owner
- If GC is the bottleneck, escalate to GC Team
- If network issues, escalate to Infrastructure Team

---

### Scenario 6: GC Integration Failures

**Alert**: `MCGCHeartbeatFailure`, `MCGCHeartbeatWarning`
**Severity**: Critical (>50% failures) / Warning (>10% failures)
**Runbook Section**: `#scenario-6-gc-integration-failures`

**Symptoms**:
- GC heartbeat failures increasing
- MC not receiving new meeting assignments
- GC may mark MC as unhealthy
- New meetings not being routed to this MC

**Diagnosis**:

```bash
# 1. Check heartbeat metrics
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &
curl http://localhost:8080/metrics | grep mc_gc_heartbeat
kill %1

# 2. Check GC service health
kubectl get pods -n dark-tower -l app=global-controller

# 3. Check MC registration status in GC
kubectl exec -it deployment/global-controller -n dark-tower -- \
  psql $DATABASE_URL -c "SELECT id, region, capacity, current_sessions, last_heartbeat, status FROM meeting_controllers ORDER BY last_heartbeat DESC LIMIT 10;"

# 4. Test GC connectivity from MC pod
kubectl exec -it deployment/meeting-controller -n dark-tower -- \
  curl -i http://global-controller.dark-tower.svc.cluster.local:8080/health

# 5. Check MC logs for GC errors
kubectl logs -n dark-tower -l app=meeting-controller --tail=100 | grep -i "gc\|heartbeat\|register"

# 6. Check network policy
kubectl get networkpolicy -n dark-tower
kubectl describe networkpolicy meeting-controller -n dark-tower
```

**Common Root Causes**:

1. **GC Service Down**: GC not running or unhealthy
   - Check: `kubectl get pods -l app=global-controller`
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
kubectl rollout restart deployment/meeting-controller -n dark-tower

# Expected recovery time: 2-3 minutes

# Option 2: Check and restart GC if unhealthy
kubectl get pods -n dark-tower -l app=global-controller
kubectl rollout restart deployment/global-controller -n dark-tower

# Expected recovery time: 2-3 minutes
# Escalate to GC Team before restarting GC

# Option 3: Verify NetworkPolicy
kubectl get networkpolicy meeting-controller -n dark-tower -o yaml
# Ensure egress to global-controller:8080 is allowed

# Option 4: Manual re-registration (if MC has admin API)
# TODO: Implement admin API for re-registration

# Verify recovery
kubectl exec -it deployment/global-controller -n dark-tower -- \
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
kubectl top pods -n dark-tower -l app=meeting-controller

# 2. Check resource limits
kubectl describe deployment meeting-controller -n dark-tower | grep -A 10 "Limits:"

# 3. Check for OOMKilled events
kubectl get events -n dark-tower --field-selector involvedObject.kind=Pod | grep -i "oom\|killed"

# 4. Check memory usage trend in Prometheus
container_memory_working_set_bytes{pod=~"meeting-controller-.*"}
container_spec_memory_limit_bytes{pod=~"meeting-controller-.*"}

# 5. Check CPU usage trend
rate(container_cpu_usage_seconds_total{pod=~"meeting-controller-.*"}[5m])

# 6. Check meeting/connection load
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &
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
kubectl scale deployment/meeting-controller -n dark-tower --replicas=5

# Expected recovery time: 30-60 seconds

# Option 2: Increase resource limits
kubectl patch deployment/meeting-controller -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"meeting-controller","resources":{"limits":{"cpu":"4000m","memory":"2Gi"},"requests":{"cpu":"1000m","memory":"1Gi"}}}]}}}}'

# Expected recovery time: 2-3 minutes (rolling update)

# Option 3: Restart pods (temporary fix for memory issues)
kubectl rollout restart deployment/meeting-controller -n dark-tower

# Expected recovery time: 2-3 minutes

# Option 4: Mark MC as draining (stop new assignments)
kubectl exec -it deployment/global-controller -n dark-tower -- \
  psql $DATABASE_URL -c "UPDATE meeting_controllers SET status = 'draining' WHERE id = '<MC_ID>';"
# This stops new meetings from being assigned while allowing current ones to finish

# Verify recovery
kubectl top pods -n dark-tower -l app=meeting-controller
# CPU should be <70%, memory should be <70%
```

**Escalation**:
- If memory leak suspected, escalate to MC Team for profiling
- If infrastructure resource constraints, escalate to Infrastructure Team
- If load is legitimately high, discuss capacity planning with Product

---

## Diagnostic Commands

### Quick Health Check

```bash
# Check service health
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &
curl http://localhost:8080/health      # Liveness
curl http://localhost:8080/ready       # Readiness
kill %1

# Check pod status
kubectl get pods -n dark-tower -l app=meeting-controller

# Check recent errors in logs
kubectl logs -n dark-tower -l app=meeting-controller --tail=100 | grep -i error
```

### Metrics Analysis

```bash
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &

# Get all metrics
curl http://localhost:8080/metrics

# Actor system metrics
curl http://localhost:8080/metrics | grep mc_actor

# Meeting metrics
curl http://localhost:8080/metrics | grep mc_meetings

# Connection metrics
curl http://localhost:8080/metrics | grep mc_connections

# Message processing metrics
curl http://localhost:8080/metrics | grep mc_message_processing

# GC integration metrics
curl http://localhost:8080/metrics | grep mc_gc

kill %1
```

### Log Analysis

```bash
# Stream logs in real-time
kubectl logs -n dark-tower -l app=meeting-controller -f

# Get logs from all pods
kubectl logs -n dark-tower -l app=meeting-controller --all-containers --tail=200

# Get logs from previous pod instance (after crash)
kubectl logs -n dark-tower <pod-name> --previous

# Search for specific errors
kubectl logs -n dark-tower -l app=meeting-controller --tail=1000 | grep -E "error|panic|fatal"

# Search for actor panics
kubectl logs -n dark-tower -l app=meeting-controller --tail=1000 | grep -A 30 "panic\|PANIC"

# Search for GC integration issues
kubectl logs -n dark-tower -l app=meeting-controller --tail=1000 | grep -i "gc\|heartbeat\|register"

# Search for meeting lifecycle events
kubectl logs -n dark-tower -l app=meeting-controller --tail=1000 | grep -i "meeting\|session\|participant"
```

### Resource Utilization

```bash
# Check CPU and memory usage
kubectl top pods -n dark-tower -l app=meeting-controller

# Check node resources
kubectl top nodes

# Check resource limits
kubectl describe deployment meeting-controller -n dark-tower | grep -A 5 "Limits:"

# Check events for resource issues
kubectl get events -n dark-tower --field-selector involvedObject.name=meeting-controller --sort-by='.lastTimestamp'
```

### Network Debugging

```bash
# Test service connectivity
kubectl run -it --rm debug --image=nicolaka/netshoot --restart=Never -- /bin/bash
# From debug pod:
curl http://meeting-controller.dark-tower.svc.cluster.local:8080/health
nslookup meeting-controller.dark-tower.svc.cluster.local

# Check service endpoints
kubectl get endpoints -n dark-tower meeting-controller

# Check network policies
kubectl get networkpolicies -n dark-tower

# Test GC connectivity
kubectl exec -it deployment/meeting-controller -n dark-tower -- \
  curl -i http://global-controller.dark-tower.svc.cluster.local:8080/health
```

---

## Recovery Procedures

### Service Restart Procedure

**When to use**: Minor issues, stuck state, memory pressure

```bash
# 1. Verify current state
kubectl get pods -n dark-tower -l app=meeting-controller

# 2. Check active meetings (will be affected)
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &
curl http://localhost:8080/metrics | grep mc_meetings_active
kill %1

# 3. Perform rolling restart (zero-downtime if multiple pods)
kubectl rollout restart deployment/meeting-controller -n dark-tower

# 4. Monitor rollout
kubectl rollout status deployment/meeting-controller -n dark-tower

# 5. Verify recovery
kubectl get pods -n dark-tower -l app=meeting-controller
curl http://meeting-controller.dark-tower.svc.cluster.local:8080/ready

# 6. Check logs for startup errors
kubectl logs -n dark-tower -l app=meeting-controller --tail=50
```

**Rollback on failure**:
```bash
kubectl rollout undo deployment/meeting-controller -n dark-tower
```

---

### Graceful Drain Procedure

**When to use**: Planned maintenance, pre-deployment

```bash
# 1. Mark MC as draining in GC
kubectl exec -it deployment/global-controller -n dark-tower -- \
  psql $DATABASE_URL -c "UPDATE meeting_controllers SET status = 'draining' WHERE id = '<MC_ID>';"

# 2. Wait for active meetings to complete (monitor metric)
watch -n 30 'kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 2>/dev/null & sleep 1; curl -s http://localhost:8080/metrics | grep mc_meetings_active; kill %1 2>/dev/null'

# 3. When meetings are zero, proceed with maintenance

# 4. After maintenance, re-enable
kubectl exec -it deployment/global-controller -n dark-tower -- \
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
