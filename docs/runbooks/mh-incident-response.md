# MH Service Incident Response Runbook

**Service**: Media Handler (mh-service)
**Owner**: SRE Team / Media Handler Service Owner
**On-Call Rotation**: PagerDuty - Dark Tower MH Team
**Last Updated**: 2026-09-08

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
   - [Scenario 13: RegisterMeeting Timeout — Clients Kicked](#scenario-13-registermeeting-timeout--clients-kicked)
   - [Scenario 14: WebTransport Server Startup Failure](#scenario-14-webtransport-server-startup-failure)
   - [Scenario 15: Media Sessions Declining — No Sender Binding](#scenario-15-media-sessions-declining--no-sender-binding)
   - [Scenario 16: Ingress Datagrams Received But Never Read](#scenario-16-ingress-datagrams-received-but-never-read)
   - [Scenario 17: Media Datagram Drop](#scenario-17-media-datagram-drop)
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
- `rate(mh_register_meeting_timeouts_total[5m]) > 0.1` sustained for > 10m WITH concurrent JWT-validation failure spike (`rate(mh_jwt_validations_total{result="failure"}[5m])` non-trivial) -> Security investigation per Scenario 13 + Scenario 2
- WebTransport server bind/listen failures fleet-wide (Scenario 14) -> P1 with Infrastructure Team escalation immediately

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

**Impact**: Subset of users unable to establish media. Connections already
established are unaffected. Note that MH has **no horizontal-scaling lever at
incident time** — instances are per-instance Deployments with their own
NodePort Services and port mappings, so adding one is a manifest change, not
a `kubectl scale`. Combined with a tripped accept guard meaning flood-or-leak
rather than demand, the only responses are find-the-source or restart. MTTR
is one rollout.

**Immediate Response**:

1. Check breakdown by status:
   ```promql
   sum by(status) (rate(mh_webtransport_connections_total[5m]))
   ```
2. Check active connection count against configured cap:
   ```promql
   mh_active_connections
   ```
3. Check the accept-time guard, NOT capacity. `MH_MAX_CONNECTIONS` is a
   resource-exhaustion guard at accept and is never a capacity figure
   (ADR-0036 §1). It sits far above expected peak, so rejections attributed
   to it mean a connection flood or a connection leak — not demand that has
   outgrown the deployment. Scaling out in response masks both. Establish
   which by comparing `mh_active_connections` against expected participant
   count: a leak shows connections climbing while joins do not.

**Root Cause Investigation**:

```bash
# TLS certificate validity — read the Secret; the MH image is distroless
# (no shell, no openssl), so `kubectl exec` cannot inspect it in-pod.
kubectl get secret mh-service-tls -n dark-tower \
  -o jsonpath='{.data.tls\.crt}' | base64 -d | \
  openssl x509 -noout -dates -subject

# MH logs for handshake failures
kubectl logs -n dark-tower -l app=mh-service --tail=500 | grep -iE "webtransport|handshake|reject|quic|tls"

# UDP port exposure (QUIC)
kubectl get svc -n dark-tower mh-service -o yaml | grep -A5 "port:"

# Pod resource pressure (may cause accept loop stalls)
kubectl top pods -n dark-tower -l app=mh-service
```

**Common Root Causes**:

1. **Accept guard tripped (`MH_MAX_CONNECTIONS`)** — a flood or a
   connection leak, never demand. Do not scale out; find the source.
2. **TLS cert expired or misissued** — no cert-manager; rotate per
   `client-dev-local.md`
   [F7](client-dev-local.md#f7--browser-refuses-the-mcmh-webtransport-handshake)
   (MC and MH together). Infrastructure Team
3. **UDP blocked** by NetworkPolicy or cloud firewall — Infrastructure Team
4. **JWT validation failures** cascading — see Scenario 2
5. **QUIC listener stuck** — restart pod

**Recovery**:

```bash
# There is no `mh-service` Deployment. MH runs as per-instance Deployments
# mh-0 and mh-1, each with its own NodePort Service, ConfigMap and Kind port
# mapping (see infra/services/mh-service/service.yaml). Adding an instance is
# a manifest change, not a `kubectl scale` — horizontal scaling is NOT an
# incident-time lever for MH.

# Rotate TLS (if expired). There is NO cert-manager in this system: the
# mh-service-tls Secret is created imperatively by
# infra/kind/scripts/setup.sh:create_mh_tls_secret() from leaves generated by
# scripts/generate-dev-certs.sh. Do NOT `kubectl delete` the Secret — it is
# the only copy, both MH pods mount it at /etc/mh-tls, and they will fail to
# mount and stay down.
#
# Rotation is documented once, in
# docs/runbooks/client-dev-local.md#f7--browser-refuses-the-mcmh-webtransport-handshake,
# and covers MC and MH together: generate-dev-certs.sh regenerates both
# leaves, so rotating MH's alone leaves MC serving a leaf whose fingerprint
# no longer matches fingerprints.json. Follow F7; do not restate it here
# (docs/TODO.md "Auto rollout-restart on cert-secret recreate in setup.sh").
#
# Leaf validity is 14 days (Chrome's serverCertificateHashes cap), so expiry
# is routine, not an exceptional branch.

# Restart (QUIC listener stuck)
kubectl rollout restart deployment/mh-0 deployment/mh-1 -n dark-tower
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

# Check recent MC deploys (MC is the only legitimate caller). MC is a PAIR of
# singleton Deployments -- mc-0 alone is half the answer, and there is no
# `deployment/mc-service`. See docs/runbooks/mc-incident-response.md §MC Topology.
kubectl rollout history deployment/mc-0 -n dark-tower
kubectl rollout history deployment/mc-1 -n dark-tower

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
   sum by(event_type, status) (rate(mh_mc_notifications_total[5m]))
   ```

**Root Cause Investigation**:

```bash
# MH -> MC connectivity. NOTE: MC's health port cannot be curled from an MH
# pod at all -- health is 8081, not 8080, and MC's NetworkPolicy admits 8081
# from Prometheus only, so the request is DROPPED and times out, which reads as
# "MC is down". MH is admitted to MC on gRPC 50052 and nothing else, so 50052
# is the only MC port an MH-side reachability check can legitimately probe.
kubectl get endpoints mc-service -n dark-tower
# MC health, from the operator's machine, per instance:
#   kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
#   curl -i http://localhost:8080/health   # repeat for mc-1
# See docs/runbooks/mc-incident-response.md §MC Topology.

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

**Related Alerts**: `MHTokenRefreshFailures`, MC-side alerts (`MCHighMailboxDepthWarning`, `MCDown`); see also [MC Scenario 13: Unexpected MH Notifications](mc-incident-response.md#scenario-13-unexpected-mh-notifications) for the receiver-side view of the same RPC pair (notifications that did reach MC but reference unknown state).

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
5. **WebTransport server bind/listen/TLS-load failure** — pod never reaches Ready because the QUIC listener cannot start. Distinguishing signature: previous-pod logs show `webtransport::server::bind()` failing (TLS identity, bind address, UDP port). See [Scenario 14: WebTransport Server Startup Failure](#scenario-14-webtransport-server-startup-failure).

**Recovery**:

```bash
# Rollback if recent deploy
kubectl rollout undo deployment/mh-service -n dark-tower

# Increase memory (OOMKilled)
kubectl patch deployment/mh-service -n dark-tower -p \
  '{"spec":{"template":{"spec":{"containers":[{"name":"mh-service","resources":{"limits":{"memory":"2Gi"}}}]}}}}'
```

**Related Alerts**: `MHHighMemory`, `MHDown` (if all pods restart simultaneously); see [Scenario 14: WebTransport Server Startup Failure](#scenario-14-webtransport-server-startup-failure) for the bind/listen/TLS-load variant of this signal.

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

### Scenario 13: RegisterMeeting Timeout — Clients Kicked

**Alert**: No alert today; surfaces in `mh_register_meeting_timeouts_total` and oncall reports of clients being disconnected shortly after WebTransport handshake.
**Severity**: warning

**Symptoms**:
- `rate(mh_register_meeting_timeouts_total[5m])` non-zero — provisional WebTransport connections being kicked because `MC::RegisterMeeting` did not arrive within `MH_REGISTER_MEETING_TIMEOUT_SECONDS` (default 15s).
- Receipt-side correlate: `mh_grpc_requests_total{method="register_meeting"}` rate dropping vs baseline, or its `status="error"` slice rising — this is the "MH-side success rate" view (see `docs/observability/metrics/mh-service.md` §RegisterMeeting Metrics; the dashboard panel is **MH Overview → "RegisterMeeting Receipts by Status"**).
- MH logs at `warn` level: `"RegisterMeeting timeout expired, disconnecting client"` (target `mh.webtransport.connection`).
- Clients disconnect ~15s after JWT validation succeeds; client-side retry against the same MH likely fails the same way until coordination is restored.
- `mh_active_connections` gauge does NOT count provisional connections, so a sustained timeout rate may not move the active-conn dashboard panel.

> **Note**: `mh_register_meeting_timeouts_total` starts at zero in production and is incremented only on a real provisional-kick. A non-zero `rate(...[5m])` is the actionable signal — the existence of the time series is not (mirrors the GC Sc 5 first-emission pattern).

**Impact**: Subset of users unable to establish media sessions on this MH despite a valid JWT. **Active media sessions on this MH are unaffected** — they were promoted before the timeout. Severity is warning because (a) MH correctly enforces the timeout (this is the documented contract), (b) clients can usually fall back to other assigned MHs (active/active topology), and (c) the timeout is the security boundary that bounds stolen-JWT-against-unregistered-meeting exposure to ≤ `register_meeting_timeout_seconds`. **Do NOT treat raising this timeout as a mitigation** — see Recovery.

> **Receipt is not application, and `accepted` is not success.** `RegisterMeeting` succeeding tells you the request arrived and parsed — nothing more. Whether forwarding policy actually took effect is a *separate* signal: `mh_media_policy_applies_total{outcome}` (dashboard panel **MH Overview → "Forwarding-Policy Applies by Outcome"**), and on the response it is `applied_generation`, never `accepted`. A meeting can register cleanly, promote its pending connections, and still forward nothing. If clients connect and stay connected but no media flows, this scenario is the wrong one — check that panel. On `apply_failed` the **prior** policy stays live, and the `reason` field on the `mh.session.policy` WARN log distinguishes the causes (a saturated or wedged config-apply mailbox vs the aggregate egress-edge bound); it is deliberately not a metric label.
>
> **"Some people can't hear anything in a meeting that's otherwise working, and it's the ones who joined most recently."** That report is this cause, and nothing else about it looks like a media-handler problem. MH is given no meeting-ended signal, so a finished meeting's egress edges are never reclaimed — headroom is freed only when a still-live meeting re-asserts a *smaller* policy. The unreclaimable floor rises at the pod's meeting-completion rate, and as it approaches `MH_MAX_TOTAL_EGRESS_EDGES` every registration that would **add** an edge starts failing: a new meeting, or an existing healthy meeting **admitting a new participant**. Everyone already talking keeps talking, because an equal-generation re-assert short-circuits before the aggregate bound is tested and keeps reporting `applied` — so the panel shows `applied` and `apply_failed` climbing together and the pod presents as healthy. `RegisterMeeting` still answers `accepted: true` and GC keeps placing meetings here, because this is a resource guard and is deliberately not advertised as capacity. New arrivals blame their own network and drop, so the affected population removes itself from the signal.
>
> **Discriminator**: `reason=total_egress_edge_cap_exceeded` on the `mh.session.policy` WARN, with `installed_total_edges` and `installed_meeting_count` on the same line — a single fat policy and an accumulation of finished meetings look identical on the metric and are told apart only there. Onset is intermittent and worsens with uptime: it clears when some unrelated meeting happens to shrink and returns when it does not, which is what gets it misfiled as a transient.
>
> **Interim remedy: restart the pod — but read the next sentence before you do, because the recovery it used to promise does not exist.** **MC's ADR-0036 §8 re-assert cadence is NOT YET IMPLEMENTED.** MC pushes forwarding policy exactly once, when the first participant joins, and story task 13 deliberately deferred the cadence to the handler-restart story (see `docs/observability/metrics/mh-service.md` §`mh_media_policy_applies_total` and ADR-0036 §8's deferral). So restarting the pod **drops forwarding policy for every live meeting on it, and they are NOT recovered automatically** — no MC-side signal fires either, because the one-shot push already happened and `mc_media_generation_divergence` holds its last healthy value. **Treat the restart as an outage for those meetings**, re-established only by their participants rejoining; prefer a low-occupancy window and expect the meetings on that pod to end. (There is no session-drain path that preserves forwarding policy: MH's `terminationGracePeriodSeconds` settle window drains **in-flight connection teardown** and does nothing for policy, so a graceful restart loses the same meetings a hard one does.) This caveat is retired by the cadence task, not before. **Raising `MH_MAX_TOTAL_EGRESS_EDGES` is not the remedy**: the ceiling is consumed by finished meetings at whatever rate meetings finish, independent of concurrent load, so doubling it doubles time-to-onset and changes nothing else. Reclamation is tracked in `docs/TODO.md` §Media Path Obligations.
>
> **`no_generation` is a ROLLOUT signal — expected during an MC rollout, actionable after one.** MC emits `policy_generation` >= 1 as of story task 13, so the healthy steady state is `applied` carrying the traffic and `outcome="no_generation"` at zero. **Do not open an incident while an MC rollout is in progress**: a not-yet-upgraded MC pod legitimately still sends 0, so the two series coexist and the mix shifts as pods cycle. **Sustained `no_generation` after the rollout completes is actionable and the owner is MC** — it means MC failed to compute a forwarding assignment. Check `mc_media_policy_pushes_total` and MC's `mc.register_meeting.trigger` logs, not this pod. Confirm which of the two you are looking at by asking whether **mixed MC versions are live right now** — that, not a rollout object's progress, is what the rule turns on:
> ```bash
> kubectl get pods -n dark-tower -l app=mc-service \
>   -o jsonpath='{.items[*].spec.containers[*].image}' | tr ' ' '\n' | sort -u
> ```
> More than one image means a rollout is in progress; one means it has completed. **Do not reach for `kubectl rollout status deployment/mc-service`** — there is no Deployment by that name (MC ships as `mc-0` and `mc-1`; `mc-service` is a Service, a PDB and a container name), and the `NotFound` it returns reads as "no rollout in progress", which would flip you to actionable during exactly the legitimate mixed-version window this note exists to protect. See `docs/observability/metrics/mh-service.md` §`mh_media_policy_applies_total`.

> **Rollback awareness**: During a deliberate rollback of MH to a pre-RegisterMeeting build, **`mh_register_meeting_timeouts_total` stays flat at zero on the rolled-back pods** (old MH never knew about RegisterMeeting and so never sets up the provisional-accept window) — but client-side coordination breaks silently because MC's `RegisterMeeting` retries are exhausting. If the metric is flat zero on some pods but the user impact is real, check `kubectl rollout history deployment/mh-0 -n dark-tower` and `kubectl rollout history deployment/mh-1 -n dark-tower` for an in-progress rollback before treating as an incident (MH ships as two per-ordinal Deployments; a rollback can touch either, which is why the sentence above says "some pods"). See [MC Scenario 12](mc-incident-response.md#scenario-12-registermeeting-coordination-failures) for the MC-side rollback-aware triage.

**Immediate Response**:

1. Confirm the timeout rate is non-trivial vs background noise:
   ```promql
   sum(rate(mh_register_meeting_timeouts_total[5m]))
   /
   clamp_min(sum(rate(mh_register_meeting_timeouts_total[1h])), 0.001)
   ```
   A 5m rate that is several multiples of the 1h baseline indicates an active incident rather than a steady-state low-rate sequencing race.
2. Cross-check MC: this is the upstream symptom of MC failing to send `RegisterMeeting`. See [MC runbook Scenario 12: RegisterMeeting Coordination Failures](mc-incident-response.md#scenario-12-registermeeting-coordination-failures).
3. Check MH→MC and MC→MH network reachability — if MC is healthy but cannot reach MH on the gRPC port, RegisterMeeting will never arrive at MH.

**Root Cause Investigation**:

```bash
# Confirm provisional-kick rate from raw /metrics (counter increase since pod start).
# THE COUNTER IS PER-POD. Use the AFFECTED pod (`mh-0` or `mh-1`) — a quiet pod
# proves nothing about a loud one, and a kick rate read off the wrong ordinal is
# how this gets closed as "not reproducing". For the fleet view use the
# per-pod split of the rate query from Detection above:
#   sum by(instance) (rate(mh_register_meeting_timeouts_total[5m]))
# RATE, not the raw counter: the counter is cumulative since each pod's start,
# so across pods with different uptimes the numbers are not comparable and a
# recently-restarted pod reads LOW — i.e. the likeliest suspect in this
# scenario would look cleanest.
# PORT 8083, not 8080: MH_HEALTH_BIND_ADDRESS is 0.0.0.0:8083 (configmap.yaml)
# and the pod exposes containerPort 8083 — there is no listener on 8080, so a
# `8080:8080` forward yields connection-refused, not an empty grep.
# `instance` is `<pod-ip>:8083`, not a pod name (the mh-service scrape job has
# no pod-name relabel), so map it to an ordinal with:
#   kubectl get pods -n dark-tower -l app=mh-service -o wide
kubectl port-forward -n dark-tower deployment/mh-0 8083:8083 &
curl -s http://localhost:8083/metrics | grep mh_register_meeting_timeouts_total
# Cross-check receipt-side: how many RegisterMeeting RPCs is MH actually receiving?
curl -s http://localhost:8083/metrics | grep 'mh_grpc_requests_total.*register_meeting'
kill %1

# Receipt-side rate by outcome (MC→MH RPC success/error from the MH receiver's view)
sum by(status) (rate(mh_grpc_requests_total{method="register_meeting"}[5m]))

# MH logs for the timeout warning (no JWT body — log line carries connection_id + meeting_id only)
kubectl logs -n dark-tower -l app=mh-service --tail=500 \
  | grep -i "RegisterMeeting timeout"

# Configured timeout for this pod (don't run `kubectl exec ... -- env` — leaks service token)
kubectl describe pod -n dark-tower -l app=mh-service \
  | grep MH_REGISTER_MEETING_TIMEOUT_SECONDS

# MC→MH gRPC reachability (run from MC pod, since MC is the originator of RegisterMeeting)
kubectl exec -it deployment/mc-0 -n dark-tower -- \
  grpcurl -plaintext mh-service.dark-tower.svc.cluster.local:50051 list
# NOTE: the exec TARGET is a Deployment (`mc-0`/`mc-1`); the grpcurl ADDRESS is a
# Service and `mh-service` is genuinely its name. Do not "correct" the address.
```

**Common Root Causes**:

1. **MC failing to send RegisterMeeting** — most common. MC RegisterMeeting trigger is exhausting retries (3 attempts, 1s/2s backoffs). See [MC Sc 12](mc-incident-response.md#scenario-12-registermeeting-coordination-failures).
2. **MC→MH NetworkPolicy / firewall** — MC reaches MH for some endpoints but not the gRPC port. Infrastructure Team.
3. **MC overloaded** — first-participant trigger is queued behind other work; the 1+2s retry budget elapses before the 15s MH-side window. See [MC Sc 1: High Mailbox Depth](mc-incident-response.md#scenario-1-high-mailbox-depth).
4. **MC restarted mid-flight** — new MC instance re-runs registration; existing provisional connections were kicked before the restarted MC could re-send. Self-healing on client retry.
5. **Client sequencing bug** — client connecting to an MH for a meeting GC has not yet assigned. `mh_active_connections` near zero with sustained timeout rate is the signal. Escalate to Client Team.

**Recovery**:

```bash
# Restoration is on the MC side — MH itself is healthy by design when this fires.
# 1. Triage MC RegisterMeeting health (MC runbook Sc 12).
# 2. Once MC is delivering RegisterMeeting again, no MH action is needed:
#    new client connections will be promoted normally; affected users
#    re-establish via active/active fallback to other assigned MHs.
# 3. Monitor for resolution — again PER-POD: watch the affected ordinal
#    (`mh-0` or `mh-1`). Recovery on one pod is not recovery on the other.
kubectl port-forward -n dark-tower deployment/mh-0 8083:8083 &
watch -n 15 'curl -s http://localhost:8083/metrics | grep mh_register_meeting_timeouts_total'
kill %1
```

Expected recovery time: bounded by MC-side fix (see MC Sc 12). Once MC starts delivering RegisterMeeting again, the MH-side timeout rate drops to zero within the next provisional-accept window (~15s); affected clients reconnect on their own retry timer (typically 1-5s), so end-to-end user recovery is 30-60s after MC is restored.

**Rollback nuance**: If you are mid-deploy and considering rolling MH back to a pre-RegisterMeeting build to mitigate, **clients are NOT stranded**. Old MH does not understand `RegisterMeeting`, so MC will exhaust retries and log them — but JWT validation still works at the old MH and clients connect normally. The active/active topology means clients fall back to remaining MHs even if one is on the old build. Coordinate with MC team before rollback so they can suppress retry-storm noise.

**Do NOT raise `MH_REGISTER_MEETING_TIMEOUT_SECONDS` as a mitigation.** The 15s default is the authorization boundary that bounds stolen-JWT-against-unregistered-meeting exposure. Sustained timeouts are a coordination problem; widening the window only increases the security blast radius. If you find yourself wanting to tune this, escalate to Security Team for review — do not patch.

**Related Alerts**: MC-side `MCHighMailboxDepthWarning` / `MCHighMailboxDepthCritical` (upstream cause if MC trigger is queued).

**Dashboards**: MH Overview → "RegisterMeeting Timeouts (R-26)" (this scenario's headline metric); MH Overview → "RegisterMeeting Receipts by Status" (receipt-side correlate); MC Overview → "RegisterMeeting RPC Rate by Status" + "RegisterMeeting RPC Latency (P50/P95/P99)" (sender-side correlate).

---

### Scenario 14: WebTransport Server Startup Failure

**Alert**: `MHDown` (firing because pod cannot become Ready) and/or `MHPodRestartingFrequently`. Distinct from runtime per-connection rejections (Scenario 5) — pod cannot bind the QUIC listener at boot, so no client traffic is accepted at all.
**Severity**: page

**Symptoms**:
- Pod in `CrashLoopBackOff` with the previous-pod logs showing one of:
  - `"Invalid WebTransport bind address"` — emitted from `webtransport::server::bind()` when `MH_WEBTRANSPORT_BIND_ADDRESS` does not parse as a `SocketAddr`.
  - `"Failed to load TLS certificate"` — emitted when the cert/key Secret is missing, unreadable, or malformed; the underlying error appears in the structured `error` field on the same log line.
  - `"Failed to create WebTransport endpoint"` — emitted when the QUIC endpoint cannot bind. The OS-level cause (`Address already in use`, permission denied, UDP unavailable) is in the `error` field of this log line, not in the top-level message.
- Readiness probe never succeeds; new pods do not register with GC.
- `mh_webtransport_connections_total` stays flat at zero (no traffic ever reaches the pod).
- `kubectl get pods` shows `0/1 Ready` for the affected pod.

**Impact**: Affected MH pod accepts no client traffic. If the failure is fleet-wide (deploy regression, bad config), this is a full MH outage — escalates to `MHDown` and meeting joins lose their media path.

**Immediate Response**:

1. Confirm scope — single pod vs all pods:
   ```bash
   kubectl get pods -n dark-tower -l app=mh-service
   ```
2. Capture previous-pod logs before they're rotated by the next restart:
   ```bash
   kubectl logs -n dark-tower <pod> --previous --tail=300 \
     | grep -iE "bind|listen|tls|quic|certificate|webtransport"
   ```
3. If fleet-wide and recent deploy is the trigger, prepare rollback (see Recovery).
4. Notify MC team — meeting assignments to this MH need to be redirected (GC will mark unhealthy on registration timeout, but explicit notification shortens MTTR).

**Root Cause Investigation**:

```bash
# Pod events for bind / cert / scheduling errors
kubectl describe pod <pod> -n dark-tower | tail -50

# Configured bind address (default 0.0.0.0:4434) — inspect via describe, NOT `exec env`
kubectl describe pod -n dark-tower -l app=mh-service \
  | grep -E "MH_WEBTRANSPORT_BIND_ADDRESS|webtransport"

# TLS secret exists and is mountable
kubectl get secret -n dark-tower mh-service-tls -o yaml \
  | grep -E "tls.crt|tls.key" | head -4

# Certificate validity (run only when TLS load is the suspected cause; do not paste bodies)
kubectl exec -it deployment/mh-service -n dark-tower -- \
  openssl x509 -in /certs/tls.crt -noout -dates -subject 2>/dev/null \
  || echo "Pod not running — inspect cert via secret directly"

# Service definition exposes UDP/4434
kubectl get svc -n dark-tower mh-service -o yaml | grep -A3 "port:"

# NetworkPolicy / cloud firewall allows UDP ingress on 4434
kubectl describe networkpolicy mh-service -n dark-tower
```

**Common Root Causes**:

1. **TLS identity load failure** — cert-manager Secret missing, malformed, or unreadable. Logs show `"Failed to load TLS certificate"` from `webtransport::server::bind()` with the underlying cause in the `error` field.
2. **Bind address misconfigured** — `MH_WEBTRANSPORT_BIND_ADDRESS` not parseable as `SocketAddr`. Logs show `"Invalid WebTransport bind address"`.
3. **UDP port already in use** — host networking conflict or another container holding `:4434`. Logs show `"Failed to create WebTransport endpoint"` with `error` field containing `"Address already in use"`.
4. **Insufficient capabilities** — pod cannot bind UDP socket (e.g., privileged-port restriction, seccomp denial). Logs show permission errors.
5. **Bad deploy** — recent rollout changed the image, config, or volume mounts and broke the bind path. `kubectl rollout history` shows a recent revision.

**Recovery**:

```bash
# Option A: Rollback (recent bad deploy)
kubectl rollout history deployment/mh-service -n dark-tower
kubectl rollout undo deployment/mh-service -n dark-tower
kubectl rollout status deployment/mh-service -n dark-tower

# Option B: TLS identity broken (Secret missing, malformed, or expired leaf).
# There is NO cert-manager in this system — do NOT delete the Secret. It is
# the only copy (created imperatively by setup.sh:create_mh_tls_secret() from
# leaves generated by scripts/generate-dev-certs.sh), both MH pods mount it
# at /etc/mh-tls, and deleting it deepens an outage you are already in.
#
# Rotation is documented once, in
# docs/runbooks/client-dev-local.md#f7--browser-refuses-the-mcmh-webtransport-handshake
# It covers MC and MH together (generate-dev-certs.sh regenerates both
# leaves) and already includes the rolling restart, so nothing is repeated
# here. F7's last step restarts the dev server — that one matters only if a
# browser client is in the loop, not for recovering the pod bind.

# Option C: Fix bind address (config/ConfigMap regression)
kubectl edit configmap mh-service -n dark-tower
# Then:
kubectl rollout restart deployment/mh-service -n dark-tower

# Option D: Single-pod port collision (rare; reschedule the pod)
kubectl delete pod <pod> -n dark-tower
```

Expected recovery time: 1-3 minutes for rollback. TLS rotation is **not**
automatic — it requires the F7 sequence (a `setup.sh` run plus a rolling
restart), so budget for it rather than waiting on re-issuance.
UDP-port-collision reschedule is 30-60s. Bind-address ConfigMap fix +
rolling restart is 2-3 minutes.

**Related Alerts**: `MHDown`, `MHPodRestartingFrequently`, `MHHighRegistrationLatency` (downstream — registration cannot complete until WebTransport server is up and pod is Ready).

---

### Scenario 15: Media Sessions Declining — No Sender Binding

**Symptom.** Clients connect to MH and get no audio. MH looks **healthy**: readiness
green, `mh_webtransport_connections_total{status="accepted"}` climbing, handshake
latency normal. `mh_media_session_starts_total{outcome!="started"}` is climbing.

Since the participant → `sender_id` binding contract landed, MC's response to
`NotifyParticipantConnected` is a **blocking precondition** for a media session: it
carries the ordinal MH binds the connection's media route to. No valid binding ⇒ no
media session ⇒ connection closed with a counted reason. This is fail-closed by
design — the alternative is forwarding with a guessed sender, which in a
one-participant meeting is indistinguishable from correct behaviour.

#### Step 1 — partition on WHICH SERVICE to open, before reading the label

**Do not start by reading the outcome value.** Four of the seven values name MC, and
**two of those four do not mean MC is unwell**. Three rows below say *do not page MC*.

| Outcome | Start in | Why |
|---|---|---|
| `declined_sender_binding_conflict` | **MH** | The `(meeting, sender)` ordinal is held by a *different* participant. Check MH's unbind path **first** — see Step 4. |
| `declined_mc_endpoint_unknown` | **MH** | No usable `mc_grpc_endpoint` for the meeting. **MH never dialled — do NOT page MC.** See Step 5. |
| `declined_mc_auth_rejected` | **MH** | MH's *outbound credential* failed: MC refused it, or MH could not build one. **MH's outbound auth, not MC's health.** See Step 6. |
| `declined_mc_unavailable` | **MC reachability** | MH dialled and got no usable answer within the ~48s budget. |
| `declined_no_sender_binding` | **MC's answer** | MC answered `0` — "I do not know this participant." Go to Step 2. |
| `declined_sender_binding_out_of_range` | **MC's allocator** | Contract violation. Should read **zero forever** — see Step 3. |

**Timing tiebreaker, and its limits.** `declined_mc_endpoint_unknown` fires **fast**
(terminal on the first attempt — re-parsing the same string can never succeed).
`declined_mc_unavailable` fires only after the **full ~48s** retry budget. If you are
watching the counter live, latency-to-first-increment separates *those two* before the
label does.

> **This tiebreaker does NOT extend to `declined_mc_auth_rejected`.** That value has two
> routes with different timings — a refusal declines immediately, a credential-build
> failure only after the full budget. It is *usually* fast but not reliably so. Stated
> explicitly because a partial timing rule is more dangerous than no timing rule: a
> responder who learned "timing separates these" will apply it to the next value unless
> told not to.

#### Step 2 — `declined_no_sender_binding`: read the MC-side counter

MC answered `0`. That is an honest answer meaning "I cannot resolve this participant" —
MC never invents an ordinal. The cause is on MC's side; query it there:

```promql
mc_media_sender_binding_responses_total
```

| MC-side reading | Cause | First move |
|---|---|---|
| **Series absent or flat** | **Version skew** — the mc-service image predates the `sender_id` field | Rebuild and redeploy MC. See §Rollout ordering in `mh-deployment.md`. |
| `outcome="participant_unknown"` | **Either** a transient join race **or** a systematic identity mismatch — see below | Check whether it is sustained |
| `outcome="meeting_unknown"` | Routing/lifecycle fault — MC has no such meeting | Not self-clearing; investigate meeting placement |
| `outcome="registry_full"` | **Capacity** — MC's per-meeting connection cap refused the registration, so it correctly answered `0` rather than handing back an ordinal for a connection it is not tracking | **Never self-clearing.** Raise the cap or add MC capacity. **Do not triage as a join race.** |
| `outcome="user_ambiguous"` | **One user, two participants.** The user joined twice (two devices), so one token `sub` maps to two roster entries and the question has no single answer | **Never self-clearing, and no operator remedy exists** — see below. Have the user leave on one device. |

> **`user_ambiguous` is user-triggerable, so expect it in normal traffic.** MH names the
> connecting party by the validated meeting token's `sub` — a contract MUST, and the
> defence against a client asserting another identity — while MC mints a fresh
> `participant_id` per join and does not bar the same user joining twice. A user on a
> phone *and* a laptop therefore produces two roster entries for one `sub`, and MC answers
> `0` rather than guessing: either candidate would bind this connection to an ordinal
> possibly belonging to that user's **other** participant, so MH would stamp the wrong
> `sender_id` on these frames — right half the time and undetectable when wrong. **MC
> refusing is the correct behaviour.**
>
> **There is no operator remedy.** Restarting nothing helps, capacity does not help, and
> waiting does not help — the remedy is a contract change (tracked in `docs/TODO.md`).
> The only field action is to have the user leave on one device. Do not escalate this as
> an MC defect; MC is behaving correctly and refusing to guess.

> **`participant_unknown` is NOT only a join race, and reading it that way will cost you
> the incident.** A low, transient rate *is* the expected race between MH's connect and
> MC's join completing. **Sustained** `participant_unknown` at or near 100% of attempts
> means the two services do not agree on participant identity at all — MH names the
> participant by its token `sub`, and if MC keys its roster by a different value, MC can
> never resolve *any* participant. This is not a race that will clear; it is a systematic
> mismatch. **Discriminator: the ratio.** A race is a small fraction of attempts and
> decays; a mismatch is ~100% and flat.
>
> This is not hypothetical. It is the exact failure this contract's own first live
> cluster run produced: `mc_media_sender_binding_responses_total{outcome="participant_unknown"} 5`
> against `mh_media_session_starts_total{outcome="declined_no_sender_binding"} 5` —
> every attempt, no successes.

**Why "series absent" is a legitimate signal here, when this runbook warns against
absence-as-signal everywhere else.** An MC binary predating this contract does not merely
fail to set the field — **it does not have `mc_media_sender_binding_responses_total`
at all.** So its absence distinguishes "old MC" from "new MC answering `0` honestly",
which is the discriminator the wire format itself does not carry (the field is a bare
`uint32`, so absent and zero are the same byte on the wire).

Two cautions, because absence-as-signal is fragile:

1. **An absent series and a scrape failure look identical.** Before reading absence as
   version skew, confirm MC exports *any* series at all
   (`up{job="mc-service"}`, or any `mc_*` metric). Otherwise an unreachable `/metrics`
   or a relabeling drop sends you to rebuild an image that was fine.
2. **DELETE THIS ROW once no pre-contract MC image can be deployed.** After that point
   absence means something else entirely and this row misleads. Nothing else will prompt
   the deletion; it is tracked in `docs/TODO.md`.

#### Step 3 — `declined_sender_binding_out_of_range`

MC returned a `sender_id` above 65535. **This should read zero forever.** It means MC
violated its own contract — a broken allocator, **or the control plane answering MH is
not MC.**

The MC→MH gRPC channel has no cryptographic peer authentication (tracked in
`docs/TODO.md`; compensating controls are network policy plus an MC-attested endpoint).
**This counter is the only signal separating "MC has an allocator bug" from "something
that is not MC is answering MH."** Treat **any** non-zero as a **security
escalation**, not merely a bug. Escalate to the security owner alongside
meeting-controller.

`MHMediaSenderBindingOutOfRange` fires on a **single occurrence** — there is no
`for:` debounce, because a counter whose steady state is zero forever has no
noise floor to debounce and any non-zero is the incident. Do not read a
low absolute count as "not yet worth acting on": one is the threshold.

> **This counter is the ceiling of the detection story, not its extent.** It
> catches an *out-of-range* answer. A forged binding **inside** `1..=65535` is
> indistinguishable from a real one at MH, so **a flat series here is not
> evidence that no impersonation occurred.** The escalation this counter
> triggers is worth running; its silence proves nothing.

#### Step 4 — `declined_sender_binding_conflict`: is the incumbent binding live or stale?

The ordinal is held by a *different* participant. The question is whether that incumbent
binding is **live** or **stale**. Check MH's log for the `meeting_id` on the decline:

- **Stale — MH's fault.** The conflict correlates with a *prior* connection for that
  meeting having gone away: look for an earlier "Connection closed and cleaned up" for
  that `meeting_id` with no live connection behind it. MH failed to release the ordinal,
  so the conflict **recurs on every reconnect for the same meeting and clears on an MH
  restart.**
- **Live — MC's fault.** Two connections for that meeting are concurrently up and MC
  handed both the same ordinal. No preceding teardown.

**Check MH first** — not because it is more likely, but because it is **the cheaper
hypothesis to falsify.**

> **Do NOT restart MH as a remedy.** "Clears on an MH restart" is a *diagnostic*, not a
> fix: restarting clears the symptom and destroys the evidence you need to find the
> unbind-path defect.

`MHMediaSenderBindingConflict` also fires on a **single occurrence**, for the same
reason as Step 3. An ordinary reconnect cannot produce this outcome —
`SenderBindings::bind` compares the incumbent's `participant_id`, so a
same-participant re-bind takes over the entry rather than conflicting — so there is
no benign population here to debounce against.

#### Step 5 — `declined_mc_endpoint_unknown`

MH has no usable `mc_grpc_endpoint` for the meeting — none recorded, or one that will
not parse. **MH never reached the network, so MC can be perfectly healthy while this
climbs. Do not page MC.**

The endpoint MH holds is on the decline log line. Read it.

> **One route here has no remedy, and it is benign: MH shutting down.** The
> endpoint lookup goes through the session actor, so a connection that reaches
> this point while MH is draining finds no endpoint and declines. That is
> **correct behaviour during a shutdown or a rolling deploy**, it is not a
> registration fault, and there is nothing to fix. Discriminate on coincidence
> with a pod terminating: if the increments stop when the rollout completes and
> the log line names a meeting whose registration was fine, you are looking at
> the shutdown route. Only the *registration* route below has a remedy. (Stated
> because the common route's remedy — "fix what `RegisterMeeting` carried" — is
> actively misleading applied to this one: there is nothing wrong with the
> registration, and a responder who hunts one during a deploy is hunting
> nothing. `declined_mc_unavailable` carries the equivalent line under §Expected
> non-incidents; this value needs its own because it is a *different* mechanism
> reaching the same benign conclusion.)

**Expect this to be near-zero.** MH's `RegisterMeeting` already validates
`mc_grpc_endpoint` at push time — non-empty, length-bounded, scheme in
`{http://, https://, grpc://}` — so the obvious defects are rejected loudly and
attributably at registration and never reach a connection. What squeezes through is
narrow: a string that passes those checks and still will not parse (`"http://"`
*exactly*, or an embedded space), plus a `None` race. **If this fires, the endpoint in
the log is almost certainly malformed in a way MH's own registration validation was too
weak to catch** — do not spend the night hunting a plausible misconfiguration the
control plane would have rejected.

#### Step 6 — `declined_mc_auth_rejected`: MH's outbound credential

MH's outbound credential failed. Two routes, **same first move, different second move.**

**First move:** look for this line in MH's log:

```
target: mh.grpc.mc_client   level: ERROR
"Authorization header parse failed"
```

- **Line PRESENT ⇒ credential-build failure.** MH could not construct a `Bearer` header
  from the token it holds — the **token itself is malformed**. Stay in MH; check AC and
  what it last issued.
  > **This is an AC-side defect, not an MH bug.** It means AC issued a token containing
  > bytes illegal in an HTTP header value. Escalate to the auth-controller owner —
  > otherwise someone finds the malformed token, restarts something, and the defect stays
  > in AC.
- **Line ABSENT while this value climbs ⇒ refusal.** MH built a valid credential and MC
  rejected it. Read `mc_caller_type_rejected_total` on MC. This is a deployment or
  credential misconfiguration, **not** an MC health problem.

> **`mh_token_refresh_total{status="error"}` is worth reading as context in BOTH cases,
> but it does NOT separate them.** A refresh can *succeed* and return a token MH cannot
> put on the wire (increments `status="success"`, then fails to build on every call), and
> a refresh can *fail* while the previously cached token still builds fine and gets
> refused. The counter is wrong in both directions as a discriminator; the log line is
> not.

#### Expected non-incidents

- **A brief `declined_mc_unavailable` bump during an MC rolling deploy is expected.** MC
  gates readiness on `httpGet /ready` port 8081 while this call is gRPC on 50052, so a
  Ready-but-not-yet-serving window is normal. **Sustained past the rollout is not.**
- **Low-rate, decaying `participant_unknown`** join races are normal. See the Step 2
  caution for when they are not.

#### Known blind spots

- **The ~48s binding wait lands AFTER `mh_webtransport_handshake_duration_seconds` is
  recorded.** During an MC outage the **handshake latency panel reads healthy** and
  `MHWebTransportHandshakeSlow` cannot fire. **Do not clear MH on that panel.** A
  dedicated instrument on the binding step is tracked in `docs/TODO.md`.
- **Connections in the binding wait count against `MH_MAX_CONNECTIONS`.** This is *not
  worse* than before the contract landed — the previous behaviour held a useless
  connection until idle timeout, where this releases at ~48s — but the pressure is real
  under a sustained MC outage.

#### Do not "fix" this by shortening the retry budget

The ~48s worst case (`MC_CONNECT_TIMEOUT` 5s + `MC_RPC_TIMEOUT` 10s per attempt,
`MAX_RETRY_ATTEMPTS` 3, 1s/2s backoff — `crates/mh-service/src/grpc/mc_client.rs`) is
**functioning as a rate limiter on the reconnect herd.** Clients cannot yet distinguish a
retryable close from a terminal one, so they retry on their own cadence; cutting the
budget to ~10s would multiply the retry rate roughly 5× against the service that is
already the bottleneck. **The deadline and the client-side close code are one decision,
not two** — see `docs/TODO.md`.

**Related Alerts**: `MHMediaSessionDeclineRate`, `MHMediaSenderBindingOutOfRange`,
`MHMediaSenderBindingConflict`, `MHHighWebTransportRejections` (a sustained MC outage
drives connections toward `MH_MAX_CONNECTIONS`), `MHTokenRefreshFailures` (context for
Step 6), `MHMCNotificationFailures` (same MC-unavailability cause at attempt
granularity — 3 retries there for 1 decline here).

---

### Scenario 16: Ingress Datagrams Received But Never Read

**Alert**: `MHIngressDatagramsNeverRead` (`warning`)

**Symptom.** `mh_media_frames_dropped_total{reason="transport_receive_dropped",
direction="ingress"}` is rising: MH's QUIC layer received datagrams that no media
ingress loop ever read. Publishers on the affected instance lost audio.

> **READ THIS BEFORE ANYTHING ELSE — THIS ALERT IS FORENSIC, NOT DETECTIVE.**
>
> The counter is a difference between quinn's received-frame count and what the
> ingress loop actually read, and that difference is only final **at connection
> close**. Nothing increments while a session is live, and meetings run for an
> hour.
>
> Two consequences, and both invert the habits that work for every other
> scenario in this runbook:
>
> - **The incident is over when you are paged.** You are reading a record, not a
>   live fault. Nothing you do to the fleet in the next ten minutes changes this
>   number. Do not restart pods to "clear" it.
> - **A FLAT SERIES DURING AN ACTIVE NO-AUDIO INCIDENT IS NOT EVIDENCE THAT
>   INGRESS IS HEALTHY.** The sessions are still open, so the counter has not
>   been computed yet. This is the single most likely way to be misled here.
>
> **MH has no detective signal for silent ingress loss.** That is a known,
> recorded gap, not an oversight of this runbook. If you need to know whether loss
> is happening *right now*, the only in-tree substitute is
> `mh_media_frames_forwarded_total{direction="ingress"}` failing to track the
> expected publisher frame rate on the steered instance, and that requires you to
> already know the expected rate.
>
> **The gap has a defined, small closure — do not re-derive it.** The *accounted*
> half is already live: `forwarded{direction="ingress"} + dropped{direction="ingress"}`
> is exactly the set of datagrams the ingress loop read (the code increments
> `forwarded{ingress}` on the `no_subscriber` path specifically to keep that sum
> whole). The missing half is one series — a periodic sum of quinn's
> `frame_rx.datagram` over live connections — after which the live loss rate is a
> PromQL subtraction with no teardown required. Spec and sampling-cadence
> trade-off are filed in `docs/TODO.md`.

#### Step 1 — establish that this is loss at all, before attributing it

The value is an **upper bound on real loss, not a measurement of it.** Any client
past the JWT gate can inflate it: datagrams carrying an unmatched HTTP/3
session-id varint still increment quinn's `frame_rx.datagram` while `wtransport`
discards them. So a rising series has three candidate causes with three different
owners, and the ratio below is what separates the first two.

```promql
# The comparison that attributes. Run both. The 2h window matches the alert and
# is load-bearing — see the note below before shortening it.
sum by(instance) (rate(mh_media_frames_dropped_total{reason="transport_receive_dropped", direction="ingress"}[2h]))
sum by(instance) (rate(mh_media_frames_forwarded_total{direction="ingress"}[2h]))
```

> **Do not shorten these windows to "see it more clearly."** The first series is
> emitted in one instant at connection close for a whole session; the second
> accrues continuously. Over a window shorter than a typical meeting the ratio
> between them inflates by roughly `session_length / window` — at 30 minutes, an
> hour-long session losing a true 5% reads about 9.5%. Shortening the window is
> the most natural thing to try here and it will make you over-state the loss.

| Reading | Meaning, and who owns it |
|---|---|
| `transport_receive_dropped` rising, `forwarded{ingress}` **flat** | Clients are publishing outside their media sessions' lifetimes. **Client / steering timing — NOT MH's forward path.** Go to Step 2. |
| **Both** rising together | Media is flowing normally and a fraction is being lost. The ratio is the whole story. Go to Step 3. |
| `transport_receive_dropped` rising, confined to one client or org | Suspect counter inflation rather than loss. Go to Step 4. |

> **Do not skip to "MH is losing media."** The predecessor of this signal — the
> triage row that read `started` climbing with flat ingress frames as
> "client-side, not MH" — misdirected three sessions by attributing a shape it
> could not actually resolve. This table is the corrected form and it earns its
> attribution from the *comparison*, not from either series alone. If neither row
> matches cleanly, say so in the incident channel rather than picking the closest.

#### Step 2 — clients publishing outside a session's lifetime

Expected in small numbers: a client reconnecting to a handler it already holds a
`SendDirective` for can race the new connection's setup. That floor is why the
alert is a ratio over 30 minutes rather than an occurrence trigger.

Escalate beyond the floor when the rate is sustained and **not** correlated with
reconnect churn:

```promql
# Is this reconnect churn, or steady-state client misbehaviour?
sum(rate(mh_webtransport_connections_total{status="accepted"}[30m]))
sum by(outcome) (rate(mh_media_session_starts_total[30m]))
```

- Rate tracks connection churn ⇒ setup races. Expected; watch, do not page anyone.
- Rate is flat-and-high while churn is low ⇒ a client build is publishing before
  its `SendDirective` or after teardown. **Owner: client.** Identify the client
  version from the join path; MH cannot attribute this from its own metrics.
- `mh_media_session_starts_total{outcome=~"declined_.*"}` also climbing ⇒ you are
  probably looking at an MC-availability incident, not this one. The declined-path
  datagrams are counted separately on `reason="no_media_session"` **precisely so
  they cannot land in this alert** — if you see both, work Scenario 15 first and
  come back.

#### Step 3 — quinn's receive buffer evicting under scheduling pressure

This is the MH-owned cause and, if the inequality below holds, it is a **sizing
defect, not a transient.**

**Confirm the relationship before concluding it — do not take this step's word
for it.** The conclusion here is an inequality between two constants that live in
`crates/mh-service/src/config.rs`, and either can be retuned without anyone
editing this runbook. Read the current values:

| Constant | What it bounds |
|---|---|
| `DATAGRAM_RECEIVE_BUFFER_BYTES` | quinn's ingress datagram buffer, in **bytes** |
| `NOMINAL_AUDIO_FRAME_BYTES` | one 20 ms audio frame, for converting the above into frames |
| `INGRESS_QUEUE_FRAMES` | MH's own per-connection ingress ring, in **frames** |

**The defect is present when `DATAGRAM_RECEIVE_BUFFER_BYTES / NOMINAL_AUDIO_FRAME_BYTES`
is LESS THAN `INGRESS_QUEUE_FRAMES`** — quinn's buffer holds fewer frames than
MH's own ring, so quinn evicts before MH's ring ever reaches its bound. MH sheds
silently, and `reason="ingress_queue_overflow"` may never fire at all. Each frame
is 20 ms, so the frame counts convert directly to absorption windows.

**If the inequality does NOT hold on the values you just read, this step is not
your cause** — MH's ring binds first, `ingress_queue_overflow` is the counter
that should be moving, and you should return to Step 2. It also means the
`docs/TODO.md` entry below was closed without this runbook being updated; say so
in the incident channel.

At the values current when this step was written the inequality held with roughly
a factor of two, which is what the `docs/TODO.md` entry (§Devloop… — search
`ingress_queue_overflow`) records and argues for fixing. **That entry carries the
arithmetic; this step deliberately does not restate it**, because a stale
inequality here is a wrong diagnosis under time pressure rather than a confusing
one.

```promql
# If Step 3 is the cause, these correlate. If they don't, reconsider Step 2.
rate(container_cpu_usage_seconds_total{container="mh-service"}[5m])
sum by(reason) (rate(mh_media_frames_dropped_total{direction="ingress"}[30m]))
```

- Correlates with CPU pressure or pod restarts ⇒ scheduling starvation outlasted
  quinn's absorption window (the frame count you computed above, times 20 ms).
  Relieve the pressure (Scenario 8); the loss stops.
- `ingress_queue_overflow` reading **zero while `transport_receive_dropped`
  rises** is the expected shape, not a contradiction — it is the inversion above.
  Do not treat the zero as evidence MH's ring is healthy.
- **There is no config knob to turn.** `DATAGRAM_RECEIVE_BUFFER_BYTES` is a
  compile-time constant, and raising it is a wire-visible change because quinn
  derives the advertised `max_datagram_frame_size` from the same field. The
  ordering defect and the missing startup validation (the ingress-side counterpart
  to `ConfigError::EgressQueueDoesNotBindFirst`) are tracked in `docs/TODO.md`.
  **Do not hand-patch the constant during an incident.**

#### Step 4 — counter inflation rather than loss

If the rise is confined to a narrow set of connections and `forwarded{ingress}`
for those meetings looks normal, the likely reading is a client sending datagrams
with an unmatched HTTP/3 session-id varint — counted by quinn, discarded by
`wtransport`, never loss at all.

MH cannot distinguish inflation from loss on its own metrics; that is a ceiling of
this signal, not a gap in this procedure. Corroborate from the client side, and if
the pattern looks deliberate rather than a build defect, treat it as a probing
signal and notify the Security owner — the same routing as
`MHCallerTypeRejected` (Scenario 7).

> **This step, and only this step, rests on a `wtransport` implementation
> detail** — that an unmatched session-id varint is counted at frame-decode and
> discarded above it. Verified against 0.7.1; `Cargo.lock` pins 0.7.2. If a
> future version drops such datagrams before quinn's counter, **this cause
> disappears and Step 4 becomes dead procedure** — the reading would then be
> genuine loss and belongs in Step 2 or 3. Re-check on any `wtransport` upgrade.
> Steps 1-3 and the alert's `warning` severity do **not** depend on this: the
> alert is a ratio for an independent reason (a sustained MC outage dominates the
> numerator), so the rule shape survives even if this paragraph does not.

#### What NOT to do

- **Do not restart MH pods.** The incident already ended; a restart destroys the
  connection-close accounting that would have told you the scale.
- **Do not use this counter as an SLI or in a capacity calculation.** It is
  client-inflatable and an upper bound. This is why no burn-rate rule rests on it.
- **Do not read a flat series as ingress health** during an open incident. See
  the forensic note at the top.

**Related Alerts**: `MHHighCPU` / `MHHighMemory` (Scenario 8 — the pressure that
drives Step 3), `MHMediaSessionDeclineRate` (Scenario 15 — work it first if both
fire), `MHCallerTypeRejected` (Scenario 7 — the Step 4 escalation route).

---

### Scenario 17: Media Datagram Drop

**Alert**: `MHMediaEgressQueueOverflowRate`
**Severity**: Warning
**Runbook Section**: `#scenario-17-media-datagram-drop`

> **Numbering note.** This is Scenario **17**, not 15. Scenarios 15 and 16 in this file were taken
> by earlier tasks in the same story and four shipped alert rules in
> `infra/docker/prometheus/rules/mh-alerts.yaml` cite their anchors. Renumbering would have broken
> four guard-resolved links.

**What this is.** Audio is carried one frame per **QUIC datagram** (ADR-0036 §1). Datagrams are
lossy by design and by transport: there is no retransmission and no ordering guarantee. Drops occur
at **both ends of the hop**, the two ends have different owners, and only one of them is visible to
MH.

| End | Counter | Who can see it |
|---|---|---|
| **MH egress** — the application egress queue bound tripping | `mh_media_frames_dropped_total{direction="egress",reason="egress_queue_overflow"}` | MH, and this alert |
| **SDK send** — the client's bounded queue above the transport | `dt_client_media_send_dropped_total{reason="egress_queue_overflow"}` | the client only |

> **THE MH-SIDE DROP IS AN APPLICATION QUEUE BOUND, NOT A BANDWIDTH BUDGET.** It is the §1
> transport-parameter bound: MH owns a bounded egress datagram queue above quinn's datagram send
> buffer, with drop-oldest, deliberately sized to trip **before** the transport ceiling so the drop
> is countable in our code rather than silently discarded inside quinn. **There is no egress
> bandwidth budget, no capacity gauge, no stream ceiling and no admission threshold in this build.**
> If you are looking for one, you are looking for something that does not exist; do not infer a
> number from this counter.

**Why the SDK-side drop matters most.** ADR-0036 §11 is explicit: the client-side send drop is the
one that matters most, **because it occurs in the sender and MH structurally cannot observe it**.
WebTransport exposes no send-side drop event, so the SDK keeps the transport queue shallow, owns a
bounded queue above it, makes the drop decision there and counts it — making the drop observable by
construction. A flat MH counter is **not** evidence that frames are arriving.

#### The loopback reading, and why it does not identify a cause

In loopback (one client, hearing its own audio back through MH), the diagnostic pair is:

```promql
rate(dt_client_media_frames_sent_total[5m])
rate(dt_client_media_frames_received_total[5m])
```

**`sent` rising while `received` stays flat means audio is not completing its round trip.** That
reading is real and it is where triage starts — but it has **at least two causes with different
remedies, and no unique client-side discriminator**:

1. **A NAT binding reaped during a mute longer than the keepalive interval.** QUIC's
   connection-level keepalive is what refreshes NAT bindings while no media flows, and no media
   flows whenever a participant is muted (§1, §5). Without an adequate keepalive an intermediary can
   reap the path and unmute is not instantaneous.
2. **MH holding stale or absent policy** — see
   [`mc-incident-response.md` Scenario 15: Media Generation Divergence](mc-incident-response.md#scenario-15-media-generation-divergence).

Both produce the identical client-side counter reading. **Do not guess between them; the ladder
below separates them at rung 1, cheaply.**

#### Triage ladder

**Fork first, on `sent`:**

| `sent` | Meaning | Go to |
|---|---|---|
| **Flat** | Nothing is leaving the device. This is not a transport problem. | Capture or mute-release never resumed. Check `dt_client_media_mute_transitions_total{action}` and the capture pipeline — [`client-dev-local.md` §4.5](client-dev-local.md#45-i-joined-and-i-hear-nothing--media-triage-ladder). Stop here. |
| **Rising, `received` flat** | Transmitting into something that is not returning. | Rungs 1–4 below, in order. |

**Rung 1 — is the QUIC connection still up?** This is the cheapest rung and it is the one that
separates the two causes:

- **A reaped binding shows as connection failure or keepalive distress** — the client's transport
  reports a closed or timing-out connection, and the session ends or re-establishes.
- **MH-not-forwarding leaves a healthy connection.** The connection is fine; nothing is coming back
  over it.

That single observation does the discrimination the counters cannot. Take it first.

**Rung 2 — generation divergence.** If the connection is healthy, MH may be forwarding under stale
or absent policy:

```promql
sum by(outcome) (increase(mc_media_policy_pushes_total[15m]))
```

Any `generation_mismatch` or `no_applied_generation` sends you to
[`mc-incident-response.md` Scenario 15](mc-incident-response.md#scenario-15-media-generation-divergence).
**Note that in this build divergence does not self-correct** — the resolution there is to force a
structural change (a rejoin), not to wait.

**Rung 3 — keepalive configuration.** Only after rungs 1 and 2. Compare the configured QUIC
keepalive interval against the mute duration that preceded the symptom. A keepalive longer than a
routine mute is the reaped-binding cause; a keepalive comfortably shorter than it is not, and rung 3
is then a dead end rather than a finding.

**Rung 4 — packet capture.** Last rung, and only when 1–3 have not resolved it.

> **A packet capture of a join or a media session is CREDENTIAL-BEARING.** The join carries the
> meeting JWT. **Do not attach a capture to a ticket, a Slack message, or an incident document.**
> Handle it as you would any credential-bearing artifact: named access, encrypted at rest, a
> deletion deadline with an owner. The same applies to a browser HAR or devtools trace of a join.

#### What each counter actually proves

| Counter | Proves | Does **not** prove |
|---|---|---|
| `dt_client_media_frames_sent_total` | frames left the device | that MH accepted or forwarded them |
| `dt_client_media_frames_received_total` | **datagrams arrived at the wire** — counted before any parse, verification or decryption | that they were openable, or that anything was heard |
| `dt_client_media_frames_dropped_total{reason}` | which post-arrival step rejected, **by name** | anything about frames that never arrived |
| `dt_client_media_frames_accepted_total` | frames completed the receive path and were handed to the decoder | that anything was **played** — see below |
| `mh_media_frames_forwarded_total{direction="egress"}` | MH sent something toward a subscriber | that it reached them |

**`received` is counted at the wire, before verification and decryption, and that is the whole
point.** It makes *"nothing is arriving"* distinguishable from *"things are arriving and failing to
open"* — two conditions that would otherwise look identical from the outside. The
post-verification story is carried by the drop-by-reason counters, under the accounting identity
`received = accepted + sum(drops by reason)`. That identity holds at the crypto/parse boundary; it
does **not** hold at playback.

**`accepted`, never `played`.** A frame handed to a decoder is not a frame that was heard. Silent
audio with `accepted` climbing means the fault is **downstream of the decoder handoff** — the
decoder, the output device, or a suspended audio context — and no counter in this scenario will
find it. `dt_client_media_decoder_errors_total` covers part of that segment and only part.

#### MH-side diagnosis, when the alert is what brought you here

```promql
# Egress drop ratio over ATTEMPTS (forwarded + all egress drops), by reason.
sum by(reason) (rate(mh_media_frames_dropped_total{direction="egress"}[5m]))
/
(
  sum(rate(mh_media_frames_forwarded_total{direction="egress"}[5m]))
+ sum(rate(mh_media_frames_dropped_total{direction="egress"}[5m]))
)
```

**Read the `reason` breakdown before concluding back-pressure.** Only `egress_queue_overflow` is
what this alert measures. Two neighbours are routinely non-zero and mean something else entirely:

- `connection_closed` — a participant left underneath a send. **Routine.** Every meeting ends this
  way, many times.
- `no_subscriber` — nothing was subscribed to that source. Counted **once per frame**, not once per
  (frame × subscriber), so in a meeting with nobody subscribed it reads as 100% of egress off a
  single frame. **Do not widen the alert's selector to include it**; that is precisely the false
  fire the restriction to `egress_queue_overflow` exists to prevent.

`mh_media_egress_queue_depth` is a trend input only. **No conclusion may rest on it alone**: it is
one process-wide, last-writer-wins gauge fed by N per-subscriber queues, and the scrape interval is
orders of magnitude longer than the queue's fill-and-drain time. A reading of zero means nothing.

**Resolution**:

- **Sustained `egress_queue_overflow`** means a subscriber the queue cannot drain into fast enough.
  In this build there is no per-stream lever: the bound is a startup-validated transport parameter,
  and there is no bandwidth budget to adjust. Escalate to `media-handler` with the reason breakdown
  and the affected pod.
- **A reaped NAT binding** resolves on reconnect; the durable fix is the keepalive interval, which is
  a configuration change and a redeploy.
- **Stale or absent policy** resolves by forcing a structural change — see MC Scenario 15. **Not by
  restarting MH**, which sheds every media session on the pod (see
  [`mh-deployment.md` §Rollout With Media Flowing](mh-deployment.md#rollout-with-media-flowing)).

**Escalation**: `media-handler` for the MH egress queue and the reason breakdown; `client` for the
SDK send queue and the capture path; `meeting-controller` if rung 2 finds divergence.

**Related Alerts**: `MHMediaSessionDeclineRate` (Scenario 15 — a session that never started is a
different fault from one that started and stopped forwarding);
`MCMediaGenerationDivergence` (MC Scenario 15 — rung 2).


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

# MH -> MC. NOT checkable by curl: MC health is 8081 and MC's NetworkPolicy
# admits 8081 from Prometheus only, so this DROPS and times out, reading as
# "MC is down". The GC line above genuinely works (GC admits 8080 from
# everywhere); MC is not symmetric. MH reaches MC only on gRPC 50052.
kubectl get endpoints mc-service -n dark-tower
# MC health, from the operator's machine, per instance:
#   kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
#   curl -i http://localhost:8080/health   # repeat for mc-1
# See docs/runbooks/mc-incident-response.md §MC Topology.

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
