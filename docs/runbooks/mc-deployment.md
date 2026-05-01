# MC Service Deployment Runbook

**Service**: Meeting Controller (mc-service)
**Version**: Phase 4a (ADR-0010 Implementation)
**Last Updated**: 2026-03-27
**Owner**: Operations Team

---

## Overview

This runbook covers deployment, rollback, and troubleshooting procedures for the Meeting Controller service. The MC service is responsible for WebTransport signaling, session management, and participant coordination within meetings.

**Critical Service**: MC downtime affects all active meetings. Users will be disconnected and unable to communicate. Follow pre-deployment checklist carefully.

---

## Manifest Structure

MC service Kubernetes manifests are managed via Kustomize with a base/overlay pattern:

```
infra/
├── services/mc-service/                    # Base manifests
│   ├── kustomization.yaml                  # Explicit resource list
│   ├── configmap.yaml
│   ├── deployment.yaml
│   ├── service.yaml
│   ├── secret.yaml
│   ├── pdb.yaml
│   ├── network-policy.yaml
│   └── service-monitor.yaml               # Present in dir but not in kustomization.yaml (needs Prometheus Operator CRD)
└── kubernetes/overlays/kind/
    └── services/mc-service/
        └── kustomization.yaml             # Kind overlay — refs base, adds Kind-specific labels
```

- **Base** (`infra/services/mc-service/`): Contains all production manifests. The `kustomization.yaml` explicitly lists each resource. Files like `service-monitor.yaml` are present in the directory but omitted from `kustomization.yaml` when they require CRDs not available in all environments.
- **Kind overlay** (`infra/kubernetes/overlays/kind/services/mc-service/`): References the base and adds Kind-specific labels.
- Deploy with: `kubectl apply -k infra/kubernetes/overlays/kind/services/mc-service/`

> **Note:** The MC WebTransport TLS secret (`mc-service-tls`) is created imperatively by `setup.sh` (via `create_mc_tls_secret()`), not managed by Kustomize. Ensure TLS secrets are provisioned before deploying MC. See [Common Deployment Issues](#common-deployment-issues) for TLS troubleshooting.

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
kubectl get deployment mc-service -n dark-tower

# Check current pod status
kubectl get pods -n dark-tower -l app=mc-service

# Check current resource utilization
kubectl top pods -n dark-tower -l app=mc-service

# Check active meetings (critical - do not deploy if high)
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl -s http://localhost:8080/metrics | grep mc_meetings_active
kill %1

# Verify readiness of current pods
kubectl get pods -n dark-tower -l app=mc-service -o json | jq '.items[].status.conditions[] | select(.type=="Ready")'
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
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "UPDATE meeting_controllers SET status = 'draining' WHERE id = '<MC_ID>';"

# Wait for active meetings to decrease
watch -n 10 'kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 2>/dev/null & sleep 1; curl -s http://localhost:8080/metrics | grep mc_meetings_active; kill %1 2>/dev/null'

# Proceed when active meetings reach acceptable level (e.g., <10)
```

### 3. Update Container Image

**Option A: Using kubectl (direct deployment)**

```bash
# Set new image version
export NEW_VERSION="v1.2.3"  # Replace with actual version tag

# Update Deployment with new image
kubectl set image deployment/mc-service \
  mc-service=mc-service:${NEW_VERSION} \
  -n dark-tower

# Verify image updated
kubectl describe deployment mc-service -n dark-tower | grep Image:
```

**Option B: Using kubectl apply -k (declarative, Kustomize)**

```bash
# Update infra/services/mc-service/deployment.yaml
# Change image tag: mc-service:latest → mc-service:v1.2.3

# Apply via Kustomize overlay (Kind environment)
kubectl apply -k infra/kubernetes/overlays/kind/services/mc-service/

# Verify change
kubectl describe deployment mc-service -n dark-tower | grep Image:
```

### 4. Rolling Update Monitoring

Deployments update pods via rolling strategy (maxSurge=1, maxUnavailable=0 for zero-downtime).

**Monitor rollout:**

```bash
# Watch pod status (Ctrl+C to exit)
kubectl get pods -n dark-tower -l app=mc-service -w

# Check rollout status
kubectl rollout status deployment/mc-service -n dark-tower

# Monitor logs from new pod
kubectl logs -f deployment/mc-service -n dark-tower
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
kubectl get pods -n dark-tower -l app=mc-service

# Check pod events for errors
kubectl get events -n dark-tower --field-selector involvedObject.kind=Pod,involvedObject.name=mc-service-<pod-suffix>
```

**Logs review:**

```bash
# Check for startup errors
kubectl logs deployment/mc-service -n dark-tower --tail=50

# Look for error patterns
kubectl logs -n dark-tower -l app=mc-service --tail=100 | grep -i "error\|panic\|fatal"
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
- [ ] Join flow works (WebTransport connect, JoinRequest/Response)

### 7. Verify GC Registration

**Confirm MC is registered and receiving assignments:**

```bash
# Check MC registration in GC database
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "SELECT id, region, capacity, current_sessions, last_heartbeat, status FROM meeting_controllers WHERE last_heartbeat > NOW() - INTERVAL '30 seconds' ORDER BY last_heartbeat DESC;"

# Verify MC is marked as healthy
# status should be 'active', last_heartbeat should be recent
```

### 8. Monitor Metrics

**Verify metrics collection:**

```bash
# Port-forward to access metrics endpoint
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &

# Fetch metrics
curl http://localhost:8080/metrics

# Kill port-forward
kill %1
```

**Check key metrics:**
- `mc_meetings_active` - Active meeting count
- `mc_connections_active` - Active WebTransport connections
- `mc_actor_mailbox_depth` - Actor mailbox depth (should be low)
- `mc_session_join_duration_seconds` - Session join latency (p95 SLO)
- `mc_redis_latency_seconds` - Redis op latency (p99 <10ms SLO)

### 9. Post-Deployment Checklist

- [ ] All pods Running and Ready
- [ ] Smoke tests pass (health, ready, metrics)
- [ ] No errors in logs (last 5 minutes)
- [ ] Metrics available in Prometheus
- [ ] GC heartbeat succeeding
- [ ] GC has MC marked as 'active'
- [ ] Actor mailbox depths low (<50)
- [ ] Session join duration within SLO (p95 < 2s)
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
   - p95 session join duration >4s (2x SLO)
   - Redis p99 latency >50ms (5x SLO)
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
kubectl rollout history deployment/mc-service -n dark-tower

# Get image from previous revision
kubectl rollout history deployment/mc-service -n dark-tower --revision=<PREVIOUS_REVISION>
```

**Step 2: Rollback Deployment**

```bash
# Rollback to previous revision
kubectl rollout undo deployment/mc-service -n dark-tower

# Or rollback to specific revision
kubectl rollout undo deployment/mc-service -n dark-tower --to-revision=<REVISION>

# Monitor rollback
kubectl rollout status deployment/mc-service -n dark-tower
```

**Step 3: Verify rollback success**

```bash
# Check pods running previous version
kubectl get pods -n dark-tower -l app=mc-service -o jsonpath='{.items[*].spec.containers[0].image}'

# Run smoke tests (see Smoke Tests section)
# Verify health, ready, metrics endpoints
```

**Step 4: Re-enable in GC (if drained)**

```bash
# If MC was in draining status, re-enable
kubectl exec -it deployment/gc-service -n dark-tower -- \
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
| `GC_REGISTRATION_URL` | **Yes** | Global Controller registration endpoint | None | `http://gc-service.dark-tower.svc.cluster.local:8080/api/v1/mc/register` |
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
  GC_REGISTRATION_URL: "http://gc-service.dark-tower.svc.cluster.local:8080/api/v1/mc/register"
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
kubectl get pods -n dark-tower -l app=gc-service

# Test GC endpoint directly from MC pod
kubectl exec -it deployment/mc-service -n dark-tower -- \
  curl -i $GC_REGISTRATION_URL

# Check MC logs for registration errors
kubectl logs deployment/mc-service -n dark-tower --tail=100 | grep -i "register\|gc"

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
kubectl get pods -n dark-tower -l app=mc-service -o wide
# Multiple pods on same node may conflict

# Check TLS secrets are mounted
kubectl describe pod <mc-pod> -n dark-tower | grep -A 5 "Mounts:"

# Check logs for TLS errors
kubectl logs deployment/mc-service -n dark-tower --tail=100 | grep -i "tls\|cert\|webtransport"
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
kubectl top pods -n dark-tower -l app=mc-service

# Check for mailbox buildup
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
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
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &

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
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &

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
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &

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
kubectl exec -it deployment/gc-service -n dark-tower -- \
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

### Test 5: Join Flow (WebTransport + Signaling)

**Purpose:** Verify the full participant join flow — WebTransport connect, JoinRequest/JoinResponse, and participant notifications.

**Prerequisites:** A valid meeting must exist (created via GC). Obtain a participant JWT from GC's join endpoint.

```bash
# Step 1: Obtain join token via GC
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
JOIN_RESPONSE=$(curl -s http://localhost:8080/api/v1/meetings/TEST-CODE \
  -H "Authorization: Bearer $TOKEN")
MC_URL=$(echo "$JOIN_RESPONSE" | jq -r '.mc_assignment.mc_url')
JOIN_TOKEN=$(echo "$JOIN_RESPONSE" | jq -r '.token')
kill %1

echo "MC URL: $MC_URL"
echo "Join token obtained: $([ -n "$JOIN_TOKEN" ] && echo 'yes' || echo 'no')"

# Step 2: Verify WebTransport connection to MC
# Use a WebTransport test client (e.g., webtransport-cli or browser devtools)
# Connect to: $MC_URL with the join token
# Expected: Connection established, HTTP/3 CONNECT succeeds

# Step 3: Send JoinRequest over WebTransport session
# Using the established WebTransport session, send a JoinRequest protobuf message
# Expected: MC responds with JoinResponse containing:
#   - participant_id (non-empty UUID)
#   - session info (meeting_id, current participants)
#   - media configuration

# Step 4: Verify participant notification
# Other participants in the meeting (if any) should receive a
# ParticipantJoined notification via their WebTransport session
# Expected: Notification contains new participant's display name and ID

# Step 5: Verify join metrics incremented
kubectl port-forward -n dark-tower deployment/mc-service 8080:8080 &
curl -s http://localhost:8080/metrics | grep -E "mc_session_joins_total|mc_session_join_duration"
kill %1

# Expected:
# mc_session_joins_total should have incremented
# mc_session_join_duration_seconds should show recent observation
```

**Success criteria:**
- WebTransport connection established successfully (HTTP/3 CONNECT 200)
- JoinRequest accepted, JoinResponse received with valid participant_id
- Existing participants receive ParticipantJoined notification
- `mc_session_joins_total` counter incremented
- `mc_session_join_duration_seconds` recorded (p95 <500ms SLO)
- No errors in MC logs related to the join flow
- Response time: JoinRequest to JoinResponse <500ms

---

## Monitoring and Verification

### Key Metrics to Monitor Post-Deployment

**Service health:**

```promql
# Pod restart count (should be 0 after initial deployment)
kube_pod_container_status_restarts_total{namespace="dark-tower",pod=~"mc-service-.*"}

# Pod readiness (should be 1 for all pods)
kube_pod_status_ready{namespace="dark-tower",pod=~"mc-service-.*"}
```

**Actor system health:**

```promql
# Mailbox depth by actor type (should be low, <50)
sum by(actor_type) (mc_actor_mailbox_depth)

# Actor panics (should be 0)
sum(increase(mc_actor_panics_total[5m]))

# Messages dropped per second (should be near 0)
sum by(actor_type) (rate(mc_messages_dropped_total[5m]))
```

**Latency:**

```promql
# p95 session join duration, success only (SLO: <2s)
histogram_quantile(0.95, sum by(le) (rate(mc_session_join_duration_seconds_bucket{status="success"}[5m])))

# p99 Redis op latency (SLO: <10ms)
histogram_quantile(0.99, sum by(le) (rate(mc_redis_latency_seconds_bucket[5m])))
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
sum(rate(mc_gc_heartbeats_total{status="success"}[5m])) / sum(rate(mc_gc_heartbeats_total[5m]))
```

### Grafana Dashboards

**Recommended dashboards:**
- **MC Overview** - Active meetings, connections, mailbox depth, panics, join flow
- **MC SLOs** - Session join duration, Redis latency, drop rate, error budget

See `infra/grafana/dashboards/mc-overview.json`.

### Alerting Rules

**Critical alerts (page on-call):**
- `MCDown` - No MC pods running for >1 minute
- `MCActorPanic` - Any actor panic
- `MCHighMailboxDepthCritical` - Mailbox depth >500 for >2 minutes
- `MCHighLatency` - p95 latency >500ms for >5 minutes
- `MCHighMessageDropRate` - Drop rate >1% for >5 minutes

See `infra/docker/prometheus/rules/mc-alerts.yaml` for full list.

### Post-Deploy Monitoring Checklist: Join Flow

Use this checklist after any deployment that touches join flow code (WebTransport session handling, JoinRequest/JoinResponse processing, participant notifications, or JWT validation). For routine deployments that do not affect the join path, the general monitoring section above is sufficient.

**15-minute check:**

```promql
# Session join rate (should be > 0 if traffic is flowing)
sum(increase(mc_session_joins_total[5m]))

# Join failure rate (should be < 1%)
sum(rate(mc_session_join_failures_total[5m]))
/ sum(rate(mc_session_joins_total[5m]))

# Join latency p95 (SLO: < 500ms)
histogram_quantile(0.95,
  sum by(le) (rate(mc_session_join_duration_seconds_bucket[5m]))
)
```

- [ ] `mc_session_joins_total` rate stable or increasing (confirms join traffic is flowing)
- [ ] `mc_session_join_duration_seconds` p95 within SLO (<500ms)
- [ ] `mc_session_join_failures_total` not spiking (failure rate <1%)
- [ ] `mc_webtransport_connections_total{status="rejected"}` not elevated vs. pre-deploy baseline
- [ ] `mc_jwt_validations_total{result="failure"}` not elevated vs. pre-deploy baseline
- [ ] No new `MCHighJoinFailureRate` or `MCHighJoinLatency` alerts firing

**1-hour check:**

- [ ] Join success rate trend is stable (not degrading)
- [ ] No pod restarts since deployment completed
- [ ] WebTransport connection rejection rate steady (no upward trend)
- [ ] JWT validation failure rate steady (no upward trend)
- [ ] Logs show no repeated error patterns related to join flow

**4-hour check:**

- [ ] All join flow alerts clear
- [ ] Join latency trend is stable (no drift toward SLO boundary)
- [ ] No anomalous patterns in join failure reasons
- [ ] Actor mailbox depths remain low (<50) under join traffic load

**Rollback criteria** (trigger immediate rollback if any):

- `mc_session_join_failures_total` rate >5% for 10 minutes
- `mc_session_join_duration_seconds` p95 >1s for 5 minutes (2x SLO)
- `mc_webtransport_connections_total{status="rejected"}` rate doubles vs. pre-deploy baseline
- `MCHighJoinFailureRate` or `MCHighJoinLatency` alert fires and does not resolve within 5 minutes

```bash
# Rollback command
kubectl rollout undo deployment/mc-service -n dark-tower

# Note: Active sessions on old pods will drain gracefully (60s).
# New joins will route to rolled-back pods once they register with GC.
```

### Post-Deploy Monitoring Checklist: MC↔MH Coordination (RegisterMeeting + Notifications)

Use this checklist after any deployment that touches the MC↔MH coordination path: `RegisterMeeting` RPC client (`crates/mc-service/src/grpc/mh_client.rs`), `MhConnectionRegistry`, MH→MC notification handling, or `MediaConnectionFailed` reporting. This is the MC-side companion to the MH-side post-deploy checklist; the canonical full checklist (with all four windows — 30-min, 2-hour, 4-hour, 24-hour — and rollback criteria) lives at:

- `docs/runbooks/mh-deployment.md` §"Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination"

Open that section first if you are deploying mh-service or both services together. The MC-specific spot-checks below let an MC-only engineer (e.g. deploying only an MC client revision) verify the MC half of coordination without flipping runbooks.

**Quick MC-side gates** — for the canonical PromQL, see `docs/runbooks/mh-deployment.md` §"Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination" → "30-minute check". Do not duplicate the queries here; thresholds and emitter-label conventions are owned in one place to avoid silent divergence:

- `mc_register_meeting_total{status="success"}` rate / total > 95% (canonical query in MH runbook). Emitter-label note: `status="success|error"` (NOT `failure`); see `crates/mc-service/src/observability/metrics.rs:340` and call sites at `crates/mc-service/src/grpc/mh_client.rs:136,144,157`.
- `mc_media_connection_failures_total{all_failed="true"}` increase over 30m = 0 (canonical query in MH runbook). Any non-zero is a P1 — clients are losing all MH paths.

**MC-only signal** (no MH-side equivalent — counts events arriving at MC, regardless of which MH originated them):

```promql
# MH→MC notifications received (sanity: traffic is flowing).
# This counter has no `status` label — it counts arrivals only.
# For MH→MC delivery success rate, use the MH-side `mh_mc_notifications_total`
# (see canonical checklist in mh-deployment.md).
sum by(event_type) (rate(mc_mh_notifications_received_total[5m]))
```

- [ ] `mc_register_meeting_total{status="success"}` rate / total >95% (run the canonical query)
- [ ] `mc_mh_notifications_received_total` rate non-zero (events arriving means MH is reaching MC)
- [ ] `mc_media_connection_failures_total{all_failed="true"}` increase over 30m = 0 (run the canonical query)
- [ ] No new `MCMediaConnectionAllFailed` alerts firing (`infra/docker/prometheus/rules/mc-alerts.yaml`)
- [ ] No mc-service pod restarts since deploy completed
- [ ] Cross-check the MH-side checklist (link above) for the full set of MH-side checks (handshake, JWT, timeout, MH→MC delivery success rate, active connections)

**Rollback (MC half)**: same as the join-flow rollback above — `kubectl rollout undo deployment/mc-service -n dark-tower`. If the issue is on the MH side (handshake, JWT, RegisterMeeting timeouts), follow the rollback criteria + `mh-service` rollback documented in `docs/runbooks/mh-deployment.md` §"Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination" → "Rollback criteria".

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
- **Source Code:** `crates/mc-service/`
- **Kubernetes Manifests (base):** `infra/services/mc-service/`
- **Kubernetes Manifests (Kind overlay):** `infra/kubernetes/overlays/kind/services/mc-service/`

---

**Document Version:** 1.2
**Last Reviewed:** 2026-03-31
**Next Review:** 2026-04-30
