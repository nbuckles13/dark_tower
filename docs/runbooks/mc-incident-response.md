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
   - [Client media signalling — where to look](#client-media-signalling--where-to-look-no-scenario-number-yet) (unnumbered)
   - [Scenario 15: Media Generation Divergence](#scenario-15-media-generation-divergence)
   - [Scenario 16: Missing Key Material](#scenario-16-missing-key-material)
   - [Heap and Core Dumps Contain Live Meeting KEKs](#heap-and-core-dumps-contain-live-meeting-keks) (unnumbered; read **before** taking any memory capture)
   - [Scenario 17: KEK Rotation Storm / Flapping Participant](#scenario-17-kek-rotation-storm--flapping-participant)
   - [Scenario 18: A Participant Hears Only Part of the Roster](#scenario-18-a-participant-hears-only-part-of-the-roster)
   - [Scenario 19: KEK Rotation Stalled](#scenario-19-kek-rotation-stalled)
   - [Scenario 20: Meeting Teardown Failing / MH Budget Ratchet](#scenario-20-meeting-teardown-failing--mh-budget-ratchet)
   - [Scenario 21: Server Mute Not Enforced](#scenario-21-server-mute-not-enforced)
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
# Capping admission (MC_MAX_MEETINGS / MC_MAX_PARTICIPANTS) is a config.env edit
# plus an apply of the environment root: minutes. The ConfigMap is
# content-addressed, so the apply itself rolls both MC pods (see
# docs/runbooks/mc-deployment.md §Config-failure triage). Adding an mc-2
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
```

> **STOP before you collect a core dump or a memory capture for this panic.** A panicking MC can
> write a core automatically — `RLIMIT_CORE` is unlimited in the container and `core_pattern` is a
> host-global setting our manifests do not control — and that file contains **the live meeting KEKs
> of every meeting on the pod**, plus JWTs in flight and participant display names. Read
> [§Heap and Core Dumps Contain Live Meeting KEKs](#heap-and-core-dumps-contain-live-meeting-keks)
> first: it states what the artifact is, how it must be handled, and the KEK-rotation step that is
> required afterwards and is the one people omit. A stack trace from the logs is sufficient for most
> panics and handles no key material.

```bash
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
   - Check: `kubectl describe pod -n dark-tower -l app=mc-service` (`CreateContainerConfigError` names the missing ConfigMap). ConfigMap names are content-hash-suffixed (`mc-service-config-<hash>`, ADR-0038), so `kubectl get configmap -n dark-tower -l app=mc-service` lists every generation.
   - Fix: re-apply the environment root, which re-creates the exact generation the pod template references

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
# ConfigMap names are content-hash-suffixed; resolve the one mc-0 references:
kubectl get configmap -n dark-tower \
  "$(kubectl get deployment/mc-0 -n dark-tower \
     -o jsonpath='{.spec.template.spec.containers[?(@.name=="mc-service")].env[?(@.name=="GC_GRPC_URL")].valueFrom.configMapKeyRef.name}')"
# Secret missing: recreate from secure backup. ConfigMap missing: re-apply the
# environment root, which re-creates it from infra/services/mc-service/*.env.

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
# Capping admission (MC_MAX_MEETINGS / MC_MAX_PARTICIPANTS) is a config.env edit
# plus an apply of the environment root: minutes. The ConfigMap is
# content-addressed, so the apply itself rolls both MC pods (see
# docs/runbooks/mc-deployment.md §Config-failure triage). Adding an mc-2
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
# Capping admission (MC_MAX_MEETINGS / MC_MAX_PARTICIPANTS) is a config.env edit
# plus an apply of the environment root: minutes. The ConfigMap is
# content-addressed, so the apply itself rolls both MC pods (see
# docs/runbooks/mc-deployment.md §Config-failure triage). Adding an mc-2
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

> **STOP before you take a heap dump to chase a suspected memory leak.** An MC heap dump contains
> **the live meeting KEKs of every meeting on that pod**, and taking one converts key material that
> ADR-0036 §4 guarantees is never persisted into a durable artifact that decrypts those meetings
> indefinitely. Read
> [§Heap and Core Dumps Contain Live Meeting KEKs](#heap-and-core-dumps-contain-live-meeting-keks)
> before proceeding — including the remediation step (KEK rotation, or ending the affected meetings)
> that is required once a dump exists. `kubectl top`, the container memory series above, and
> `mc_*` gauge trends answer most capacity questions and handle no key material.

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

11. **`sender_id_space_exhausted` — RETIRED; never emitted.** Sender-id exhaustion no longer fails
    a join. When a meeting's `sender_id` namespace runs out, MC performs an immediate KEK-epoch
    reset: a new KEK, and a fresh namespace that excludes every id still bound (live and
    grace-period roster members, plus ids a media handler may still hold). **The join succeeds and
    the meeting repairs itself. Do NOT end the meeting.** The value is absent from the join-failure
    vocabulary (`JOIN_FAILURE_ERROR_TYPES`, `crates/mc-service/src/observability/metrics.rs`); if
    you see it on a dashboard, it is history from a pre-R-16 image.

    > **Corrected 2026-09-30 (story 2 task 17, R-29; supersedes the interim marker task 9 placed
    > here on 2026-09-26).** This root cause previously said exhaustion **refused** the admission,
    > that the meeting was **permanently broken**, that ids were **never recycled**, and that the
    > only fix was to **end the meeting and have participants rejoin**, paged by
    > `MCSenderIdSpaceExhausted`. After R-16 every one of those is false. A responder who trusted
    > the old text would have **ended a meeting that was already repairing itself**, destroying every
    > participant's session, and would have waited for an alert that is retired and can never fire.

    - **Is this what happened?** Start with the counter, not the log — it is the fail-closed answer:
      `increase(mc_meeting_kek_generated_total{trigger="sender_space_exhausted"}[15m])`. The info
      rule `MCKekEpochResetOnSenderIdExhaustion` (`infra/docker/prometheus/rules/mc-alerts.yaml`)
      fires when this expression is `> 0`; its triage home is
      [Scenario 17](#scenario-17-kek-rotation-storm--flapping-participant), not this scenario.
    - **Which meeting?** `kubectl logs -n dark-tower -l app=mc-service --tail=5000 | grep "sender_id namespace"`
      — **`--tail` is required**: with a label selector and no `--tail`, kubectl returns only the
      last 10 lines per pod and this grep silently prints nothing. The stem matches the reset record
      (`sender_id namespace exhausted; KEK epoch reset and reissue`), the one-shot-per-epoch
      `sender_id namespace past high watermark` WARN that precedes each reset (the closest thing to a
      pre-event signal), and the two residual refusal records below. `meeting_id` is a span field
      under `fmt::layer().json()`, so it is in the record's span object, not a top-level
      `meeting_id=` you can grep for. No record carries key material or a `sender_id` value. The
      emit sites (and the reason the stem must stay contiguous) are in
      `crates/mc-service/src/actors/meeting.rs`.
    - **How close is the next reset?** `mc_meeting_sender_ids_issued_max` — consumption **since the
      last epoch reset**, max across live meetings. Do not compute headroom over a meeting's whole
      life, and do not restate the namespace size here: it is defined by the allocator,
      `crates/mc-service/src/media_admission/sender_id.rs`. `MC_MAX_PARTICIPANTS` does **not**
      bound consumption — ids are consumed by admissions, not concurrent participants — so the
      exposure is a long-lived, high-churn meeting, not a large one.
    - **Why reissuing an id is safe now, and was not before.** Reusing an id under a **live** KEK
      collides two senders on one key id — and since the wrap nonce derives from the key id, on one
      AES-GCM nonce. The reset installs a new KEK in the same atomic step, so a reissued id is never
      under the KEK its previous holder used. Ids are never reissued **within** one KEK generation.
      The atomicity argument lives in `AdmissionEpoch::admit()`,
      `crates/mc-service/src/media_admission/epoch.rs`.
    - **The residual join failure, and where it lands.** Two arms still refuse the joiner,
      fail-closed, with the key state and namespace unchanged. Both surface as
      **`error_type="internal"`** (root cause 9), so if root cause 9 is climbing, run the grep above
      before reading it as a generic internal error. **The two arms have different remedies; the
      ERROR message stem tells them apart** (emit sites: `crates/mc-service/src/actors/meeting.rs`):
      - `sender_id namespace exhausted and the KEK rotation that must accompany an epoch reset
        failed; refusing admission` — split further on the `reason` label of
        `mc_meeting_kek_rotation_failures_total` (vocabulary and permanence: `KekRotationFailed`,
        `crates/mc-service/src/media_admission/kek.rs`):
        - `reason="rng"` — the system CSPRNG failed. Transient in principle: nothing changed, so
          the next join retries the reset. **Do not end the meeting**; escalate to
          `meeting-controller`.
        - `reason="generation_exhausted"` — the KEK generation counter is at its ceiling and never
          wraps. **Permanent for this meeting actor**: every join that reaches namespace exhaustion
          is refused for the rest of the meeting. **Ending the meeting and moving people to a new
          one IS the remedy, and nothing else clears it** (effectively unreachable; `kek.rs` says
          why). Same remedy as [Scenario 19](#scenario-19-kek-rotation-stalled), which pages on
          the stalled rotation.
      - `sender_id namespace exhausted and a fresh epoch had nothing allocatable after excluding the
        bound ids; refusing admission` — **permanent for this meeting's NEW joiners; existing
        participants are unaffected.** This is NOT bounded by the participant cap and is not
        near-unreachable: the exclusion set includes MC's record of ids a media handler may still
        hold, which only grows when MH releases are lost, so operational churn over a long meeting
        (MH crashes and restarts, lost notifications) can reach it. The argument is on
        `AdmitFailed::NoAllocatableId`, `crates/mc-service/src/media_admission/epoch.rs`; the
        missing reconciliation is tracked in `docs/TODO.md` §Media Path Obligations. **Moving the
        people who still need to join into a new meeting is the only lever** — do not tell them to
        keep retrying. Escalate to `meeting-controller`.
    - **Treat a reset as possibly DRIVEN until you have ruled that out.** Two causes reach it and they
      need different responses:
      - *Deliberate.* Every join consumes one id, MC has **no join rate limit and no `jti` replay
        check** (neither is implemented — see `docs/TODO.md` §Media Path Obligations), and the
        browser reconnect path is unwired so every reconnect is a fresh join. One valid meeting
        token therefore permits unbounded joins until it expires, so an authenticated participant
        can burn the namespace cheaply and quickly. The signature is the watermark record arriving
        *shortly* before the reset record — i.e. "sudden" — and a single `sub` accounting for the
        admissions. Check the join rate per token and per source before assuming a bug.
      - *Accidental.* A client reconnect loop creating fresh participants.

      What changed with R-16 is the **consequence**, not the threat: a self-repairing rotation storm
      rather than a permanently broken meeting. A driven cause still needs a security response; the
      meeting still does not need ending. Rotation-storm triage is
      [Scenario 17](#scenario-17-kek-rotation-storm--flapping-participant); if one sender went
      inaudible to incumbents only after a reset, read its stale-SDK-bundle paragraph first.

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
# Capping admission (MC_MAX_MEETINGS / MC_MAX_PARTICIPANTS) is a config.env edit
# plus an apply of the environment root: minutes. The ConfigMap is
# content-addressed, so the apply itself rolls both MC pods (see
# docs/runbooks/mc-deployment.md §Config-failure triage). Adding an mc-2
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
# Capping admission (MC_MAX_MEETINGS / MC_MAX_PARTICIPANTS) is a config.env edit
# plus an apply of the environment root: minutes. The ConfigMap is
# content-addressed, so the apply itself rolls both MC pods (see
# docs/runbooks/mc-deployment.md §Config-failure triage). Adding an mc-2
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
- `mc_register_meeting_total{status="error"}` non-zero or rising vs baseline. MC retries each MH up to 3 attempts with 1s/2s backoffs (see the per-(meeting, handler) push worker in `crates/mc-service/src/media_routing/pusher.rs`); a steady error rate means retries are being exhausted. Since story 2 a failed push stays unconfirmed and is re-sent by the meeting's next structural change of any kind.
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

**Alert**: No alert today; diagnostic signal in `mc_mh_notifications_unapplied_total{reason}` (one bounded reason per notification that did not become participant connectivity) beside `mc_mh_notifications_received_total{event_type}`.
**Severity**: info (operational drift) / page (if security branch — see Common Root Causes)

> **Why this matters more since story 2 task 20**: MH's `NotifyParticipantConnected` / `Disconnected` are now the **source of truth for who hears whom** (ADR-0036 §9): a participant is routed only through handlers MH reports it connected to, recorded in the meeting actor (the former `MhConnectionRegistry` and its 1000-connection cap are gone). A notification that does not become connectivity can therefore mean silence, not just drift.
>
> **Note on metric asymmetry**: MC counts notifications that REACHED it; delivery failures are counted on the MH sender side as `mh_mc_notifications_total{status="error"}` — see [MH Scenario 10: MH→MC Notification Failures](mh-incident-response.md#scenario-10-mhmc-notification-failures). This scenario is the complement: notifications that reached MC but reference state MC does not expect.

**Symptoms** — read `mc_mh_notifications_unapplied_total` PER REASON, never as one rate:
- `unknown_connection` — **routine**: every connection MC declined to bind sends a Disconnected for a key MC never recorded. Only sustained volume far above the join rate is a signal.
- `meeting_unknown` — notifications naming a meeting this MC does not hold (both events).
- `handler_not_in_set` — the MH-asserted `handler_id` is not byte-identical to a handler of the meeting's frozen set. **Sustained**, it means an MH process restarted under a new (per-incarnation) id and story-4 re-registration has not happened: that meeting's connectivity to the dead id is stale and **will not clear on its own** — see Scenario 18 arm (a).
- `connection_bound_refused` — **expected-empty**: one participant exceeded the per-handler live-connection bound (a reconnect storm or leaked sessions).
- `retired_connection` — a Connected delivered after its own Disconnected (MH abandoned the RPC); MC's tombstone refused it. Non-zero means MH→MC RPCs are slow enough to be abandoned — check MH Scenario 10.
- `mc_mh_notifications_without_connection_id_total` non-zero — an MH image predating `connection_id` is still in the fleet: that MH gets the degraded legacy semantics (a stale disconnect can remove a live connection).

**Impact**: operational drift in the unknown-meeting shapes; **silence** in the `handler_not_in_set` and `connection_bound_refused` shapes (a participant MC believes is not on a handler gets no edges there). The security reading is unchanged: MC authorizes MH on scope alone and every MH pod shares one service identity, so a compromised MH can assert connections for handlers of a meeting's own set (blast radius: misrouting inside that meeting's registered handlers — never cross-meeting; `docs/TODO.md` §Media Path Obligations).

**Diagnosis**:

```bash
# 1. Every way a notification did not become connectivity, per reason
sum by (reason) (rate(mc_mh_notifications_unapplied_total[5m]))

# 2. Volume against expectation: ~1 connect per (participant x handler) per session
sum by(event_type) (rate(mc_mh_notifications_received_total[5m]))
sum(mc_connections_active)

# 3. Is an old MH still sending without connection_id?
sum(rate(mc_mh_notifications_without_connection_id_total[5m]))

# 4. SHAPE the signal — diffuse vs concentrated. Per-notification INFO lines at
#    mc.grpc.media_coordination carry meeting_id, participant_id, handler_id,
#    connection_id and the disposition (connectivity=recorded|<reason>).
kubectl logs -n dark-tower -l app=mc-service --tail=2000 \
  | grep -E "Participant (connected to|disconnected from) MH"
# Look for: one handler_id repeatedly (a restarted MH), or scattered meetings from one
# MH source identity (misbehaviour).

# 5. Cross-check with GC's view of meeting assignments (operational-drift hypothesis)
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c \
  "SELECT meeting_id, mc_id, status FROM meetings WHERE updated_at > NOW() - INTERVAL '1 hour' ORDER BY updated_at DESC LIMIT 50;"
```

**Common Root Causes**:

1. **Diffuse, many-MH, many-meeting `meeting_unknown` → Operational drift.** GC reassigned meetings but an old MH notified briefly; or MC restarted and lost its meeting actors, so in-flight notifications name meetings the new instance never held. Self-heals as participants reconnect (a fresh join re-dials every MH).
2. **Concentrated, single-MH source identity, many unknown meetings → Authenticated-MH misbehavior.** Compromised MH credentials probing the MediaCoordinationService, or a misconfigured MH notifying the wrong MC. **Treat as a security incident**: preserve logs, snapshot the metrics, do **NOT** restart MC, escalate to Security Team.
3. **Sustained `handler_not_in_set` on one meeting → an MH restarted.** Its new process registered under a new id outside the meeting's frozen set. Pre-existing gap (story 4 handler-restart detection); see Scenario 18 arm (a) for the remedy participants have today.
4. **`connection_bound_refused` → reconnect storm / leaked sessions from one participant.** Self-scoped (only that participant is affected). Correlate with the client's reconnect behaviour.

**Remediation**:

```bash
# Operational-drift branch: no action — monitor.
# Authenticated-misbehavior branch:
kubectl logs -n dark-tower -l app=mc-service --tail=5000 \
  > /tmp/mc-incident-$(date -u +%Y%m%dT%H%M%SZ).log
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
curl -s http://localhost:8080/metrics | grep -E "mc_mh_notifications_(received|unapplied)_total" \
  > /tmp/mc-metrics-$(date -u +%Y%m%dT%H%M%SZ).txt
kill %1
# Do NOT restart MC. Do NOT rotate the MH service token (yet) — Security needs evidence.
# Escalate to Security Team via #security-incidents.
```

Expected recovery time: branch-dependent. Operational drift self-heals over 5-15m. Authenticated-misbehavior: bounded by Security's investigation (do not auto-recover). `handler_not_in_set`: persists until affected participants rejoin (story 4 closes it).

**What this scenario tells you**: a diagnostic-appendix scenario. Use it when you are already investigating a different signal and `mc_mh_notifications_unapplied_total` does not fit the surrounding incident. If you found this scenario via an alert, the alert is wrong — file an issue.

**Escalation**:
- Authenticated-misbehavior branch: Security Team immediately.
- Operational-drift branch: no escalation; track for trend.
- `connection_bound_refused` sustained, or any `handler_not_in_set` without a known MH restart: MC Team.

**Related Alerts**: `MCActorPanic`, MH-side `MHCallerTypeRejected`; MH-side [Scenario 10: MH→MC Notification Failures](mh-incident-response.md#scenario-10-mhmc-notification-failures) (the sender-side view of the same RPC pair).

**Dashboards**: MC Media Path → Media Connectivity row ("MH Notifications Unapplied by Reason", "Notifications Without Connection Id"), and MC Overview → MH Coordination row ("MH Notifications by Event"). The two are separate dashboards since story 2 task 12; each links to the other.

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

**Read this first: this path is live and user-visible.** MC composes and sends
the `SendDirective`, and the browser SDK honours it — `AudioPipeline` publishes
to `directive.targets[0]` (story task 19, landed). A client never told to send
simply produces nothing: no error, no join failure, no WebTransport rejection,
no absent-frame signal. **These counters are the only evidence**, and **no alert
rule keys on them yet** (owed at story task 21; obligation filed in
`docs/TODO.md` §Media Path Obligations). So the absence of a page is not the
absence of the failure — reach for this section on a *reported* symptom rather
than waiting for one to fire.

**The symptom that leads here**: "I can't hear anyone" / one-way audio / a
participant who joined successfully and is silent, with `mc_session_joins_total`
showing success and no WebTransport rejection.

**Questions, in the order worth asking them.** Each has a primary counter;
further metrics appear below as companions or escalation pointers, so the
headings are **not** a complete list of the metrics named here. **Scope
boundary**: this section covers the client-facing signalling path only. The
MC->MH control plane (`mc_media_policy_pushes_total`,
`mc_media_generation_divergence`) is cited below only to route you, and is
triaged in Scenarios 12 and 13, not here. All primary counters are on the standard MC metrics
endpoint (`deployment/mc-0`, port 8081 — see §MC Topology), and none carries any
meeting, participant or stream identity.

```bash
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
curl -s http://localhost:8080/metrics | grep -E 'mc_media_(receive_capability_declarations|send_directives|slot_states|slot_view_emissions|unreachable_senders|mute_requests|server_mute_requests|unmute_requests)_total|mc_media_receive_slot_cap|mc_participant_outbound_messages_dropped_total'
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

Every rejection token names a **client** defect, so a rise is a client
fleet problem and points at a client release, not an MC deploy:
`duplicate_slot_id`, `slot_count_over_cap` (the client asked for more slots than
`MC_MAX_RECEIVE_SLOTS` — compare its configured N with the `mc_media_receive_slot_cap`
gauge; a cap below legitimate clients' N is a configuration mismatch. A current SDK refuses an
over-cap N LOCALLY — it reads the cap off `JoinResponse.max_receive_slots` — and counts it on
`dt_client_media_receive_slots_rejected_total`, so an MC-side `slot_count_over_cap` means an SDK
predating that field or a client ignoring it; for "hears nobody" with this rate flat, read the
client counter and `docs/runbooks/client-dev-local.md` F18), `slot_id_out_of_range`, `pinned_sender_id_zero`,
`pinned_sender_id_out_of_range`, `declaration_budget_exhausted` (the client blew
`MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS` on one connection — a re-declaration
loop). `media_kind_unspecified` most often means a **version-skewed** client, not
one that forgot a field: read a rise as client-fleet skew first.

(`slot_id_not_planned` is retired: since story 2 declared audio slots are the
assignment's input, so no well-formed declaration can miss a plan.)

Do **not** build a ratio whose denominator includes `accepted_unchanged`: it is
client-inflatable at near-zero server cost, so any such ratio is evadable. Use
`accepted` alone.

**2. `mc_media_send_directives_total{outcome}` — is MC telling clients to send?**
This is the one that answers "why is this participant silent". Failure predicate:

```promql
# NOT outcome!="emitted" -- `emitted_empty_targets` is a specified success
# per ADR-0036 §5, and ROUTINE since story 2: anyone nobody holds in a slot
# (a solo participant included) is directed to send nothing.
sum by (outcome) (rate(mc_media_send_directives_total{outcome!~"emitted|emitted_empty_targets"}[5m]))
```

| Token | What it means | Where to go |
|---|---|---|
| `meeting_state_unavailable` | MC could not get the meeting actor handle. | Actor health — Scenario 1 (mailbox depth) and Scenario 2 (actor panics). |
| `assignment_failed` | MC could not render the meeting's slot table into a forwarding assignment. | File against `meeting-controller`; also read the `Forwarding assignment could not be rendered` ERROR at `mc.register_meeting.trigger`. |
| `handler_url_unresolved` | **MC-internal defect since story 2** — every url comes from one frozen handler set, so a miss cannot happen by construction. Fails closed: nothing is sent to that participant. | File against `meeting-controller`. |
| `unknown_stream_number`, `transport_mode_unspecified` | **MC-internal defects**, fail-closed. Not operator-actionable. | File against `meeting-controller`; these are diagnostics, not pageable. |

**Where the logs are.** Since story 2 the MEETING ACTOR composes every
participant's view, so every composition failure logs at **ERROR** on
`mc.actor.meeting` carrying `outcome` ("Media signalling could not be composed").
The one connection-side value, `meeting_state_unavailable`, is a WARN on
`mc.webtransport.connection` when the actor did not register a declaration. The
counter is still the complete record.

```bash
kubectl logs deployment/mc-0 -n dark-tower --tail=500 | grep -i "media signalling\|receive capability"
```

A participant whose view failed to compose stays otherwise **healthy** — joined,
on the roster, every other post-join message working. That is deliberate graceful
degradation, and it is also why the failure has no other symptom.

**3. `mc_media_slot_states_total{slot_state}` — are the slots MC filled actually
carrying anything?** Note the label is **`slot_state`**, not `outcome` — this is
the one of the three that is not an outcome vocabulary. Its domain mirrors the
wire `SlotState` enum **exhaustively (all eight variants, not the three reachable
today)**, which is what makes MC's distribution directly comparable with the
client's.

```promql
sum by (slot_state) (rate(mc_media_slot_states_total[5m]))
```

`active` is the healthy value. `source_muted` is a source's mute — client OR
server mute (one wire state for both) — and is normal. `fewer_sources_than_slots`
means the subscriber declared more slots than there are senders it shares a
CONNECTED handler with to fill them — normal in a meeting smaller than N, ALWAYS
the state of a solo participant (loopback is removed, R-3), and the state of every
slot of a participant not yet connected to any handler. `source_unreachable` stays
**unemitted**: a participant the subscriber shares no connected handler with is
named in `StreamAssignments.unreachable_sender_ids` instead (see Scenario 18 and
`mc_media_unreachable_senders_total`). A non-zero `unspecified` is an **MC
defect** — that value should never reach the wire — and is visible here
precisely because the vocabulary was not pruned to the reachable subset.

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
| `applied` | The audio flag moved on a declared connection; the meeting actor re-emits the changed slot view to the subscribers HOLDING this source. A failed re-emit is counted by the actor on counter 2 and on `mc_media_slot_view_emissions_total{outcome="composition_failed"}`. |
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
mailbox** — one actor hop plus a re-emit to the source's holders. Unbounded, one
client toggling in a loop spends that at line rate. The resulting fan-out is also
bounded per MEETING by the actor's per-turn flush bound — whose visible edge is
`mc_media_slot_view_emissions_total{outcome="deferred"}` (routine under load; the
views still converge) — so do not route a meeting-wide latency investigation here.

**Caveat that also applies to `mc_media_slot_states_total` above**: both counters
are skewable by one participant (and slot states also by meeting churn, since the
actor re-emits on peers' joins and leaves). Any SLO built on them needs
per-connection normalisation.

**5. `mc_participant_outbound_messages_dropped_total{payload_kind}` — did the
client actually RECEIVE what MC decided to send?** Not a media-path metric (no
`key_custody`, generic outbound choke point), and listed here anyway because it
is **the completeness caveat on counter 2**.

```promql
sum by (payload_kind) (rate(mc_participant_outbound_messages_dropped_total[5m]))
# payload_kind ∈ {signaling_raw, participant_update_joined, participant_update_left,
#                 participant_update_muted, meeting_kek_update}
# (participant_update split into _joined/_left in story 2 task 9 and _muted in task 12;
#  payload_kind=~"participant_update.*" recovers the old merged series)
```

**`emitted` does not mean the client was told.** `record_send_directive` fires
`emitted` **before** the message is handed to the participant actor, so two
failure modes sit *after* the counter and neither moves it:

- **Mailbox FULL** — `try_send` fails, the drop is counted **here** under
  `payload_kind="signaling_raw"`. This is the only queryable evidence.
- **Participant mailbox CLOSED** (participant actor gone) — counted on
  `mc_media_slot_view_emissions_total{outcome="delivery_failed"}`, an EARLIER hop,
  disjoint from this one.

So the honest reading of a silent participant is: `accepted` incrementing,
`emitted` incrementing — **and a non-zero `signaling_raw` drop rate or
`delivery_failed` rate is the signal that contradicts them.** Check it
before concluding from counter 2 that the client was directed.

**The WARN beside it is one-shot per connection; the counter is not.** Every drop
is counted, only the first is logged, because a wedged outbound channel produces
one drop per roster broadcast — O(participants x events) identical lines from a
single bad connection, on a path a client can drive. **So log-line volume
understates this badly**: one line can stand for thousands of drops, and the
counter is the complete record of repeat occurrences.

**Escalation**: `meeting-controller` for `unknown_stream_number`,
`transport_mode_unspecified`, `handler_url_unresolved`, `assignment_failed` and
any `composition_failed`; `client` for the capability rejection tokens and for a
sustained `rate_limited` (a repeat loop in the SDK's mute path). For "I can hear
some people but not others", go to Scenario 18 first.

**6. `mc_media_server_mute_requests_total{action,outcome}` and
`mc_media_unmute_requests_total{outcome}` — did a host's server mute (or a
participant's request to be unmuted) land?** Both are client-facing signalling:
the first counts a host's `ServerMuteRequest` dispositions, the second a muted
participant's `UnmuteRequest` relays to the host. An unmute REQUEST never lifts
a mute — only a host's server-unmute does — so `relayed` rising with the
participant still muted is correct behaviour, not a fault. A
`participant_update_muted` drop on counter 5 is the one drop here with no
re-sync path: that client shows a wrong mute indicator until the next mute
change on that participant. For "a host muted someone and they are still
heard", go to Scenario 21.

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
| `registry_full` | **RETIRED** (story 2 task 20): the per-meeting connection registry and its cap are gone; MC never declines a binding for capacity | **Version skew only** — an MC image predating the retirement during a rolling deploy. Redeploy MC; not a capacity signal, there is no cap to raise |
| `user_ambiguous` | **One user, two participants** — the user joined from two devices, so one token `sub` maps to two roster entries and MC will not guess which | **No operator remedy.** Never self-clearing; the fix is a contract change (`docs/TODO.md`) — and `connection_id` is NOT it (it tells connections apart, not joins). Field action: have the user leave on one device. **Not an MC defect** — MC is correctly refusing to guess, and records connectivity for neither entry |
| `resolved` | MC answered an ordinal | Should not co-occur with an MH decline; if it does, suspect skew between the pods you are querying |

> **`participant_unknown` is not automatically a join race.** Sustained at ~100% of
> attempts it means MC and MH do not agree on participant identity at all — MH names the
> participant by its token `sub`, and if MC keys its roster by a different value it can
> never resolve *any* participant. A race is a small fraction and decays; a mismatch is
> flat and total. **The ratio is the discriminator, not the label.**

**`declined_no_sender_binding` on MH is the UNION of all three unresolved MC outcomes**, and
MH structurally cannot split them — it observes only `sender_id == 0`. That is why this
series is not redundant with MH's: all three have different remedies, from "clears itself"
(`participant_unknown` race) through "fix the MC↔MH registration path" (`meeting_unknown`)
to "no operator remedy exists" (`user_ambiguous`).

**Do not page MC on every MH decline.** Three of MH's **six** decline outcomes
(`declined_sender_binding_conflict`, `declined_mc_endpoint_unknown`,
`declined_mc_auth_rejected`) have their first move inside MH, and one of those names MC
in the label without implying MC is unwell. Scenario 15 Step 1 partitions on which
service to open before the label is read.

---

### Scenario 15: Media Generation Divergence

**Alert**: `MCMediaGenerationDivergence`
**Severity**: Page
**Runbook Section**: `#scenario-15-media-generation-divergence`

**What this is.** MC sends MH a meeting registration carrying a **generation** number. MH's
response echoes the generation it has **applied** — never the highest it has received (ADR-0036
§8). When the two differ, **MH is forwarding under stale or absent policy while every liveness
signal reads green**: `up` is 1, readiness passes, the gRPC call returned success, the channel is
healthy, handshake latency is normal. ADR-0036 §8 calls this shape a **partial blackhole reporting
healthy**, and notes it is harder to diagnose than a total one precisely because nothing looks
wrong.

The mechanism it catches: MC sends generation 7 → MH enqueues it on the session actor's bounded
mailbox and returns success → the mailbox is full, or the apply errors → **MC believes MH runs
generation 7; MH runs generation 4.** The call succeeded. Echoing on *receipt* rather than on
*apply* would reproduce exactly the bug the field exists to catch.

> **ANY STRUCTURAL CHANGE RE-PUSHES; FORCE ONE ONLY IF THE MEETING IS QUIESCENT.**
>
> ADR-0036 §8 says divergence "is self-correcting — a lost response is re-asserted on the next
> tick." **That describes a cadence that does not exist yet** (story 4). What does exist since
> story 2: **every** structural change — a join, a leave, a capability declaration, a mute —
> re-renders the meeting and re-publishes to every handler, and a push that failed stays
> UNCONFIRMED, so the next structural change of any kind re-sends it even when that handler's
> snapshot did not change. In a multi-party meeting with any churn, a divergence therefore often
> **clears on its own** within the next roster event — check whether it already has (Step 1's
> counter stops moving) before acting.
>
> If the meeting is **quiescent** (nobody joining, leaving or toggling), nothing will re-push: force
> a structural change. **An MC restart against a live handler is a separate arm and self-corrects on
> the first post-restart structural change**: MC re-derives generations from 1 while MH still holds
> K, MH echoes K, and MC adopts K as a floor and re-pushes at K+1 — do not force anything for it;
> see Scenario 18 arm (e) for what participants see meanwhile. **This arm is NOT under any
> `outcome` label and does not fire this alert**: it is recorded on
> `mc_media_policy_generation_adoptions_total{outcome="adopted"}` instead (at most one per live
> (meeting, handler) per MC restart; the sibling `{outcome="superseded"}` counts replies discarded
> for a newer render and has no such bound) and logged as a WARN at
> `mc.register_meeting.trigger` ("adopting it as a floor",
> carrying `sent_generation`, `applied_generation`, `adopted_generation`). So after an MC restart a
> clean Step 1 split is expected — read the adoption counter beside it. Conversely, a
> `generation_mismatch` with `applied > sent` in the ERROR line that DID reach this alert is a
> FAILED adoption ("Could not adopt the handler's generation") or a higher echo after a confirm
> (e.g. the `u64::MAX` ratchet wedge): escalate to `meeting-controller`, it will not self-correct.

**Symptoms**:
- Alert `MCMediaGenerationDivergence` firing.
- Participants report no audio, or audio that stopped, while the client stays connected.
- MH pod is Ready, MC pod is Ready, no error rate anywhere.
- `mh_media_frames_forwarded_total{direction="egress"}` flat or missing for the affected handler
  while ingress is non-zero.

**Blast radius**: per **(meeting, handler)**. Other meetings on the same MC and the same MH are
unaffected. That is why this is a diagnosis job, not a restart job.

**Diagnosis**:

Step 1 — **split on the outcome label first. The two values have different first moves.**

```promql
sum by(outcome) (increase(mc_media_policy_pushes_total[15m]))
```

| `outcome` | Means | First move |
|---|---|---|
| `generation_mismatch` | MH echoed an **older** generation than MC sent. The apply path ran and did not take effect — a full session-actor mailbox, or an apply error. | Open **MH**. Read `mh_media_policy_applies_total{outcome}`; `apply_failed` and `rejected_invalid` are the two that produce this. |
| `no_applied_generation` | MH echoed **nothing**. Either an MH build that predates the applied-generation echo (a rollout skew), or the apply never ran at all. | Confirm the MH image first — `kubectl get deploy -n dark-tower mh-0 mh-1 -o jsonpath='{.items[*].spec.template.spec.containers[*].image}'`. A skew is the cheap explanation and it clears itself on rollout completion. |
| `transport_mode_mismatch` | **A DIFFERENT FAILURE, BUT IT PAGES HERE AND THIS IS YOUR RUNBOOK.** §8's separate "any two-ends-must-agree configuration is echoed back with a loud mismatch" check: MC declared a transport mode, MH echoed a different one. Not generation divergence, and the remedy below is not the rejoin. It is routed here because nothing else pages on it and a page with no runbook is worse than a page in an imperfect one. | Compare MC's declared transport mode against MH's applied one — a configuration disagreement, not a policy-apply failure, so **the resolution steps below do not apply**: a rejoin re-sends the same disagreeing declaration. **A REMEDY ALREADY EXISTS — go to [Scenario 12: RegisterMeeting Coordination Failures](#scenario-12-registermeeting-coordination-failures) FIRST**, which carries it along with the trap: **roll MH FORWARD; do NOT roll MC back.** Rolling MC back returns it to `policy_generation: 0` registrations, which MH installs nothing for — the pre-change media blackhole rather than a fix, and it is the intuitive move when two ends disagree about a version-skewed value. Escalate to `meeting-controller` and `media-handler` together only if that does not resolve it; the value is asserted on one side and enforced on the other. |
| *anything else* | **A NEW OUTCOME VALUE.** The selector is negated (`outcome!~"match\|handler_id_mismatch"`), so an outcome added after this runbook was written pages here rather than falling silently outside the alert — deliberately. | You are the first responder to see it. Read `PolicyPushOutcome` in `crates/mc-service/src/media_routing/confirm.rs` for what the emitting site means by it, then **add a row to this table**. Do not narrow the selector to make the page stop. |
| `handler_id_mismatch` | **Diagnostic, not an incident.** `MH_HANDLER_ID` is per-incarnation today, so an ordinary MH pod restart produces this outcome by construction. | Ignore unless it persists with no MH restart. The alert deliberately excludes it for this reason. |

> **Do not "fix" `no_applied_generation` by making MH stricter.** MH deliberately does **not** reject
> `policy_generation: 0`, because MC and MH roll independently: MH-first rejection would register no
> meeting and kick every client at the registration timeout. That is ADR-0036 §8's opening
> paragraph almost verbatim — the permanent-media-blackhole-reached-through-an-ordinary-rolling-deploy
> failure the applied-generation echo exists to prevent. A responder who reads "rollout skew" and
> reaches for stricter MH validation is reaching for the exact change the design forbids.

Step 2 — **read the magnitude, second, not first.**

```promql
max(mc_media_generation_divergence)
```

This gauge is `|sent − applied|`. **It is a triage aid and must never be the primary evidence**, for
two reasons its catalog entry records: it is **last-write-wins at pod level**, so a healthy push for
an unrelated meeting overwrites a diverged reading; and it is written once per registration push,
which is coarse relative to the scrape interval, so a real divergence can be overwritten before it is
ever scraped. A reading of `0` here is **not** evidence that nothing diverged. The counter in Step 1
is the durable record.

Step 3 — **confirm MH's side of the story.**

```promql
sum by(outcome) (increase(mh_media_policy_applies_total[15m]))
```

`applied` is the healthy outcome. `apply_failed` and `rejected_invalid` are the two that produce
`generation_mismatch` on MC's side; `rejected_stale` is MH correctly refusing an out-of-order
generation and is not a fault. If MH shows `applied` for the generation MC sent, the divergence is
in the response path, not the apply path.

**Resolution**:

> **These steps are for `generation_mismatch` and `no_applied_generation`.** For
> `transport_mode_mismatch` see its row above — a rejoin re-sends the same disagreeing declaration
> and will not clear it.

1. **If the meeting is quiescent, force a structural change — prefer a capability re-declaration
   or a mute toggle.** Either makes MC re-publish, and the unconfirmed push is re-sent. In a
   meeting with churn this usually happens by itself; confirm Step 1's counter is still moving
   before acting. **Do not reach for leave-and-rejoin in a meeting with one participant**: as of
   story 2 task 12 the last participant leaving *ends* the meeting — MC releases the handler and
   notifies GC — so the rejoin lands on a re-created meeting rather than re-publishing to the
   diverged one. The divergence clears, but you have replaced the meeting instead of repairing it,
   and the evidence goes with it. A mute toggle is the cheapest safe trigger and costs the
   participant nothing.
2. If the structural change does not clear it, the apply is failing repeatedly rather than transiently: open MH
   and read `mh_media_policy_applies_total{outcome}` and MH's logs for the session actor's mailbox
   state.
3. If `no_applied_generation` and the images are skewed, let the rollout complete; the condition
   clears when every MH pod runs a build that echoes the applied generation.

> **DO NOT RESTART MH AS A REMEDY.** It clears the symptom and destroys the evidence, and — because
> MH sheds media sessions on restart with no drain (ADR-0036 §11, and `mh-deployment.md`
> §Rollout With Media Flowing) — it takes down every *working* media session on that pod while
> recovering none of the ones that were already dark. There is no periodic re-assert in this build to
> rescue them (story 4). A restart makes the blast radius strictly larger and the diagnosis impossible.

**Escalation**:
- `meeting-controller` owns the push side; `media-handler` owns the apply side. The label in Step 1
  tells you which to open first — that is the whole point of splitting on it.
- Both are required reviewers on any change here: the metric is emitted by MC but its **value** is a
  statement about MH's apply path.

**Related**:
- [`mh-incident-response.md` Scenario 17: Media Datagram Drop](mh-incident-response.md#scenario-17-media-datagram-drop)
  — stale or absent MH policy is one of the two causes of "client sending, nothing coming back", and
  this scenario is the other half of that fork.
- `docs/observability/metrics/mc-service.md` — canonical definitions for
  `mc_media_policy_pushes_total` and `mc_media_generation_divergence`.

---

### Scenario 16: Missing Key Material

**Alert**: `MCMediaMissingKeyMaterial`, `MCKekPushFailureRate`; tripwires `MCClientKekConflictingKey`, `MCClientRosterKeyRebind`, `ClientKekRetentionViolation`
**Severity**: Warning
**Runbook Section**: `#scenario-16-missing-key-material`

> **This scenario also hosts the client key-lifecycle TRIPWIRES** (story 2 task 16). These are
> three client tripwire arms, each alerted on its first occurrence. Each has its own rung
> below: [conflicting KEK](#tripwire-conflicting-kek),
> [roster key rebind](#tripwire-roster-key-rebind) and
> [KEK retention violation](#tripwire-kek-retention-violation). They are here because two of the
> three remedies are MC's key-and-roster delivery path. The third is an SDK regression and is
> hosted here only because there is no client incident runbook.

> **`MCKekPushFailureRate` routes here too** (story 2 task 9). A KEK push that did not reach a member
> IS missing key material at that client, one hop earlier: the server-side cause of the
> `no_kek_for_generation` arm below. Split on `mc_meeting_kek_pushes_total{outcome}` first —
> `dropped_outbound` (the member's outbound channel was full or closed: a slow or wedged client
> connection), `actor_unavailable` (the participant actor exited: MC Scenario 2), `timed_out` (no
> answer within 2 s: read `mc_actor_mailbox_depth`, MC Scenario 1). `participant_gone` is benign and
> is excluded from the alert. This counter is also the **small-denominator** detector that
> `MCMediaMissingKeyMaterial`'s fleet-wide client ratio cannot be. Security consequence to hold in
> mind: a member that misses a push does not rotate its own transmit keys, so a departed participant
> keeps opening that member's media past W.
>
> **Do not wait for it to clear: there is NO re-push.** A dropped `MeetingKekUpdate` is counted and
> never re-sent, and the client has no message to ask for the key again. The member stays on the
> stale generation until the NEXT rotation's push reaches it, or it rejoins. In a quiet meeting
> nobody leaves, so that is **indefinitely**. Meanwhile it cannot open frames under the new
> generation, and once the others' retention of the old generation lapses they drop its media as
> stale, so it can no longer be heard either. The remedy for an affected member is a page reload,
> which is a fresh join and carries the current KEK. The durable fix is task-sized and tracked in
> `docs/TODO.md` §Media Path Obligations, "Control-plane messages that must not be lost ride a
> droppable channel". The full rotation walk is [the rotation arm](#rotation-arm--silence-after-someone-left) below.

**What this is.** A receiving client is dropping frames because it does not have the key material
needed to open them. The counter is **client-side**, by design: MH never opens a frame and
**structurally cannot observe any of the four conditions** (ADR-0036 §4, §11).

```promql
sum by(reason) (rate(dt_client_media_frames_dropped_total{reason=~"no_kek_for_generation|kek_generation_stale|no_roster_entry|unwrap_failed"}[5m]))
```

**The three key-generation and roster reasons are expected as transients** at join and immediately
after a KEK rotation. `unwrap_failed` has **no** healthy transient (Arm 4). **The sustained case is
the signal**, and ADR-0036 §11 states it is the *only* signal for a join or
rotation path that has silently stopped delivering keys. That is why the alert's `for:` window is
long rather than its threshold being high — see the comment on the rule.

> **THIS ALERT CAN NOW FIRE, AND YOU MAY WELL HAVE BEEN PAGED.** Before 2026-09-23 it could not:
> `dt_client_*` metrics reached no Prometheus. The export path is now built and proven end to end,
> so a page from this alert is a real signal about real client-side drops.
>
> **But absence of a page is still not evidence that key delivery is healthy**, for a different
> reason than before. The series can go *missing* without the alert firing: five controls on the
> collector's metrics path drop client data by design, and on a deployed cluster all five look
> identical to "no browser is running" — and there is no per-browser `up` signal, nor can there be
> (ADR-0036 §11 bars a per-session `service.instance.id`), so the presence of a client series is the
> only liveness signal the browser fleet has. If you arrived here from a user report rather than a
> page, run the quiet-series ladder in `gc-deployment.md` §When the client series go quiet **before**
> concluding that drops are not happening.
>
> **This alert's coverage is not meeting-wide.** Guest participants produce no client telemetry at
> all, so a guest-only failure renders identically to a healthy meeting. Latent today (the SDK has no
> guest join path yet) and tracked in one place — `docs/TODO.md`, the guest-telemetry entry — which
> is where the mechanism, the trigger and the owner live. Do not restate them here.

> **TIMESTAMPS ON CLIENT SERIES ARE COLLECTOR-ARRIVAL TIME, NOT BROWSER-EVENT TIME — AND THIS
> SCENARIO IS WHERE THAT BITES.**
>
> The collector restamps every delta datapoint on arrival (it must: browsers with skewed clocks share
> one stream identity, and a single far-future point would otherwise stall the whole fleet's
> accumulation). Browsers buffer while the collector is unreachable and flush on recovery, so **every
> buffered point is stamped at the moment of recovery.**
>
> **The entire triage below is a correlation between a client-side counter and MC-side events, so a
> re-dated client series will mis-align that correlation by the full duration of any collector
> outage** — and the client side will look like it started late. Before correlating, check whether
> `up{job="otel-collector"} == 0` anywhere in the window: if it was, the client-side onset you are
> reading is the flush, not the event, and the true onset is somewhere inside the gap. A long outage
> also overflows the browser's bounded export queue, so the visible burst *understates* what was
> lost. (The ratio this alert fires on is unaffected — numerator and denominator were buffered in the
> same batch and compress together — which is why it is ratio-shaped and why an absolute-rate alert
> on a `dt_client_*` series would fire on the artefact alone.)

#### First fork — is this key material at all? "The sixth person can't be heard"

**Rule these two out before working the key-material ladder.** Neither drops a frame for key
material, so this alert and the arms below cannot see them. A report of "I hear some people but not
all" in a meeting larger than the listener's slot count lands here far more often than on a key
fault.

- **Slot-cap rejection.** The client's receive-slot count N exceeded MC's cap, and MC rejected the
  **whole** declaration. The listener hears nobody. Read `mc_media_receive_slot_cap` beside
  `mc_media_receive_capability_declarations_total{outcome="slot_count_over_cap"}`. A rising over-cap
  rate with the cap below the client's configured N is a configuration mismatch, not a key fault.
  Client side: `client-dev-local.md` F18.
- **Static fill.** Slots fill in **join order** and are never reselected (story 2 R-2, R-4). With more
  senders than slots, the earliest joiners hold the slots, and a later joiner is inaudible to that
  listener until a slot frees, which happens when someone leaves. That is designed behaviour, not a
  fault. Read effective N (the client's declared slot count, on the UI's slot rows and the E2E bus
  `receiveSlots.declared`) and `mc_media_receive_slot_cap`. If the number of other senders the
  listener shares a handler with exceeds N, the unheard participants are the latest joiners. Nothing
  drops, nothing is counted as a drop, and `mc_media_slot_states_total{slot_state="active"}` is
  healthy. Client side: `client-dev-local.md` F17. Speaker-based selection, story 5, replaces static
  fill; until then there is no debugger for which sender holds which slot beyond the UI's slot rows.
- A rostered participant the listener shares **no connected handler** with is a routing fork, not
  this scenario: [Scenario 18](#scenario-18-a-participant-hears-only-part-of-the-roster).

**Triage splits on the `reason` label first, because the four arms have different remedies — and
because they are not equally instrumented.**

#### Arm 1 — `no_roster_entry`

No usable identity key for the frame's `sender_id`, **including the case where MC published an
empty key**. Per ADR-0036 §3, this means the receiver cannot resolve the sender's AC-attested
identity public key, so **signature verification — the whole sender-attribution story — is
failing**, not merely decryption.

**This arm has server-side corroboration.** Use it:

```promql
sum by(presence) (rate(mc_join_identity_key_presence_total[15m]))
```

A rising `absent` ratio answers *"are clients publishing keys at all?"* directly. That counter
exists precisely so that "every client omits the key and nobody notices" cannot be a silent steady
state.

**Its neighbour, which is a different condition**: a malformed key — present but not 0 and not 32
bytes — is refused at the trust boundary and lands on
`mc_session_join_failures_total{error_type="identity_key_invalid"}`. *No key* and *bad key* have
different remedies; do not conflate them.

Remedy is in MC's **roster publication** path, not key generation.

#### Arm 2 — `no_kek_for_generation`

The frame's wrap announces a KEK generation **newer** than any the client holds: MC's KEK push has
not arrived. Remedy is in MC's **KEK delivery** path. There are **two** KEK sources, and a check on
only one of them excludes half the delivery path:

- `dt_client_media_kek_updates_total{source="join_response"}` — the KEK carried on the join
  response;
- `dt_client_media_kek_updates_total{source="kek_update"}` — a rotation pushed to an
  already-joined client (`MeetingKekUpdate`). **This is the arm's main case after a rotation**, and
  a query filtered to `join_response` alone cannot see it.

> **THIS ARM HAS NO SERVER-SIDE COUNTER, AND CANNOT HAVE ONE AS THINGS STAND. Read that as a gap,
> not as reassurance.**
>
> `mc_meeting_kek_generated_total` increments unconditionally and is identically the
> meeting-creation count; MC has no meeting-creation counter to divide it by, so — in its catalog
> entry's own words — **the inference is not computable even in principle**, and **no alert may be
> built on the absence of that counter moving**.
>
> The gap narrowed on 2026-09-23 but did not close. The client-side counter now DOES reach
> Prometheus, so `dt_client_media_kek_updates_total` (both `source` values) is queryable and this
> arm has a real client-side signal for the first time. **There is still no MC-side counter, and
> still nothing that can corroborate from the server.** So the arm remains asymmetric with Arm 1,
> which has `mc_join_identity_key_presence_total` to answer "are clients publishing keys at all?"
> directly. **The absence of a signal here is not evidence that the KEK is present.**

**Check the non-dump path first, and completely, before anything else.** Both of these are
observable without touching MC's memory:

1. **The per-join response-side condition.** The KEK is not provisioned for a join when the join
   response's `meeting_kek` is not exactly 32 bytes. This is checkable from the client side.
2. **`sum by(source) (rate(dt_client_media_kek_updates_total[15m]))`** — the client-side
   observation that a KEK was delivered and cached, split by source. `join_response` flat at zero
   across a join means join-time delivery, not usage; `kek_update` flat while rotations are
   expected (participants leaving) means the rotation push is not arriving.

If and only if both are exhausted and you still need to know whether the KEK is live in the meeting
actor, **stop here and read
[§Heap and Core Dumps Contain Live Meeting KEKs](#heap-and-core-dumps-contain-live-meeting-keks)
before doing anything else.** That question has no instrument in a running process, and the thing
you are about to reach for is a heap dump — which is why the gate exists.

> **NO STEP IN THIS SCENARIO MAY PRINT, LOG, OR OTHERWISE MATERIALISE A KEK TO CONFIRM IT IS
> PRESENT.** Not a temporary log line, not a debug endpoint, not a `dbg!`. ADR-0036 §11 places KEK
> and transmit-key material inside the credential-leak guard's scope for MC logs for exactly this
> situation, and *"I'll just add a log line to check the key is there"* is the shape that gets typed
> under incident pressure. A KEK in a log is a KEK in the log pipeline, on every node that ships it,
> for the retention period.

#### Arm 3 — `kek_generation_stale`

The frame's wrap announces a KEK generation **older** than the client still retains. A client
holds the current KEK generation plus at most **one** previous generation, and drops the previous
one after a retention window derived from MC's rotation debounce **W** (roughly W/2, bounded by a
client-side floor and ceiling). A frame still arriving under a generation that has aged out of
that window is dropped here. It is not an "MC has not sent it yet" condition (that is Arm 2);
the key was delivered, then correctly discarded.

A short burst right after a rotation is expected: frames that were already in flight under the
old generation, or a joiner that arrives after a rotation and sees pre-rotation frames. A
**sustained** rate means senders keep wrapping under a generation that receivers have already
retired: either the retention window is too short for the rotation push skew in this deployment,
or a sender's own rotation push is landing late (check that sender side against Arm 2's
`source="kek_update"` signal).

**At the shipped default, raising W does NOT help — read this before touching it.** Retention is
`min(W/2, ceiling)`, and the client ceiling `KEK_RETENTION_CEILING_MS` (30s, in
`packages/sdk-core/src/config/clientConfig.ts`) is the binding parameter: at the default
`MC_KEK_ROTATION_DEBOUNCE_SECONDS=60` (`infra/services/mc-service/config.env`), W/2 equals that
ceiling EXACTLY, so retention is **already at its design maximum**. Raising W to 90, 120 or 300
yields the identical 30s — and flips `ceiling_clamped` to permanently non-zero fleet-wide, a second
signal manufactured by a change that cannot help. If you have already raised W and see
`ceiling_clamped` climbing, that is the confirmation you hit the ceiling, not a new fault.

So at the default config, sustained `kek_generation_stale` is **not a W-tuning problem**: receiver
retention is maximal, and the cause is on the SENDER side — go to the rotation push path (Arm 2's
`source="kek_update"` signal) and find the sender whose push lands late. Lengthening retention
past 30s means raising the client ceiling, which is a **code change, deliberately bounded** for
key minimisation (a superseded KEK is held in client memory only as long as in-flight frames need
it — see the comment at the constant). It is a design decision, not a knob for incident pressure.

**Raising W IS the remedy in exactly one case: a deployment that has LOWERED W below 60s**, where
W/2 is genuinely under the ceiling and retention tracks it. Above 60s, W affects only MC's
rotation debounce and no longer affects client retention at all. That decoupling is why this arm
lives in an MC scenario even though the counter is client-side: MC's KEK-and-roster delivery path
owns both the push timing and W, as it does for Arms 1 and 2.

Before you change W, check the client's own retention signals, which show whether the client is
honouring the W it was sent:

- `dt_client_media_kek_retention_anomalies_total{outcome=...}` — `floor_substituted` (the client
  received no or zero W, as after an MC rollback below the field — see `mc-deployment.md`
  §Rollback), `ceiling_clamped` (W/2 exceeds the client ceiling, so raising W further buys
  nothing — see above), `below_rewrap_latency` (W/2 is under the client's send-queue drain time;
  reachable only when W has been lowered far below the default, and the remedy is raising W back
  toward 60s).
- `dt_client_media_kek_generations_retained_total` next to
  `dt_client_media_kek_updates_total{source="kek_update"}` — rotations arriving while the retained
  counter stays flat means rotations are happening and **no** previous generation is being kept.

This arm has no server-side counter either (same gap as Arm 2).

#### Arm 4 — `unwrap_failed`

**It joined this alert's selector in story 2 task 16.** Before that it was selected by no alert
rule. The frame is **authenticated**: the signature covers the wrapped-key block and verification
runs before unwrap, so transit corruption lands on `signature_invalid`, never here. What remains is
a KEK-unwrap tag mismatch on a frame a roster member really sent. Its reachable causes are:

1. **The sender and this receiver hold DIFFERENT KEK bytes under the same generation.** This is a
   KEK-distribution split, and the reason the arm belongs to MC's delivery path. Its per-client
   witness is the [conflicting KEK tripwire](#tripwire-conflicting-kek): a client refusing to
   overwrite a held generation with different bytes. That tripwire is the small-denominator
   detector the fleet-wide ratio cannot be.
2. **A sender-side wrap or key-schedule bug.** Route to `client`.
3. **An authenticated roster member emitting bad wraps**, whether misbehaving or compromised. Route
   to `security`.

A drop lands here only when the unwrap failed **and** no usable transmit key for the frame's key id
was already cached. With a usable cached key the same mismatch is the non-dropping
`wrap_key_id_mismatch`. That is why this arm has no healthy transient: a sustained rate is always
signal.

#### Rotation arm — silence after someone left

Since story 2, every roster removal triggers a KEK rotation, debounced to once per W (Scenario 17).
**Every rotation is now a moment when key material can go missing**, and the arms above get a steady
duty cycle of transients, not only the one at join. This arm crosses Arms 2 and 3. Walk it in this
order.

**Rung 0 — confirm that a client series is present in Prometheus at all.** Every rung below reads
client telemetry. If the client pipe is dead, `MCMediaMissingKeyMaterial` stays green and every client
query comes back empty. Use the quiet-series ladder in `gc-deployment.md` §When the client series go
quiet.

**Rung 1 — split on the `reason` label, then on what the onset correlates with.**

- `no_kek_for_generation` means the frame's generation is newer than any the client holds, so the new
  KEK did not arrive (Arm 2). `kek_generation_stale` means the generation is older than the client
  retains, so the old KEK was already dropped (Arm 3).
- **The rotation arm correlates with a LEAVE. The join arm correlates with a JOIN.** A rotation shows
  as `mc_meeting_kek_generated_total{trigger="participant_left"}` (or `sender_space_exhausted`, Scenario
  17 Arm 2) incrementing at the onset, and on the client as
  `dt_client_media_kek_updates_total{source="kek_update"}`. A join delivers the KEK on the join
  response (`source="join_response"`) and increments no rotation `trigger`. A reconnect re-issue also
  counts as `join_response` and increments no `trigger`. If the onset sits on a join with no rotation
  near it, go back to Arm 2's join path. This arm is for onsets that sit on a leave.
- Remember that client timestamps are **collector-arrival time** (the callout above). Check
  `up{job="otel-collector"}` over the window before trusting any correlation.

**Rung 2 — per-recipient push outcomes: was the new KEK never sent, or sent and not acknowledged?**

```promql
sum by(outcome) (increase(mc_meeting_kek_pushes_total[15m]))
```

- **Never sent**: `dropped_outbound` (the member's outbound channel was full or closed),
  `actor_unavailable` (the participant actor had exited, Scenario 2) and `timed_out` (no answer within
  the push timeout; read `mc_actor_mailbox_depth`, Scenario 1). MC knows the member did not get it.
  **There is no re-push** (the callout above): that member stays on the old generation until the next
  rotation reaches it or it rejoins. The remedy for the affected member is a page reload.
- **Sent, not acknowledged**: `delivered` means **handed to the connection's outbound stream**. It is
  not a client acknowledgement, and none exists on the wire. All-`delivered` does not prove the client
  installed it. The client-side confirmation is `dt_client_media_kek_updates_total{source="kek_update"}`
  rising in step with MC's rotations. If MC shows `delivered` and the client counter does not move, the
  loss is between MC's outbound stream and the SDK's install (a client refusal shows on
  `dt_client_media_kek_install_refusals_total{outcome}`), so route it to `client`.
- `participant_gone` is benign: the member was inside the reconnect grace and gets the current KEK on
  return.

**Rung 3 — retention expiry. This rung is CLIENT telemetry only, and MC-side green does NOT rule it
out.** Every MC signal can be healthy here: the rotation happened, pending age cleared, every push was
`delivered`. Receivers can still be dropping frames because they retired the previous generation
before in-flight frames under it stopped arriving. Read the client:

- `dt_client_media_frames_dropped_total{reason="kek_generation_stale"}` sustained after the rotation,
  not just a short burst.
- **The PAIR** `dt_client_media_kek_generations_retained_total` against
  `dt_client_media_kek_updates_total{source="kek_update"}`. Rotations arriving while the retained
  counter stays flat means **no** previous generation is being kept, which causes an audio gap at
  every rotation. Neither series alone says anything.
- `dt_client_media_kek_retention_anomalies_total{outcome}`: `floor_substituted` (the client got no W,
  e.g. an MC rolled back below the field), `below_rewrap_latency` (W lowered far below the default)
  and `ceiling_clamped` (a configuration state). Whether raising W helps is decided in Arm 3, and at
  the shipped default it does not.

#### Tripwires — client key-lifecycle counters that should never move

Three client tripwire arms are **expected to stay at or near zero** (retention violation and conflicting KEK have no known benign cause; rebind has one, below), and each is alerted on its first occurrence.
Read these properties before triaging any of them:

- **The alert is presence-shaped**: `sum(X) > 0`, not `increase()`. The SDK exports a counter only
  when it is recorded, so the series does not exist until the first increment and first appears
  already at ≥ 1. `increase()` would never see that first event.
- **"Still firing" does not mean "still happening".** The alert clears on its own when the
  collector's prometheus exporter drops the idle series, after its `metric_expiration`
  (`infra/services/otel-collector/collector.yaml`). An idle open tab does not hold it up, because
  the SDK exports only the attribute sets recorded in each interval. No one has to ack it.
- **A second occurrence while the alert is firing is invisible in the alert state.** Read the raw
  counter (`sum by(outcome)(X)`): it rises.
- **Silence proves nothing about the pipe.** The alert cannot fire while the client pipe is dead.
  Liveness is `up{job="otel-collector"}` plus the quiet-series ladder in `gc-deployment.md`.
- **A new legitimate cause is added to the rule comment.** The rule is never retired or silenced for
  it. The first legitimate-looking increment is exactly the event that tempts someone to retire a
  tripwire.

#### Tripwire: conflicting KEK

**Alert**: `MCClientKekConflictingKey`, on `dt_client_media_kek_install_refusals_total{outcome="conflicting_key"}`.

A client was handed **different KEK bytes under a generation it already holds**, and refused them.
The SDK has no reconnect: one KEK holder lives per meeting session and dies with its signaling
connection, and a rejoin gets a new holder. So neither a stale tab nor a reconnect race reaches
this arm. An equal-bytes redelivery returns `already_held` from `MeetingKekHolder.install`, which is not a refusal and is never counted. The known causes are:

1. An MC defect that delivers different bytes under a held generation on a live connection.
2. A compromised or misbehaving MC.
3. A future SDK change that adds reconnect and swaps keys instead of starting a new session with a
   new holder. The fix is in the SDK; the refusal must not be relaxed.

The paired receive-side symptom is Arm 4 (`unwrap_failed`) on peers holding the other bytes. Route
to `meeting-controller` (KEK issuance and delivery), with `security` required.

#### Tripwire: roster key rebind

**Alert**: `MCClientRosterKeyRebind`, on `dt_client_media_roster_key_rebinds_total{outcome="rebind"}`.

A client saw a sender id it already had bound to one identity key arrive bound to a **different**
identity. `downgrade` is a separate, expected-non-zero arm and is not alerted. The known causes are:

1. An MC defect in roster publication.
2. A cache-poisoning attempt. Route to `security`.
3. **A lost `ParticipantLeft`** (MC's roster push dropped under outbound backpressure), followed by
   an R-16 sender-id reissue. The client never learned that the old binding ended.

**Check cause 3's necessary condition FIRST.** A sender id is reissued only under a later KEK
generation, and an epoch reset always bumps it (`crates/mc-service/src/media_admission/epoch.rs`,
"The invariant, stated once"). So cause 3 requires a sender-id-space-exhaustion epoch reset in that
meeting:

```promql
sum(mc_meeting_kek_generated_total{trigger="sender_space_exhausted"}) > 0
```

Read it as the **raw counter**, with no range window. It covers MC's process lifetime, and a meeting
cannot outlive its MC process. A `[24h]`-style window would wrongly rule the cause out for an older
meeting. The counter is **fleet-wide, with no meeting label**: non-zero **supports** cause 3 but does not rule it in for THIS meeting, and only
a fleet-wide zero **rules it OUT**, which leaves causes 1 and 2 (`meeting-controller` and
`security`). `MCKekEpochResetOnSenderIdExhaustion` having fired is corroboration.

If there was a reset, `mc_participant_outbound_messages_dropped_total{payload_kind="participant_update_left"}`
read as the **raw counter over MC's process lifetime** is supporting evidence. **Never use a
short-window zero on it as a rule-out**: the lost Left can precede the reissue by hours, because
the reissue comes only at the next exhaustion. The counter is fleet-wide, so it never confirms any
single increment either. Route to `meeting-controller`.

#### Tripwire: KEK retention violation

**Alert**: `ClientKekRetentionViolation` (`infra/docker/prometheus/rules/client-alerts.yaml`), on
`dt_client_media_kek_retention_violations_total`.

**This is a client SDK regression, and an operator cannot remediate it.** An SDK install path tried to make a client hold more than the current KEK generation plus ONE
previous (`MAX_HELD_GENERATIONS`). `RetentionGuard` **fails safe**: it zeroizes and drops the excess
in the same call, so no superseded key was kept past that install. This is a COUNT bound, not the
time bound `min(W/2, ceiling)`, which this counter does not observe. What is broken is the
key-lifetime code that should have made the guard unnecessary, so a further regression could take the
fail-safe with it, and that is why it alerts. The bound it protects is ADR-0036 §4's key
minimisation in a browser. The invariant is held by a unit test (`RetentionGuard` in
`packages/sdk-core/src/media/setup/kekSource.ts`), so there is no known legitimate cause. The
counter exists to catch a future refactor that breaks it.

It is a warning rather than a page only because it is zero-forever and unit-test-held. **Action**:

1. Roll back the web-app/SDK release that introduced it.
2. File a `client` bug with the release version (`client_version` on the series).
3. Involve `security`.

There is no MC-side remedy, and changing W does not help.

#### Neighbouring reasons that are NOT this scenario

| `reason` | What it actually is | Route to |
|---|---|---|
| `decrypt_failed` | The **SFrame payload** decrypt failed — the key schedule or the sender | `client` |
| `no_transmit_key` | A frame with neither a cached key nor a usable wrap — a **protocol violation**, not a fourth key reason | `protocol` / `client` |
| `sender_not_assigned` | A verified frame from a sender outside the client's MC-stated slot assignment — **misrouting**, not key delivery; deliberately excluded from this alert | `meeting-controller` (placement) / `media-handler` (forwarding) |

`unwrap_failed` and `decrypt_failed` are both AES-GCM failures on one receive path that route to
opposite teams. Read the label, not the symptom.

**Escalation**: `meeting-controller` owns the Arm 1–3 remedies and Arm 4 cause 1 (roster publication, KEK delivery,
rotation debounce).
`security` is a required reviewer on any change to KEK handling.

**Related**:
- [`client-dev-local.md` §4.5](client-dev-local.md#45-i-joined-and-i-hear-nothing--media-triage-ladder)
  — the client-side ladder that gets a developer here. **Do not duplicate the remedies there**; that
  file points at this scenario.
- `docs/observability/metrics/client.md` §`dt_client_media_frames_dropped_total` — the frozen
  reject-reason vocabulary and the `received = accepted + sum(drops by reason)` identity.

---

### Heap and Core Dumps Contain Live Meeting KEKs

**Read this before taking a heap dump, a core dump, or any memory capture of an MC pod.** It is not
a numbered scenario because it is not a failure mode — it is a hazard attached to a diagnostic
action that several scenarios lead to.

#### What is actually in the file

An MC heap or core dump contains, for that pod:

- **The meeting KEK of every meeting live on that pod** (ADR-0036 §4) — the symmetric key that
  unwraps every participant's transmit key.
- **Meeting and user JWTs in flight**, and the join-token material accompanying them.
- **Participant display names**, and the roster's identity public keys.

**Enumerated deliberately.** If this section said only "contains the KEK", the handling procedure
gets applied to the KEK and everything else in the list walks out in the same file.

#### Why this is different from every other artifact you might collect

ADR-0036 §4's security argument rests on one property, stated there in these terms: the KEK is
**never derived and never persisted** — it is random, lives in the meeting actor, and dies with the
meeting. *"Compromise must be live: a database, a backup, or a log yields nothing, and no master
secret exists whose loss reaches backward across meetings."*

**A dump is precisely the act that converts live-only key material into a durable artifact.** A dump
taken while a meeting is running decrypts any ciphertext of that meeting captured anywhere else —
indefinitely, and including after the meeting has ended. It is the one operation that defeats the
property the design's key management is built on.

Note also what the custody model already is: every service reports `key_custody=operator`. MC holds
the KEK and can read media; MH, the network, and storage cannot. That is the accepted position. What
is **not** accepted is turning MC's in-memory custody into a file.

#### Default posture: do not take one

Work the cheaper signals first. They answer most questions and none of them handle key material:

1. Metrics — the counters named in the scenario that sent you here.
2. `kubectl top pods -n dark-tower -l app=mc-service`, and the pod's own `/metrics`.
3. [Scenario 1: High Mailbox Depth](#scenario-1-high-mailbox-depth) and
   [Scenario 7: Resource Pressure](#scenario-7-resource-pressure) ladders, in full.
4. Logs, at the deployed level. **Not at a raised level** — see the prohibition below.

> **GATE — YOU ARE NOW HANDLING KEY MATERIAL.**
>
> If you proceed past this point, the artifact you produce is **private-key-equivalent**. Everything
> in §Handling applies from the moment it exists, and the meetings live at that moment are to be
> treated as **key-compromised** (§Remediation). Decide deliberately, and record who decided.

#### Handling, if one is taken

- **Classify it private-key-equivalent.** Not "sensitive", not "internal" — the same class as a TLS
  private key.
- **Never attach it to a ticket, a Slack message, or an incident document.** Reference it by
  identifier; never by content.
- **Never `kubectl cp` it to a shared bastion, a shared volume, or anything backed up.** A backup
  turns a bounded exposure into an unbounded one, which is the specific property §4 relies on not
  existing.
- **Encrypt at rest immediately**, before it is written anywhere that outlives the session.
- **Named-responder access only.** One or more named people, recorded — not a group, not a role.
- **A deletion deadline with a named owner.** A deadline with no owner is not a deadline. Record
  both.
- **A record of who held it**, for the duration and afterwards.

#### Remediation — the step that gets forgotten

**The meetings that were live at dump time are key-compromised.** Protecting the file is not the
remedy; it is half of it.

- The lever is **MC's KEK rotation** (ADR-0036 §4): MC generates a new KEK and pushes it to every
  member over signalling, and senders wrap under the new KEK from then on.
- **There is no operator-triggered rotation** (story 2 task 9 adds none). MC rotates only on a roster
  departure — debounced to at most once per W — or on `sender_id` exhaustion. A departure or an
  exhaustion that happens anyway will rotate the KEK, but you cannot make one happen on demand, so in
  practice the remedy for a dump-compromised live meeting is the next line.
- **If rotation cannot be driven for a live meeting, end the meeting.** An unrotated meeting whose
  KEK is in a file on someone's laptop is not a meeting that should continue.
- A procedure that protects the file and not the meetings is not a procedure.

#### Can MC produce a dump today without anyone asking?

**Yes, on this dev cluster, and nothing in our manifests constrains it.** Verified rather than
assumed, from inside a running MC pod:

```bash
# --- WSL2 --- read the posture; this prints no key material
MC_POD=$(kubectl get pods -n dark-tower -l app=mc-service -o name | head -1)
kubectl exec -n dark-tower "${MC_POD#pod/}" -- sh -c 'ulimit -c; cat /proc/sys/kernel/core_pattern'
```

At the time of writing this returns `unlimited` and `|/wsl-capture-crash %t %E %p %s`.

Two facts follow, and both matter:

- **`RLIMIT_CORE` is unlimited in the MC container.** Nothing in
  `infra/services/mc-service/mc-{0,1}-deployment.yaml` sets it; the pod spec has resource limits and
  a `securityContext`, neither of which touches core dumps.
- **`kernel.core_pattern` is not namespaced.** It is a host-global kernel setting, so what happens
  when an MC process aborts is determined by the **node**, not by anything in our manifests or our
  chart. On this dev cluster the host pipes cores to a WSL2 helper.

So an MC abort can write a core containing live meeting KEKs **with nobody asking and nobody
notified**. That is a standing exposure this procedure does not cover — a procedure governs
deliberate dumps, and this one is automatic. Stated plainly rather than left to be discovered,
because ADR-0036's own rule is to prefer structural impossibility over a control that has to notice,
and here there is currently neither.

**This is a statement about the dev cluster as measured above.** Re-run the command on any other
environment before assuming it holds there; a different node has a different `core_pattern`.

**Tracked, not merely written down.** This procedure governs **deliberate** dumps; the exposure
above is **automatic**, and a runbook sentence is read by someone already in an incident — which is
after the core has been written. The structural fix is `setrlimit(RLIMIT_CORE, 0)` at MC process
start (Kubernetes has no pod-spec ulimit field and `core_pattern` is host-global, so the process is
the only place in our control), and it is a real tradeoff against crash forensics rather than pure
hardening, which is why it is a task. See `docs/TODO.md` §Observability Debt, "An MC abort can write
a core dump containing live meeting KEKs".

#### What this section must never contain

No copy-pasteable command that writes a dump to a shared path, and no command that prints key
material to a terminal. The one command above reads two kernel/limit values and nothing else.

> **Related prohibition, which applies everywhere in this runbook**: raising a log level is not an
> acceptable diagnostic gate on the media path. ADR-0036 §11: the incident that motivates the level
> change is the same incident that produces the sensitive trace, and enabling debug logging on a pod
> is one routine action away from a fleet-wide voice-activity trace entering the shipping pipeline.


---

---

### Scenario 17: KEK Rotation Storm / Flapping Participant

**Alert**: `MCKekRotationStorm` (warning), `MCKekEpochResetOnSenderIdExhaustion` (info)
**Severity**: Warning / Info
**Runbook Section**: `#scenario-17-kek-rotation-storm--flapping-participant`

**What this is.** MC rotates a meeting's KEK for two reasons (story 2 R-12, R-16). Each rotation
pushes the new KEK and generation to every member present
(`mc_meeting_kek_pushes_total{outcome}`, once per recipient) and makes every sender re-wrap its transmit
keys. A storm multiplies that control-plane and client key work across the fleet. Rotation excludes a
departed participant from future media keys. It does not make past media unreadable, and it is not
described here as anything more.

**Fork FIRST on the `trigger` label** of `mc_meeting_kek_generated_total`. The two rotation triggers
have different bounds, different causes and different remedies, and they are never summed:

```promql
sum by(trigger) (rate(mc_meeting_kek_generated_total{trigger!="meeting_created"}[10m]))
# Per active meeting, against 1/W (W as this pod loaded it):
sum(rate(mc_meeting_kek_generated_total{trigger="participant_left"}[10m])) / sum(avg_over_time(mc_meetings_active[10m]))
1 / max(mc_meeting_kek_rotation_window_seconds)
```

#### Arm 1 — `participant_left` (`MCKekRotationStorm`): read the rate against 1/W

Leave-triggered rotation is **bounded at one per meeting per W** by the debounce, measured from the
OLDEST un-rotated departure and never restarted by a later one.

**The query is a FLEET AVERAGE per active meeting, and averages dilute** (the rule's own threshold
provenance says it proves a broad failure, not one meeting's). For one suspected meeting, count its
`mc.kek.lifecycle` "Meeting KEK rotated" INFO lines with `trigger=participant_left` (the meeting rides
the actor span) and check their spacing against W.

- **Rate per active meeting ABOVE 1/W: the debounce is not applying, broadly.** One bad meeting among
  many quiet ones may not lift the average. No amount of human churn gets there, because many departures in a busy meeting coalesce into one rotation (MC Media → KEK
  Departures Coalesced per Rotation, `mc_meeting_kek_rotation_coalesced_leaves`). A rate above the
  bound is an MC defect, and the known trap is a debounce that restarts on each departure. Check that
  `mc_meeting_kek_rotation_window_seconds` equals the ConfigMap's `MC_KEK_ROTATION_DEBOUNCE_SECONDS` on
  every MC pod. A W far below the intended value is a configuration fault that inflates rotation cost. A W
  ABOVE it lengthens how long a departed participant's KEK stays current (W is that exposure bound),
  so treat a mismatch in that direction as a security fact. Otherwise escalate to
  `meeting-controller`. (The alert fires above 1/W by the multiple in `MCKekRotationStorm`'s expr, so
  that ordinary variance does not page. Read the rate against 1/W itself.)
- **Rate AT OR BELOW 1/W is consistent with the debounce working fleet-wide.** The average cannot
  clear a single meeting; use the log-line spacing above for that. If it holds, the cost you are
  seeing is the JOIN path. A
  meeting whose members keep leaving and rejoining rotates at most once per W however fast they cycle,
  but each rejoin costs a full join. Go to the flapper residual below.

#### Arm 2 — `sender_space_exhausted` (`MCKekEpochResetOnSenderIdExhaustion`)

Exhaustion-triggered rotation is **immediate and unbounded by W**. It is self-limited to once per
exhausted sender-id namespace in one meeting. The namespace size is the allocator's
(`crates/mc-service/src/media_admission/sender_id.rs`; after a reset the fresh namespace is smaller,
because ids bound at the reset are excluded), so read it there rather than from a number copied here.
**Human churn does not reach this.** Treat it as a possibly **driven** cause, a flapping client or a
scripted join loop, until ruled out. MC has no join rate limit and no `jti` replay check. Identify the
meeting from
`kubectl logs -n dark-tower -l app=mc-service --tail=5000 | grep "sender_id namespace"` (the reset and
the high-watermark records share that stem). The meeting is **recovering, not broken. Do not end it.**

#### The recorded decision: flapper eviction does not ship

**This is a recorded decision (story 2 R-15), with this residual.** The debounce bounds leave-triggered
rotation to one per W. Exhaustion-triggered rotation is immediate but self-limited to once per
exhausted namespace. **Nothing bounds a flapping client's join-path cost.** Per cycle it costs a roster
read, a meeting-wide assignment recompute and a control-plane push to MH: O(N) work plus a gRPC call at
the flapper's rate. The leverage grows with meeting size, **so a two-person test shows nothing**.
Reproduce a suspected flapper in a meeting of realistic size, or not at all.

**The visibility signal is `mc_meeting_sender_ids_issued_max`.** It is the maximum across live
meetings, not a sum, so one meeting near a reset is not hidden. It measures namespace consumption
**since the last epoch reset**, not admissions, and it deliberately has no alert, because exhaustion
self-repairs into Arm 2. A value climbing steadily in a meeting of stable size is a flapper. Its
join-path cost shows on MC's meeting-actor mailbox (`mc_actor_mailbox_depth{actor_type="meeting"}`,
Scenario 1) and on the push rate to MH (`mc_media_policy_pushes_total`).

**Remedy, within the decision.** There is no eviction lever, no per-participant block and no
host-side removal in this build. (`LeaveReason::Removed` exists in MC's vocabulary, but nothing
issues it.) The flapper stops only when the client stops, or when the meeting ends. If the join-path cost
threatens other meetings on the pod (Scenario 1's mailbox signals), escalate to `meeting-controller`
with the meeting identified from the log line above. Do not restart MC: that sheds every session on
the pod to stop one client.

**If a reset happened and ONE sender then went inaudible to incumbents only**, suspect a **stale SDK
bundle** before anything server-side. A pre-story-2 client scopes replay state by `sender_id` alone
and drops a reissued sender's frames as replays, silently and permanently until the user reloads. No
MC signal shows it, and `MCMediaMissingKeyMaterial` cannot (the frame opens, then is refused as a
replay). Cross-reference [Scenario 18](#scenario-18-a-participant-hears-only-part-of-the-roster) and
`mc-deployment.md` §Coordination (SDK before MC).

**Not this scenario**: a rotation that should have happened and did not (pending age above
`mc_meeting_kek_rotation_overdue_threshold_seconds`, `MCKekRotationOverdue`) is
[Scenario 19](#scenario-19-kek-rotation-stalled), a page; key material missing at a client after a
rotation is [Scenario 16](#scenario-16-missing-key-material), rotation arm.

### Scenario 18: A Participant Hears Only Part of the Roster

**Alert**: none (see Why no alert)
**Severity**: triage on report
**Runbook Section**: `#scenario-18-a-participant-hears-only-part-of-the-roster`

**Symptom**: "I can hear some people but not others", or "I can't hear anyone", in a meeting where
everyone joined successfully, is on the roster, and no alert is firing. **Fork on the cause before
acting — their remedies are opposite.**

**The model, since story 2 task 20 (ADR-0036 §9).** Every participant is offered EVERY handler of
its meeting and connects to all it can. A subscriber hears a sender **if and only if the two share
at least one handler both are connected to**, where "connected" is what the HANDLERS report to MC
(`NotifyParticipantConnected`), never what a client claims. With everyone connected everywhere,
everyone hears everyone — so **a split is now a connectivity FAULT, not a design outcome.** (Task 6's
round-robin placement, where a two-person meeting could hear nothing by design, is gone.)

**Why no alert**: arm (b) is a correct steady state, and an unfilled slot is the steady state of any
meeting smaller than N. The connectivity arms are rare and per-participant; their population signal
is below, and any alert on it would need a duration shape — an operations decision routed to story 2
task 16 (alert inventory, `docs/user-stories/2026-09-21-hear-each-other.md`), which receives the
settle and unapplied signals.

**Unreachability is SERVER-SIDE EVIDENCE today.** A peer the reporter shares no connected handler
with is named in the reporter's `StreamAssignments.unreachable_sender_ids`. The client SDK records
that field, but **no UI renders it yet** (story 2 task 15 owns the rendering) — so the reporter sees
only silence, and **asking "does the peer show as unreachable?" cannot rule an arm in or out.** Use
the evidence below, not the reporter's screen.

**Step 0 — what is each participant connected to, and which handler carries each edge?** Two INFO
lines at `mc.actor.meeting`, keyed by participant (never by sender id; no urls). The **latest line
per participant is its current state**:

```bash
# Its observed handlers and phase (not_connected | establishing | settled):
kubectl logs deployment/mc-0 -n dark-tower --tail=5000 | grep "Participant media connectivity changed"
# How many of its slots each handler carries, e.g. in_edges="mh-...:2,mh-...:1":
kubectl logs deployment/mc-0 -n dark-tower --tail=5000 | grep "Participant in-edge handlers changed"
# Repeat for mc-1. Match participant_id to the reporter and to who they cannot hear.
```

**Population signal** — how many participants did NOT reach every handler (per connectivity episode,
so it IS a count; use it instead of `mc_media_unreachable_senders_total`, which is emission-weighted
and rises with churn):

```promql
sum(increase(mc_media_connect_settles_total{outcome="window_elapsed"}[1h]))
/ sum(increase(mc_media_connect_settles_total[1h]))
```

**Known-good, not a fault: the connect settle window.** MC withholds routing for a participant until
it has connected to every handler or `MC_MEDIA_CONNECT_SETTLE_MS` has passed since its first
connection (`mc_media_connect_settle_window_seconds`). A participant that reaches every handler pays
nothing; one that reaches only some **starts hearing and being heard up to one window later**. This
delays time-to-first-audio only — it never appears in `mc_session_join_duration_seconds` or
`MCHighJoinLatency`, which stop at the JoinResponse.

**(a) A participant is not connected to a handler it should be — a REAL connectivity fault.** Step 0
shows the reporter and the silent peer with **disjoint** connected sets, or one of them in
`not_connected`. Fork on what MH reported:

- **The client never reached the handler** (the handler is missing from its Step-0 set and MH shows
  no session for it): a client-to-MH path problem — the per-pod UDP NodePorts (4434/4436 in Kind),
  `infra/services/mh-service/network-policy.yaml`, the handler's certificate, or an unhealthy MH.
  Go to MH Scenario 1 and the network-policy/NodePort checks. `window_elapsed` rising is this arm.
- **MH reported a handler MC ignored**: `mc_mh_notifications_unapplied_total{reason="handler_not_in_set"}`
  is non-zero. **Sustained, it means an MH process restarted under a new id** (the id is
  per-incarnation) **and story-4 re-registration has not happened; the meeting's connectivity to
  the dead id is stale and will NOT clear on its own** — MC keeps routing edges to a handler that
  no longer holds them, so the slots still read `ACTIVE`. Remedy today: the affected participants
  rejoin (a fresh join re-dials every handler of the meeting's set); the durable fix is story 4.
- **A client's single MH transport dropped mid-session** (Step 0 shows a handler leaving the set
  while MC signalling stayed up): MC moved the edges correctly, but the SDK **does not re-dial a
  single dropped handler** — the participant stays narrowed to its surviving handlers until it
  rejoins. Pre-existing client limitation; remedy: reload.
- **Nothing reached MC at all** (every participant `not_connected`, `mc_mh_notifications_received_total`
  flat while joins continue): the MH→MC notification path is broken — MH Scenario 10.

**(b) Over-subscription — EXPECTED.** Reporter and silent peer share a connected handler, but the
reporter's slots are all full with earlier joiners: the N+2th sender gets no slot, is NOT in the
unreachable set ("no slot" is not "unreachable"), and the reporter's slots show `ACTIVE` for the
earliest joiners. **Remedy: raise the client's N** (`VITE_DT_RECEIVE_SLOTS`), within
`MC_MAX_RECEIVE_SLOTS` (published as `mc_media_receive_slot_cap`). A declaration above the cap is
rejected whole and counted as `slot_count_over_cap` — see the client media signalling section. A
current SDK refuses such an N itself (the cap is advertised on `JoinResponse.max_receive_slots`) and
counts it on `dt_client_media_receive_slots_rejected_total` instead, so a flat MC over-cap rate does
not rule this arm out.

**(c) MH refused the WHOLE snapshot on a policy bound — a REAL fault.** MC co-locates edges: in an
all-connected meeting **every** edge lands on ONE handler while the other carries none, and MC does
not yet know a handler's egress ceiling (capacity-aware spreading is deferred, `docs/TODO.md`
§Media Path Obligations). So a large meeting concentrates its whole egress on one handler and can
exceed a per-meeting or aggregate MH bound there. **WHICH handler is now spread per meeting** (the
tiebreak is rotated by a hash of the meeting id), so across many meetings the pods should be
roughly balanced — read each against its OWN ceiling, and treat sustained severe skew as worth
investigating rather than expected. What is NOT spread is one meeting's own egress: it is all on one
handler, and the idle sibling is **not** spare capacity for a meeting refused here — MC will not
move a placed edge there. MH then rejects the **entire** registration and
keeps the previous policy — this surfaces to MC as a **gRPC error** on the retry/terminal split
("RegisterMeeting retries exhausted" / "failed terminally" at `mc.register_meeting.trigger`),
**not** as a generation mismatch. Evidence on MH:

```promql
sum by (outcome) (increase(mh_media_policy_applies_total{outcome="rejected_invalid"}[15m]))
```

MH's WARN names which bound tripped in its `reason` field. **Remedy: topology/limits, not a
restart** — `mh-incident-response.md` (egress budget / policy bounds). Nothing re-pushes until the
next structural change.

**(c2) "Some peers hear ME, some don't" — the sender side.** A sender is directed at every handler
owning one of its edges, so this is now a first-class symptom. The client counts a frame it was
directed to send to a handler it holds **no** transport for as
`dt_client_media_send_dropped_total{reason="not_connected"}`. Split on the client's transports:

- **The client holds NO transport at all** → a client lifecycle bug (sending before any transport
  is up). Client team.
- **The client holds some transports, but not the one MC directed it at** → **MC's connectivity
  view is stale** — a server-side defect by construction, because MC only targets handlers owning
  the sender's edges and an edge exists only where MH reported both parties connected. This is the
  only signal today for a lost `NotifyParticipantDisconnected` (the server-side detector is deferred
  with story 4). Capture Step 0 for the sender and escalate to `meeting-controller`.

**(d) Applied-generation divergence** — the snapshot was accepted but not applied. Go to
[Scenario 15](#scenario-15-media-generation-divergence).

**(e) After an MC restart: "nobody hears anybody", with client-side `signature_invalid` rather
than silence.** This is the discriminator: arms (a)–(d) all present as *silence*; signature
failures mean frames ARE arriving and being REJECTED at the client. **Cause**: an MC restart
re-issued per-meeting sender ids from 1 (`crates/mc-service/src/media_admission/sender_id.rs`,
process memory) while MH still held the pre-restart edge table, so MH forwards under a table that
maps sender ids to the wrong people. (MH's routing table is install-only, and an MC restart can leave
it behind: as of story 2 task 12 MC releases a meeting's handlers with `EndMeeting` when the meeting
ends or empties, and best-effort on a graceful shutdown, but a crash releases nothing, and a shutdown
whose process exits before its teardown finishes releases only what it reached. This arm is about
exactly that remainder, so its premise still holds on a narrower footing.) **It fails closed** — the wrong audio is never played, it is rejected at the signature
check — so this is an **availability** event, not a confidentiality one; do not escalate it as media
crossing.

- **Expected resolution**: it self-clears on the first post-restart structural change, when MC
  adopts MH's echoed generation as a floor and re-pushes above it, replacing MH's table wholesale.
  **Where to see it**: `mc_media_policy_generation_adoptions_total{outcome="adopted"}` rises (at
  most one per live (meeting, handler)) and the WARN "adopting it as a floor" appears at
  `mc.register_meeting.trigger` — NOT a `generation_mismatch` on `mc_media_policy_pushes_total`,
  which by design stays clean for this arm. If it does NOT clear on the next structural change,
  adoption is not working (an ERROR "Could not adopt the handler's generation", recorded as
  `generation_mismatch`, pages `MCMediaGenerationDivergence`): capture MC's
  `mc.register_meeting.trigger` log lines and escalate to `meeting-controller`.
- **If MC was ROLLED BACK to a build without floor adoption**, this arm does not self-clear at all:
  see `mc-deployment.md` §Rollback, the one case where restarting MH IS the remedy.
- **REQUIRED STEP — every participant who was connected to a meeting on that MC before the restart
  RELOADS THE PAGE.** A client's media connection to MH outlives an MC restart (the SDK does not
  tear the MH transport down when MC signalling drops, and has no reconnect path), and MH bound that
  connection's sender id when it connected — so if the client's rejoin gets a DIFFERENT sender id,
  no policy push can ever correct that connection. A page reload is what drops the MH transport;
  "leave and click join again" may not. **Scope it to meetings on the restarted MC and tell ALL
  their pre-restart participants — do not try to find "the affected ones".** The affected set is
  unobservable from the client's presentation: the SDK renders no banner (the error is only
  stored), slots still claim `ACTIVE`, and silence looks exactly like arms (a)–(d). Participants may
  report "I can't hear anyone" — or nothing at all.
- **Do not restart MH to clear it.** It sheds every session on the pod, recovers no affected
  meeting, and destroys the evidence.

**(f) `mc_media_handler_set_divergence_total` is non-zero.** A join carried a handler set that
differs from the meeting's frozen set (Redis and the actor disagree). MC kept the frozen set, so no
edge moved. This is an MC-internal invariant violation: **capture and escalate** to
`meeting-controller`; restarting nothing fixes it.

**Escalation**: none for (b); for (a) per its fork (MH/network for an unreached handler; rejoin for
`handler_not_in_set`; MH Scenario 10 for a broken notification path); `media-handler` + operations
for (c); client team or `meeting-controller` for (c2) per its fork; per Scenario 15 for (d);
`meeting-controller` for (e) if it does not self-clear, and for (f).

### Scenario 19: KEK Rotation Stalled

**Alert**: `MCKekRotationOverdue`
**Severity**: Page
**Runbook Section**: `#scenario-19-kek-rotation-stalled`

**What this is.** A departed participant still holds a working meeting KEK, and the rotation that should
have revoked it has not happened. **This is a confidentiality exposure, not a media-quality problem**:
that departed participant can open every current participant's media in the meeting.

**The page is NOT a leading indicator.** It fires when the oldest un-rotated departure's age exceeds
twice W for 2 minutes — so when you are paged, the exposure bound W has **already been exceeded by
roughly 2x**. At the W ceiling (300 s) that is ~12 minutes after the departure.

**Step 1 — confirm the rule, don't re-derive it.** The page compares two gauges MC publishes; read both:

```promql
max(mc_meeting_kek_rotation_pending_age_seconds)            # oldest un-rotated departure, max across meetings
max(mc_meeting_kek_rotation_overdue_threshold_seconds)      # W x 2
max(mc_meeting_kek_rotation_window_seconds)                 # W as this pod loaded it
```

`mc_meeting_kek_rotation_window_seconds` must equal the ConfigMap's `MC_KEK_ROTATION_DEBOUNCE_SECONDS`
on every MC pod. A mismatch is a stale ConfigMap or a pod that did not roll (the MC ConfigMap does
not roll pods on edit) — and for W that is a security fact, since W is the exposure bound.

**Step 2 — fork on whether rotation is failing or not being attempted.**

```promql
sum by(reason) (increase(mc_meeting_kek_rotation_failures_total[15m]))
```

1. **`reason="rng"` rising — check this first; it is overwhelmingly the likelier arm.** The system
   CSPRNG failed. MC fails closed — no key change, no fallback RNG, no default key — and retries after
   W. The MC log carries `Meeting KEK rotation failed` at ERROR on `mc.kek.lifecycle`. Confirm the page
   clears after the next retry; if the CSPRNG keeps failing, the pod's entropy source is broken —
   restart the pod (the KEK dies with the actor, so every future joiner gets a new one) and escalate.
2. **`reason="generation_exhausted"` — permanent for that meeting, and effectively unreachable.** The
   meeting's `u16` KEK generation is at its ceiling (~22.7 days of uninterrupted rotation at the W
   floor, in one meeting that never restarted). MC will never rotate it again; **this page will not
   clear until that meeting ends.** End the meeting — participants rejoin into a new KEK.
3. **Neither is moving — the rotation is not being attempted.** Suspect a wedged meeting actor: read
   `mc_actor_mailbox_depth{actor_type="meeting"}` and MC Scenario 1. Pending age is sampled from times
   the actors recorded, so a wedged actor keeps ageing here even though it is doing nothing. A pod
   restart ends the exposure (the KEK dies with the actor) at the cost of every session on the pod.

**Not this scenario:** pushes that fail AFTER a successful rotation do not keep pending age up — it
clears at rotation. Undelivered pushes are `MCKekPushFailureRate`
([Scenario 16](#scenario-16-missing-key-material)).

---

### Scenario 20: Meeting Teardown Failing / MH Budget Ratchet

**Alerts**: `MCEndMeetingOwnershipRejected`, `MCEndMeetingFailureRate`, `MCPushQuiesceTimeouts`,
`MCTeardownFenceBackstop`, `MCNotifyMeetingEndedFailing`, `MCMeetingEndedNotificationsDropped`
(all warning; `infra/docker/prometheus/rules/mc-alerts.yaml`); `MHMediaEgressEdgeHeadroomLow`
(warning; `infra/docker/prometheus/rules/mh-alerts.yaml`).

> **Arriving from `MHMediaEgressEdgeHeadroomLow`? Fork LEAK vs LOAD first** (story 2 task 16). The
> alert reads installed edges against `mh_media_egress_stream_ceiling`, the admission bound that binds
> first.
>
> - **LEAK** is this scenario: `mh_media_egress_edges` climbs with the handler's uptime while
>   `mh_media_meeting_teardowns_total{outcome="released"}` stays flat, so ended meetings are not
>   being released. That includes the residual that no MC-side alert can see, an MC that never
>   called `EndMeeting` (for example after an MC crash). On a long-lived pod this is a true
>   positive. Do not tune the alert's fraction up to silence it.
> - **LOAD** is not this scenario: the edges track `mh_media_registered_meetings` and real demand.
>   Teardown is healthy and the remedy is capacity or placement (more handlers, or rebalancing),
>   not anything below.

**What changed (story 2 task 12, R-20).** A meeting now ENDS when its last roster participant is
removed (a clean leave, or a disconnect whose grace expired). MC then, off the meeting actor's
path: stops every policy push for the meeting and waits for each to return (the quiesce); sends
`EndMeeting` with its own `mc_id` to every handler the meeting was programmed on; and, only after
that, tells GC the meeting ended (`NotifyMeetingEnded`), so GC's next join for that id is a NEW
assignment. A graceful MC shutdown releases its meetings' handlers the same way, inside a window
derived from the pod's termination grace — read the effective value off this pod's startup line as
`shutdown_release_budget_seconds` rather than recomputing it (`teardown::SHUTDOWN_RELEASE_BUDGET`
= grace − pre-drain sleep − margin, drift-tested against both manifests) — but does NOT tell GC (the meeting is not over; a successor MC may take it). A shutdown
teardown still running at that deadline — in practice an unreachable handler — is cut off by process
exit and reported on the pod's last ERROR line as `teardowns_cut_off=<n>`; each is one more
registration the handler keeps until its restart. While a meeting id is being
torn down MC holds a **fence** on it: a create for that id is parked and answered when the teardown
completes (up to `MAX_QUEUED_CREATES` parked, in `crates/mc-service/src/actors/controller.rs`; beyond that the join fails with
`mc_session_join_failures_total{error_type="teardown_in_progress"}`).

**Why it matters.** A handler that never receives `EndMeeting` keeps the meeting's registration,
routes and edge budget until it restarts. Since R-21 that consumes the ENFORCED
`MH_MAX_REGISTERED_MEETINGS`, so the cost is progressive denial of NEW meetings on that handler,
with no client-visible cause until then. A notify GC never received leaves GC reusing the ended
meeting's assignment, and joins to that id fail with `meeting_not_found`.

**The bounds — read them off the pod, never compute them from this page.** MC's
`Configuration loaded successfully` startup line carries three structured fields
(`mc-deployment.md` §5):

- `push_quiesce_bound_seconds` — how long the quiesce waits for one in-flight `RegisterMeeting`
  attempt (`MH_CONNECT_TIMEOUT + MH_RPC_TIMEOUT + QUIESCE_SLACK`).
- `teardown_fence_hold_max_seconds` — the worst case one teardown can take: a quiesce that times
  out, the equal further wait after it, then every `EndMeeting` attempt at its full deadline with the
  backoff between them. This is how long a rejoin of the same meeting id can be held.
- `shutdown_release_budget_seconds` — how long a graceful shutdown lets its meetings' releases
  run before process exit cuts them off, derived from the pod's `terminationGracePeriodSeconds`.
  A teardown still running at that deadline is reported as `teardowns_cut_off=<n>` on the pod's
  last ERROR line and leaks its registration until that handler restarts.

```bash
kubectl logs -n dark-tower deployment/mc-0 | grep "Configuration loaded successfully" | tail -1
```

**Symptom that gets reported as a join outage.** "Joins to a meeting that just emptied fail or
hang." A rejoin DURING teardown finds GC's assignment still live (the notify is sent after
teardown), so it reaches MC and gets `meeting_not_found`, for up to
`teardown_fence_hold_max_seconds` in the worst case (normally milliseconds). A create that reaches
MC by another route is parked behind the fence instead. **The remedy is MH reachability, not the
join path**: a long teardown is a slow or unreachable handler. `mc_media_push_quiesce_total{outcome="timed_out"}`
rising alongside `mc_session_join_failures_total{error_type="teardown_in_progress"}` IS "the fence
is wedged".

**Triage — split on the counter that moved:**

```promql
sum by (outcome) (increase(mc_media_end_meeting_total[15m]))       # one per (meeting, handler)
sum by (outcome) (increase(mc_media_push_quiesce_total[15m]))      # one per meeting teardown
sum(increase(mc_media_teardown_fence_backstop_total[30m]))
sum by (status) (increase(mc_gc_notify_meeting_ended_total[15m]))
sum(increase(mc_gc_meeting_ended_notifications_dropped_total[15m]))
```

| Moved | Meaning | First move |
|---|---|---|
| `mc_media_end_meeting_total{outcome="unimplemented"}` | MH predates the RPC | Rollout order was reversed: MH rolls forward first, MC rolls back first (`mc-deployment.md` §Coordination). Counted once, never retried; the meeting is held by that MH until it restarts |
| `{outcome="unavailable_exhausted"}` | MH unreachable; the bounded retries ran out | Fix MH reachability. The meetings already lost leak exactly like the crash path |
| `{outcome="rejected_ownership"}` | MH says another MC owns a meeting that ENDED (`FAILED_PRECONDITION`) | MC sends only its own `mc_id`, so this is an MC defect or a stale/misrouted MC. Recovery order: `mh-incident-response.md` ownership-reject arm |
| `{outcome="superseded_by_successor"}` | the same refusal on a graceful-shutdown release | **Expected during a rolling MC deploy**: a successor already took the meeting over, and MH refused a release that would have cut it. Consistent with supersession, not proof of it — rising OUTSIDE a rollout window is worth a look. No alert. MH's `rejected_ownership` is the UNION of this value and the row above: compare against their sum, never token to token |
| `{outcome="invalid_argument"}` or `{outcome="error"}` | MH refused the request shape, or an unclassified failure | MC defect; the `mc.teardown` ERROR line carries the gRPC code and message. Escalate to `meeting-controller` |
| `mc_media_push_quiesce_total{outcome="timed_out"}` | a push worker was still mid-`RegisterMeeting` at the bound; MC waited one more window, then released anyway | A queued late apply is refused MH-side (`mh_media_released_meeting_apply_refusals_total`); a registration still in flight is only ORDERED by the wait, which fails open. Confirm against the handler's `mh_media_registered_meetings`, and read `mh-incident-response.md` for the two races |
| `mc_media_teardown_fence_backstop_total` | the fence was lifted by its deadline, not by the teardown's report: the teardown task hung or died without unwinding. GC is still told a meeting that ENDED has ended (the cause is recorded at reap), so the id stays joinable; if joins to it return `meeting_not_found`, check `mc_gc_notify_meeting_ended_total{status="error"}` | Escalate on a repeat — every firing is also a meeting whose `EndMeeting` may never have gone out. **Not mutually exclusive with `timed_out`**: backstop WITHOUT a matching `timed_out` is a different fault (hung outside the drain) from the two together |
| `mc_gc_notify_meeting_ended_total{status="error"}` | GC did not take the notify | Sent at most once for anything GC may have processed, so the error is final. GC keeps the row live; joins to that id get `meeting_not_found` until MC restarts or is marked unhealthy. Check GC health (`gc-incident-response.md`) |
| `mc_gc_meeting_ended_notifications_dropped_total` | MC's notify queue was full; the end was never reported | Expected-empty: MC is losing events, not GC being down. Escalate to `meeting-controller` |

**Logs.** `mc.teardown` carries one INFO per released meeting ("Meeting released on its media
handlers", with `released`, `failed` and `quiesce_timed_out`) and one ERROR per failed handler
naming the outcome and gRPC code. `mc.grpc.gc_client` carries the notify result.

**What these alerts cannot see.** They fire only for teardowns MC ATTEMPTED. A meeting whose MC
never completes `EndMeeting` — a crash, a kill mid-teardown, or a rollback to a build without
teardown — is an ABSENT event. An edge-holding one reaches `MHMediaEgressEdgeHeadroomLow` (LEAK);
a zero-edge registration has no rule (`docs/TODO.md` §Observability Debt, "No leading indicator for
registered-meeting exhaustion"), so read `mh_media_registered_meetings` against `mh_media_registered_meetings_limit` on
each handler: rising with pod uptime while `mh_media_meeting_teardowns_total{outcome="released"}`
stays flat is that residual (`docs/TODO.md`, "A meeting whose MC never sends `EndMeeting` is never
reclaimed").

**A mass teardown is a connection burst, not a throughput problem.** MC opens a fresh gRPC channel per MH call, so N meetings ending at once means up to N x 2 concurrent TCP/TLS handshakes (and up to twice that when quiesce times out) against at most two endpoints. Measured `nofile` on the MC pod is 1,048,576, so this is not near exhaustion — **but if a `nofile` limit is ever set low on the MC container, descriptor exhaustion would be process-global and would take the DB pool, Redis and client sessions down for healthy meetings on the same pod.** If a teardown storm coincides with unrelated MC failures, check `ulimit -n` inside the pod first:

```bash
kubectl exec -n dark-tower deployment/mc-0 -- sh -c 'ulimit -n'   # expect ~1048576
```

The named remedy is channel REUSE (one channel per endpoint, multiplexed), not a teardown concurrency cap — a cap would lengthen the fence and the rejoin hold without lowering the push-side peak (`docs/TODO.md`, "MC opens a fresh gRPC channel per MH call").

**Do not** restart MC to "clear" a teardown: a crash-like exit is exactly the path that releases
nothing. **Do not** restart MH unless the registered-meetings check says the handler is at its
limit — it sheds every live media session on the pod.

**A split meeting.** If participants of one meeting id end up on two MCs, suspect GC failover
(`atomic_assign`'s unhealthy/stale-MC arm re-pointing the assignment to another MC on a join) racing the old MC's in-flight notify — the notify is not
incarnation-safe (`docs/TODO.md`, "`NotifyMeetingEnded` is not incarnation-safe"). See
`gc-incident-response.md` beside the `MCNotifyMeetingEndedFailing` triage.

**Escalation**: `meeting-controller` for `rejected_ownership` (and `superseded_by_successor` outside a rollout), `invalid_argument`, `error`, the
backstop and dropped notifications; `media-handler` for `unimplemented` (rollout order) and
`unavailable_exhausted` (reachability); `global-controller` for notify errors that persist while GC
is healthy.

---

### Scenario 21: Server Mute Not Enforced

**The symptom**: "The host muted someone and we can still hear them", or the reverse, "I was
unmuted by the host and nobody can hear me".

**How server mute works (story 2 task 12, R-8..R-11).** Only a HOST (the meeting creator's
`MeetingRole::Host` claim) may server-mute. MC records the mute, broadcasts who-muted-whom to every
participant (including the muted one, and replays it to late joiners), and programs the muted
participant's sender id into `server_muted_sources` on the next registration snapshot to every
handler carrying that sender's edges — a change to the muted set alone advances
`policy_generation`. MH drops the muted sender's frames at ingress. A participant's
`UnmuteRequest` is only RELAYED to the host; it never lifts the mute. A server mute survives a
reconnect within grace, and does not survive a fresh join.

**Step 1 — did the host's request land?**

```promql
sum by (action, outcome) (increase(mc_media_server_mute_requests_total[15m]))
```

`applied` is the success. `unchanged` is a repeat of the current state (no push). `not_permitted`
means the requester was not a host — refused BEFORE the target was looked at, so a non-host learns
nothing. `unknown_target` (host only) means the named participant is not in the meeting.
`rate_limited` is the per-connection bound. The `mc.webtransport.connection` INFO line
"Server mute decision" names requester, target, action and outcome for each decided request —
the meeting-scoped record. A refusal reaches the requester as one generic `Forbidden` error (`not_permitted` and
`unknown_target` are byte-identical on the wire by design; the first-per-connection WARN "Server-mute
request refused" tells them apart). Refusal replies are rate-limited, and a suppressed reply is counted
on `mc_media_refusal_replies_suppressed_total{surface="server_mute"}`, so a looping client may see no
error at all.

**Step 2 — does MC believe a mute is in force?** `mc_media_server_muted_sources` is the number of
participants MC holds server-muted, summed across this pod's meetings. It is identity-free by
design (ADR-0036 §11): it answers "is any mute in force on this MC pod", never "in THIS meeting";
use the Step 1 log line for that.

**Step 3 — is MH dropping?** `mh_media_frames_dropped_total{reason="server_muted"}` on the handlers.
Compare **fleet-aggregate and directionally only**: `sum(mc_media_server_muted_sources)` across MC
instances against `sum(rate(mh_media_frames_dropped_total{reason="server_muted"}[5m]))` across MH
instances. Never divide a level by a rate, and never compare per instance (MC and MH meeting sets
do not nest). A gauge above zero with a flat drop rate is only a candidate disagreement, and only
after excluding a muted source that is silent, disconnected, in grace or not yet on that handler —
a drop counter reads zero for all of those.

**Step 4 — did the snapshot reach MH?** A mute that landed in MC but not at MH is a push problem:
`mc_media_policy_pushes_total{outcome}` and Scenario 12 / Scenario 15. Server mute rides the same
snapshot as every other policy change, so a push that is failing fails for the mute too.

**Not a mute problem**: a participant who muted THEMSELF (client mute, enforced at capture, ADR-0036
§5) — `mc_media_mute_requests_total`. `slot_state="source_muted"` covers both self and server mute
by design, so it cannot tell them apart; the `ParticipantMuteUpdate` a client receives can.

**Escalation**: the routing for "applied at MC, confirmed, MH still forwards" (MH defect) and "applied
at MC, not confirmed" (push path) is decided in one place, MH Scenario 19 Arms 3 and 4. Route from
there.

**MH-side arms, including the ownership gap** (a registration MC did not send overwriting the muted
set): [`mh-incident-response.md` Scenario 19](mh-incident-response.md#scenario-19-server-mute-not-taking-effect-at-ingress).

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
