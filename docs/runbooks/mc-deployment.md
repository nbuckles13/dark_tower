# MC Service Deployment Runbook

**Service**: Meeting Controller (mc-service)
**Version**: Phase 4a (ADR-0010 Implementation)
**Last Updated**: 2026-02-09
**Owner**: Operations Team

---

## Overview

This runbook covers deployment, rollback, and troubleshooting procedures for the Meeting Controller service. The MC service is responsible for WebTransport signaling, session management, and participant coordination within meetings.

**Critical Service**: MC downtime affects all active meetings. Users will be disconnected and unable to communicate. Follow pre-deployment checklist carefully.

---

## Table of Contents

1. [Pre-Deployment Checklist](#pre-deployment-checklist)
2. [Deployment Steps](#deployment-steps)
3. [Rollback Procedure](#rollback-procedure)
4. [Configuration Reference](#configuration-reference)
5. [Common Deployment Issues](#common-deployment-issues)
6. [Smoke Tests](#smoke-tests)
7. [Monitoring and Verification](#monitoring-and-verification)

---

## Pre-Deployment Checklist

Complete ALL items before deploying to production:

### Code Quality

- [ ] **Code review approved** by at least one reviewer
- [ ] **CI tests passing** in GitHub Actions
  ```bash
  # Verify CI status
  gh pr checks <PR-NUMBER>
  ```
- [ ] **Test coverage meets minimum** (targeting 90%+ for critical paths)
  ```bash
  # Check coverage report
  cargo llvm-cov --workspace --lcov --output-path lcov.info
  ```
- [ ] **Linting passes** with zero warnings
  ```bash
  cargo clippy --workspace --lib --bins -- -D warnings
  ```
- [ ] **Security scan passes** (Trivy, no CRITICAL vulnerabilities)
  ```bash
  trivy image mc-service:latest --severity CRITICAL
  ```

### Infrastructure

- [ ] **Config changes documented** in this runbook or ADR
  - New environment variables added to ConfigMap/Secret
  - Default values appropriate for production
  - Breaking changes communicated to dependent services (GC, MH)

- [ ] **Rollback plan confirmed**
  - Previous container image available in registry
  - Rollback criteria defined (see [Rollback Procedure](#rollback-procedure))

- [ ] **Capacity planning verified**
  - Current resource utilization <70% (CPU/memory)
  - Active meeting count within capacity limits
  - GC has awareness of MC capacity

- [ ] **GC coordination confirmed**
  - GC informed of maintenance window (if applicable)
  - GC can route new meetings to other MC instances
  - Draining period sufficient for active meetings to complete

### Coordination

- [ ] **Maintenance window scheduled** (if downtime expected)
  - Dependent services notified (GC, MH)
  - Users notified if user-facing impact
  - On-call engineer available

- [ ] **Runbook reviewed** by deployment engineer
  - All steps understood
  - Required access verified (kubectl, monitoring)

---

## Deployment Steps

### 1. Pre-Deployment Verification

**Verify current state:**

```bash
# Check current deployment status
kubectl get deployment meeting-controller -n dark-tower

# Check current pod status
kubectl get pods -n dark-tower -l app=meeting-controller

# Check current resource utilization
kubectl top pods -n dark-tower -l app=meeting-controller

# Check active meetings (critical - do not deploy if high)
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &
curl -s http://localhost:8080/metrics | grep mc_meetings_active
kill %1

# Verify readiness of current pods
kubectl get pods -n dark-tower -l app=meeting-controller -o json | jq '.items[].status.conditions[] | select(.type=="Ready")'
```

**Expected output:**
- Deployment shows desired replicas (2+)
- All pods in Running state
- All pods Ready=True
- CPU <70%, Memory <70%
- Active meetings at manageable level (ideally <50 per pod)

### 2. Initiate Graceful Drain (Optional, for zero-impact deployments)

**For critical deployments with active meetings:**

```bash
# Mark MC as draining in GC (stops new meeting assignments)
# This is done via GC admin API or database update
kubectl exec -it deployment/global-controller -n dark-tower -- \
  psql $DATABASE_URL -c "UPDATE meeting_controllers SET status = 'draining' WHERE id = '<MC_ID>';"

# Wait for active meetings to decrease
watch -n 10 'kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 2>/dev/null & sleep 1; curl -s http://localhost:8080/metrics | grep mc_meetings_active; kill %1 2>/dev/null'

# Proceed when active meetings reach acceptable level (e.g., <10)
```

### 3. Update Container Image

**Option A: Using kubectl (direct deployment)**

```bash
# Set new image version
export NEW_VERSION="v1.2.3"  # Replace with actual version tag

# Update Deployment with new image
kubectl set image deployment/meeting-controller \
  meeting-controller=mc-service:${NEW_VERSION} \
  -n dark-tower

# Verify image updated
kubectl describe deployment meeting-controller -n dark-tower | grep Image:
```

**Option B: Using kubectl apply (declarative)**

```bash
# Update infra/services/meeting-controller/deployment.yaml
# Change image tag: mc-service:latest → mc-service:v1.2.3

# Apply updated manifest
kubectl apply -f infra/services/meeting-controller/deployment.yaml

# Verify change
kubectl describe deployment meeting-controller -n dark-tower | grep Image:
```

**Option C: Using Skaffold (development)**

```bash
# Build and deploy with Skaffold
skaffold run -p meeting-controller
```

### 4. Rolling Update Monitoring

Deployments update pods via rolling strategy (maxSurge=1, maxUnavailable=0 for zero-downtime).

**Monitor rollout:**

```bash
# Watch pod status (Ctrl+C to exit)
kubectl get pods -n dark-tower -l app=meeting-controller -w

# Check rollout status
kubectl rollout status deployment/meeting-controller -n dark-tower

# Monitor logs from new pod
kubectl logs -f deployment/meeting-controller -n dark-tower
```

**Expected sequence:**
1. New pod created (beyond current replica count)
2. New pod starts and becomes Ready (health check + readiness check pass)
3. Old pod marked for termination
4. Old pod drains connections gracefully (SIGTERM, 60s drain period)
5. Old pod terminates
6. Repeat for all pods

**Typical timeline:**
- Pod startup: 10-20 seconds (GC registration + actor system init)
- Pod termination: 60-90 seconds (graceful connection drain)
- Total per pod: ~90 seconds
- **Total rollout: ~5-6 minutes for 3 replicas**

### 5. Verify Deployment Success

**Pod health:**

```bash
# All pods Running and Ready
kubectl get pods -n dark-tower -l app=meeting-controller

# Check pod events for errors
kubectl get events -n dark-tower --field-selector involvedObject.kind=Pod,involvedObject.name=meeting-controller-<pod-suffix>
```

**Logs review:**

```bash
# Check for startup errors
kubectl logs deployment/meeting-controller -n dark-tower --tail=50

# Look for error patterns
kubectl logs -n dark-tower -l app=meeting-controller --tail=100 | grep -i "error\|panic\|fatal"
```

**Expected log messages:**
```
Starting Meeting Controller
Configuration loaded successfully
Registering with Global Controller...
GC registration successful
Actor system initialized
WebTransport listener started on 0.0.0.0:4433
Prometheus metrics recorder initialized
Meeting Controller ready
```

### 6. Run Smoke Tests

**See [Smoke Tests](#smoke-tests) section below for detailed test procedures.**

Minimum required smoke tests:
- [ ] Health check returns 200 OK
- [ ] Readiness check returns 200 OK
- [ ] Metrics endpoint returns Prometheus format
- [ ] GC heartbeat succeeding

### 7. Verify GC Registration

**Confirm MC is registered and receiving assignments:**

```bash
# Check MC registration in GC database
kubectl exec -it deployment/global-controller -n dark-tower -- \
  psql $DATABASE_URL -c "SELECT id, region, capacity, current_sessions, last_heartbeat, status FROM meeting_controllers WHERE last_heartbeat > NOW() - INTERVAL '30 seconds' ORDER BY last_heartbeat DESC;"

# Verify MC is marked as healthy
# status should be 'active', last_heartbeat should be recent
```

### 8. Monitor Metrics

**Verify metrics collection:**

```bash
# Port-forward to access metrics endpoint
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &

# Fetch metrics
curl http://localhost:8080/metrics

# Kill port-forward
kill %1
```

**Check key metrics:**
- `mc_meetings_active` - Active meeting count
- `mc_connections_active` - Active WebTransport connections
- `mc_actor_mailbox_depth` - Actor mailbox depth (should be low)
- `mc_message_processing_duration_seconds` - Message latency (p95 <500ms SLO)

### 9. Post-Deployment Checklist

- [ ] All pods Running and Ready
- [ ] Smoke tests pass (health, ready, metrics)
- [ ] No errors in logs (last 5 minutes)
- [ ] Metrics available in Prometheus
- [ ] GC heartbeat succeeding
- [ ] GC has MC marked as 'active'
- [ ] Actor mailbox depths low (<50)
- [ ] Message processing latency within SLO (<500ms p95)
- [ ] No actor panics in metrics

---

## Rollback Procedure

### When to Rollback

**Immediate rollback criteria** (do not wait):

1. **Pod startup failures**
   - Pods stuck in CrashLoopBackOff >2 minutes
   - Pods failing readiness checks consistently
   - GC registration failures
   - Actor system initialization failures

2. **Critical functionality broken**
   - Actor panics occurring
   - High message drop rate (>1%)
   - WebTransport connections failing to establish
   - GC heartbeat failures

3. **Severe performance degradation**
   - p95 message latency >1s (2x SLO)
   - Mailbox depth critical (>500)
   - Memory usage >90%

4. **Security issues discovered**
   - Vulnerability in new code
   - Authorization failures
   - Token validation bypass

**Monitoring period before declaring success:**
- Minimum: 15 minutes post-deployment
- Recommended: 1 hour for major changes
- Critical changes: 24 hours with on-call monitoring

### How to Rollback

**Step 1: Identify previous version**

```bash
# Find previous image version
kubectl rollout history deployment/meeting-controller -n dark-tower

# Get image from previous revision
kubectl rollout history deployment/meeting-controller -n dark-tower --revision=<PREVIOUS_REVISION>
```

**Step 2: Rollback Deployment**

```bash
# Rollback to previous revision
kubectl rollout undo deployment/meeting-controller -n dark-tower

# Or rollback to specific revision
kubectl rollout undo deployment/meeting-controller -n dark-tower --to-revision=<REVISION>

# Monitor rollback
kubectl rollout status deployment/meeting-controller -n dark-tower
```

**Step 3: Verify rollback success**

```bash
# Check pods running previous version
kubectl get pods -n dark-tower -l app=meeting-controller -o jsonpath='{.items[*].spec.containers[0].image}'

# Run smoke tests (see Smoke Tests section)
# Verify health, ready, metrics endpoints
```

**Step 4: Re-enable in GC (if drained)**

```bash
# If MC was in draining status, re-enable
kubectl exec -it deployment/global-controller -n dark-tower -- \
  psql $DATABASE_URL -c "UPDATE meeting_controllers SET status = 'active' WHERE id = '<MC_ID>';"
```

**Step 5: Post-rollback verification**

- [ ] All pods running previous image version
- [ ] Smoke tests pass
- [ ] Actor mailbox depths normal
- [ ] Message latency within SLO
- [ ] No errors in logs
- [ ] GC heartbeat succeeding

**Step 6: Incident retrospective**

- Document rollback reason
- Create incident report
- Identify root cause
- Update pre-deployment checklist if needed

---

## Configuration Reference

### Environment Variables

| Variable | Required | Description | Default | Example |
|----------|----------|-------------|---------|---------|
| `GC_REGISTRATION_URL` | **Yes** | Global Controller registration endpoint | None | `http://global-controller.dark-tower.svc.cluster.local:8080/api/v1/mc/register` |
| `MC_REGION` | **Yes** | Geographic region for this MC | None | `us-west-2` |
| `MC_CAPACITY` | No | Maximum concurrent meetings | `100` | `100` |
| `WEBTRANSPORT_BIND_ADDRESS` | No | WebTransport bind address | `0.0.0.0:4433` | `0.0.0.0:4433` |
| `HTTP_BIND_ADDRESS` | No | HTTP/metrics bind address | `0.0.0.0:8080` | `0.0.0.0:8080` |
| `ACTOR_MAILBOX_SIZE` | No | Default actor mailbox capacity | `1000` | `1000` |
| `GC_HEARTBEAT_INTERVAL_SECS` | No | Heartbeat interval to GC | `10` | `10` |
| `RUST_LOG` | No | Logging level | `info` | `info,mc_service=debug` |

### Kubernetes Secrets

**Secret: `mc-service-secrets`** (namespace: `dark-tower`)

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: mc-service-secrets
  namespace: dark-tower
type: Opaque
data:
  TLS_CERT: <base64-encoded-cert>
  TLS_KEY: <base64-encoded-key>
```

### Kubernetes ConfigMap

**ConfigMap: `mc-service-config`** (namespace: `dark-tower`)

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: mc-service-config
  namespace: dark-tower
data:
  GC_REGISTRATION_URL: "http://global-controller.dark-tower.svc.cluster.local:8080/api/v1/mc/register"
  MC_REGION: "us-west-2"
  MC_CAPACITY: "100"
  GC_HEARTBEAT_INTERVAL_SECS: "10"
```

### Resource Limits

**Current configuration** (from `deployment.yaml`):

```yaml
resources:
  requests:
    cpu: 500m      # 0.5 CPU cores
    memory: 512Mi  # 512 MiB RAM
  limits:
    cpu: 2000m     # 2 CPU cores
    memory: 1Gi    # 1 GiB RAM
```

**Tuning guidance:**
- **requests**: Guaranteed resources, used for scheduling
- **limits**: Maximum resources, pod killed if exceeded
- MC is more CPU-intensive than memory-intensive (message processing)
- Scale horizontally for capacity, not vertically

---

## Common Deployment Issues

### Issue 1: GC Registration Failures

**Symptoms:**
- Pods not reaching Ready state
- Logs show: `Failed to register with GC`, `GC unreachable`
- MC not appearing in GC database

**Causes:**
- GC service not running or not healthy
- `GC_REGISTRATION_URL` incorrect in ConfigMap
- NetworkPolicy blocking MC → GC traffic
- GC rejecting registration (capacity, region mismatch)

**Resolution:**

```bash
# Check GC service is running
kubectl get pods -n dark-tower -l app=global-controller

# Test GC endpoint directly from MC pod
kubectl exec -it deployment/meeting-controller -n dark-tower -- \
  curl -i $GC_REGISTRATION_URL

# Check MC logs for registration errors
kubectl logs deployment/meeting-controller -n dark-tower --tail=100 | grep -i "register\|gc"

# Verify GC_REGISTRATION_URL in ConfigMap
kubectl get configmap mc-service-config -n dark-tower -o yaml | grep GC_REGISTRATION_URL
```

**Fix:**
1. Ensure GC service is running and ready
2. Correct `GC_REGISTRATION_URL` in ConfigMap
3. Adjust NetworkPolicy to allow MC → GC traffic (TCP:8080)

### Issue 2: WebTransport Listener Failures

**Symptoms:**
- Pods failing readiness checks
- Logs show: `Failed to bind WebTransport listener`, `Address already in use`
- No WebTransport connections possible

**Causes:**
- Port conflict on host
- TLS certificate/key not mounted
- Invalid TLS configuration
- Previous pod not fully terminated

**Resolution:**

```bash
# Check for port conflicts
kubectl get pods -n dark-tower -l app=meeting-controller -o wide
# Multiple pods on same node may conflict

# Check TLS secrets are mounted
kubectl describe pod <mc-pod> -n dark-tower | grep -A 5 "Mounts:"

# Check logs for TLS errors
kubectl logs deployment/meeting-controller -n dark-tower --tail=100 | grep -i "tls\|cert\|webtransport"
```

**Fix:**
1. Verify TLS secrets exist and are mounted
2. Check pod anti-affinity rules (avoid port conflicts)
3. Wait for old pods to fully terminate before redeployment

### Issue 3: Actor System Initialization Failures

**Symptoms:**
- Pods crash on startup
- Logs show: `Actor system failed to initialize`, `Panic in actor`
- CrashLoopBackOff state

**Causes:**
- Configuration errors
- Resource exhaustion (file descriptors, memory)
- Bug in actor initialization code

**Resolution:**

```bash
# Check previous pod logs (before crash)
kubectl logs <mc-pod> -n dark-tower --previous

# Check resource limits
kubectl describe pod <mc-pod> -n dark-tower | grep -A 5 "Limits:"

# Check events
kubectl get events -n dark-tower --field-selector involvedObject.name=<mc-pod>
```

**Fix:**
1. Review actor configuration
2. Increase resource limits if needed
3. Rollback to previous version if bug introduced

### Issue 4: High Memory Usage After Deployment

**Symptoms:**
- Memory usage growing rapidly after deployment
- Pods approaching memory limit
- Potential OOMKilled events

**Causes:**
- Memory leak in new code
- Increased connection/meeting load
- Actor mailbox buildup

**Resolution:**

```bash
# Monitor memory usage
kubectl top pods -n dark-tower -l app=meeting-controller

# Check for mailbox buildup
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &
curl http://localhost:8080/metrics | grep mc_actor_mailbox_depth
kill %1

# Check connection count (each connection uses memory)
curl http://localhost:8080/metrics | grep mc_connections_active
```

**Fix:**
1. If mailbox buildup: investigate slow message processing
2. If connections high: verify capacity limits
3. If memory leak: rollback and investigate

---

## Smoke Tests

Run these tests immediately after deployment to verify core functionality.

### Test 1: Health Check (Liveness)

**Purpose:** Verify process is running and responsive.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &

# Test health endpoint
curl -i http://localhost:8080/health

# Expected response:
# HTTP/1.1 200 OK
# Content-Length: 2
#
# OK

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- Response body: `OK`
- Response time: <100ms

### Test 2: Readiness Check

**Purpose:** Verify actor system ready and GC registered.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &

# Test readiness endpoint
curl -i http://localhost:8080/ready

# Expected response:
# HTTP/1.1 200 OK
# Content-Type: application/json
#
# {"status":"ready","gc_registered":true,"actor_system":"healthy"}

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- JSON body with `status: "ready"`
- `gc_registered: true`
- `actor_system: "healthy"`
- Response time: <500ms

### Test 3: Metrics Endpoint

**Purpose:** Verify Prometheus metrics are exposed.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower deployment/meeting-controller 8080:8080 &

# Fetch metrics
curl -s http://localhost:8080/metrics | head -50

# Expected output (Prometheus text format):
# # HELP mc_meetings_active Number of active meetings
# # TYPE mc_meetings_active gauge
# mc_meetings_active 0
# # HELP mc_connections_active Number of active WebTransport connections
# # TYPE mc_connections_active gauge
# mc_connections_active 0
# ...

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- Prometheus text format
- Metrics present: `mc_meetings_active`, `mc_connections_active`, `mc_actor_mailbox_depth`
- Response time: <1s

### Test 4: GC Heartbeat Verification

**Purpose:** Verify MC is registered with GC and heartbeats are succeeding.

```bash
# Check GC database for MC registration
kubectl exec -it deployment/global-controller -n dark-tower -- \
  psql $DATABASE_URL -c "SELECT id, region, capacity, current_sessions, last_heartbeat, status FROM meeting_controllers ORDER BY last_heartbeat DESC LIMIT 5;"

# Expected: MC entry with:
# - status = 'active'
# - last_heartbeat within last 15 seconds
# - capacity and current_sessions reasonable
```

**Success criteria:**
- MC appears in GC database
- Status is 'active'
- last_heartbeat is recent (<15 seconds old)

---

## Monitoring and Verification

### Key Metrics to Monitor Post-Deployment

**Service health:**

```promql
# Pod restart count (should be 0 after initial deployment)
kube_pod_container_status_restarts_total{namespace="dark-tower",pod=~"meeting-controller-.*"}

# Pod readiness (should be 1 for all pods)
kube_pod_status_ready{namespace="dark-tower",pod=~"meeting-controller-.*"}
```

**Actor system health:**

```promql
# Mailbox depth by actor type (should be low, <50)
sum by(actor_type) (mc_actor_mailbox_depth)

# Actor panics (should be 0)
sum(increase(mc_actor_panics_total[5m]))

# Message drop rate (should be near 0%)
sum(rate(mc_messages_dropped_total[5m])) / (sum(rate(mc_messages_dropped_total[5m])) + sum(rate(mc_message_processing_duration_seconds_count[5m])))
```

**Latency:**

```promql
# p95 message processing latency (SLO: <500ms)
histogram_quantile(0.95, sum by(le) (rate(mc_message_processing_duration_seconds_bucket[5m])))
```

**Capacity:**

```promql
# Active meetings per pod
sum(mc_meetings_active)

# Active connections per pod
sum(mc_connections_active)
```

**GC integration:**

```promql
# Heartbeat success rate (should be near 100%)
sum(rate(mc_gc_heartbeat_total{status="success"}[5m])) / sum(rate(mc_gc_heartbeat_total[5m]))
```

### Grafana Dashboards

**Recommended dashboards:**
- **MC Overview** - Active meetings, connections, mailbox depth, panics
- **MC SLOs** - Message latency, drop rate, error budget

See `infra/grafana/dashboards/mc-overview.json`.

### Alerting Rules

**Critical alerts (page on-call):**
- `MCDown` - No MC pods running for >1 minute
- `MCActorPanic` - Any actor panic
- `MCHighMailboxDepthCritical` - Mailbox depth >500 for >2 minutes
- `MCHighLatency` - p95 latency >500ms for >5 minutes
- `MCHighMessageDropRate` - Drop rate >1% for >5 minutes

See `infra/docker/prometheus/rules/mc-alerts.yaml` for full list.

---

## Emergency Contacts

**On-Call Rotation:** See PagerDuty schedule

**Escalation:**
1. **L1:** On-call SRE (PagerDuty)
2. **L2:** MC service owner / Backend team lead
3. **L3:** Infrastructure architect

**Related Teams:**
- **GC Team:** For registration and heartbeat issues
- **Infrastructure Team:** For Kubernetes, network, resource issues
- **Media Handler Team:** For MC → MH connectivity issues

---

## References

- **ADR-0010:** Global Controller Architecture (MC registration)
- **ADR-0011:** Observability Framework
- **ADR-0012:** Infrastructure Architecture
- **Source Code:** `crates/meeting-controller/`
- **Kubernetes Manifests:** `infra/services/meeting-controller/`

---

**Document Version:** 1.0
**Last Reviewed:** 2026-02-09
**Next Review:** 2026-03-09
