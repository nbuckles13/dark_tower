# MC Service Incident Response Runbook

**Service**: Meeting Controller (mc-service)
**Owner**: SRE Team
**On-Call Rotation**: PagerDuty - Dark Tower MC Team
**Last Updated**: 2026-05-01

---

## MC Topology — Read This Before Any `kubectl` Command

MC ships as **two singleton Deployments**, `mc-0` and `mc-1` (each `replicas: 1`).
There is **no `deployment/mc-service`** — `mc-service` is a Service, a PDB and the
container name, so `kubectl ... deployment/mc-service` fails with
`deployments.apps "mc-service" not found`.

- **Every command below names `mc-0`. Repeat it for `mc-1`.** A symptom on one
  instance says nothing about the other; they are independent processes with
  independent state.
- **Health/metrics is port `8081`**, not 8080. Port-forwards here map local 8080
  to remote 8081, so `curl localhost:8080/metrics` is correct once forwarded.
  **Do not "simplify" these to `service/mc-service`.** The `mc-service` ClusterIP
  in `infra/services/mc-service/service.yaml` selects on `app` + `component` and
  deliberately omits `instance:`, so it fans across mc-0 and mc-1 both and would
  scrape a nondeterministic instance — a silent wrong answer for a metrics check,
  rather than an error. `deployment/mc-0` is the only form that names what you
  are measuring.
- **You cannot `curl` MC's health port from another pod, and the failure is a
  TIMEOUT rather than a refusal.** MC's NetworkPolicy
  (`infra/services/mc-service/network-policy.yaml`) admits TCP 8081 from
  **Prometheus only**; GC and MH are admitted on gRPC 50052 and nothing else. So
  `curl http://mc-service.dark-tower.svc.cluster.local:8081/health` from a GC,
  MH or debug pod is silently dropped — and a hang, during an incident, reads as
  "MC is wedged", which is precisely the conclusion a dependency-health check
  exists to rule out. **The sibling services are not symmetric and that is what
  makes this a trap**: AC admits 8082 from gc/mc/mh and GC admits 8080 from
  everywhere, so in a block of three dependency curls the AC and GC lines
  genuinely work and only the MC line cannot. Use the port-forward form below
  from the operator's own machine, per instance. To answer "can GC reach MC at
  all", read `kubectl get endpoints mc-service -n dark-tower` plus GC's own
  registration/heartbeat metrics — MC's health endpoint is not the instrument.
- **Do not add replicas.** `MC_WEBTRANSPORT_ADVERTISE_ADDRESS` comes from a
  per-instance ConfigMap, so extra replicas all advertise the same address to GC
  and clients get routed to a pod that does not hold their session (ADR-0023
  session binding). Capacity is added by adding an *instance*, not a replica.
  The manifests state this at the `replicas: 1` field itself — see the comment
  above `replicas:` in `infra/services/mc-service/mc-0-deployment.yaml`, which
  carries the same arithmetic.
- **`kubectl exec` depends on the image variant.** `infra/docker/mc-service/Dockerfile`
  defines both a shell-free `runtime` stage (distroless `cc-debian12`) and a
  `runtime-with-healthcheck` stage (`:debug` + busybox). The build passes no
  `--target`, so the last stage wins and busybox is present *by stage ordering,
  not by decision*. If a `--target runtime` ever lands, every `exec`-based step
  in this runbook stops working at once — prefer the shell-free alternatives in
  §Diagnostic Commands where they exist.

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
   - [Scenario 14: Elevated Involuntary Departures / Slow Roster Removal](#scenario-14-elevated-involuntary-departures--slow-roster-removal)
   - [Client media signalling — where to look](#client-media-signalling--where-to-look-no-scenario-number-yet) (unnumbered; story task 21 takes Scenarios 15/16)
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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
   - Fix: Investigate specific actor logic, optimize, or raise resource
     limits. MC has no horizontal-scale option (see §MC Topology).

2. **Message Storm**: Burst of messages from clients
   - Check: Connection count spike, message rate spike
   - Fix: Implement rate limiting, or cap admission via MC_MAX_MEETINGS /
     MC_MAX_PARTICIPANTS. MC has no horizontal-scale option (see §MC Topology).

3. **Blocking Operations**: Actor performing blocking I/O
   - Check: Logs for slow operations, trace spans
   - Fix: Make blocking operations async, use dedicated executor

4. **Resource Contention**: CPU/memory pressure
   - Check: `kubectl top pods`
   - Fix: Increase resource limits (`kubectl patch`, below). MC has no horizontal-scale option (see §MC Topology).

5. **GC Integration Slow**: Slow responses from GC
   - Check: GC heartbeat latency
   - Fix: Investigate GC health, see Scenario 6

**Remediation**:

```bash
# Option 1: SHED LOAD or ADD AN INSTANCE -- MC has NO horizontal-scale option.
#           Read the block below BEFORE typing anything: the command an
#           operator reaches for here makes the incident measurably worse.
# WRONG: kubectl scale deployment/mc-0 --replicas=5
# MC DOES NOT SCALE BY REPLICAS -- scaling makes it WORSE, not better, and the
# arithmetic is why. --replicas=5 gives five pods all labelled instance: mc-0:
#   * the mc-service-0 NodePort selects instance: mc-0, so all five become
#     endpoints and arriving QUIC/UDP connections spread across them;
#   * all five read MC_WEBTRANSPORT_ADVERTISE_ADDRESS from the single
#     mc-0-config, so they advertise an IDENTICAL client-facing URL;
#   * but MC_ID and MC_GRPC_ADVERTISE_ADDRESS are generated PER POD.
# So GC registers five distinct MCs advertising one client URL. GC assigns a
# meeting to one; the NodePort then picks an endpoint independently of that
# assignment. That is a 4-in-5 miss -- roughly 80% join failure for every mc-0
# meeting -- and the miss rate RISES with each replica added. An operator
# reaching for `scale` under load gets the exact opposite of what they expect.
# The ADR-0023 session-binding failure is the symptom; the cause is that
# instance identity is per-pod while the advertise address is per-instance.
# Adding MC capacity means adding an INSTANCE (mc-2: its own ConfigMap,
# Deployment, Service and UDP NodePort), not raising a replica count.
# Shed load instead by capping admission -- see MC_MAX_MEETINGS /
# MC_MAX_PARTICIPANTS in docs/runbooks/mc-deployment.md.

# Expected recovery time: NONE from scaling -- it is not an available lever.
# Capping admission (MC_MAX_MEETINGS / MC_MAX_PARTICIPANTS) is a ConfigMap edit
# plus a roll: minutes, and it takes effect only on restart (no content hash --
# see docs/runbooks/mc-deployment.md §Config-failure triage). Adding an mc-2
# instance is a manifest change (ConfigMap + Deployment + Service + Kind port
# mapping) and a deploy -- plan it, do not attempt it mid-incident.

# Option 2: Restart affected pods (clears mailbox but may drop messages)
# CAUTION: This will disconnect active meetings on this pod
kubectl delete pod <MC_POD_NAME> -n dark-tower

# Expected recovery time: 30 seconds
# WARNING: Active meetings on this pod will be affected

# Option 3: Identify and kill problematic meetings (if specific meeting causing issues)
# Requires admin API or database intervention
# Escalate to Service Owner

# Option 4: Increase mailbox capacity (temporary, requires config change)
# NOTE: there is no ACTOR_MAILBOX_SIZE variable -- it is one of the six
# phantoms named in docs/runbooks/mc-deployment.md §Environment Variables.
# Mailbox capacity is a compile-time constant in the actor modules; changing
# it is a code change, not a ConfigMap edit -- CONTROLLER_CHANNEL_BUFFER,
# MEETING_CHANNEL_BUFFER and PARTICIPANT_CHANNEL_BUFFER in crates/mc-service/
# src/actors/. Depth thresholds are in src/actors/metrics.rs.
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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
watch -n 5 'kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 2>/dev/null & sleep 1; curl -s http://localhost:8080/metrics | grep mc_actor_panics_total; kill %1 2>/dev/null'
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
kubectl rollout history deployment/mc-0 -n dark-tower

# If panic started after deployment:
kubectl rollout undo deployment/mc-0 -n dark-tower

# Expected recovery time: 2-3 minutes

# Step 3: If panic is isolated, restart affected pod
kubectl delete pod <MC_POD_NAME> -n dark-tower

# Expected recovery time: 30 seconds

# Step 4: Monitor for recurrence
watch -n 10 'kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 2>/dev/null & sleep 1; curl -s http://localhost:8080/metrics | grep mc_actor_panics_total; kill %1 2>/dev/null'

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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
kubectl describe deployment mc-0 -n dark-tower

# 5. Check resource quotas
kubectl describe resourcequota -n dark-tower

# 6. Check node status
kubectl get nodes
kubectl describe node <node-name>

# 7. Check for recent deployments
kubectl rollout history deployment/mc-0 -n dark-tower
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
kubectl rollout undo deployment/mc-0 -n dark-tower
kubectl rollout status deployment/mc-0 -n dark-tower

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
kubectl patch deployment/mc-0 -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"mc-service","resources":{"limits":{"memory":"2Gi"}}}]}}}}'

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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
kubectl exec -it deployment/mc-0 -n dark-tower -- ping gc-service.dark-tower.svc.cluster.local
```

**Common Root Causes**:

1. **Mailbox Backpressure**: Messages queued due to slow processing
   - Check: Mailbox depth metrics
   - Fix: See Scenario 1

2. **CPU Contention**: High CPU usage causing processing delays
   - Check: `kubectl top pods`
   - Fix: Raise the CPU limit and investigate CPU-intensive operations.
     MC has no horizontal-scale option (see §MC Topology).

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
# Scenario A: CPU Bound (CPU >80%) -- raise the CPU limit; do NOT scale.
kubectl patch deployment/mc-0 -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"mc-service","resources":{"limits":{"cpu":"4000m"},"requests":{"cpu":"1000m"}}}]}}}}'
kubectl patch deployment/mc-1 -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"mc-service","resources":{"limits":{"cpu":"4000m"},"requests":{"cpu":"1000m"}}}]}}}}'
# Expected recovery time: 2-3 minutes (rolling update, one pod at a time).
#
# WRONG: kubectl scale deployment/mc-0 --replicas=5
# MC DOES NOT SCALE BY REPLICAS -- scaling makes it WORSE, not better, and the
# arithmetic is why. --replicas=5 gives five pods all labelled instance: mc-0:
#   * the mc-service-0 NodePort selects instance: mc-0, so all five become
#     endpoints and arriving QUIC/UDP connections spread across them;
#   * all five read MC_WEBTRANSPORT_ADVERTISE_ADDRESS from the single
#     mc-0-config, so they advertise an IDENTICAL client-facing URL;
#   * but MC_ID and MC_GRPC_ADVERTISE_ADDRESS are generated PER POD.
# So GC registers five distinct MCs advertising one client URL. GC assigns a
# meeting to one; the NodePort then picks an endpoint independently of that
# assignment. That is a 4-in-5 miss -- roughly 80% join failure for every mc-0
# meeting -- and the miss rate RISES with each replica added. An operator
# reaching for `scale` under load gets the exact opposite of what they expect.
# The ADR-0023 session-binding failure is the symptom; the cause is that
# instance identity is per-pod while the advertise address is per-instance.
# Adding MC capacity means adding an INSTANCE (mc-2: its own ConfigMap,
# Deployment, Service and UDP NodePort), not raising a replica count.
# Shed load instead by capping admission -- see MC_MAX_MEETINGS /
# MC_MAX_PARTICIPANTS in docs/runbooks/mc-deployment.md.

# Expected recovery time: NONE from scaling -- it is not an available lever.
# Capping admission (MC_MAX_MEETINGS / MC_MAX_PARTICIPANTS) is a ConfigMap edit
# plus a roll: minutes, and it takes effect only on restart (no content hash --
# see docs/runbooks/mc-deployment.md §Config-failure triage). Adding an mc-2
# instance is a manifest change (ConfigMap + Deployment + Service + Kind port
# mapping) and a deploy -- plan it, do not attempt it mid-incident.

# Scenario B: Mailbox Backpressure
# See Scenario 1 remediation

# Scenario C: Memory Pressure
kubectl patch deployment/mc-0 -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"mc-service","resources":{"limits":{"memory":"2Gi"}}}]}}}}'

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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
curl http://localhost:8080/metrics | grep mc_gc_heartbeat
kill %1

# 2. Check GC service health
kubectl get pods -n dark-tower -l app=gc-service

# 3. Check MC registration status in GC
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "SELECT id, region, capacity, current_sessions, last_heartbeat, status FROM meeting_controllers ORDER BY last_heartbeat DESC LIMIT 10;"

# 4. Test GC connectivity from MC pod
kubectl exec -it deployment/mc-0 -n dark-tower -- \
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

6. **MC is REFUSING assignments (not failing to receive them)**: the first five causes are all
   GC-side or network-side, and all present as "assignments are not arriving". This one presents
   identically from MC's symptom list but is MC-side: MC is receiving `AssignMeetingWithMh` and
   answering `accepted: false`. **The pod stays Ready and Kubernetes does not restart it**, so it
   will not look like a pod problem, and GC keeps offering meetings MC keeps refusing.
   - Check, MC side first: **`mc_meeting_assignments_total{status="rejected",
     rejection_reason="unhealthy"}`** — the "Meeting Assignment Outcomes" panel on the MC Overview
     dashboard. GC's `gc_mc_assignments_total{status, rejection_reason}` is the other end of the
     same RPC and uses the same label vocabulary, so read them side by side to confirm GC is seeing
     what MC is emitting.
   - Then **disambiguate with the log**, because `unhealthy` conflates the Redis MH-assignment store
     failure with the `create_meeting` failure and only the log record separates them:
     `kubectl logs -n dark-tower -l app=mc-service --tail=5000 | grep -E "Failed to create meeting actor|Failed to initialise meeting key material"`.
   - `"Failed to initialise meeting key material"` means the system CSPRNG could not produce the
     meeting KEK (ADR-0036 §4). MC fails closed and refuses the meeting rather than starting one
     with absent or weak key material — that behaviour is correct and must not be "fixed".
   - Fix: no in-place remedy. Restart the pod and escalate — a CSPRNG failure points at host entropy
     or the kernel, not at MC. If the log shows a different `create_meeting` failure (e.g. the Redis
     MH-assignment store), triage that instead; `UNHEALTHY` conflates them.

**Remediation**:

```bash
# Option 1: Restart MC to force re-registration
kubectl rollout restart deployment/mc-0 -n dark-tower

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
kubectl describe deployment mc-0 -n dark-tower | grep -A 10 "Limits:"

# 3. Check for OOMKilled events
kubectl get events -n dark-tower --field-selector involvedObject.kind=Pod | grep -i "oom\|killed"

# 4. Check memory usage trend in Prometheus
container_memory_working_set_bytes{pod=~"mc-service-.*"}
container_spec_memory_limit_bytes{pod=~"mc-service-.*"}

# 5. Check CPU usage trend
rate(container_cpu_usage_seconds_total{pod=~"mc-service-.*"}[5m])

# 6. Check meeting/connection load
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
curl http://localhost:8080/metrics | grep -E "mc_meetings_active|mc_connections_active"
kill %1

# 7. Check mailbox depth (held messages consume memory)
curl http://localhost:8080/metrics | grep mc_actor_mailbox_depth
```

**Common Root Causes**:

1. **High Meeting Load**: Too many meetings on one MC
   - Check: mc_meetings_active metric
   - Fix: GC should distribute across mc-0/mc-1 — check GC's assignment, and
     cap admission if both are loaded. MC has no horizontal-scale option (see §MC Topology).

2. **Connection Surge**: Spike in WebTransport connections
   - Check: mc_connections_active metric
   - Fix: Implement rate limiting; cap admission via MC_MAX_PARTICIPANTS.
     MC has no horizontal-scale option (see §MC Topology).

3. **Mailbox Accumulation**: Messages queued in mailboxes
   - Check: mc_actor_mailbox_depth
   - Fix: Address backpressure (see Scenario 1)

4. **Memory Leak**: Memory continuously growing
   - Check: Memory trend over hours (never decreasing)
   - Fix: Restart pods, investigate with profiling

5. **CPU-Intensive Operations**: Heavy message processing
   - Check: Message processing latency by type
   - Fix: Optimize processing; raise resource limits. MC has no horizontal-scale option (see §MC Topology).

**Remediation**:

```bash
# Option 1: SHED LOAD or ADD AN INSTANCE -- MC has NO horizontal-scale option.
#           Read the block below BEFORE typing anything: the command an
#           operator reaches for here makes the incident measurably worse.
# WRONG: kubectl scale deployment/mc-0 --replicas=5
# MC DOES NOT SCALE BY REPLICAS -- scaling makes it WORSE, not better, and the
# arithmetic is why. --replicas=5 gives five pods all labelled instance: mc-0:
#   * the mc-service-0 NodePort selects instance: mc-0, so all five become
#     endpoints and arriving QUIC/UDP connections spread across them;
#   * all five read MC_WEBTRANSPORT_ADVERTISE_ADDRESS from the single
#     mc-0-config, so they advertise an IDENTICAL client-facing URL;
#   * but MC_ID and MC_GRPC_ADVERTISE_ADDRESS are generated PER POD.
# So GC registers five distinct MCs advertising one client URL. GC assigns a
# meeting to one; the NodePort then picks an endpoint independently of that
# assignment. That is a 4-in-5 miss -- roughly 80% join failure for every mc-0
# meeting -- and the miss rate RISES with each replica added. An operator
# reaching for `scale` under load gets the exact opposite of what they expect.
# The ADR-0023 session-binding failure is the symptom; the cause is that
# instance identity is per-pod while the advertise address is per-instance.
# Adding MC capacity means adding an INSTANCE (mc-2: its own ConfigMap,
# Deployment, Service and UDP NodePort), not raising a replica count.
# Shed load instead by capping admission -- see MC_MAX_MEETINGS /
# MC_MAX_PARTICIPANTS in docs/runbooks/mc-deployment.md.

# Expected recovery time: NONE from scaling -- it is not an available lever.
# Capping admission (MC_MAX_MEETINGS / MC_MAX_PARTICIPANTS) is a ConfigMap edit
# plus a roll: minutes, and it takes effect only on restart (no content hash --
# see docs/runbooks/mc-deployment.md §Config-failure triage). Adding an mc-2
# instance is a manifest change (ConfigMap + Deployment + Service + Kind port
# mapping) and a deploy -- plan it, do not attempt it mid-incident.

# Option 2: Increase resource limits
kubectl patch deployment/mc-0 -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"mc-service","resources":{"limits":{"cpu":"4000m","memory":"2Gi"},"requests":{"cpu":"1000m","memory":"1Gi"}}}]}}}}'

# Expected recovery time: 2-3 minutes (rolling update)

# Option 3: Restart pods (temporary fix for memory issues)
kubectl rollout restart deployment/mc-0 -n dark-tower

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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
curl http://localhost:8080/metrics | grep -E "mc_meetings_active|mc_connections_active"
kill %1

# 6. Check MC logs for join errors
kubectl logs -n dark-tower -l app=mc-service --tail=500 | grep -i "join\|JoinRequest\|session"

# 7. Check Redis health (session state depends on Redis)
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
   - Fix: Check GC load balancing across mc-0/mc-1; raise MC_MAX_MEETINGS if
     the cap is genuinely too low. MC has no horizontal-scale option (see §MC Topology).

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

10. **`identity_key_invalid`**: The joiner presented an `identity_public_key` whose length was
    neither 0 nor exactly 32 bytes (ADR-0036 §4). Covers every rejected length as one value
    deliberately — the client-facing message is byte-identical across all of them, so this label is
    the ONLY place the cause is visible.
    - **A client that does NOT send the field cannot produce this label.** Length 0 is the
      contract's NO KEY PUBLISHED state and is **admitted**; that population is counted on
      `mc_join_identity_key_presence_total{presence="absent"}`, a separate series. Do not triage
      this label as a client-rollout gap — if you are looking for "clients that have not implemented
      the key yet", it is on the presence metric and it is not a failure.
    - Check: the client sent *something* and got the **encoding** wrong — PEM, JWK, base64 or a
      DER wrapper rather than the raw 32 bytes — or a genuinely wrong-length key. Correlate the
      onset with a client release that changed key handling. A broad spike is an encoding
      regression; a narrow one from a single source is worth a security look.
    - Fix: roll the offending client build forward or back. There is no MC-side remedy and no
      deploy-ordering lever — see `mc-deployment.md` §Coordination, which records that identity-key
      handling adds no ordering constraint.
    - **Not** remediable MC-side: MC publishes the key to every participant on the roster, so a
      client-controlled blob of arbitrary length must never be admitted.

11. **`sender_id_space_exhausted`**: The meeting consumed all 65535 per-meeting `sender_id`s and MC
    refused the admission rather than wrapping onto a live id (invariant R-35).
    - Check: `kubectl logs -n dark-tower -l app=mc-service --tail=5000 | grep "sender_id space exhausted"`
      — **`--tail` is required**: with a label selector and no `--tail`, kubectl returns only the
      last 10 lines per pod and this grep silently prints nothing. The JSON record carries
      `meeting_id` (a span field under `fmt::layer().json()`, so it is in the record's span object,
      not a top-level `meeting_id=` you can grep for), plus cumulative `admissions_total` and
      `meeting_age_seconds`. An earlier one-shot `"sender_id namespace past high watermark"` record
      for the same meeting tells you whether consumption was gradual or sudden. Neither carries key
      material or a `sender_id` value.
    - **The namespace is consumed by CUMULATIVE LIFETIME ADMISSIONS, not concurrent participants.**
      Ids are never recycled, because reusing one under a live KEK collides two senders on one key
      id — and since the wrap nonce derives from the key id, on one AES-GCM nonce. So
      `MC_MAX_PARTICIPANTS` does **not** bound this: the exposure is a long-lived, high-churn
      meeting, not a large one.
    - Fix: **End the meeting and have participants rejoin a new one.** That is the only remediation,
      and it is unattractive on purpose. There is no in-place lever: the KEK-epoch reset that would
      reclaim the namespace is deferred with all KEK rotation, and a fresh meeting means a fresh
      KEK and a fresh namespace. Do not attempt to "reset" the allocator — reissuing a live
      `sender_id` is the exact collision the refusal exists to prevent.
    - **Escalate if seen at all, and treat it as possibly DRIVEN until you have ruled that out.**
      Two causes reach this state and they need different responses:
      - *Deliberate.* Every join consumes one never-recycled id, MC has **no join rate limit and no
        `jti` replay check** (neither is implemented — see `docs/TODO.md` §Media Path Obligations),
        and the browser reconnect path is unwired so every reconnect is a fresh join. One valid
        meeting token therefore permits unbounded joins until it expires. An authenticated
        participant can burn the namespace **cheaply, quickly and permanently**, and the only
        remedy is to destroy the meeting. The signature is the watermark record arriving *shortly*
        before the exhaustion record — i.e. "sudden" — and a single `sub` accounting for the
        admissions. Check the join rate per token and per source before assuming a bug.
      - *Accidental.* A client reconnect loop creating fresh participants. **The "~18 hours of
        continuous churn at one admission per second" figure describes THIS case only** — it is not
        an estimate for the driven one, which is bounded by connection rate, not by human churn.
      In both cases the meeting is permanently broken; the difference is whether you also need a
      security response.

**Remediation**:

```bash
# Step 1: Identify dominant error_type from dashboard or PromQL
sum by(error_type) (increase(mc_session_join_failures_total[5m]))

# Step 2: Apply targeted fix based on error_type (see root causes above)

# Step 3: If redis errors — check Redis health
kubectl get pods -n dark-tower -l app=redis
kubectl exec -it deployment/redis -n dark-tower -- redis-cli ping
# Expected: PONG

# Step 4: If capacity exceeded — SHED LOAD or ADD AN INSTANCE. MC has no
#         horizontal-scale option; read the block below before acting.
# WRONG: kubectl scale deployment/mc-0 --replicas=5
# MC DOES NOT SCALE BY REPLICAS -- scaling makes it WORSE, not better, and the
# arithmetic is why. --replicas=5 gives five pods all labelled instance: mc-0:
#   * the mc-service-0 NodePort selects instance: mc-0, so all five become
#     endpoints and arriving QUIC/UDP connections spread across them;
#   * all five read MC_WEBTRANSPORT_ADVERTISE_ADDRESS from the single
#     mc-0-config, so they advertise an IDENTICAL client-facing URL;
#   * but MC_ID and MC_GRPC_ADVERTISE_ADDRESS are generated PER POD.
# So GC registers five distinct MCs advertising one client URL. GC assigns a
# meeting to one; the NodePort then picks an endpoint independently of that
# assignment. That is a 4-in-5 miss -- roughly 80% join failure for every mc-0
# meeting -- and the miss rate RISES with each replica added. An operator
# reaching for `scale` under load gets the exact opposite of what they expect.
# The ADR-0023 session-binding failure is the symptom; the cause is that
# instance identity is per-pod while the advertise address is per-instance.
# Adding MC capacity means adding an INSTANCE (mc-2: its own ConfigMap,
# Deployment, Service and UDP NodePort), not raising a replica count.
# Shed load instead by capping admission -- see MC_MAX_MEETINGS /
# MC_MAX_PARTICIPANTS in docs/runbooks/mc-deployment.md.

# Expected recovery time: NONE from scaling -- it is not an available lever.
# Capping admission (MC_MAX_MEETINGS / MC_MAX_PARTICIPANTS) is a ConfigMap edit
# plus a roll: minutes, and it takes effect only on restart (no content hash --
# see docs/runbooks/mc-deployment.md §Config-failure triage). Adding an mc-2
# instance is a manifest change (ConfigMap + Deployment + Service + Kind port
# mapping) and a deploy -- plan it, do not attempt it mid-incident.

# Step 5: If internal errors persist — restart as last resort
# See Recovery Procedures: #service-restart-procedure
# WARNING: Active meetings on restarted pods will be affected

# Verify recovery
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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

# 4. Check TLS certificate validity.
#    Read it from the SECRET, not from inside the pod: the mc-service runtime
#    image is distroless and carries no `openssl` (and, per §MC Topology, may
#    lose its shell entirely if a `--target runtime` build ever lands), so the
#    exec form fails for a reason unrelated to the certificate.
kubectl get secret mc-service-tls -n dark-tower \
  -o jsonpath='{.data.tls\.crt}' | base64 -d | \
  openssl x509 -noout -dates -subject
# Verify: notAfter is in the future
#
# The in-pod path, if you have a shell and want to confirm what the pod ACTUALLY
# mounted rather than what the Secret holds: the mc-tls volume mounts at
# /etc/mc-tls (mode 0400), so the file is /etc/mc-tls/tls.crt -- NOT /certs/,
# which this step named for years and which has never existed. MC_TLS_CERT_PATH
# in mc-service-config is the authoritative answer:
#   kubectl set env deployment/mc-0 --list -n dark-tower | grep MC_TLS

# 5. Check MC logs for TLS/QUIC errors
kubectl logs -n dark-tower -l app=mc-service --tail=500 | grep -iE "tls|quic|certificate|handshake|reject"

# 6. Check UDP port connectivity (WebTransport uses QUIC/UDP)
kubectl get svc -n dark-tower mc-service -o yaml | grep -A5 "port:"
# Verify UDP port 4433 is exposed

# 7. Check network policies for UDP traffic
kubectl get networkpolicy -n dark-tower
kubectl describe networkpolicy mc-service -n dark-tower

# 8. Check MC capacity (rejections may be due to connection limits)
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
   - Fix: Raise the connection cap (MC_MAX_PARTICIPANTS) or add an mc-2
     instance. MC has no horizontal-scale option (see §MC Topology).

5. **TLS Certificate Mismatch**: Client connecting with wrong SNI or hostname
   - Check: MC logs for TLS handshake errors mentioning SNI
   - Fix: Verify DNS resolution and client connection URL

6. **Transport-Level Establishment Errors** (`status="error"`): connection-SETUP failures
   only — malformed QUIC handshake packets, accept/decode failures during session
   establishment. As of task #64 this label is establishment-scoped: mid-session
   transport loss (network interruptions after join) NO LONGER lands here — it now
   surfaces on `mc_participant_disconnects_total{cause="connection_lost"}` (see Scenario 14).
   - Check: `mc_webtransport_connections_total{status="error"}` rate for establishment errors
   - For mid-session drops (participants disappearing after joining), do NOT rely on
     `{status="error"}` — check `mc_participant_disconnects_total{cause="connection_lost"}`
     per Scenario 14 instead.
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

# Option 3: If capacity exceeded — SHED LOAD or ADD AN INSTANCE. MC has no
#           horizontal-scale option; read the block below before acting.
# WRONG: kubectl scale deployment/mc-0 --replicas=5
# MC DOES NOT SCALE BY REPLICAS -- scaling makes it WORSE, not better, and the
# arithmetic is why. --replicas=5 gives five pods all labelled instance: mc-0:
#   * the mc-service-0 NodePort selects instance: mc-0, so all five become
#     endpoints and arriving QUIC/UDP connections spread across them;
#   * all five read MC_WEBTRANSPORT_ADVERTISE_ADDRESS from the single
#     mc-0-config, so they advertise an IDENTICAL client-facing URL;
#   * but MC_ID and MC_GRPC_ADVERTISE_ADDRESS are generated PER POD.
# So GC registers five distinct MCs advertising one client URL. GC assigns a
# meeting to one; the NodePort then picks an endpoint independently of that
# assignment. That is a 4-in-5 miss -- roughly 80% join failure for every mc-0
# meeting -- and the miss rate RISES with each replica added. An operator
# reaching for `scale` under load gets the exact opposite of what they expect.
# The ADR-0023 session-binding failure is the symptom; the cause is that
# instance identity is per-pod while the advertise address is per-instance.
# Adding MC capacity means adding an INSTANCE (mc-2: its own ConfigMap,
# Deployment, Service and UDP NodePort), not raising a replica count.
# Shed load instead by capping admission -- see MC_MAX_MEETINGS /
# MC_MAX_PARTICIPANTS in docs/runbooks/mc-deployment.md.

# Expected recovery time: NONE from scaling -- it is not an available lever.
# Capping admission (MC_MAX_MEETINGS / MC_MAX_PARTICIPANTS) is a ConfigMap edit
# plus a roll: minutes, and it takes effect only on restart (no content hash --
# see docs/runbooks/mc-deployment.md §Config-failure triage). Adding an mc-2
# instance is a manifest change (ConfigMap + Deployment + Service + Kind port
# mapping) and a deploy -- plan it, do not attempt it mid-incident.

# Option 4: Restart MC (if QUIC listener crashed)
# See Recovery Procedures: #service-restart-procedure

# Verify recovery
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
kubectl exec -it deployment/mc-0 -n dark-tower -- \
  curl -s http://ac-service.dark-tower.svc.cluster.local:8080/.well-known/jwks.json | head -c 500
# Verify: returns JSON with "keys" array containing at least one key

# 5. Check JWKS endpoint returns expected key IDs
kubectl exec -it deployment/mc-0 -n dark-tower -- \
  curl -s http://ac-service.dark-tower.svc.cluster.local:8080/.well-known/jwks.json | grep '"kid"'
# Expected format: kid values like "auth-prod-2026-01"
# During key rotation: should see BOTH old and new kid values (overlap period)

# 6. Check clock skew between MC and AC pods
kubectl exec -it deployment/mc-0 -n dark-tower -- date -u
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
kubectl exec -it deployment/mc-0 -n dark-tower -- \
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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
**Severity**: page
**Runbook Section**: `#scenario-11-media-connection-failures`

**Symptoms**:
- `MCMediaConnectionAllFailed` is firing: **>80% of client-reported per-MH media connections are in the `failed` state over a 5-minute window** at meaningful volume (fleet aggregate). This is a *silent broken call* — the affected clients reached MC signaling successfully (they had to, in order to report the status), but the client→MH **media plane** is not working.
- The `failed` share of `mc_participant_mh_status_total{state=~"connected|failed"}` is elevated. Clients report per-MH outcomes for their assigned Media Handlers via the `MediaConnectionUpdate` signaling message; a healthy population reports mostly `connected`.
- Affected participants have signaling (MC) connectivity but no working media path — they appear in the roster but send/receive no audio/video.

**Impact**: Affected participants cannot send or receive media. **MC takes no automatic remediation** — the metric is observability-only (R-60); the connection is between the client and MH, and MC neither brokers nor repairs it. Signaling/join continue to succeed, which is exactly why this fails *silently* on the paths MC's other alerts watch.

**Treat `mh_url`, `failure_reason`, and `failure_code` as untrusted client input.** These are self-reported by the browser over the `MediaConnectionUpdate` message; MC truncates each to `MAX_CLIENT_STRING_BYTES` (256, on a UTF-8 boundary) before store/log, but their *content* is not authenticated as truthful (the message is authenticated as "from this session", the field values are not). Always corroborate against MH-side metrics/logs before concluding a specific MH is at fault.

> **Distinguish genuine failure from abuse/amplification first.** `mc_participant_mh_status_dropped_total{reason}` counts statuses MC refused to record: `reason="over_limit"` (a single `MediaConnectionUpdate` carried more than `MAX_MH_STATUSES_PER_UPDATE`=64 statuses — 4× a legitimate client's assigned-MH count) and `reason="cap"` (a participant tried to exceed `MAX_MH_STATUSES_PER_PARTICIPANT`=16 distinct MH entries). A spike in `dropped_total` alongside the `failed` share points at a **misbehaving/abusive client**, not an MH outage — triage that as a client/security issue, not a media-plane incident.

> **Note**: `mc_participant_mh_status_total` starts at zero in production. The `state="failed"` series first appears the first time any client reports a per-MH failure; a brand-new time series with no historical baseline is not itself an incident — the **failed-share ratio** (below) is the actionable signal, and `MCMediaConnectionAllFailed` only pages at >80% share sustained for 5m at meaningful volume (mirrors the GC Sc 5 first-emission pattern).

> **Limitation — what this alert does NOT catch.** `MCMediaConnectionAllFailed` fires on the **fleet-aggregate** failed share, not per participant (the metric has no participant/`mh_url` label — bounded cardinality + PII). It deliberately does **not** reproduce the deleted per-client `all_failed` boolean. Consequence: an **isolated single-participant total media failure** is diluted in the fleet aggregate and will NOT move the ratio or page — that is now a client-side-telemetry / support-ticket signal, not a server-alerted incident. The high 0.80 threshold is exactly what filters the benign partial-failure noise the old `all_failed=false` label used to carry: a client that loses 1 of 2 MHs and falls back contributes only ~33-50% share, structurally below the paging line. The tradeoff for bounded cardinality is single-user insensitivity — document it here so on-call is not misled into expecting a page for a lone "no media" report.

**Diagnosis**:

```bash
# 1. Confirm the alert's own condition — failed share over the last 5m (fleet).
#    This is the query MCMediaConnectionAllFailed pages on (>0.80).
kubectl port-forward -n dark-tower deployment/mc-0 8081:8081 &
curl -s http://localhost:8081/metrics | grep mc_participant_mh_status_total
kill %1
```
```promql
# Failed share — byte-identical to the MCMediaConnectionAllFailed alert expr
# (infra/docker/prometheus/rules/mc-alerts.yaml). Pages when ratio > 0.80 AND
# there is meaningful volume. The second line is the volume floor (>0.1/s over
# connected+failed) that stops a divide-by-near-zero from paging on a trickle —
# it is why a brand-new low-traffic `failed` series does not page.
(
  sum(rate(mc_participant_mh_status_total{state="failed"}[5m]))
  /
  sum(rate(mc_participant_mh_status_total{state=~"connected|failed"}[5m]))
) > 0.80
and
sum(rate(mc_participant_mh_status_total{state=~"connected|failed"}[5m])) > 0.1

# 2. Rule out abuse/amplification before chasing MH — is dropped_total spiking?
sum by(reason) (increase(mc_participant_mh_status_dropped_total[5m]))

# 3. Scope: compare against active meetings/connections — a high share against
#    a handful of participants is different from a fleet-wide media outage.
sum(mc_meetings_active)
sum(mc_connections_active)

# 4. CORROBORATE on the MH side (do NOT trust failure_reason alone).
#    MH WebTransport handshake health + JWT validation health:
sum by(status) (rate(mh_webtransport_connections_total[5m]))
histogram_quantile(0.95, sum by(le) (rate(mh_webtransport_handshake_duration_seconds_bucket[5m])))
sum by(failure_reason) (rate(mh_jwt_validations_total{result="failure"}[5m]))
```
```bash
# 5. MC logs for the MediaConnectionUpdate handler (target mc.webtransport.connection)
kubectl logs -n dark-tower -l app=mc-service --tail=500 \
  | grep -iE "MediaConnectionUpdate|mh_status|over_limit"

# 6. MH pod health (per-pod failures correlate with the client reports)
kubectl get pods -n dark-tower -l app=mh-service
```

**Common Root Causes**:

The client reports a media failure; the cause is almost always on the **client→MH media path**, upstream of MC. MC signaling is healthy by definition here (the report arrived). Triage by corroborating evidence.

> **Strong-signal short-circuit**: if the elevated `failed` share correlates with elevated `mh_register_meeting_timeouts_total` (MH Sc 13) AND/OR `mc_register_meeting_total{status="error"}` (MC Sc 12) in the same window, treat the RegisterMeeting coordination break as the upstream root cause — clients reach MH, get JWT-validated, then provisional-kicked because the meeting was never registered, which surfaces to clients as failed media. Fix the coordination and the failed share decays. Skip to root cause #3.

1. **MH WebTransport rejections fleet-wide** — clients reach MH but are rejected during handshake. Check `mh_webtransport_connections_total{status="rejected"}`. See [MH Sc 5: WebTransport Rejections](mh-incident-response.md#scenario-5-webtransport-rejections).
2. **MH JWT validation failures** — valid JWTs at MC, rejected at MH (JWKS skew, key rotation). See [MH Sc 2: JWT Validation Failures](mh-incident-response.md#scenario-2-jwt-validation-failures).
3. **MH RegisterMeeting timeouts** — clients arriving before MC registered the meeting; MH provisional-kicks them. See [MH Sc 13](mh-incident-response.md#scenario-13-registermeeting-timeout--clients-kicked) and [MC Sc 12](#scenario-12-registermeeting-coordination-failures).
4. **MH down or unreachable** — full MH outage or NetworkPolicy regression between client and MH (UDP/4434). Check MH pod health per MH Sc 1.
5. **TLS / certificate issues at MH** — clients fail the QUIC handshake. **Do not conclude this from `failure_reason="tls"` alone** — verify with `openssl x509` against the actual MH cert and MH-side handshake metrics.
6. **Network path / cloud firewall** — UDP egress from client → MH blocked. A diffuse pattern across many `mh_url` values from many clients points here.
7. **Capacity exhaustion at MH** — `mh_active_connections` at cap, new clients rejected. See MH Sc 5 + Sc 8.
8. **Misbehaving/abusive client** — if `mc_participant_mh_status_dropped_total{reason="over_limit"|"cap"}` is the dominant signal, this is not an MH incident; treat as a client/security issue (self-reported amplification), not a media-plane outage.

**Remediation**:

```bash
# Step 1: Identify the dominant upstream cause from the corroborating MH metrics
#   (Diagnosis steps 2-6). The MC-side runbook does not "fix" this scenario —
#   remediation is on the MH media path the clients are reporting against.
# Step 2: MH WebTransport rejections / capacity -> MH Sc 5 (scale MH / rotate TLS).
# Step 3: MH RegisterMeeting timeouts -> MC Sc 12 (fix MC->MH RegisterMeeting delivery).
# Step 4: MH down -> MH Sc 1 (restore MH service).
# Step 5: dropped_total-dominated -> client/security path, not MH remediation.

# Step 6: Monitor for resolution — the failed share is a leading indicator of
#         user impact. Recovery = the ratio decaying back below the paging line
#         and MCMediaConnectionAllFailed clearing.
kubectl port-forward -n dark-tower deployment/mc-0 8081:8081 &
watch -n 30 'curl -s http://localhost:8081/metrics | grep mc_participant_mh_status_total'
kill %1
```

Expected recovery time: bounded by the upstream MH/network fix; once resolved, in-flight failure reports stop within 30-60s and the failed share decays to baseline over the next 5m window. New clients establish media within their normal connect timeout (~5s).

**Escalation**:
- If MH is healthy by every MH-side metric and the failed share persists >80%, escalate to Infrastructure Team — likely a client↔MH network path issue MH itself cannot observe.
- If `MCMediaConnectionAllFailed` stays firing for >15 minutes, treat as a P1 media outage — affected participants have no usable media.
- If client reports point at TLS/cert issues but the MH cert is verifiably valid, escalate to Client Team (possible client trust-store misconfiguration).
- If `mc_participant_mh_status_dropped_total` dominates, escalate to Security Team (client-side amplification / abuse), not Infrastructure.

**Dashboards**: MC Overview → "Client-Reported MH Status by State" panel (the `state` breakdown driving this alert); MH Overview → WebTransport handshake-status panel + JWT-validation panel for corroborating evidence.

**Related Alerts**: MH-side `MHHighWebTransportRejections` / `MHWebTransportHandshakeSlow` / `MHHighJwtValidationFailures` (the MH-path causes this scenario corroborates against); MC-side [Scenario 12: RegisterMeeting Coordination Failures](#scenario-12-registermeeting-coordination-failures) (the common upstream root cause when clients are kicked before media establishes).

---

### Scenario 12: RegisterMeeting Coordination Failures

**Alert**: No alert today; surfaces in `mc_register_meeting_total{status="error"}` rate, `mc_register_meeting_duration_seconds` p95, and `RegisterMeeting retries exhausted` error logs at target `mc.register_meeting.trigger`. A second, distinct string — `RegisterMeeting failed terminally; not retried` — is a **different fault with the opposite remedy**; see Symptoms. May also co-fire MH-side [Scenario 13: RegisterMeeting Timeout — Clients Kicked](mh-incident-response.md#scenario-13-registermeeting-timeout--clients-kicked).
**Severity**: warning
**Runbook Section**: `#scenario-12-registermeeting-coordination-failures`

<!-- TODO: alert MCRegisterMeetingFailureRate — no alert rule exists today; this scenario is metric-driven triage. When observability adds a rule, link it here and remove this TODO. -->

> **Note**: `mc_register_meeting_total{status="error"}` and `mc_register_meeting_duration_seconds` start at zero in production and only emit on the first first-participant join. A brand-new series is not itself an incident; `rate(...{status="error"}[5m]) > 0` against a non-trivial total rate is the actionable signal.

**Symptoms**:
- `mc_register_meeting_total{status="error"}` non-zero or rising vs baseline. MC retries each MH up to 3 attempts with 1s/2s backoffs (see `register_meeting_with_handlers` in `crates/mc-service/src/webtransport/connection.rs`); a steady error rate means retries are being exhausted.
- `mc_register_meeting_duration_seconds` p95 climbing — RegisterMeeting RPCs are succeeding but slowly, eating into the MH-side 15s timeout budget.
- MC error log: `"RegisterMeeting retries exhausted"` (target `mc.register_meeting.trigger`) with `mh_grpc_endpoint`, `attempts_made` and the underlying error. This is the **retryable** class — flaky MC→MH coordination; the symptoms and remedies in this scenario are written for it.
- MC error log: `"RegisterMeeting failed terminally; not retried"` (same target, `terminal = true`; `attempts_made` is whichever attempt hit the terminal outcome, so it is **not** always 1 — a transport error on attempt 1 followed by a terminal divergence on attempt 2 reports 2. `terminal = true` is the discriminator, never the attempt count) — **a DIFFERENT fault with the OPPOSITE remedy; do not treat it as this scenario.** MC deliberately does not retry it, because a `transport_mode_mismatch` is a genuine two-ends version skew between MC and MH and backoff cannot make a skewed handler agree. **Roll MH FORWARD; do NOT roll MC back** — rolling MC back returns it to `policy_generation: 0` registrations, which MH installs nothing for, i.e. the pre-change media blackhole rather than a fix. See `mc-deployment.md` §"Post-Deploy Monitoring Checklist: MC↔MH Coordination" → Rollback (MC half). The two strings are deliberately distinct so a `grep` for one cannot silently match the other.
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
kubectl exec -it deployment/mc-0 -n dark-tower -- \
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
kubectl rollout undo deployment/mc-0 -n dark-tower
# (Or revert the NetworkPolicy change — coordinate with Infrastructure.)

# Step 4: If MH JWKS rejection — escalate AC Team (see MH Sc 2 + MC Sc 10);
#          MC service token may need rotation if auth_rejected dominates.

# Step 5: If MC mailbox backpressure — see Scenario 1 remediation. Do NOT
#         scale: MC has no horizontal-scale option (block below).
# WRONG: kubectl scale deployment/mc-0 --replicas=5
# MC DOES NOT SCALE BY REPLICAS -- scaling makes it WORSE, not better, and the
# arithmetic is why. --replicas=5 gives five pods all labelled instance: mc-0:
#   * the mc-service-0 NodePort selects instance: mc-0, so all five become
#     endpoints and arriving QUIC/UDP connections spread across them;
#   * all five read MC_WEBTRANSPORT_ADVERTISE_ADDRESS from the single
#     mc-0-config, so they advertise an IDENTICAL client-facing URL;
#   * but MC_ID and MC_GRPC_ADVERTISE_ADDRESS are generated PER POD.
# So GC registers five distinct MCs advertising one client URL. GC assigns a
# meeting to one; the NodePort then picks an endpoint independently of that
# assignment. That is a 4-in-5 miss -- roughly 80% join failure for every mc-0
# meeting -- and the miss rate RISES with each replica added. An operator
# reaching for `scale` under load gets the exact opposite of what they expect.
# The ADR-0023 session-binding failure is the symptom; the cause is that
# instance identity is per-pod while the advertise address is per-instance.
# Adding MC capacity means adding an INSTANCE (mc-2: its own ConfigMap,
# Deployment, Service and UDP NodePort), not raising a replica count.
# Shed load instead by capping admission -- see MC_MAX_MEETINGS /
# MC_MAX_PARTICIPANTS in docs/runbooks/mc-deployment.md.

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

**Related Alerts**: MH-side `mh_register_meeting_timeouts_total` (downstream effect on receiving MH), `MCHighMailboxDepthWarning` (upstream cause when trigger task is queued).

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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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

### Scenario 14: Elevated Involuntary Departures / Slow Roster Removal

**Symptom**: Users report the roster is slow to drop a participant who left, OR
`mc_participant_leaves_total{reason="timeout"}` is an unusually high share of all leaves.

**Background — expected roster-remove latency (task #64).** The MC removes a participant
from the roster on one of two paths:

- **Clean tab-close** (WebTransport `Connection::closed()` → `ApplicationClosed`/`ConnectionClosed`):
  removed IMMEDIATELY, grace skipped → `ParticipantLeft{Voluntary}`. Expected latency ≈
  network RTT (sub-second).
- **Crash / network-loss** (only the QUIC idle timeout can detect it): the participant
  enters the ADR-0023 grace period (for reconnection) and is removed only when grace
  expires → `ParticipantLeft{Timeout}`.

**Worst-case roster-remove latency is DERIVED from config (single source of truth):**

```
worst_case_crash_removal =
    MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS      (idle-timeout detection; default 10s)
  + MC_DISCONNECT_GRACE_PERIOD_SECONDS    (ADR-0023 reconnection grace; default 30s)
  + 5s                                    (grace-check tick interval, code constant)
```

With defaults: **10 + 30 + 5 = 45 seconds**. If either env var is overridden, recompute
from the pod's actual values. Read them **without a shell** — `kubectl exec` needs
busybox, which is unavailable in exactly the `CreateContainerConfigError` case where you
most want these values:
```bash
kubectl set env deployment/mc-0 --list -n dark-tower | grep -E 'MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS|MC_DISCONNECT_GRACE_PERIOD_SECONDS'
# or, straight from the pod spec:
kubectl get pod -n dark-tower -l instance=mc-0 -o jsonpath='{.spec.containers[0].env}'
```
`MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS` is operator-overridable (fail-loud: an invalid or `0`
value crashes the pod at startup rather than silently reverting to a library default);
the grace period is a code constant today.

Why 10s idle: it is a DETECTION timeout, not an eject timer. A server keep-alive interval
(idle/3 ≈ 3.3s) keeps a healthy-but-quiet connection alive, so a live participant is never
spuriously ejected; the 30s grace then absorbs genuine transient blips (reconnection).

**Triage**:

```promql
# Involuntary-departure share. Gate on a volume floor so a tiny all-crash meeting
# doesn't read as a 100% incident.
(
  sum(rate(mc_participant_leaves_total{reason="timeout"}[5m])) /
  sum(rate(mc_participant_leaves_total[5m]))
) > 0.5
and
sum(rate(mc_participant_leaves_total[5m])) > 0.1
```

```promql
# Disconnect cause breakdown — the oncall discriminator:
sum(rate(mc_participant_disconnects_total[5m])) by (cause)
```

- `connection_lost` rising while `leaves{timeout}` stays flat → clients are dropping and
  RECONNECTING within grace (transient network churn — usually healthy; check client
  network / LB idle timeouts).
- `connection_lost` AND `leaves{timeout}` rising together → genuine involuntary departures
  (crashes / hard network loss). Investigate client stability, pod health, and whether
  `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS` is misconfigured too LOW (would eject healthy quiet
  sessions — cross-check that `keep_alive` < idle in the `"QUIC idle timeout + keep-alive
  configured"` startup log).
- `client_closed` dominates → normal (clean tab-closes); no action.

**If roster removal is slower than the derived worst case**: the idle timeout may not be
taking effect. Confirm the startup log line `QUIC idle timeout + keep-alive configured`
shows the expected `idle_timeout_secs`; if absent, the pod is running an older image
without task #64.

**Alert**: No formal alert this iteration — thresholds pending a production baseline (owned
by Observability/Operations follow-up). Surfaces via the PromQL above and the MC Overview
departure panels.

**Escalation**: MC Team for a genuine involuntary-departure spike; Operations if a
misconfigured `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS` is suspected.

---

### Client media signalling — where to look (no scenario number yet)

Deliberately **unnumbered**. Story task 21 owns numbered Scenarios 15/16 for the
ADR-0036 media path and will key its triage off these same tokens; this section
exists so the counters shipped in task 14 are reachable from this runbook in the
meantime, rather than only from the metric catalog and the Grafana panel — both
of which are found by someone who *already* suspects the media path.

**Read this first: how much of it is live today.** MC composes and sends the
`SendDirective` now, but the browser SDK does **not yet honour it** (story task
19). Until task 19 lands, every failure below is **invisible to users** and none
of it is pageable — treat this as diagnosis for a reported media problem, not as
a signal to act on proactively. **When task 19 lands, that inverts**: a client
never told to send simply produces nothing — no error, no join failure, no
absent-frame signal — and these counters become the only evidence. See the alert
obligation filed in `docs/TODO.md` §Media Path Obligations.

**The symptom that leads here**: "I can't hear anyone" / one-way audio / a
participant who joined successfully and is silent, with `mc_session_joins_total`
showing success and no WebTransport rejection.

**Five questions, in the order worth asking them.** Each has a primary counter;
three further metrics appear below as companions or escalation pointers, so the
five headings are **not** a complete list of the metrics named here. **Scope
boundary**: this section covers the client-facing signalling path only. The
MC->MH control plane (`mc_media_policy_pushes_total`,
`mc_media_generation_divergence`) is cited below only to route you, and is
triaged in Scenarios 12 and 13, not here. All primary counters are on the standard MC metrics
endpoint (`deployment/mc-0`, port 8081 — see §MC Topology), and none carries any
meeting, participant or stream identity.

```bash
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
curl -s http://localhost:8080/metrics | grep -E 'mc_media_(receive_capability_declarations|send_directives|slot_states|unmatched_plan_slots|mute_requests)_total|mc_participant_outbound_messages_dropped_total'
kill %1
# Repeat for mc-1 -- a symptom on one instance says nothing about the other.
```

**1. `mc_media_receive_capability_declarations_total{outcome}` — is the client
asking correctly?** Note the failure predicate carefully:

```promql
# Rejections. NOT outcome!="accepted" -- `accepted_unchanged` is a SUCCESS
# (an identical re-declaration; MC does no work and sends nothing).
sum by (outcome) (rate(mc_media_receive_capability_declarations_total{outcome!~"accepted|accepted_unchanged"}[5m]))
```

Every rejection token except one names a **client** defect, so a rise is a client
fleet problem and points at a client release, not an MC deploy:
`duplicate_slot_id`, `slot_count_over_cap` (the client asked for more slots than
`MC_MAX_RECEIVE_SLOTS`), `slot_id_out_of_range`, `pinned_sender_id_zero`,
`pinned_sender_id_out_of_range`, `declaration_budget_exhausted` (the client blew
`MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS` on one connection — a re-declaration
loop). `media_kind_unspecified` most often means a **version-skewed** client, not
one that forgot a field: read a rise as client-fleet skew first.

The exception is **`slot_id_not_planned`**, which is **not** a client defect. The
declaration is well-formed and MC cannot serve it, because the join-time
forwarding-policy push fixes the egress slot id before the client can declare.
The remedy is the capability-triggered re-push (a later story), not a client
change. **Do not escalate this one to the client team.**

Do **not** build a ratio whose denominator includes `accepted_unchanged`: it is
client-inflatable at near-zero server cost, so any such ratio is evadable. Use
`accepted` alone.

**2. `mc_media_send_directives_total{outcome}` — is MC telling clients to send?**
This is the one that answers "why is this participant silent". Failure predicate:

```promql
# NOT outcome!="emitted" -- `emitted_empty_targets` is a specified success
# per ADR-0036 §5 and becomes routine once selective forwarding lands.
sum by (outcome) (rate(mc_media_send_directives_total{outcome!~"emitted|emitted_empty_targets"}[5m]))
```

| Token | What it means | Where to go |
|---|---|---|
| `no_planned_egress_slot` | **The severe one.** Failed at join, so it silences the connection for its *whole life*, not one declaration. MC computed a forwarding assignment fine; that assignment simply contains no egress plan naming this subscriber. | MC↔MH coordination — Scenario 12 (`RegisterMeeting`) and `mc_media_policy_pushes_total`. The client is not at fault and reconnecting will not help. |
| `meeting_state_unavailable` | MC could not get the meeting actor handle. | Actor health — Scenario 1 (mailbox depth) and Scenario 2 (actor panics). |
| `assignment_failed` | MC could not compute a forwarding assignment at all — distinct from `no_planned_egress_slot`, where the computation succeeded. | Scenario 12; check MH registration and `mc_media_generation_divergence`. |
| `handler_url_unresolved` | No MH WebTransport endpoint resolved for the assigned handler. | MH health and MC's view of it — Scenario 11 / Scenario 13. |
| `unknown_stream_number`, `transport_mode_unspecified` | **MC-internal defects**, fail-closed. Not operator-actionable. | File against `meeting-controller`; these are diagnostics, not pageable. |

Log coverage is **uneven across these values, and the counter is the complete
record — the logs are not.** Stated precisely, because filtering on a token the
line does not carry returns nothing and reads like "it never happened":

- **Join-time failures** (`no_planned_egress_slot` and the other
  context-resolution values) log a **WARN carrying `outcome`**, once at join,
  never per message. This is the only case where filtering on the token works.
- **Per-composition `build_send_directive` failures** (`unknown_stream_number`,
  `transport_mode_unspecified`, `handler_url_unresolved`) log at **ERROR** and do
  carry `outcome`. All three return through the same `Err(outcome)` arm, so the
  list is the full set of values `build_send_directive` itself produces — do not
  read the two MC-defect values as the whole of it.
- **`meeting_state_unavailable`** logs a WARN with **no `outcome` field**, and
  **`assignment_failed`** logs at ERROR with no `outcome` field (one of its two
  sites carries `reason`, which is the assignment error's own label, not this
  token).

So `level=warn AND outcome=assignment_failed` matches nothing by construction.
Search the message text, or read the counter:

```bash
kubectl logs deployment/mc-0 -n dark-tower --tail=500 | grep -i "media signalling\|send directive"
```

A connection whose context failed to build stays otherwise **healthy** — the
participant remains joined, the roster is correct, mute and every other post-join
message keep working. That is deliberate graceful degradation, and it is also why
the failure has no other symptom.

**3. `mc_media_slot_states_total{slot_state}` — are the slots MC filled actually
carrying anything?** Note the label is **`slot_state`**, not `outcome` — this is
the one of the three that is not an outcome vocabulary. Its domain mirrors the
wire `SlotState` enum **exhaustively (all eight variants, not the four reachable
today)**, which is what makes MC's distribution directly comparable with the
client's.

```promql
sum by (slot_state) (rate(mc_media_slot_states_total[5m]))
```

`active` is the healthy value. `source_muted` is the sender's own mute and is
normal. `fewer_sources_than_slots` means the client declared more slots than
there are sources — normal in a small meeting, not a fault. `source_unreachable`
is the one worth chasing: the source exists and MC cannot reach it. A non-zero
`unspecified` is an **MC defect** — that value should never reach the wire —
and is visible here precisely because the vocabulary was not pruned to the
reachable subset.

`mc_media_unmatched_plan_slots_total` counts the inverse: MC planning into a slot
the client never declared. It carries **no** `slot_state`/`outcome` label (only
`key_custody`, so cardinality 1), and it is incremented **by the slot count**,
not by 1 per composition — so the series is a *slot* rate, not a *composition*
rate, and it shares no denominator with the counters above. Do not ratio it
against them. A sustained non-zero rate means MC's
egress plan and the client's declared layout disagree — the same underlying
mismatch as `slot_id_not_planned` seen from the other side. Do not transpose the
two: *plan's slot was not declared* here, *client's slot has no plan* there.

**4. `mc_media_mute_requests_total{outcome}` — is client mute landing, and is one
connection driving meeting-wide work?** Note the failure predicate is stated
**positively**, unlike the two above:

```promql
# POSITIVE form on purpose. Do NOT "harmonise" this to outcome!~"..." to match
# its neighbours: stated positively, a variant added later defaults to
# not-a-failure instead of silently joining the failure set.
sum by (outcome) (rate(mc_media_mute_requests_total{outcome=~"rate_limited|actor_unavailable"}[5m]))
```

| Token | Meaning |
|---|---|
| `applied` | Reported to the meeting actor and recomposition **attempted** (not necessarily succeeded — see the mute-path gap in `docs/TODO.md`). |
| `applied_no_recompose` | Reported, nothing to re-convey — no declaration yet, or a **video-only** change. **Expect this to be the largest bucket** once clients wire camera buttons: it is what an ordinary camera button produces, and it is healthy. |
| `unchanged` | Identical to the report already in force; MC correctly did nothing. **Not a denominator — see below.** |
| `rate_limited` | The per-connection mute-work token bucket is exhausted (burst 8, sustained 4/s — `MUTE_WORK_BURST` / `MUTE_WORK_REFILL_INTERVAL_MS` in `webtransport/connection.rs`). |
| `actor_unavailable` | Environmental and terminal. Pairs with `meeting_state_unavailable` above — check actor health, Scenarios 1 and 2. |

**`unchanged` must NOT appear in a ratio denominator** — same rule, same reason,
as `accepted_unchanged` on the declarations counter. The no-op short-circuit
fires *before* any actor hop, roster read or recomposition, so a client repeating
one state drives `unchanged` at line rate for the cost of a tuple compare and a
counter increment. "What fraction of mute reports are being applied", computed
over it, is **a number one participant can drive to zero**. The denominator is
`applied` + `applied_no_recompose`.

**`unchanged` is exempt from the rate limiter on purpose — do not "fix" the
ordering.** Reversing it would let a client spamming a steady state drain its own
bucket on messages that do no work, suppressing its next *genuine* toggle, while
the metric reported `rate_limited` for an expensive path that was never
approached. Same family as the positive-predicate note above: an asymmetry that
looks like an inconsistency and is load-bearing.

**`rate_limited` is not by itself an incident.** A human never reaches the bound —
8 toggles of burst is more than any push-to-talk flurry, and 4/s sustained is
orders of magnitude above human rates. A sustained non-zero rate means **one
connection** is toggling far above human rates: read it as a client-side repeat
loop or reactive-state bug first, an abusive peer second. Either way the blast
radius is that one connection.

**The dropped report is safe, and this is why**: ADR-0036 §5 enforces client mute
**at capture, on the client**, so when MC drops a report the audio genuinely
stopped — only the indicator other participants see is stale, and it self-corrects
on that client's next toggle. Do not escalate a `rate_limited` rate as an
audio-leak risk; it is not one.

**Why the bound exists at all, and what it protects**: this is the only
repeatable client-driven path that puts work on the **shared meeting actor's
mailbox** — one `GetState` roster snapshot per composition — plus a
per-connection roster read, assignment computation and outbound message.
Unbounded, one client toggling in a loop spends all of that at line rate.

**This is NOT a meeting-wide latency pointer, and an earlier revision of this
section said it was.** The bound was originally sized against
`handle_self_mute`'s O(N) awaited `broadcast_update`, which did head-of-line-block
joins, leaves and every other connection's state read. That fan-out was removed
(`MuteChanged` has no consumer, so it delivered zero bytes to zero clients), and
the meeting-wide blast radius went with it. The remaining cost is dominated by
the one connection driving it. **Do not spend an incident here looking for the
cause of meeting-wide latency** — that symptom's pointers are Scenario 12
(MC↔MH coordination) and the actor mailbox-depth panels, not this counter.

**Caveat that also applies to `mc_media_slot_states_total` above**: both counters
are **per-composition**, and compositions are partly client-triggered, so a raw
fleet-wide ratio over either is skewable by a single participant. Any SLO built on
them needs per-connection normalisation. The skew is bounded by the same limiter.

**5. `mc_participant_outbound_messages_dropped_total{payload_kind}` — did the
client actually RECEIVE what MC decided to send?** Not a media-path metric (no
`key_custody`, generic outbound choke point), and listed here anyway because it
is **the completeness caveat on counter 2**.

```promql
sum by (payload_kind) (rate(mc_participant_outbound_messages_dropped_total[5m]))
# payload_kind ∈ {signaling_raw, participant_update}
```

**`emitted` does not mean the client was told.** `record_send_directive` fires
`emitted` **before** the message is handed to the participant actor, so two
failure modes sit *after* the counter and neither moves it:

- **Mailbox FULL** — `try_send` fails, the drop is counted **here** under
  `payload_kind="signaling_raw"`. This is the only queryable evidence.
- **Mailbox CLOSED** (actor gone) — a WARN in `webtransport::connection`, no
  counter. Deliberate: the FULL case is already covered here, and a WARN is
  proportionate to actor-gone.

So the honest reading of a silent participant is: `accepted` incrementing,
`emitted` incrementing, nothing disagreeing — **and a non-zero
`signaling_raw` drop rate is the one signal that contradicts them.** Check it
before concluding from counter 2 that the client was directed.

**The WARN beside it is one-shot per connection; the counter is not.** Every drop
is counted, only the first is logged, because a wedged outbound channel produces
one drop per roster broadcast — O(participants x events) identical lines from a
single bad connection, on a path a client can drive. **So log-line volume
understates this badly**: one line can stand for thousands of drops, and the
counter is the complete record of repeat occurrences.

**Escalation**: `meeting-controller` for `unknown_stream_number` /
`transport_mode_unspecified` and any `slot_id_not_planned` rate;
`media-handler` for `no_planned_egress_slot`, `assignment_failed` and
`handler_url_unresolved`; `client` for the capability rejection tokens and for a sustained `rate_limited`
(a repeat loop in the SDK's mute path).

---

### MH media sessions declining — MC-side disambiguation (cross-pointer)

**The procedure lives in MH's runbook**, not here:
[`mh-incident-response.md` §Scenario 15: Media Sessions Declining — No Sender Binding](mh-incident-response.md#scenario-15-media-sessions-declining--no-sender-binding).
Thresholds and queries are owned in one place to avoid silent divergence — the same
discipline as the MC↔MH post-deploy checklist. **Do not duplicate them here.**

What lives on this side is the **disambiguator**. When MH reports
`mh_media_session_starts_total{outcome="declined_no_sender_binding"}`, MC answered `0`
("I cannot resolve this participant") and only MC's counter says why:

```promql
mc_media_sender_binding_responses_total
```

| `outcome` | Meaning | Remedy |
|---|---|---|
| *(series absent or flat)* | **Version skew** — this MC image predates the `sender_id` field | Redeploy MC. See the rollback **Carve-out #2** in `mc-deployment.md`: do NOT roll MC back on a decline spike. |
| `participant_unknown` | Transient join race **if a small, decaying fraction**; a **systematic identity mismatch** if sustained near 100% | Check the ratio, not the presence — see below |
| `meeting_unknown` | Routing/lifecycle fault; MC has no such meeting | Not self-clearing; investigate meeting placement |
| `registry_full` | Per-meeting connection cap refused the registration, so MC correctly answered `0` rather than handing back an ordinal for a connection it is not tracking | **Capacity.** Never self-clearing — do not triage as a join race |
| `user_ambiguous` | **One user, two participants** — the user joined from two devices, so one token `sub` maps to two roster entries and MC will not guess which | **No operator remedy.** Never self-clearing; the fix is a contract change (`docs/TODO.md`). Field action: have the user leave on one device. **Not an MC defect** — MC is correctly refusing to guess |
| `resolved` | MC answered an ordinal | Should not co-occur with an MH decline; if it does, suspect skew between the pods you are querying |

> **`participant_unknown` is not automatically a join race.** Sustained at ~100% of
> attempts it means MC and MH do not agree on participant identity at all — MH names the
> participant by its token `sub`, and if MC keys its roster by a different value it can
> never resolve *any* participant. A race is a small fraction and decays; a mismatch is
> flat and total. **The ratio is the discriminator, not the label.**

**`declined_no_sender_binding` on MH is the UNION of all four unresolved MC outcomes**, and
MH structurally cannot split them — it observes only `sender_id == 0`. That is why this
series is not redundant with MH's: all four have different remedies, from "clears itself"
(`participant_unknown` race) through "add capacity" (`registry_full`) to "no operator
remedy exists" (`user_ambiguous`).

**Do not page MC on every MH decline.** Three of MH's **six** decline outcomes
(`declined_sender_binding_conflict`, `declined_mc_endpoint_unknown`,
`declined_mc_auth_rejected`) have their first move inside MH, and one of those names MC
in the label without implying MC is unwell. Scenario 15 Step 1 partitions on which
service to open before the label is read.

---

## Diagnostic Commands

### Quick Health Check

```bash
# Check service health
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &

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
kubectl describe deployment mc-0 -n dark-tower | grep -A 5 "Limits:"

# Check events for resource issues
kubectl get events -n dark-tower --field-selector involvedObject.name=mc-service --sort-by='.lastTimestamp'
```

### Network Debugging

```bash
# Test service connectivity
kubectl run -it --rm debug --image=nicolaka/netshoot --restart=Never -- /bin/bash
# From debug pod:
# NOTE: do NOT curl MC's health port from this pod. Nothing listens on 8080,
# and 8081 is admitted from Prometheus only -- the drop is a TIMEOUT, which
# reads as "MC is wedged". See the netpol bullet in §MC Topology.
nslookup mc-service.dark-tower.svc.cluster.local
# Reachability that actually answers the question, from the operator's machine:
#   kubectl get endpoints mc-service -n dark-tower
#   kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
#   curl -i http://localhost:8080/health   # repeat for mc-1

# Check service endpoints
kubectl get endpoints -n dark-tower mc-service

# Check network policies
kubectl get networkpolicies -n dark-tower

# Test GC connectivity
kubectl exec -it deployment/mc-0 -n dark-tower -- \
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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
curl http://localhost:8080/metrics | grep mc_meetings_active
kill %1

# 3. Perform rolling restart (zero-downtime if multiple pods)
kubectl rollout restart deployment/mc-0 -n dark-tower

# 4. Monitor rollout
kubectl rollout status deployment/mc-0 -n dark-tower

# 5. Verify recovery
kubectl get pods -n dark-tower -l app=mc-service
# Health is 8081 and is NOT reachable cross-pod (netpol admits Prometheus only)
# -- port-forward from here rather than curling the ClusterIP. See §MC Topology.
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
curl -i http://localhost:8080/ready
kill %1
# Repeat for mc-1 -- one instance ready says nothing about the other.

# 6. Check logs for startup errors
kubectl logs -n dark-tower -l app=mc-service --tail=50
```

**Rollback on failure**:
```bash
kubectl rollout undo deployment/mc-0 -n dark-tower
```

---

### Graceful Drain Procedure

**When to use**: Planned maintenance, pre-deployment

```bash
# 1. Mark MC as draining in GC
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "UPDATE meeting_controllers SET status = 'draining' WHERE id = '<MC_ID>';"

# 2. Wait for active meetings to complete (monitor metric)
watch -n 30 'kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 2>/dev/null & sleep 1; curl -s http://localhost:8080/metrics | grep mc_meetings_active; kill %1 2>/dev/null'

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
- 2026-07-07: Reintroduce Sc 11 (Media Connection Failures) in browser-client-join Task #6, rebuilt atop `mc_participant_mh_status_total{state}` + the `MCMediaConnectionAllFailed` page alert (fires at >0.80 failed-share for 5m). Detection/diagnosis rewritten from the old `all_failed` boolean to the failed-share ratio; adds the `mc_participant_mh_status_dropped_total{reason}` abuse-vs-failure triage. Completes the 2026-05-03 reintroduction commitment.
- 2026-05-03: Remove Sc 11 + `MCMediaConnectionAllFailed` alert + dashboard panel id 45 in browser-client-join Task #2 (proto `MediaConnectionFailed` + `mc_media_connection_failures_total` deleted via R-60 redesign). Task #6 reintroduces all four atop `mc_participant_mh_status_total{state}`. Tracked in `docs/TODO.md`.
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
