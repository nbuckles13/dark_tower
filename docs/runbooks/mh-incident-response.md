# MH Service Incident Response Runbook

**Service**: Media Handler (mh-service)
**Owner**: SRE Team / Media Handler Service Owner
**On-Call Rotation**: PagerDuty - Dark Tower MH Team
**Last Updated**: 2026-04-17

---

## Table of Contents

1. [Severity Classification](#severity-classification)
2. [Escalation Paths](#escalation-paths)
3. [Common Failure Scenarios](#common-failure-scenarios)
   - [Scenario 1: Complete Service Outage](#scenario-1-complete-service-outage)
   - [Scenario 2: JWT Validation Failures](#scenario-2-jwt-validation-failures)
   - [Scenario 3: GC Heartbeat Failures](#scenario-3-gc-heartbeat-failures)
   - [Scenario 4: GC Registration Latency](#scenario-4-gc-registration-latency)
   - [Scenario 5: WebTransport Rejections](#scenario-5-webtransport-rejections)
   - [Scenario 6: WebTransport Handshake Slow](#scenario-6-webtransport-handshake-slow)
   - [Scenario 7: Caller Type Rejected](#scenario-7-caller-type-rejected)
   - [Scenario 8: Resource Pressure](#scenario-8-resource-pressure)
   - [Scenario 9: Token Refresh Failures](#scenario-9-token-refresh-failures)
   - [Scenario 10: MH→MC Notification Failures](#scenario-10-mhmc-notification-failures)
   - [Scenario 11: Pod Restarting Frequently](#scenario-11-pod-restarting-frequently)
   - [Scenario 12: GC Heartbeat Latency](#scenario-12-gc-heartbeat-latency)
4. [Diagnostic Commands](#diagnostic-commands)
5. [Recovery Procedures](#recovery-procedures)
6. [Postmortem Template](#postmortem-template)
7. [Additional Resources](#additional-resources)

---

## Severity Classification

Incident severity follows the alert-severity taxonomy defined in
[`docs/observability/alert-conventions.md` §Severity Taxonomy](../observability/alert-conventions.md#severity-taxonomy).

| Alert severity | Incident priority | Response time | Routing |
|---|---|---|---|
| `page` | P1 (Critical) | 15 min | PagerDuty + `#incidents` Slack |
| `warning` | P2 (High) / P3 (Medium) | 1-4 hrs | Slack `#mh-oncall`, business hours |
| `info` | P4 (Low) | Next business day | Dashboards only |

### Severity Upgrade Triggers

- Any `warning` persisting > 2 hours with active user impact -> Upgrade to P1 + page
- Any caller-type rejection spike with concurrent JWT failure spike -> Security Team notification, treat as P1
- Multiple MH pods down (full regional MH outage) -> P1 with Infrastructure Team escalation

---

## Escalation Paths

### Initial Response

**On-Call Engineer** (First Responder):

1. Acknowledge alert within 5 minutes
2. Assess severity using the alert severity and symptoms described below
3. Post incident notice in `#incidents` Slack channel (for P1/P2)
4. Begin investigation using diagnostic commands below
5. Engage specialists as needed

### Escalation Chain

```
On-Call Engineer (0-15 min)
    | (P1 unresolved at 30 min, P2 unresolved at 2h)
MH Service Owner / Tech Lead
    | (multi-service or architectural)
Engineering Manager
    | (region-wide or infrastructure)
Infrastructure Team / SRE Lead
```

### Specialist Contacts

| Team | When to Engage | Contact |
|------|----------------|---------|
| **MC Team** | MC -> MH RegisterMeeting RPC issues, MH -> MC notification delivery | #mc-oncall, PagerDuty: MC-Team |
| **GC Team** | Registration failures, heartbeat issues | #gc-oncall, PagerDuty: GC-Team |
| **AC Team** | JWT validation failures traced to JWKS, token refresh failures | #ac-oncall, PagerDuty: AC-Team |
| **Infrastructure/SRE** | Kubernetes, networking, UDP/QUIC, TLS cert rotation | #infra-oncall, PagerDuty: SRE |
| **Security Team** | Caller-type rejection spike, JWT tampering signal | #security-incidents (P1 only) |

### External Dependencies

- **Global Controller**: MH registration, load-report heartbeats, meeting assignment
- **Meeting Controller**: MC -> MH RegisterMeeting, MH -> MC connect/disconnect notifications
- **Auth Controller**: JWKS endpoint (JWT validation), OAuth token refresh
- **Kubernetes / cloud**: UDP listener for QUIC/WebTransport, TLS cert-manager

---

## Common Failure Scenarios

### Scenario 1: Complete Service Outage

**Alert**: `MHDown`
**Severity**: page

**Symptoms**:
- All MH pods unreachable; `up{job="mh-service"} == 0` for 1 minute
- Clients cannot establish WebTransport media sessions
- Active media sessions disrupted

**Impact**: Users in active meetings lose media. New meeting joins have no working media path.

**Immediate Response**:

1. Confirm pod state: `kubectl get pods -n dark-tower -l app=mh-service`
2. Check recent deployment: `kubectl rollout history deployment/mh-service -n dark-tower`
3. If recent bad deploy suspected, prepare rollback.
4. Post to `#incidents` and page MC team so they can redirect new meeting assignments away from the failing MH.

**Root Cause Investigation**:

- Pod events: `kubectl describe pods -n dark-tower -l app=mh-service`
- Previous-pod logs (if crashed): `kubectl logs -n dark-tower <pod> --previous --tail=200`
- Node health: `kubectl get nodes`, `kubectl describe node <node>`
- OOMKilled events: `kubectl get events -n dark-tower | grep -i "oom\|killed"`
- Secrets / config: `kubectl get secret,configmap -n dark-tower | grep mh-service`

**Recovery**:

```bash
# Option A: Rollback (if recent deployment is the cause)
kubectl rollout undo deployment/mh-service -n dark-tower
kubectl rollout status deployment/mh-service -n dark-tower

# Option B: Force reschedule (transient node issue)
kubectl delete pods -n dark-tower -l app=mh-service

# Option C: Increase memory limits if OOMKilled
kubectl patch deployment/mh-service -n dark-tower -p \
  '{"spec":{"template":{"spec":{"containers":[{"name":"mh-service","resources":{"limits":{"memory":"2Gi"}}}]}}}}'
```

Expected recovery: 1-3 minutes.

**Related Alerts**: `MHPodRestartingFrequently`, `MHGCHeartbeatFailureRate` (MH can't heartbeat while down).

---

### Scenario 2: JWT Validation Failures

**Alert**: `MHHighJwtValidationFailures`
**Severity**: warning

**Symptoms**:
- JWT validation failure rate > 10% for 5 minutes on `mh_jwt_validations_total`
- Subset of clients unable to complete WebTransport handshake
- MH logs show JWT validation failures; specific reason available at debug level

**Impact**: Subset of users unable to establish media sessions. Active sessions unaffected (they authenticated at connect time). Severity is warning because MH correctly rejecting invalid tokens is contract-compliant behavior; the signal flags that failures are elevated and a system cause (AC JWKS, key rotation, clock skew) may be at play.

**Immediate Response**:

1. Check breakdown by `failure_reason` label:
   ```promql
   sum by(failure_reason) (rate(mh_jwt_validations_total{result="failure"}[5m]))
   ```
2. Check AC service health — JWKS is the authoritative key source.
3. If a sudden spike vs steady low rate: steady suggests probing (treat as security event), spike suggests legitimate issue (AC/key rotation).

**Root Cause Investigation**:

```bash
# Breakdown by token_type (meeting vs service)
sum by(token_type) (rate(mh_jwt_validations_total{result="failure"}[5m]))

# AC service health + JWKS reachability from MH
kubectl get pods -n dark-tower -l app=ac-service
kubectl exec -it deployment/mh-service -n dark-tower -- \
  curl -s http://ac-service.dark-tower.svc.cluster.local:8080/.well-known/jwks.json | head -c 500

# Clock skew (JWTs are time-sensitive)
kubectl exec -it deployment/mh-service -n dark-tower -- date -u
kubectl exec -it deployment/ac-service -n dark-tower -- date -u

# MH logs at debug level for failure reasons
kubectl logs -n dark-tower -l app=mh-service --tail=500 | grep -iE "jwt|jwks|token|validation"
```

**Common root causes** (see MC runbook Scenario 10 for longer discussion — the pattern is near-identical for MH):

1. **AC JWKS endpoint down** — MH caches JWKS with 5-min TTL; failures start after cache expires
2. **Clock skew** on nodes — NTP drift
3. **Key rotation in progress** — AC rotated, MH cache stale
4. **Token tampering / probing** — steady-rate failures from unauthorized sources
5. **Token type mismatch** — client sending `service` token where `meeting` expected (or vice versa)

**Recovery**:

- AC restart (if AC is the root cause): `kubectl rollout restart deployment/ac-service -n dark-tower`
- NTP fix on affected node
- Wait up to 5 min for JWKS cache refresh after AC-side fix
- If tampering suspected: do NOT restart, preserve logs, escalate Security Team immediately

**Related Alerts**: `MHWebTransportHandshakeSlow` (JWKS lookups slow), `MHHighWebTransportRejections` (downstream effect).

---

### Scenario 3: GC Heartbeat Failures

**Alert**: `MHGCHeartbeatFailureRate`
**Severity**: warning

**Symptoms**:
- `mh_gc_heartbeats_total{status="error"}` rate > 50% for 2 minutes
- GC logs show stale `last_heartbeat` for this MH
- New meeting assignments routing away from this MH (GC considers it unhealthy)

**Impact**: Partial outage. **Existing WebTransport sessions on this MH continue working** — they do not depend on heartbeats. New meetings will be routed to other MH instances. If ALL MH instances are failing heartbeats, a full outage of new meeting creation follows.

**Immediate Response**:

1. Check how many MH instances are affected (single-pod vs fleet-wide):
   ```promql
   count(rate(mh_gc_heartbeats_total{status="error"}[5m]) > 0)
   ```
2. If fleet-wide, GC is likely the root cause — escalate to GC Team.
3. If isolated, restart affected pod to force fresh registration + heartbeat loop.

**Root Cause Investigation**:

```bash
# MH -> GC connectivity from MH pod
kubectl exec -it deployment/mh-service -n dark-tower -- \
  curl -i http://gc-service.dark-tower.svc.cluster.local:8080/health

# MH logs for heartbeat errors
kubectl logs -n dark-tower -l app=mh-service --tail=300 | grep -iE "heartbeat|SendLoadReport|gc"

# GC service health
kubectl get pods -n dark-tower -l app=gc-service

# MH registration status in GC DB (MH should appear with recent last_heartbeat)
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c \
  "SELECT id, region, capacity, last_heartbeat, status FROM media_handlers ORDER BY last_heartbeat DESC LIMIT 10;"

# NetworkPolicy for MH -> GC egress
kubectl get networkpolicy -n dark-tower
kubectl describe networkpolicy mh-service -n dark-tower
```

**Common Root Causes**:

1. **GC service down or overloaded** — escalate to GC Team
2. **Network policy blocking MH -> GC** — Infrastructure Team
3. **Invalid service token** — MH cannot authenticate to GC; see Scenario 9
4. **DNS resolution failure** — CoreDNS issue; Infrastructure Team

**Recovery**:

```bash
# Force re-registration + fresh heartbeat loop
kubectl rollout restart deployment/mh-service -n dark-tower
```

**Related Alerts**: `MHHighRegistrationLatency`, `MHTokenRefreshFailures`, `MHGCHeartbeatLatencyHigh`.

---

### Scenario 4: GC Registration Latency

**Alert**: `MHHighRegistrationLatency`
**Severity**: warning

**Symptoms**:
- `mh_gc_registration_duration_seconds` p95 > 1.0s for 5 minutes
- MH pods take longer than expected to become ready after restart

**Impact**: Slow recovery after MH restart/deploy. During the slow-registration window, GC may temporarily treat the MH as unavailable and route meetings elsewhere.

**Immediate Response**:

1. Check GC service latency — MH registration latency is bounded by GC RPC response time.
2. Check network path between MH and GC (cross-AZ or cross-region adds latency).

**Root Cause Investigation**:

```promql
# GC-side RegisterMH RPC latency
histogram_quantile(0.95, rate(gc_rpc_duration_seconds_bucket{method="RegisterMH"}[5m]))

# MH -> GC network latency
# (from MH pod)
kubectl exec -it deployment/mh-service -n dark-tower -- \
  ping -c 5 gc-service.dark-tower.svc.cluster.local
```

**Recovery**:
- If GC is overloaded: scale GC, escalate GC Team
- If network latency: Infrastructure Team

**Related Alerts**: `MHGCHeartbeatFailureRate`, `MHGCHeartbeatLatencyHigh`.

---

### Scenario 5: WebTransport Rejections

**Alert**: `MHHighWebTransportRejections`
**Severity**: warning

**Symptoms**:
- `mh_webtransport_connections_total{status="rejected"}` rate > 10% for 5 minutes
- Clients unable to complete WebTransport session setup
- Possibly correlated JWT failures (see Scenario 2)

**Impact**: Subset of users unable to establish media. Connections already established are unaffected.

**Immediate Response**:

1. Check breakdown by status:
   ```promql
   sum by(status) (rate(mh_webtransport_connections_total[5m]))
   ```
2. Check active connection count against configured cap:
   ```promql
   mh_active_connections
   ```
3. If driven by capacity, scale horizontally.

**Root Cause Investigation**:

```bash
# TLS certificate validity
kubectl exec -it deployment/mh-service -n dark-tower -- \
  openssl x509 -in /certs/tls.crt -noout -dates -subject

# MH logs for handshake failures
kubectl logs -n dark-tower -l app=mh-service --tail=500 | grep -iE "webtransport|handshake|reject|quic|tls"

# UDP port exposure (QUIC)
kubectl get svc -n dark-tower mh-service -o yaml | grep -A5 "port:"

# Pod resource pressure (may cause accept loop stalls)
kubectl top pods -n dark-tower -l app=mh-service
```

**Common Root Causes**:

1. **Connection capacity exceeded** — scale horizontally
2. **TLS cert expired or misissued** — cert-manager; Infrastructure Team
3. **UDP blocked** by NetworkPolicy or cloud firewall — Infrastructure Team
4. **JWT validation failures** cascading — see Scenario 2
5. **QUIC listener stuck** — restart pod

**Recovery**:

```bash
# Scale out (capacity)
kubectl scale deployment/mh-service -n dark-tower --replicas=5

# Rotate TLS (if expired)
kubectl delete secret mh-service-tls -n dark-tower
# cert-manager recreates; then:
kubectl rollout restart deployment/mh-service -n dark-tower

# Restart (QUIC listener stuck)
kubectl rollout restart deployment/mh-service -n dark-tower
```

**Related Alerts**: `MHHighJwtValidationFailures`, `MHWebTransportHandshakeSlow`.

---

### Scenario 6: WebTransport Handshake Slow

**Alert**: `MHWebTransportHandshakeSlow`
**Severity**: warning

**Symptoms**:
- `mh_webtransport_handshake_duration_seconds` p95 > 1.0s for 5 minutes
- Clients experiencing slow connection setup to media

**Impact**: Slow "time to first frame" in meetings. Not a failure but a degraded experience.

**Immediate Response**:

1. Check JWKS endpoint latency from MH — handshake includes JWT validation.
2. Check TLS/QUIC error rate — retries slow the handshake.
3. Check pod CPU — busy accept loop slows handshakes.

**Root Cause Investigation**:

```bash
# JWKS fetch latency (if emitted; otherwise AC-side metric)
kubectl exec -it deployment/mh-service -n dark-tower -- \
  time curl -s http://ac-service.dark-tower.svc.cluster.local:8080/.well-known/jwks.json > /dev/null

# MH CPU utilization
kubectl top pods -n dark-tower -l app=mh-service
```

**Common Root Causes**:

1. **Slow JWKS lookups** — AC latency or network path
2. **CPU saturation** — scale out, see Scenario 8
3. **Cold JWKS cache** — expected after restart; transient

**Recovery**: Scale horizontally if CPU-bound. Escalate to AC Team if JWKS is slow.

**Related Alerts**: `MHHighCPU`, `MHHighJwtValidationFailures`.

---

### Scenario 7: Caller Type Rejected

**Alert**: `MHCallerTypeRejected`
**Severity**: warning

**Symptoms**:
- `mh_caller_type_rejected_total` incrementing
- Layer-2 gRPC routing has rejected a caller whose `service_type` does not match the expected value (MH RPC endpoints expect `meeting-controller`)

**Impact**: No user-visible impact from the rejection itself (the request is denied). However, the signal means either (a) a service is misconfigured and calling the wrong endpoint, OR (b) an unauthorized service is probing MH endpoints.

**Immediate Response**:

1. Check which `actual_type` is being rejected:
   ```promql
   sum by(actual_type, expected_type) (rate(mh_caller_type_rejected_total[5m]))
   ```
2. If `actual_type` is a known internal service (e.g. `global-controller`), this is a misconfiguration — find and fix the offending caller.
3. If `actual_type` is `unknown` or an unexpected value, notify Security Team — this may be probing.

**Root Cause Investigation**:

```bash
# MH logs for caller rejection with service identity
kubectl logs -n dark-tower -l app=mh-service --tail=500 | grep -iE "caller|service_type|rejected|layer 2|layer2"

# Check recent MC deploys (MC is the only legitimate caller)
kubectl rollout history deployment/mc-service -n dark-tower

# gRPC call graph in traces (if Jaeger configured)
```

**Common Root Causes**:

1. **MC misconfiguration** — MC sending malformed identity; escalate to MC Team
2. **Unauthorized service** — someone added a new caller without Layer-2 update; Security Team
3. **Token tampering** — if `actual_type` is absent or garbled; Security Team

**Recovery**:

- Misconfiguration: fix caller's service identity claim
- Unauthorized: revoke credentials, rotate service tokens
- Tampering: preserve logs, escalate Security Team; do not restart MH until evidence captured

**Related Alerts**: `MHHighJwtValidationFailures` (may co-fire for tampering).

---

### Scenario 8: Resource Pressure

**Alert**: `MHHighMemory`, `MHHighCPU`
**Severity**: warning

**Symptoms**:
- Memory > 85% of limit for 10 minutes, OR CPU > 80% for 5 minutes
- Secondary effects: slow handshakes (Scenario 6), handshake rejections (Scenario 5)

**Impact**: Approaching limits. OOM kill risk on memory, latency risk on CPU.

**Immediate Response**:

```bash
kubectl top pods -n dark-tower -l app=mh-service
kubectl describe deployment mh-service -n dark-tower | grep -A 5 "Limits:"
```

**Root Cause Investigation**:

```promql
# Active connection load on this pod
mh_active_connections

# Memory trend (leak if never decreasing)
container_memory_working_set_bytes{pod=~"mh-service-.*"}

# CPU trend
rate(container_cpu_usage_seconds_total{pod=~"mh-service-.*"}[5m])
```

**Common Root Causes**:

1. **High connection load** — `mh_active_connections` elevated; scale out
2. **Memory leak** — never-decreasing memory; restart, profile
3. **CPU-intensive media forwarding** — investigate, scale

**Recovery**:

```bash
# Scale horizontally
kubectl scale deployment/mh-service -n dark-tower --replicas=5

# Increase limits
kubectl patch deployment/mh-service -n dark-tower -p \
  '{"spec":{"template":{"spec":{"containers":[{"name":"mh-service","resources":{"limits":{"cpu":"4000m","memory":"2Gi"}}}]}}}}'

# Rolling restart (if leak suspected)
kubectl rollout restart deployment/mh-service -n dark-tower
```

**Related Alerts**: `MHHighWebTransportRejections`, `MHWebTransportHandshakeSlow`, `MHPodRestartingFrequently`.

---

### Scenario 9: Token Refresh Failures

**Alert**: `MHTokenRefreshFailures`
**Severity**: warning

**Symptoms**:
- `mh_token_refresh_total{status="error"}` rate > 10% for 5 minutes
- `mh_token_refresh_failures_total` broken down by `error_type`
- MH may lose ability to call GC and MC once its cached service token expires

**Impact**: Initially none — MH continues using cached token until it expires. Once expired, MH cannot authenticate outbound RPCs (GC registration, heartbeats, MC notifications). Escalates to `MHGCHeartbeatFailureRate` and `MHMCNotificationFailures`.

**Immediate Response**:

1. Break down by `error_type`:
   ```promql
   sum by(error_type) (rate(mh_token_refresh_failures_total[5m]))
   ```
2. Check AC service health.
3. If AC is down, prioritize AC recovery — MH will resume once AC is healthy AND cached tokens haven't expired.

**Root Cause Investigation**:

```bash
# AC service health
kubectl get pods -n dark-tower -l app=ac-service

# MH -> AC connectivity
kubectl exec -it deployment/mh-service -n dark-tower -- \
  curl -i http://ac-service.dark-tower.svc.cluster.local:8080/health

# MH logs for token errors
kubectl logs -n dark-tower -l app=mh-service --tail=500 | grep -iE "token|refresh|oauth|ac-service"
```

**Common Root Causes** (by `error_type`):

1. **`http`** — Network / AC endpoint unreachable
2. **`auth_rejected`** — MH credentials rejected; may need client-secret rotation
3. **`invalid_response`** — AC returned unexpected payload; AC Team
4. **`acquisition_failed`** — OAuth flow failed; AC Team
5. **`configuration`** — MH config missing AC endpoint / client id; check ConfigMap
6. **`channel_closed`** — internal channel shut down; restart MH

**Recovery**:

- If AC is the root cause: escalate to AC Team, restore AC health
- If `configuration`: fix ConfigMap, restart MH
- If cached token about to expire and AC is still down: MH is about to lose outbound auth — consider restart to pick up fresh config even if that doesn't help

**Related Alerts**: `MHGCHeartbeatFailureRate`, `MHMCNotificationFailures` (downstream effects once token expires).

---

### Scenario 10: MH→MC Notification Failures

**Alert**: `MHMCNotificationFailures`
**Severity**: warning

**Symptoms**:
- `mh_mc_notifications_total{status="error"}` rate > 10% for 5 minutes
- MH unable to notify MC of `connected` / `disconnected` events
- MC participant state may drift from actual MH connection state

**Impact**: MC's view of who is connected may be stale. Symptoms for users: stale presence indicators, delayed disconnect detection, participants appearing still-present after they've actually left. Usually recoverable as subsequent events retry or catch up.

**Immediate Response**:

1. Check MC service health:
   ```bash
   kubectl get pods -n dark-tower -l app=mc-service
   ```
2. Check breakdown by event:
   ```promql
   sum by(event, status) (rate(mh_mc_notifications_total[5m]))
   ```

**Root Cause Investigation**:

```bash
# MH -> MC connectivity
kubectl exec -it deployment/mh-service -n dark-tower -- \
  curl -i http://mc-service.dark-tower.svc.cluster.local:8080/health

# MH logs for MC notification errors
kubectl logs -n dark-tower -l app=mh-service --tail=500 | grep -iE "mc notification|mh->mc|notify|connected|disconnected"

# Service token health (MH auth to MC uses the same service token as MH->GC)
kubectl logs -n dark-tower -l app=mh-service --tail=200 | grep -iE "token|auth"
```

**Common Root Causes**:

1. **MC service degraded or down** — MC Team
2. **Network policy** blocking MH -> MC — Infrastructure Team
3. **Token expired** — see Scenario 9
4. **MC overload** — MC inbox full; see MC runbook Scenario 1

**Recovery**:

- If MC is unhealthy: restore MC first (MC Team)
- Once MC is healthy, fire-and-forget design means MH will naturally stop failing on new events; existing state drift resolves as participants reconnect or are garbage-collected by MC

**Related Alerts**: `MHTokenRefreshFailures`, MC-side alerts (`MCHighMailboxDepthWarning`, `MCDown`).

---

### Scenario 11: Pod Restarting Frequently

**Alert**: `MHPodRestartingFrequently`
**Severity**: warning

**Symptoms**:
- A pod restarting more than once per hour sustained
- Active WebTransport connections on that pod disrupted on each restart

**Impact**: Subset of users repeatedly disconnected. Suggests a crash loop or failed liveness probe.

**Immediate Response**:

1. Identify the affected pod: `kubectl get pods -n dark-tower -l app=mh-service`
2. Check previous-pod logs: `kubectl logs -n dark-tower <pod> --previous --tail=200`
3. Check for OOMKilled events.

**Root Cause Investigation**:

```bash
# Pod events (liveness probe fails, OOMKilled, etc.)
kubectl describe pod <pod> -n dark-tower

# Memory / CPU at time of restart
kubectl top pods -n dark-tower -l app=mh-service

# Recent deployment changes
kubectl rollout history deployment/mh-service -n dark-tower
```

**Common Root Causes**:

1. **Liveness probe failing** — health endpoint slow; investigate
2. **OOMKilled** — increase memory limits, investigate leak
3. **Panic on startup** — recent deploy introduced bug; rollback
4. **Missing dependency at init** — e.g., can't reach GC or AC at startup

**Recovery**:

```bash
# Rollback if recent deploy
kubectl rollout undo deployment/mh-service -n dark-tower

# Increase memory (OOMKilled)
kubectl patch deployment/mh-service -n dark-tower -p \
  '{"spec":{"template":{"spec":{"containers":[{"name":"mh-service","resources":{"limits":{"memory":"2Gi"}}}]}}}}'
```

**Related Alerts**: `MHHighMemory`, `MHDown` (if all pods restart simultaneously).

---

### Scenario 12: GC Heartbeat Latency

**Alert**: `MHGCHeartbeatLatencyHigh`
**Severity**: info

**Symptoms**:
- `mh_gc_heartbeat_latency_seconds` p95 > 100ms for 5 minutes
- Leading indicator only; no user-visible impact yet

**Impact**: None currently. Watch for escalation to `MHGCHeartbeatFailureRate` or extended cascading-latency symptoms.

**Immediate Response**:

1. No immediate remediation needed. Capture the signal for trend analysis.
2. If trending worse over hours, investigate GC or network path before escalation.

**Root Cause Investigation**:

```promql
# Heartbeat p95 trend
histogram_quantile(0.95, rate(mh_gc_heartbeat_latency_seconds_bucket[5m]))

# GC-side RPC latency for heartbeat
histogram_quantile(0.95, rate(gc_rpc_duration_seconds_bucket{method="SendLoadReport"}[5m]))
```

**Common Root Causes**:

1. GC under load
2. Network path degradation (cross-AZ)
3. MH pod CPU contention during metric emission

**Recovery**: Usually self-heals. Escalate only if sustained or escalating.

**Related Alerts**: `MHGCHeartbeatFailureRate`, `MHHighRegistrationLatency`.

---

## Diagnostic Commands

### Quick Health Check

```bash
kubectl port-forward -n dark-tower deployment/mh-service 8080:8080 &
curl http://localhost:8080/health      # Liveness
curl http://localhost:8080/ready       # Readiness
kill %1

kubectl get pods -n dark-tower -l app=mh-service
kubectl logs -n dark-tower -l app=mh-service --tail=100 | grep -i error
```

### Metrics Analysis

```bash
kubectl port-forward -n dark-tower deployment/mh-service 8080:8080 &

# All metrics
curl http://localhost:8080/metrics

# By subsystem
curl http://localhost:8080/metrics | grep mh_gc_registration
curl http://localhost:8080/metrics | grep mh_gc_heartbeat
curl http://localhost:8080/metrics | grep mh_webtransport
curl http://localhost:8080/metrics | grep mh_jwt_validations
curl http://localhost:8080/metrics | grep mh_mc_notifications
curl http://localhost:8080/metrics | grep mh_token_refresh
curl http://localhost:8080/metrics | grep mh_caller_type_rejected
curl http://localhost:8080/metrics | grep mh_active_connections

kill %1
```

### Log Analysis

```bash
kubectl logs -n dark-tower -l app=mh-service -f
kubectl logs -n dark-tower <pod> --previous --tail=500
kubectl logs -n dark-tower -l app=mh-service --tail=1000 | grep -iE "error|panic|fatal"
kubectl logs -n dark-tower -l app=mh-service --tail=1000 | grep -iE "webtransport|handshake"
kubectl logs -n dark-tower -l app=mh-service --tail=1000 | grep -iE "jwt|jwks|token"
kubectl logs -n dark-tower -l app=mh-service --tail=1000 | grep -iE "gc|register|heartbeat"
```

### Resource Utilization

```bash
kubectl top pods -n dark-tower -l app=mh-service
kubectl top nodes
kubectl describe deployment mh-service -n dark-tower | grep -A 5 "Limits:"
kubectl get events -n dark-tower --field-selector involvedObject.name=mh-service --sort-by='.lastTimestamp'
```

### Network Debugging

```bash
# MH -> GC
kubectl exec -it deployment/mh-service -n dark-tower -- \
  curl -i http://gc-service.dark-tower.svc.cluster.local:8080/health

# MH -> MC
kubectl exec -it deployment/mh-service -n dark-tower -- \
  curl -i http://mc-service.dark-tower.svc.cluster.local:8080/health

# MH -> AC
kubectl exec -it deployment/mh-service -n dark-tower -- \
  curl -i http://ac-service.dark-tower.svc.cluster.local:8080/health

# Network policies
kubectl get networkpolicies -n dark-tower
kubectl describe networkpolicy mh-service -n dark-tower
```

---

## Recovery Procedures

### Service Restart Procedure

**When to use**: Stuck state, memory pressure, suspected listener crash.

```bash
kubectl get pods -n dark-tower -l app=mh-service
kubectl rollout restart deployment/mh-service -n dark-tower
kubectl rollout status deployment/mh-service -n dark-tower
kubectl logs -n dark-tower -l app=mh-service --tail=50
```

**Rollback on failure**:
```bash
kubectl rollout undo deployment/mh-service -n dark-tower
```

**Impact**: Active WebTransport sessions on restarted pods are terminated. Clients will reconnect to other MH instances (GC reassigns).

---

### JWKS Cache Flush Procedure

**When to use**: After AC key rotation, when MH is validating against stale keys.

MH caches JWKS with a 5-minute TTL. There is no hot-flush endpoint — a pod restart forces a cold fetch:

```bash
kubectl rollout restart deployment/mh-service -n dark-tower
```

Alternatively, wait up to 5 minutes for the TTL to expire — JWT validation failures resolve automatically once the cache refreshes.

---

### Graceful Drain Procedure

**When to use**: Planned maintenance.

1. Mark MH as draining in GC (stops new meeting assignments):
   ```bash
   kubectl exec -it deployment/gc-service -n dark-tower -- \
     psql $DATABASE_URL -c \
     "UPDATE media_handlers SET status = 'draining' WHERE id = '<MH_ID>';"
   ```
2. Wait for `mh_active_connections` to reach zero (or acceptable low).
3. Proceed with maintenance.
4. Re-enable after maintenance:
   ```bash
   kubectl exec -it deployment/gc-service -n dark-tower -- \
     psql $DATABASE_URL -c \
     "UPDATE media_handlers SET status = 'active' WHERE id = '<MH_ID>';"
   ```

---

## Postmortem Template

Use this template for all P1 and P2 incidents.

```markdown
# Postmortem: [Incident Title]

**Date**: YYYY-MM-DD
**Severity**: P1/P2/P3
**Duration**: [Start] - [End] (Total: X hours Y minutes)
**Status**: Resolved / Mitigated / Investigating
**Author**: [On-call engineer]
**Reviewers**: [Tech Lead, Engineering Manager]

## Executive Summary
[1-2 sentences describing what happened and impact]

## Impact
- Affected meetings: [metric]
- Affected participants: [estimate]
- Duration of impact: [X min]
- SLA breach: Yes/No

## Timeline (UTC)
| Time | Event |
|------|-------|
| HH:MM | Alert fired |
| HH:MM | On-call acknowledged |
| HH:MM | Investigation began |
| HH:MM | Root cause identified |
| HH:MM | Remediation started |
| HH:MM | Service recovered |
| HH:MM | Incident resolved |

## Root Cause
[Detailed explanation]

## Action Items
| Action | Owner | Due | Priority | Status |
|--------|-------|-----|----------|--------|
| ... | ... | ... | ... | Open |
```

---

## Additional Resources

- **ADR-0011**: Observability Framework
- **ADR-0023**: Media Handler Architecture
- **ADR-0029**: Dashboard Metric Presentation
- **ADR-0031**: Service-owned Dashboards and Alerts
- **Alert Conventions**: `docs/observability/alert-conventions.md`
- **MH Metrics Catalog**: `docs/observability/metrics/mh-service.md`
- **MC Runbook** (reference precedent): `docs/runbooks/mc-incident-response.md`
- **Slack Channels**:
  - `#incidents` — Active incident coordination
  - `#mh-oncall` — MH team channel
  - `#mc-oncall` — MC coordination
  - `#gc-oncall` — GC coordination
  - `#ac-oncall` — AC coordination
  - `#infra-oncall` — Infrastructure
  - `#security-incidents` — Security Team (P1 only)

---

**Remember**: When in doubt, escalate. Incident response is a team sport.
