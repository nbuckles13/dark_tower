# MH Service Incident Response Runbook

**Service**: Media Handler (mh-service)
**Owner**: SRE Team / Media Handler Service Owner
**On-Call Rotation**: PagerDuty - Dark Tower MH Team
**History**: `git log --follow -- docs/runbooks/mh-incident-response.md`

> **First triage command for any MH page: `./infra/kind/scripts/mh.sh list`** (every hostPort slot:
> state, owner, address, pod, revision, `stale`). MH is a pool of per-slot Deployments `mh-s<k>`;
> there is no `mh-service` Deployment and MH has no rollout history. In the commands below,
> `<mh-pod>` is a pod name, `<k>` a slot number and `<owner>` that slot's `owner` (normally
> `baseline`) from that list. Replacing an MH keeps its owner: `mh.sh up --owner <owner>`, then
> `mh.sh drain <k> --owner <owner>` (without `--owner`, both default to `manual`: the drain refuses
> `foreign-owner`, and forcing it leaves a stray `manual` MH the next deploy never removes).
> **Rolling back with `git checkout <previous-good-commit> && deploy.sh`:** if that commit predates
> the slot pool (story 4 I-2), read `mh-deployment.md` "ROLLING ACROSS THE SLOT-POOL CHANGE" first
> (helper rebuild + cluster recreate; otherwise deploy fails `blueprint-changed`). Never `rollout restart` an
> `mh-s<k>`, and never `kubectl delete pod --force --grace-period=0` an MH pod.

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
   - [Scenario 13: RegisterMeeting Timeout — Clients Kicked (RETIRED)](#scenario-13-registermeeting-timeout--clients-kicked)
   - [Scenario 14: WebTransport Server Startup Failure](#scenario-14-webtransport-server-startup-failure)
   - [Scenario 15: Media Sessions Declining — No Sender Binding](#scenario-15-media-sessions-declining--no-sender-binding)
   - [Scenario 16: Ingress Datagrams Received But Never Read](#scenario-16-ingress-datagrams-received-but-never-read)
   - [Scenario 17: Media Datagram Drop](#scenario-17-media-datagram-drop)
   - [Scenario 18: Media Egress Budget Exhaustion](#scenario-18-media-egress-budget-exhaustion)
   - [Scenario 19: Server Mute Not Taking Effect At Ingress](#scenario-19-server-mute-not-taking-effect-at-ingress)
   - [Scenario 20: Video Keyframe Storm](#scenario-20-video-keyframe-storm)
   - [Scenario 21: Slow Video Subscriber (Stream-Credit Stall)](#scenario-21-slow-video-subscriber-stream-credit-stall)
   - [Scenario 22: Video Slot Stuck Pending](#scenario-22-video-slot-stuck-pending)
   - [Scenario 23: Client Sending While Client-Muted](#scenario-23-client-sending-while-client-muted)
   - [Scenario 24: Media-Dial Credential Rejections](#scenario-24-media-dial-credential-rejections)
   - [Scenario 25: Meetings Held Without Clients](#scenario-25-meetings-held-without-clients)
   - [Scenario 26: Owner-Silence Release With Clients Present](#scenario-26-owner-silence-release-with-clients-present)
   - [Scenario 27: RegisterMeeting Refused or Meeting Not Released](#scenario-27-registermeeting-refused-or-meeting-not-released)
   - [Close codes: what MH told the client](#close-codes-what-mh-told-the-client)
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
- Sustained `mh_jwt_validations_total{token_type="media_dial",failure_reason=~"signature_invalid|wrong_type"}` increase (forged or confused dial tokens) -> Security investigation per [Scenario 24](#scenario-24-media-dial-credential-rejections)
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
| **MC Team** | MC -> MH RegisterMeeting RPC issues, MH -> MC notification delivery, `media_dial` token minting/refresh | #mc-oncall, PagerDuty: MC-Team |
| **GC Team** | Registration failures, heartbeat issues | #gc-oncall, PagerDuty: GC-Team |
| **AC Team** | Service-token (MC->MH gRPC) validation failures traced to JWKS, token refresh failures | #ac-oncall, PagerDuty: AC-Team |
| **Infrastructure/SRE** | Kubernetes, networking, UDP/QUIC, TLS cert rotation | #infra-oncall, PagerDuty: SRE |
| **Security Team** | Caller-type rejection spike, JWT tampering signal | #security-incidents (P1 only) |

### External Dependencies

- **Global Controller**: MH registration, load-report heartbeats, meeting assignment
- **Meeting Controller**: MC -> MH RegisterMeeting, MH -> MC connect/disconnect notifications
- **Auth Controller**: JWKS endpoint (MC->MH service-token validation only; client `media_dial` tokens are MC-minted and verified locally), OAuth token refresh
- **Kubernetes / cloud**: UDP listener for QUIC/WebTransport, TLS cert-manager

---

## Common Failure Scenarios

### Scenario 1: Complete Service Outage

**Alert**: `MHDown` (page), **re-scoped fleet/region-level** (story 4):
`max by (region) (gc_mh_available{state="healthy", region!="other"}) == 0` for 1m (own region: the `other` cell is always 0 in a single-region deployment; `max by (region)` because every GC replica publishes the same gauge) — no healthy, non-draining MH in the region, so **no
replacement capacity at all**. With GC down the series is absent and this alert is silent; `GCDown` pages that case.
**Severity**: page

**A single MH death is routine and is NOT this scenario.** MC declares the dead handler within N and
moves its meetings onto survivors or a replacement
([`mc-incident-response.md` Scenario 24](mc-incident-response.md#scenario-24-media-handler-declared-dead-or-impaired));
user impact from it pages only through `MCMediaMeetingsAwaitingHandler`
([Scenario 25 there](mc-incident-response.md#scenario-25-meetings-awaiting-a-media-handler)).

**Symptoms**:
- GC sees no healthy MH: `max by (region) (gc_mh_available{state="healthy", region!="other"}) == 0`
- `mc_media_meetings_without_handlers` > 0 on every MC with meetings; new meetings cannot be placed
  (GC has no MH to assign)
- `./infra/kind/scripts/mh.sh list` shows no slot `serving`

**Impact**: Every meeting in the region is media-dark and awaiting a handler; new meetings fail to
start. Signalling and roster still work.

**Immediate Response**:

1. `./infra/kind/scripts/mh.sh list --json` — slot states, `restart_count`, `stale`,
   `terminating_past_grace`. Then `kubectl get pods -n dark-tower -l app=mh-service`.
2. GC's view: `max by (region, state) (gc_mh_available)`. MHs `serving` in `mh.sh list` but not `healthy`
   in GC = the MH→GC path (load reports, registration) — [Scenario 3](#scenario-3-gc-heartbeat-failures),
   `gc-incident-response.md`.
3. Recent deploy? Each slot's `revision` / `stale` (MH has no rollout history).
4. Post to `#incidents`; page MC and GC owners.

**Root Cause Investigation**:

- Pod events: `kubectl describe pods -n dark-tower -l app=mh-service`
- Previous-pod logs (if crashed): `kubectl logs -n dark-tower <pod> --previous --tail=200`
- Node health: `kubectl get nodes`, `kubectl describe node <node>`
- OOMKilled events: `kubectl get events -n dark-tower | grep -i "oom\|killed"`
- Secrets / config: `kubectl get secret,configmap -n dark-tower | grep mh-service` (ConfigMap names are content-hash-suffixed, `mh-service-config-<hash>`, and old generations are listed too; the pod template's `configMapKeyRef` names the live one)

**Recovery**:

```bash
# Option A: Rollback (if recent deployment is the cause)
git checkout <previous-good-commit> && ./infra/kind/scripts/deploy.sh   # MH has no rollout history; deploy runs mh.sh upgrade
./infra/kind/scripts/mh.sh list   # every slot serving, none stale

# Option B: Add capacity (no MH serving, slots free)
./infra/kind/scripts/mh.sh up --owner baseline        # returns once Available (GC-selectable); repeat to B
# NEVER `kubectl delete pod --force --grace-period=0` an MH pod (it defeats the slot lock;
# mh-deployment.md "Never force-delete an MH pod"). A stuck pod is investigated, not forced.

# Option C: Increase memory limits if OOMKilled
# MH has no live Deployment to patch: raise resources.limits in infra/services/mh-service/deployment.yaml,
# then ./infra/kind/scripts/deploy.sh (mh.sh upgrade replaces every MH, start-new-then-drain-old)
```

Expected recovery: 1-3 minutes. Meetings awaiting a handler recover on their own once a grant
arrives (MC retries within its replacement budget).

**Related Alerts**: `MCMediaMeetingsAwaitingHandler`, `MHPodRestartingFrequently`, `MHGCHeartbeatFailureRate`.

---

### Scenario 2: JWT Validation Failures

**Alert**: `MHHighJwtValidationFailures`
**Severity**: warning

`mh_jwt_validations_total` has two paths with separate reason sets. **Split by
`token_type` first**:

```promql
sum by(token_type, failure_reason) (rate(mh_jwt_validations_total{result="failure"}[5m]))
```

- `token_type="media_dial"` — client WebTransport admission (MC-minted dial tokens,
  verified against the meeting's registered key; **AC and JWKS are not involved**).
  `no_meeting_key`, `malformed` and `signature_invalid` there are **attacker-drivable**
  (an unauthenticated peer moves them with a forged token), so this alert's ratio can be
  pulled by probing alone: split by `failure_reason` before acting.
  Go to [Scenario 24: Media-Dial Credential Rejections](#scenario-24-media-dial-credential-rejections).
- `token_type="service"` — MC→MH gRPC service tokens (`MhAuthLayer`, AC JWKS). The rest
  of this scenario.

**Symptoms (service path)**:
- `mh_jwt_validations_total{token_type="service",result="failure"}` climbing
- MC's `RegisterMeeting` / `EndMeeting` calls to this MH failing `UNAUTHENTICATED`;
  MC-side push failures

**Impact**: MC cannot program this handler: new meetings get no policy, and MC's
detector may declare the handler impaired. Existing media sessions keep forwarding
under their installed policy.

**Root Cause Investigation**:

```bash
# AC service health + JWKS reachability from MH
kubectl get pods -n dark-tower -l app=ac-service
kubectl exec -it pod/<mh-pod> -n dark-tower -- \
  curl -s http://ac-service.dark-tower.svc.cluster.local:8080/.well-known/jwks.json | head -c 500

# Clock skew (JWTs are time-sensitive)
kubectl exec -it pod/<mh-pod> -n dark-tower -- date -u
kubectl exec -it deployment/ac-service -n dark-tower -- date -u

# MH logs for failure reasons
kubectl logs -n dark-tower -l app=mh-service --tail=500 | grep -iE "jwt|jwks|token|validation"
```

**Common root causes** (service path; see MC runbook Scenario 10 for longer discussion):

1. **AC JWKS endpoint down** — MH caches JWKS with 5-min TTL; failures start after cache expires
2. **Clock skew** on nodes — NTP drift
3. **Key rotation in progress** — AC rotated, MH cache stale
4. **scope_mismatch** — a caller without `service.write.mh` (see also Scenario 7)

**Recovery**:

- AC restart (if AC is the root cause): `kubectl rollout restart deployment/ac-service -n dark-tower`
- NTP fix on affected node
- Wait up to 5 min for JWKS cache refresh after AC-side fix
- If tampering suspected: do NOT restart, preserve logs, escalate Security Team immediately

**Related Alerts**: `MHCallerTypeRejected` (Scenario 7), MC-side RegisterMeeting failures (MC Scenario 12).

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
kubectl exec -it pod/<mh-pod> -n dark-tower -- \
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
# Force re-registration + fresh heartbeat loop: a new MH process, one at a time
./infra/kind/scripts/mh.sh up --owner <owner> && ./infra/kind/scripts/mh.sh drain <k> --owner <owner>
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

> **There is no GC-side RegisterMH latency metric — do not go looking for one.**
> This step used to offer
> `histogram_quantile(0.95, rate(gc_rpc_duration_seconds_bucket{method="RegisterMH"}[5m]))`.
> **`gc_rpc_duration_seconds` has never existed**, and neither does any other
> GC histogram over INBOUND gRPC: GC's ten histograms cover its own outbound
> calls (`gc_grpc_mc_call_duration_seconds`, `gc_ac_request_duration_seconds`),
> HTTP (`gc_http_request_duration_seconds`), the DB and the join/creation flows.
> A phantom selector is worse than a missing step, because "No data" reads as
> "GC is not the problem" and the responder moves on. So split the latency
> yourself: `mh_gc_registration_duration_seconds` (above) is the WHOLE round
> trip, and the `ping` below is the network half — GC-side service time is what
> is left over. The gap is filed in `docs/TODO.md` §Observability Debt (owner
> `global-controller`: a server-side RPC latency histogram); until it lands,
> this subtraction is the measurement.

```bash
# MH -> GC network latency
# (from MH pod)
kubectl exec -it pod/<mh-pod> -n dark-tower -- \
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
- Possibly correlated credential rejections (see [Scenario 24](#scenario-24-media-dial-credential-rejections))

**Impact**: Subset of users unable to establish media. Connections already
established are unaffected. Note that MH has **no horizontal-scaling lever at
incident time** beyond the K hostPort slots — `./infra/kind/scripts/mh.sh up` adds an MH on a
free slot (K is fixed at cluster creation), never a `kubectl scale`. Combined with a tripped accept guard meaning flood-or-leak
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
4. **Credential rejections** cascading — see [Scenario 24](#scenario-24-media-dial-credential-rejections)
5. **QUIC listener stuck** — restart pod

**Recovery**:

```bash
# There is no `mh-service` Deployment. MH runs as a POOL of per-slot
# Deployments mh-s<k> (one hostPort slot each), managed by
# ./infra/kind/scripts/mh.sh (`list` shows every slot). Adding an MH is
# `mh.sh up` onto a free slot (at most K), never `kubectl scale`.

# Rotate TLS (if expired). There is NO cert-manager in this system: the
# mh-service-tls Secret is created imperatively by
# infra/kind/scripts/provision.sh:create_tls_secret() from leaves generated by
# scripts/generate-dev-certs.sh. Do NOT `kubectl delete` the Secret — it is
# the only copy, both MH pods mount it at /etc/mh-tls, and they will fail to
# mount and stay down.
#
# Rotation is documented once, in
# docs/runbooks/client-dev-local.md#f7--browser-refuses-the-mcmh-webtransport-handshake,
# and covers MC and MH together: generate-dev-certs.sh regenerates both
# leaves, so rotating MH's alone leaves MC serving a leaf whose fingerprint
# no longer matches fingerprints.json. Follow F7; do not restate it here.
#
# Leaf validity is 14 days (Chrome's serverCertificateHashes cap), so expiry
# is routine, not an exceptional branch.

# Restart (QUIC listener stuck): replace ONE MH at a time, start-new-then-drain-old
./infra/kind/scripts/mh.sh up --owner <owner> && ./infra/kind/scripts/mh.sh drain <k> --owner <owner>   # never rollout restart an mh-s<k>
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

1. Handshake includes `media_dial` verification (local Ed25519 verify against the registered key — no network fetch) and one session-actor key lookup; a slow session actor shows here.
2. Check TLS/QUIC error rate — retries slow the handshake.
3. Check pod CPU — busy accept loop slows handshakes.

**Root Cause Investigation**:

```bash
# MH CPU utilization
kubectl top pods -n dark-tower -l app=mh-service
```

**Common Root Causes**:

1. **CPU saturation** — scale out, see Scenario 8
2. **Session-actor backlog** — the key lookup waits on the actor mailbox; check MH logs for mailbox-full WARNs

**Recovery**: Scale horizontally if CPU-bound.

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
kubectl describe pod/<mh-pod> -n dark-tower | grep -A 5 "Limits:"   # <mh-pod>: from ./infra/kind/scripts/mh.sh list
```

**Root Cause Investigation**:

```promql
# Active connection load on this pod
mh_active_connections

# Memory trend (leak if never decreasing)
container_memory_working_set_bytes{container="mh-service"}

# CPU trend
rate(container_cpu_usage_seconds_total{container="mh-service"}[5m])
```

**Common Root Causes**:

1. **High connection load** — `mh_active_connections` elevated; scale out
2. **Memory leak** — never-decreasing memory; restart, profile
3. **CPU-intensive media forwarding** — investigate, scale

**Recovery**:

```bash
# Scale horizontally: one more MH per free hostPort slot (at most K)
./infra/kind/scripts/mh.sh up --owner manual
# Once the incident clears, retire it: mh.sh drain <k> --owner manual (deploy.sh never removes a
# `manual` MH, and it holds a slot the next upgrade needs)

# Increase limits
# MH has no live Deployment to patch: raise resources.limits in infra/services/mh-service/deployment.yaml,
# then ./infra/kind/scripts/deploy.sh (mh.sh upgrade replaces every MH, start-new-then-drain-old)

# Restart (if leak suspected): one MH at a time, start-new-then-drain-old
./infra/kind/scripts/mh.sh up --owner <owner> && ./infra/kind/scripts/mh.sh drain <k> --owner <owner>
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
kubectl exec -it pod/<mh-pod> -n dark-tower -- \
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

**Impact**: MC's view of who is connected may be stale **for a short, bounded time**. Since story 4 (ADR-0039 §6) a lost `Disconnected` **self-corrects within N** (`mc_media_handler_heartbeat_deadline_seconds`), normally on the next heartbeat (about N/3): every per-meeting heartbeat carries MH's connection ids, and MC's remove-only reconcile retires a connection absent from it (and releases its `sender_id` binding), counted `mc_media_handler_connected_set_corrections_total`. A failed `Connected` is not drift: the bind is its only add path, so MH declines that session (`mh_media_session_starts_total{outcome="declined_mc_unavailable"}`, [Scenario 15](#scenario-15-media-sessions-declining--no-sender-binding)) and the client redials. A **sustained** correction rate is the warning `MCMediaHandlerConnectedSetCorrections`: it means the notification path is lossy.

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
- Once MC is healthy, MH stops failing on new events; existing drift is reconciled by the next heartbeats (watch `sum(increase(mc_media_handler_connected_set_corrections_total[15m]))` rise and settle). A burst about equal to one handler's whole connected set → [`mc-incident-response.md` Scenario 28](mc-incident-response.md#scenario-28-clients-report-handler-loss-while-heartbeats-look-healthy) forged branch

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

# Recent deployment changes (MH has no rollout history: revision + stale per slot)
./infra/kind/scripts/mh.sh list
```

**Common Root Causes**:

1. **Liveness probe failing** — health endpoint slow; investigate
2. **OOMKilled** — increase memory limits, investigate leak
3. **Panic on startup** — recent deploy introduced bug; rollback
4. **Missing dependency at init** — e.g., can't reach GC or AC at startup
5. **WebTransport server bind/listen/TLS-load failure** — pod never reaches Ready because the QUIC listener cannot start. Distinguishing signature: previous-pod logs show `webtransport::server::bind()` failing (TLS identity, bind address, UDP port). See [Scenario 14: WebTransport Server Startup Failure](#scenario-14-webtransport-server-startup-failure).

**Recovery**:

```bash
# Rollback if recent deploy (MH has no rollout history; deploy runs mh.sh upgrade)
git checkout <previous-good-commit> && ./infra/kind/scripts/deploy.sh

# Increase memory (OOMKilled)
# MH has no live Deployment to patch: raise resources.limits in infra/services/mh-service/deployment.yaml,
# then ./infra/kind/scripts/deploy.sh (mh.sh upgrade replaces every MH, start-new-then-drain-old)
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
```

> The GC-side half of this heartbeat's latency is **not measurable today**. This
> step used to offer
> `histogram_quantile(0.95, rate(gc_rpc_duration_seconds_bucket{method="SendLoadReport"}[5m]))`,
> and `gc_rpc_duration_seconds` has never existed — the same phantom as
> §"Slow GC Registration" above, and the same reason: GC has no histogram over
> inbound gRPC. The MH-side p95 is the whole round trip; treat a rise in it with
> a flat network path as GC-side until the filed histogram exists
> (`docs/TODO.md` §Observability Debt, owner `global-controller`).

**Common Root Causes**:

1. GC under load
2. Network path degradation (cross-AZ)
3. MH pod CPU contention during metric emission

**Recovery**: Usually self-heals. Escalate only if sustained or escalating.

**Related Alerts**: `MHGCHeartbeatFailureRate`, `MHHighRegistrationLatency`.

---

### Scenario 13: RegisterMeeting Timeout — Clients Kicked

**RETIRED (story 4 MH-1, ADR-0039 §3).** MH no longer holds a client for a meeting it has not registered: a dial for a meeting MH holds no key for is DECLINED at accept (`mh_jwt_validations_total{token_type="media_dial",failure_reason="no_meeting_key"}`, [Scenario 24](#scenario-24-media-dial-credential-rejections)). The registration-timeout config key and its timeouts counter are deleted. The arms that outlived it (egress-edge ratchet, teardown discriminator table, meeting cap, ownership reject / generation wedge, `no_generation`) moved verbatim to [Scenario 27](#scenario-27-registermeeting-refused-or-meeting-not-released). This number is not reused.

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
kubectl exec -it pod/<mh-pod> -n dark-tower -- \
  openssl x509 -in /certs/tls.crt -noout -dates -subject 2>/dev/null \
  || echo "Pod not running — inspect cert via secret directly"

# Each MH's UDP/4434 is published through its hostPort SLOT (no Service): the
# pod's webtransport hostPort == its slot's MH_HOST_PORT
./infra/kind/scripts/mh.sh list   # slot, host_port, address, pod

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
# Option A: Rollback (recent bad deploy). MH has no rollout history. If the commit
# predates the slot pool (story 4 I-2): mh-deployment.md "ROLLING ACROSS THE SLOT-POOL CHANGE" first.
./infra/kind/scripts/mh.sh list   # which slots run which revision
git checkout <previous-good-commit> && ./infra/kind/scripts/deploy.sh   # mh.sh upgrade
./infra/kind/scripts/mh.sh list   # every slot serving, none stale

# Option B: TLS identity broken (Secret missing, malformed, or expired leaf).
# There is NO cert-manager in this system — do NOT delete the Secret. It is
# the only copy (created imperatively by provision.sh:create_tls_secret() from
# leaves generated by scripts/generate-dev-certs.sh), both MH pods mount it
# at /etc/mh-tls, and deleting it deepens an outage you are already in.
#
# Rotation is documented once, in
# docs/runbooks/client-dev-local.md#f7--browser-refuses-the-mcmh-webtransport-handshake
# It covers MC and MH together (generate-dev-certs.sh regenerates both
# leaves; provision hashes the public certs, so a renewed leaf rebuilds the
# cluster with fresh Secrets and fresh pods), so nothing is repeated here. F7's last step restarts the dev server — that one matters only if a
# browser client is in the loop, not for recovering the pod bind.

# Option C: Fix bind address (config/ConfigMap regression)
# Fast lever: deploy the previous revision (MH has no rollout history); its template
# still references the previous content-hash-suffixed ConfigMap (nothing prunes old generations).
git checkout <previous-good-commit> && ./infra/kind/scripts/deploy.sh
# Durable fix: correct infra/services/mh-service/config.env and apply the
# environment root (Kind: ./infra/kind/scripts/deploy.sh, or `dev-cluster deploy`).
# The ConfigMap is content-addressed (ADR-0038), so the deploy replaces every MH
# via `mh.sh upgrade` (each drains and MC moves its meetings). Do not `kubectl edit`
# the live ConfigMap: its name is hash-suffixed and the next apply overwrites it.

# Option D: Single-pod port collision (rare): replace that MH onto another slot, never a bare pod delete
./infra/kind/scripts/mh.sh up --owner <owner> && ./infra/kind/scripts/mh.sh drain <k> --owner <owner>
```

Expected recovery time: 1-3 minutes for rollback. TLS rotation is **not**
automatic — it requires the F7 sequence (a provision run, which rebuilds the
dev cluster when a leaf changed), so budget for it rather than waiting on re-issuance.
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

**Do not start by reading the outcome value.** Four of the eight values name MC, and
**two of those four do not mean MC is unwell**. Four rows below say *do not page MC*.

| Outcome | Start in | Why |
|---|---|---|
| `declined_sender_binding_conflict` | **MH** | The `(meeting, sender)` ordinal is held by a *different* participant. Check MH's unbind path **first** — see Step 4. |
| `declined_mc_endpoint_unknown` | **MH** | No usable `mc_grpc_endpoint` for the meeting. **MH never dialled — do NOT page MC.** See Step 5. |
| `declined_mc_auth_rejected` | **MH** | MH's *outbound credential* failed: MC refused it, or MH could not build one. **MH's outbound auth, not MC's health.** See Step 6. |
| `declined_mc_unavailable` | **MC reachability** | MH dialled and got no usable answer within the ~48s budget. |
| `declined_no_sender_binding` | **MC's answer** | MC answered `0` — "I do not know this participant." Go to Step 2. |
| `declined_sender_binding_out_of_range` | **MC's allocator** | Contract violation. Should read **zero forever** — see Step 3. |
| `declined_pending_cap` | **The client** | The verified (meeting, participant) already had `MH_MAX_PENDING_CONNECTIONS_PER_PARTICIPANT` connections awaiting MC's bind answer; MH closed the excess DECLINED **without asking MC — do NOT page MC**. Expected near-zero; sustained = a client redial storm (story 4 MH-1, ADR-0039 §5). See [Scenario 24](#scenario-24-media-dial-credential-rejections). |

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
| `outcome="registry_full"` | **Retired** (story 2 task 20) — MC's connection registry and its per-meeting cap were removed; connectivity now lives in MC's meeting actor, which has no refusing cap. Present only from an MC image that predates the retirement | **Version skew.** Redeploy MC. Not a capacity signal on a current image. |
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

> **One route here has no remedy, and it is benign: MH's process exiting.** The
> endpoint lookup goes through the session actor. **During the drain** (story 4
> MH-3: from SIGTERM until the pod holds nothing or reaches `grace − 5 s`) the
> actor keeps running and endpoints resolve, so a decline mid-drain is NOT this
> route — triage it like any other. Only a connection that reaches this point
> AFTER the drain, once the shutdown token is cancelled and the process is
> exiting, finds no endpoint and declines. That is **correct behaviour at process
> exit**, it is not a registration fault, and there is nothing to fix.
> Discriminate on coincidence with the pod's final `mh.drain` "Drain finished"
> log line: if the increments come after it and
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
**functioning as a rate limiter on the reconnect herd.** MH now SENDS the distinction —
the MC-unreachable decline closes with `MC_UNAVAILABLE`, MC's refusals with `DECLINED`
or `BINDING_CONFLICT` ([§Close codes](#close-codes-what-mh-told-the-client)) — but the
client only ACTS on it from story 4 task C1. Until then clients retry on their own
cadence; cutting the budget to ~10s would multiply the retry rate roughly 5× against the
service that is already the bottleneck. **The deadline and the client-side handling of
the close code are one decision, not two.** Keep the budget until C1 has shipped.

#### Declines from a removed or out-of-set handler are expected (story 4)

After MC declares a handler dead or removes it (a zombie MH, or a late `Connected` from a handler
MC already removed), MC's binding gate answers `sender_id` 0 and records nothing for that handler's
`NotifyParticipantConnected` (ADR-0039 §6): `mc_media_sender_binding_responses_total{outcome="refused"}`
and `mc_mh_notifications_unapplied_total{reason="handler_not_in_set"}` on MC, a
`declined_no_sender_binding` here. **That is the fence working, not a binding defect.** MC releases
the removed handler's existing bindings itself, after removal. Read it as a fault only when no
declaration or removal precedes it ([`mc-incident-response.md` Scenario 24](mc-incident-response.md#scenario-24-media-handler-declared-dead-or-impaired)).
There is no empty-`connection_id` legacy mode: MC refuses a notification without one
(`reason="connection_id_missing"`).

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
past the credential gate can inflate it: datagrams carrying an unmatched HTTP/3
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

**What this is.** Audio is carried one frame per **QUIC datagram** (ADR-0036 §1). *(Scope, corrected 2026-10-21, story 3 OPS-A: the media path is no longer datagram-only. From MH-A2, video rides one QUIC unidirectional stream per group beside datagram audio in the same meeting. This scenario covers the datagram path. Stream-path drops and resets are MH-A2's and MH-B's reasons, and their scenarios are [Scenario 20](#scenario-20-video-keyframe-storm), [Scenario 21](#scenario-21-slow-video-subscriber-stream-credit-stall) and [Scenario 22](#scenario-22-video-slot-stuck-pending), shipped by OPS-B.)* Datagrams are
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
> is countable in our code rather than silently discarded inside quinn. **The egress bandwidth
> budget, its stream ceiling and its admission threshold (story 2) are a DIFFERENT control**: they
> refuse a forwarding policy at admission and never shed a datagram, so they cannot move this counter.
> Budget exhaustion is [Scenario 18](#scenario-18-media-egress-budget-exhaustion); do not infer a
> budget number from this counter. *(Corrected at story 2 task 18, 2026-09-30: this previously said
> no budget, ceiling or threshold existed in this build, which story 2 task 8 falsified.)*

**Why the SDK-side drop matters most.** ADR-0036 §11 is explicit: the client-side send drop is the
one that matters most, **because it occurs in the sender and MH structurally cannot observe it**.
WebTransport exposes no send-side drop event, so the SDK keeps the transport queue shallow, owns a
bounded queue above it, makes the drop decision there and counts it — making the drop observable by
construction. A flat MH counter is **not** evidence that frames are arriving.

#### The client-side reading, and why it does not identify a cause

> **Recorded correction (story 2 task 17).** This subsection was "The loopback reading": one
> client hearing its own audio back through MH, so `sent` rising with `received` flat meant a
> failed round trip. **Loopback is removed (story 2 R-3).** A reader trusting the old text would
> have read a participant's silence as a round-trip fault and hunted a reaped NAT binding or stale
> MH policy that does not exist. Two facts replace it. (1) **A solo participant sends NOTHING**: MC
> directs a publisher nobody holds to an empty target set and the SDK builds no frame on an empty
> set, so solo reads `sent` flat, `received` flat, and **MH `no_subscriber` flat** — sources:
> `DirectiveOutcome::EmittedEmptyTargets` in `crates/mc-service/src/media_signaling/directive.rs`,
> `submit()` in `packages/sdk-core/src/media/pipeline/egress.ts`. (2) **`received` counts OTHER
> senders' audio**; a client's own `sent` has no bearing on it.

The client-side pair is still where triage starts:

```promql
rate(dt_client_media_frames_sent_total[5m])
rate(dt_client_media_frames_received_total[5m])
```

but `received` flat is not by itself a fault: it can be correct, an MC-side fault, or an MH-side
one. **Fork it with the table in
[`client-dev-local.md` F13](client-dev-local.md#f13--sent-rising-received-flat-transmitting-into-something-that-is-not-returning)**
(`dt_client_media_receive_source_deficit_total` plus the client's slot states). That table is the
single home for the fork and is not restated here. Only its "MC says a source is active and nothing
is arriving" outcome continues into this scenario.

**A source that MC calls active and that is not arriving** has **at least two causes with
different remedies, and no unique client-side discriminator**:

1. **A NAT binding reaped during a mute longer than the keepalive interval.** QUIC's
   connection-level keepalive is what refreshes NAT bindings while no media flows, and no media
   flows whenever a participant is muted (§1, §5). Without an adequate keepalive an intermediary can
   reap the path and unmute is not instantaneous.
2. **MH holding stale or absent policy** — see
   [`mc-incident-response.md` Scenario 15: Media Generation Divergence](mc-incident-response.md#scenario-15-media-generation-divergence).

Both produce the identical client-side counter reading. **Do not guess between them; the ladder
below separates them at rung 1, cheaply.**

**`no_subscriber` is MORE load-bearing without loopback, not less.** It is MH's view of the
opposite mismatch: a sender whose frames reach **no** egress edge. Healthy only as a short
in-flight tail — frames already sent when a sender's last holder leaves, before MC's empty
directive lands. **Sustained `no_subscriber` means a client is sending against an empty edge set**,
and that has **two causes** — a solo meeting produces neither:

1. **MC's send directive and MH's installed policy disagree** — MC's assignment and push path, rung
   2's divergence family.
2. **A client is ignoring its directive** while MC and MH agree perfectly. The sender is an
   untrusted client: a stale SDK bundle, or a misbehaving or hostile participant pushing datagrams
   at MH.

**Discriminator: rung 2.** If the Scenario 15 divergence checks come back clean, it is cause 2 —
stop chasing MC's push path and look at the client. The aggregation floor is the pod (no sender
dimension), so narrow by pod and onset time, then treat it like the driven case in
[MC Scenario 8 root cause 11](mc-incident-response.md#scenario-8-join-failures) (join rate per
token and per source; a stale bundle clears on reload). Source for the
reading: `docs/observability/metrics/mh-service.md` §`mh_media_frames_dropped_total` (the solo
bullet and the `no_subscriber` row of the reason table).

#### Triage ladder

**Fork first, on `sent`:**

| `sent` | Meaning | Go to |
|---|---|---|
| **Flat** | Nothing is leaving the device. This is not a transport problem. | Either nobody holds this participant — a solo participant ALWAYS, and **correct** — or capture or mute-release never resumed. Check `dt_client_media_mute_transitions_total{action}` and the capture pipeline — [`client-dev-local.md` F12](client-dev-local.md#f12--joined-unmuted-and-dt_client_media_frames_sent_total-is-flat). Stop here. |
| **Rising, `received` flat** | Somebody holds this participant; whether anybody is sent TO it is a separate fact. | Fork on the [`client-dev-local.md` F13](client-dev-local.md#f13--sent-rising-received-flat-transmitting-into-something-that-is-not-returning) table. Only "MC says a source is active and nothing is arriving" continues to rungs 1–4 below, in order. |

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
Since story 2 any structural change (join, leave, declaration, mute) re-pushes, so a meeting with
churn may clear by itself. **A quiescent diverged meeting is no longer stranded** (story 4): there is
no periodic re-assert, but MC compares the applied generation on every heartbeat, and a handler that
stays behind for longer than L (2N) is declared Impaired (`trigger="apply_lag"`) and the meeting is
migrated ([`mc-incident-response.md` Scenario 24](mc-incident-response.md#scenario-24-media-handler-declared-dead-or-impaired)). **An MC restart is under NO `outcome` label by design**: it is recorded on
`mc_media_policy_generation_adoptions_total` (and a WARN "adopting it as a floor" at
`mc.register_meeting.trigger`), so after an MC restart read that counter alongside the split rather
than concluding from a clean breakdown that nothing happened.

**Rung 3 — keepalive configuration.** Only after rungs 1 and 2. Compare the configured QUIC
keepalive interval against the mute duration that preceded the symptom. A keepalive longer than a
routine mute is the reaped-binding cause; a keepalive comfortably shorter than it is not, and rung 3
is then a dead end rather than a finding.

**Rung 4 — packet capture.** Last rung, and only when 1–3 have not resolved it.

> **A packet capture of a join or a media session is CREDENTIAL-BEARING.** The join carries the
> meeting JWT, and every MH dial carries a `media_dial` token. **Do not attach a capture to a ticket, a Slack message, or an incident document.**
> Handle it as you would any credential-bearing artifact: named access, encrypted at rest, a
> deletion deadline with an owner. The same applies to a browser HAR or devtools trace of a join.

#### What each counter actually proves

| Counter | Proves | Does **not** prove |
|---|---|---|
| `dt_client_media_frames_sent_total` | datagrams left the device — one per target handler, so a two-handler sender posts two per captured frame; this is not a capture rate | that MH accepted or forwarded them |
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
- `no_subscriber` — a sender's frame reached no egress edge. Counted **once per frame**, not once
  per (frame × subscriber), so while it happens it can read as 100% of egress off a single frame.
  A **short tail** after a sender's last holder leaves is routine; **sustained is a fault** (MC's
  directive and MH's policy disagree, or a client is ignoring its directive — see
  "`no_subscriber` is MORE load-bearing" above for the discriminator). Either way
  it is not back-pressure: **do not widen the alert's selector to include it**; that is precisely
  the false fire the restriction to `egress_queue_overflow` exists to prevent.

`mh_media_egress_queue_depth` is a trend input only. **No conclusion may rest on it alone**: it is
one process-wide, last-writer-wins gauge fed by N per-subscriber queues, and the scrape interval is
orders of magnitude longer than the queue's fill-and-drain time. A reading of zero means nothing.

**Resolution**:

- **Sustained `egress_queue_overflow`** means a subscriber the queue cannot drain into fast enough.
  In this build there is no per-stream lever: the bound is a startup-validated transport parameter,
  and the egress budget is an admission control that does not act on this queue (raising it changes
  nothing here). Escalate to `media-handler` with the reason breakdown
  and the affected pod.
- **A reaped NAT binding** resolves on reconnect; the durable fix is the keepalive interval, which is
  a configuration change and a redeploy.
- **Sustained `no_subscriber`** (a client sending where MH has no edge for it): if rung 2 finds
  divergence, treat it as stale policy, next bullet. If rung 2 is clean, the client is ignoring its
  directive — a stale SDK bundle (reload) or a misbehaving/hostile participant (security
  response); restarting MH or forcing a push fixes neither.
- **Stale or absent policy** resolves by forcing a structural change — see MC Scenario 15. **Not by
  restarting MH**: a replacement drains the pod and MC moves every meeting on it (a blip per meeting; only what is not moved by the drain deadline is shed), and it fixes nothing here (see
  [`mh-deployment.md` §Rollout With Media Flowing](mh-deployment.md#rollout-with-media-flowing)).

**Escalation**: `media-handler` for the MH egress queue and the reason breakdown; `client` for the
SDK send queue and the capture path; `meeting-controller` if rung 2 finds divergence.

**Related Alerts**: `MHMediaSessionDeclineRate` (Scenario 15 — a session that never started is a
different fault from one that started and stopped forwarding);
`MCMediaGenerationDivergence` (MC Scenario 15 — rung 2).


---

### Scenario 18: Media Egress Budget Exhaustion

**Alert**: `MHMediaEgressBudgetExhausted` (warning); the early warning for two of its three arms is `MHMediaEgressEdgeHeadroomLow` (warning)
**Severity**: Warning
**Runbook Section**: `#scenario-18-media-egress-budget-exhaustion`

**What this is.** MH admits egress streams against ONE derived stream ceiling
(`mh_media_egress_stream_ceiling`: the egress budget divided by the heavier per-stream cost). The same
value is advertised to GC as `max_streams`. A generation-advancing policy apply that would take the
handler's installed egress streams above the ceiling is refused whole. The prior generation stays live,
and the refusal is counted as `mh_media_stream_admission_total{outcome="rejected_stream_ceiling"}` (and as
the same event on `mh_media_policy_applies_total{outcome="rejected_stream_ceiling"}`; different
denominators, never summed). The alert compares the windowed refusal share,
`mh_media_stream_admission_rejection_ratio`, bare against its threshold gauge
`mh_media_stream_admission_rejection_ratio_threshold`, the value of `MH_EGRESS_REJECTION_RATIO_THRESHOLD`.
The number lives only in `infra/services/mh-service/config.env`. The window, the defined idle and
all-rejected values, and why the gauge and a `rate()` ratio disagree are in
`docs/observability/metrics/mh-service.md` §Egress Admission Metrics. Read them there, not here.

**What the user sees.** Everyone already talking keeps talking, because an equal-generation push
never re-tests admission (an equal generation is a no-op). **A participant joining a meeting on this handler hears nobody and is heard by
nobody**: their join added edges, and the apply that would install them was refused. GC stops placing
NEW meetings on a handler whose reported `current_streams` has reached its `max_streams`
(`get_candidate_mhs`, new-meeting only). An already-assigned meeting keeps targeting its handlers
(R-6 sticky join), so the refusals continue however full the pod is. **Sustained rejection while
placement keeps targeting this pod is a triage step, not a second alert**: it is the sticky-meeting
shape, or GC holding a load report older than the refusal. Read GC's view of the handler
(`gc-incident-response.md`) before assuming placement is broken.

**Is there a leading indicator? It depends on the arm. Do not read "no page yet" as headroom.**
- **Edge occupancy leads the two ceiling arms.** `MHMediaEgressEdgeHeadroomLow` reads installed edges
  over the **same** `mh_media_egress_stream_ceiling` that the rejection hits, so it fires before
  admission reaches the ceiling, whether from genuine demand (Arm A) or the unreclaimed-edge floor
  (Arm B).
- **Nothing leads congestion or measured bandwidth.** The budget's basis is `unmeasured`
  (`mh_media_egress_budget_bytes_per_second{basis}`): it is a configured figure, not a measurement.
  Nothing in this build measures bandwidth, so a handler whose real link is saturated below its
  configured budget produces no signal here at all. **This has no leading indicator until story 5
  lands the congestion-withheld rank gauge.** ADR-0036 §11 names sustained congestion-withholding
  as egress exhaustion's leading indicator. That gauge does not exist yet, and nothing today stands
  in for it.
- **Nothing on edges leads the empty-meeting arm** (Arm C). It binds on the registered-meetings cap,
  not on edges, so the edge-headroom warning stays silent throughout. It has no leading indicator
  today. Its missing rule is tracked in `docs/TODO.md` §Observability Debt ("No leading indicator for
  registered-meeting exhaustion").

**Step 1 — confirm the comparison, then fork.** All three arms present identically to the user: new
participants are silent, and everyone already talking is fine. **They have three different remedies.**
Read the fork before touching anything. Arms A and B are also identical on the rejection ratio. Arm C
refuses at the registration cap, which the ratio never counts
(`mh_media_policy_applies_total{outcome="rejected_meeting_cap"}` is its signal). So check Arm C whether
or not the alert fired, and check it alongside A or B when it did. *(Premise correction, story 2
task 18: the task text said all three present identically at the rejection ratio. Against the shipped
code, Arm C does not, because `rejected_meeting_cap` is decided on the register path before
admission.)*

```promql
mh_media_stream_admission_rejection_ratio                  # per pod, the alert's left side
mh_media_stream_admission_rejection_ratio_threshold        # per pod, its right side
mh_media_egress_edges                                      # installed edges (measured)
mh_media_egress_stream_ceiling                             # the bound admission enforces
mh_media_egress_stream_ceiling_recommended_min             # advisory: an unsized placeholder budget?
mh_media_registered_meetings                               # registrations HELD, not meetings in progress
mh_media_registered_meetings_limit                         # MH_MAX_REGISTERED_MEETINGS
sum by(outcome) (increase(mh_media_meeting_teardowns_total[1h]))
```

Pod uptime, for Arm B's discriminator: `kubectl get pods -n dark-tower -l app=mh-service -o custom-columns=POD:.metadata.name,STARTED:.status.startTime`.

Compare **each pod against its own ceiling**, never against the fleet: a meeting's edges sit on one
handler and never move, so the idle sibling is not headroom for a meeting refused on the busy one.
The `mh.session.policy` WARN on each refusal carries `installed_meeting_count` and
`installed_total_streams`, which is the only place one fat policy and many small ones are told apart.

#### Arm A — genuine budget exhaustion (LOAD)

**Signature**: `mh_media_egress_edges` at or near the ceiling and **tracking concurrent load**. It
rises and falls with `mh_media_registered_meetings` and with the meetings and participants you
know are live. `released` teardowns are moving normally.

**First check whether the budget was ever sized.** If `mh_media_egress_stream_ceiling` is below
`mh_media_egress_stream_ceiling_recommended_min`, the deployment is running the base ConfigMap's
deliberately unsized placeholder (MH logged an INFO "configure me" line naming
`MH_EGRESS_BUDGET_BPS` at startup). That is a configuration state, not an incident.

**Remedy: capacity, by redeploy only. Prefer adding handlers.** Raise `MH_EGRESS_BUDGET_BPS` in the
ConfigMap for the target environment (the Kind value is
`infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml`) **only to a
figure the node's provisioned egress actually supports**. The budget is `unmeasured`: raising it past
the real link turns a visible admission refusal into congestion that no signal in this build shows.
Two consequences to plan around:
- **A budget change replaces every MH.** The ConfigMap is content-addressed (ADR-0038 §2), so the
  change renders a new `mh-template` revision and `deploy.sh` runs `mh.sh upgrade` over every MH:
  each is drained and MC moves its meetings (a blip per meeting; only what is not moved by the drain
  deadline is shed; `mh-deployment.md` §Rollout With Media Flowing). Prefer a low-occupancy window.
- **MH refuses to start if the raised ceiling exceeds `MH_MAX_TOTAL_EGRESS_EDGES`.** Raise that key in
  the same change if the refusal names it (`mh-deployment.md` §Both pods CrashLoop immediately after
  apply).

**Never lower `MH_EGRESS_BUDGET_BPS` as a way to "stop" anything.** That is a new-meeting outage plus
silent media in existing meetings (`mh-deployment.md` §Operator levers).

#### Arm B — the unreclaimed-edge floor (LEAK)

**Signature**: `mh_media_egress_edges` **rising with pod uptime and uncorrelated with concurrent
load**, while `mh_media_meeting_teardowns_total{outcome="released"}` stays flat or lags the meetings you
know have ended. **That correlation is the discriminator**: edges held by meetings nobody released.
Since story 2 task 12 MC calls `EndMeeting` for every meeting it ends. The floor now accumulates only
from meetings whose MC never **completed** it (an MC crash or kill mid-teardown, exhausted retries,
an MH predating the RPC, a rollback to an MC build without teardown). The per-cause table is in
[Scenario 27](#scenario-27-registermeeting-refused-or-meeting-not-released), in the "Some people can't hear
anything" callout and the teardown table below it. Do not re-derive it here.

**An edge-headroom warning on a long-lived pod is a TRUE POSITIVE for a reclamation regression.**
Triage it as one. **Never tune it away by raising the headroom fraction or the threshold.** The fraction
only moves the time-to-onset. The floor keeps rising at whatever rate unreleased meetings end, so a
higher threshold hides the regression without changing its outcome. The same holds for raising the
budget or `MH_MAX_TOTAL_EGRESS_EDGES`: doubling the bound doubles time-to-onset and fixes nothing.

**Remedy.** Recovery is a restart of **the pod showing the signature**, which releases every held
edge: `./infra/kind/scripts/mh.sh up --owner <owner>`, then `./infra/kind/scripts/mh.sh drain <its slot> --owner <owner>` (never
`rollout restart` an `mh-s<k>`). The leak is per pod. Replace another MH only if it independently
shows the signature, and only after the first replacement is serving. Restarting both at
once migrates every meeting on a healthy pod for nothing (and with no survivor, sheds them), and it can empty GC's candidate pool, which
gives new-meeting 503s ([`gc-incident-response.md` Scenario 3](gc-incident-response.md#scenario-3-mc-assignment-failures)).
**What a restart costs, stated for the shipped build:** `mh.sh drain` drains the pod on SIGTERM and
MC moves its meetings to the replacement or survivors (a small blip per meeting; only what is not
moved by the drain deadline is shed; `mh-deployment.md` §Rollout With Media Flowing). There is no
periodic re-assert (ADR-0039 dropped it): the moved meetings register afresh on their new handler. The durable fix is the **teardown path**: find which MC-side cause stopped
`EndMeeting` from completing ([MC Scenario 20](mc-incident-response.md#scenario-20-meeting-teardown-failing--mh-budget-ratchet))
and route it there.

#### Arm C — empty-meeting growth

**Signature**: `mh_media_registered_meetings` high against `mh_media_registered_meetings_limit`, with
`mh_media_egress_edges` flat or low. There are many meetings at near-zero edges. An empty egress set is
a legal registration, so the budget is structurally blind to it. What refuses here is the
registration cap, which counts `mh_media_policy_applies_total{outcome="rejected_meeting_cap"}` and answers
`RESOURCE_EXHAUSTED`. The stream-ceiling refusal is not involved. The user sees the same thing as a
ceiling refusal: new meetings on the handler get no media.

**Remedy.** Tell leaked registrations apart from an empty-meeting flood. Leaked registrations show
occupancy rising with uptime while `released` stays flat, which is Arm B's cause with zero edges; they
are fixed by reclamation or a pod restart. An empty-meeting flood is many short-lived creates, so
look for who is creating them. **Do not raise `MH_MAX_REGISTERED_MEETINGS`**: it has a hard
ceiling in code, and a larger cap only postpones the same exhaustion. The full arm is Scenario 27's
meeting-cap callout.

**Video, since story 3: real video bytes now flow, and the cost model changed.**
- **Stream costs are derived, not configured.** MH derives each per-stream egress cost from the
  shared `media-codec-profile` bitrate plus a code-derived per-frame overhead × frame rate, for audio
  and video (`mh_media_stream_cost_audio_bytes_per_second`, `mh_media_stream_cost_video_bytes_per_second`;
  the *Per-Stream Egress Costs* panel). The `MH_STREAM_COST_*` keys are retired. **The ceiling divides
  the budget by the LARGER cost**, because MH stays type-blind: it cannot tell an audio edge from a
  video edge at admission. A ceiling that dropped sharply at the story-3 deploy is that division, not
  a regression.
- **`EGRESS_STREAM_CEILING_RECOMMENDED_MIN` is now P×(N+M) = 6×(5+4) = 54**
  (`mh_media_egress_stream_ceiling_recommended_min`; `crates/mh-service/src/config.rs`). A ceiling
  below that gauge is an unsized budget for the demo shape (Arm A's first check).
- **Relief that keeps audio: the R-26 video-off lever**, `MC_MAX_VIDEO_RECEIVE_SLOTS=0`. It keeps
  audio, costs an MC restart per pod and never rolls MH
  ([`mh-deployment.md` §Operator levers for media](mh-deployment.md#operator-levers-for-media-video-off-and-nothing-else)).
  Use it before any rollback.
- **Leading indicator for the load arm: the fleet backlog-reset alert**, `MHVideoSubscriberBacklogResets`
  (A2, [Scenario 21](#scenario-21-slow-video-subscriber-stream-credit-stall)). Subscribers' paths
  saturate and MH resets their groups before admission refuses. A2 firing with edge headroom
  falling is Arm A arriving.
- A patched publisher keying without cause is also egress pressure
  ([Scenario 20](#scenario-20-video-keyframe-storm) arm (d)).

**What this scenario is NOT.**
- **Not `MHMediaEgressQueueOverflowRate`.** That is the §1 transport queue shedding datagrams on a
  live subscriber ([Scenario 17](#scenario-17-media-datagram-drop)). Admission refuses policies and
  never sheds a datagram.
- **Not `apply_failed`.** Edges reaching `mh_media_egress_edges_limit` (the resource-guard backstop)
  show as `mh_media_policy_applies_total{outcome="apply_failed"}` with
  `reason=total_egress_edge_cap_exceeded` on the WARN. Because config load refuses a ceiling above
  that limit, reaching it means the ceiling check did not bind, which is a media-handler defect, not
  capacity.

**Escalation**: `media-handler` for Arm A sizing and any `apply_failed` backstop hit; `meeting-controller`
for Arm B's teardown cause (MC Scenario 20); `global-controller` if GC keeps placing NEW meetings on a
handler whose reported `current_streams` is at `max_streams`.

**Related**: [Scenario 27](#scenario-27-registermeeting-refused-or-meeting-not-released) (the ratchet, the
meeting-cap arm and the ownership-reject recovery order);
[MC Scenario 20](mc-incident-response.md#scenario-20-meeting-teardown-failing--mh-budget-ratchet)
(`MHMediaEgressEdgeHeadroomLow`'s LEAK vs LOAD fork); `mh-deployment.md` §Operator levers.

---

### Scenario 19: Server Mute Not Taking Effect At Ingress

**Alert**: none on MH's own drop counter, by decision. A non-zero muted-drop rate is healthy moderation, and the inverse
("a mute that should be dropping is not") needs a per-meeting correlation ADR-0036 §11 bars.
The decision's home is the `DELIBERATELY NO ALERT` comment in `infra/docker/prometheus/rules/mc-alerts.yaml`.
**Since story 3**, `ClientVideoServerMutedSenderFrames` (A5, warning; client-reported, video only; rule owned by
observability, OBS-B) anchors here. It is receive-side corroboration that MH's video ingress drop failed,
not proof: see the video arm below.
**Severity**: triage on report, as a moderation failure. Arm 2 is a security incident.
**Runbook Section**: `#scenario-19-server-mute-not-taking-effect-at-ingress`

**The report**: "The host muted someone and we can still hear (or see) them."

**How it is enforced (story 2 R-9).** A host's server mute is decided at MC and carried to MH as
`server_muted_sources` on the meeting's next registration snapshot. A change to the muted set alone
advances `policy_generation`. MH drops that sender's frames at **ingress**, keyed by sender. MH does
not read the stream component: it sits in the SFrame key id inside the payload, which MH forwards
without parsing (ADR-0036 §7, type-blind). Each drop is counted as
`mh_media_frames_dropped_total{direction="ingress",reason="server_muted"}`, present at zero from
process start. MH logs nothing about mutes (ADR-0036 §11 deny scope).

> **Corrected 2026-10-21 (story 3 OPS-A): the paragraph above is the story-2 mechanism, and it
> holds only until MH-A1 and MC-A.** Story 2's mute is **sender-scoped**: every stream of a muted
> sender is dropped. It was exact only because media was audio-only. From story 3 it is **per
> declared stream**. MC lists every published stream in the registration's `source_streams`
> (sender, stream number, transport, disposition), and a server-muted stream is
> `DROP_AT_INGRESS`. MH attributes each ingress frame by (connection-bound sender, uplink
> relay-region `stream_id`), looked up in `source_streams`, without parsing the payload. The uplink
> `stream_id` is the publisher-stamped MC send id (`SendStream.stream_number`, which is 0 for audio
> and 1 for video today). It is **hop-local, unauthenticated and publisher-set**. There is no
> signed stream number in the header. **R-16 still holds, in slot terms**: a muted stream is
> dropped at ingress, so a frame relabelled onto the other stream id can reach only slots of the
> other kind, and receivers take the decoder from the slot kind (wire-freeze Amendment 3). A
> reader who kept the sender-scoped model would expect a video server mute to silence audio too,
> and would read audio still flowing as a failed mute. The per-kind arms of this scenario
> (including "camera light still on after server mute") are OPS-B's extension. Server mute stays
> reachable by any holder of the service credential the registration contract accepts (Arm 2),
> now per kind.
>
> **Corrected 2026-10-23 (story 3 MC-B):** `server_muted_sources` no longer exists. MC-B deleted it
> from the contract, together with MH's story-2 reading of a registration that declares no
> `source_streams`: such a registration with egresses is now REJECTED
> (`mh_media_policy_applies_total{outcome="rejected_transport_inconsistent"}`), never read
> sender-scoped. A server mute of kind K is now K's stream at `DROP_AT_INGRESS` AND K's send
> directive withdrawn (empty targets), so a compliant client stops sending K and MH's
> `server_muted` drops are the backstop: **flat `server_muted` drops under an applied mute are the
> expected healthy state**, not evidence the mute failed. "We can still hear them" is then
> answered by whether the stream is at `DROP_AT_INGRESS` in the applied snapshot (MC Scenario 21).

The MC half of this triage,
covering which host asked, what MC recorded, and whether the push was confirmed, is
[MC Scenario 21](mc-incident-response.md#scenario-21-server-mute-not-enforced). Start there for
Steps 1-2, then use the arms below, **in cost order**.

**Comparing MC's mute state with MH's drops** is fleet-aggregate and directional only. The rule, and
the benign silent-source exclusion, are in MC Scenario 21 Step 3. Do not compare per instance.

#### Arm 1 — refused at MC, while someone believes it applied

Cheapest to check, so check it first. The query and every `outcome` value's meaning are in
[MC Scenario 21 Step 1](mc-incident-response.md#scenario-21-server-mute-not-enforced). Any outcome
other than `applied` means MC never recorded a mute, so nothing was sent to MH. The requester's
belief is the only state that says "muted". The shipped web app renders server-mute state **from MC's
broadcast, not optimistically**. **If a client UI shows a participant as server-muted while MC
recorded no `applied` for it, the client is rendering optimistically. That is a client defect**
(route to `client`), not an enforcement failure. Remedy: the requester must be the meeting's host.
There is no other authority.

#### Arm 2 — the policy was overwritten by a registration MC did not send

**This is the ownership gap, and it is a security incident until explained.** The registration
contract authorizes on **caller class alone**: the MC service credential, meaning scope
`service.write.mh` with service_type `meeting-controller`. It has **no per-meeting ownership binding**.
`mc_id` is caller-supplied, and any validation-passing registration rebinds the meeting's owner and
replaces its muted set. So a holder of the MC service credential can set or clear a mute. That
set includes every MC instance in the fleet (one shared credential), a buggy or failed-over MC, and
anyone else holding it. **It is not bounded to MC pods**, because the MC→MH channel is plaintext and the
bearer crosses the pod network in cleartext (`docs/TODO.md` §Media Path Obligations, "The MC→MH
control channel that now carries a ROUTING AUTHORITY is plaintext"). The open authorization gap is
tracked in one place: `docs/TODO.md` §Media Path Obligations, "MH does not verify meeting ownership".

**Signature.** The likely entry point is **`MCMediaGenerationDivergence`**. MH holds a generation the
owning MC never issued, so the owning MC's pushes read `generation_mismatch` on
`mc_media_policy_pushes_total{outcome}`, and MH records them as `rejected_stale` on
`mh_media_policy_applies_total{outcome}`. **Discriminator: a divergence with no MC restart behind it.**
An MC restart's floor adoption is recorded on `mc_media_policy_generation_adoptions_total`, never as a
mismatch (MC Scenario 15). If the writer used a DIFFERENT `mc_id`, MH logs the WARN "Meeting
registration taken over by a DIFFERENT MC" (target `mh.session`, fields `registered_mc_id` and
`caller_mc_id`). **A write that reuses the owning MC's own `mc_id` leaves no such WARN.** Then only
the generation signature shows it.

**Remedy.**
1. Identify the unexpected writer from the WARN and MC's logs. **If no MC failover or MC defect explains
   it, treat it as a security incident**: rotate or revoke the MC client credential in AC and involve
   `security`. Rotation stops the current bearer's further use. It does not close on-path exposure of
   the next one until the MC→MH channel is encrypted with mTLS peer authentication (the plaintext-channel
   entry above). The channel does check the bearer's scope and service_type today.
2. **Rotating the credential does not restore the affected meeting.** MH's installed generation never
   moves back down, so a write at an inflated generation leaves the owning MC's pushes stale for that
   meeting on that handler. Recovering it has a counter-intuitive order, and its one home is
   [Scenario 27](#scenario-27-registermeeting-refused-or-meeting-not-released), the ownership-reject arm.
   Follow it there; it will not self-correct (MC Scenario 15). Escalate to `meeting-controller`.

This path is **unsupported and untooled**. It is not an operator lever, and this runbook gives no
procedure for exercising it (`mh-deployment.md` §Operator levers).

#### Arm 3 — applied but not enforced

**Signature**: MC applied the mute (`applied`, Arm 1), and the push that carried it was **confirmed**.
Neither metric is per meeting, so confirm per meeting from logs. `mc_media_policy_pushes_total{outcome="match"}`
says only that confirmations are happening at all (it has no meeting label, ADR-0036 §11). The
per-meeting evidence is MC's `mc.register_meeting.trigger` "RegisterMeeting succeeded" line, on the
pusher span that carries the meeting and the handler endpoint (`crates/mc-service/src/media_routing/pusher.rs`),
with a `policy_generation` at or after the one the mute advanced to. **That line is DEBUG**, so at the
default level it is absent. What remains at INFO is the negative: no divergence ERROR from the pusher
for that meeting and handler (MC Scenario 15). Then the muted participant is
known to be sending on that handler — since MC-B a compliant client stops sending when its directive
is withdrawn, so "known to be sending" now means a client that IGNORED its empty targets (evidence:
MH ingress frames from that sender still arriving, or MC Scenario 21 Step 1b rose while the camera
light stayed on) — yet `mh_media_frames_dropped_total{reason="server_muted"}` on that
pod stays flat. That series is per pod, so "flat" holds only if no other mute on the handler is
dropping. MH installed a muted set it is not enforcing. That is an MH defect.

**Remedy: redeploy.** Roll back to, or forward to, an MH build that enforces. **There is no runtime lever**:
no per-meeting toggle, no admin API, no flag. Restarting MH does not help. The same build re-installs
the same policy, and the replacement only drains the pod and moves every meeting on it (a blip per
meeting) onto an MH running the same build. Escalate to `media-handler` with the
generation MC confirmed and the flat drop rate.

#### Arm 4 — not applied: generation divergence

**Signature**: MC applied the mute, and the push was **not** confirmed. `mc_media_policy_pushes_total`
shows `generation_mismatch` or `no_applied_generation`, or the RegisterMeeting failed outright. The
mute rides the same snapshot as every other policy change, so a push that is failing fails for the mute
too. One mute-specific cause: a muted set larger than `MH_MAX_MUTED_SOURCES_PER_MEETING`
(`mh_media_muted_sources_per_meeting_limit`; *retired by story 3: from MH-A1 the bound is
`MH_MAX_SOURCE_STREAMS_PER_MEETING` on `source_streams`, every published stream and not only muted
ones, so an undersized value refuses registrations whether or not anyone is muted; corrected
2026-10-21*) makes MH refuse the **whole** registration as
`mh_media_policy_applies_total{outcome="rejected_invalid"}`. The previous policy, without the mute,
stays live. *(Corrected 2026-10-21, story 3 OPS-A: a whole-registration reject is answered with a
gRPC `invalid_argument`, so MC records no push outcome and MC Scenario 15 shows nothing for it. MC's
reading is `mc_register_meeting_total{status="error"}` and the WARN "RegisterMeeting RPC failed"
(target `mc.grpc.mh_client`): work [MC Scenario 12](mc-incident-response.md#scenario-12-registermeeting-coordination-failures).
A push that was answered but not applied, `generation_mismatch` or `no_applied_generation`, is
[MC Scenario 15](mc-incident-response.md#scenario-15-media-generation-divergence).)*

#### Video arm — per declared stream, enforced in slot terms (story 3)

**"We can still SEE them" is triaged here too, and enforcement is per declared stream.** MC
declares the muted kind's stream with SourceStream disposition `DROP_AT_INGRESS`. MH attributes each
ingress frame by (connection-bound sender, uplink relay-region `stream_id` = the MC send id), looks
it up in `source_streams`, and never parses the payload (wire-freeze Amendment 3). **The uplink
label is publisher-set and unauthenticated.** A frame relabelled onto the other stream's id reaches
only slots of the OTHER kind, and receivers take the decoder from the slot kind. That is why R-16
still holds, and it is stated in terms of slots: **nothing from a sender whose video is
server-muted reaches any subscriber's video slot, however it is labelled or transported, while audio
keeps flowing.** Audio still flowing under a video server mute is correct and is not a failed mute.

**Proof of enforcement is MH's own counter**:
`mh_media_frames_dropped_total{direction="ingress",reason="server_muted"}` rising on the handler.
Since MC-B a compliant client stops capture when the video directive is withdrawn, so flat drops
under an applied mute are the healthy state. Rising drops mean a client that kept sending and MH
held the line.

**Receive-side corroboration, not proof**:
`dt_client_media_frames_dropped_total{reason="sender_server_muted", media_kind="video", content_kind="main"}`, alerted as
`ClientVideoServerMutedSenderFrames` (A5, warning, 10m; rule owned by observability, OBS-B). It
counts video frames that reached a receiver from a sender that receiver knows is server-muted for
video. The receiver drops them, so nothing renders. Any sustained value is the receiver-side signal
that **MH's video ingress drop failed**: the frames got past MH. It is client-reported, so it
corroborates and does not prove. Read it against MH's `server_muted` drops and work Arm 3 (applied
but not enforced) if MH's counter is flat on the handler while A5 fires. **A short burst at server
UNMUTE is benign**: the publisher's resume keyframe beat `ParticipantMuteUpdate` to the receiver,
which re-requests it, counted as `dt_client_media_recovery_requests_sent_total{source="server_mute_lifted"}`.
If that series moves with the burst, it is unmute churn, not an MH failure, so do not route it to Arm 3.

```promql
sum(rate(mh_media_frames_dropped_total{direction="ingress",reason="server_muted"}[5m]))
# 2026-10-26 (series identity, not traffic): client per-kind series carry content_kind beside media_kind; "main" = mic/camera (no screen share yet).
sum(rate(dt_client_media_frames_dropped_total{reason="sender_server_muted",media_kind="video", content_kind="main"}[5m]))
sum by (media_kind, content_kind) (increase(mc_media_send_directive_withdrawals_total{reason="server_muted"}[15m]))
```

**Sub-arm: "Camera light still on after server mute".** MC withdraws that kind's directive (R-16),
so capture should stop and, for video, the camera light goes off. A client still capturing means
the withdrawal was **lost or ignored**:
- `mc_media_send_directive_withdrawals_total{media_kind="video", content_kind="main", reason="server_muted"}` did NOT
  rise for an `applied` mute of a **watched** kind: MC did not withdraw, or could not deliver
  (`mc_media_slot_view_emissions_total{outcome="delivery_failed"}`). That is MC's half,
  [MC Scenario 21 Step 1b](mc-incident-response.md#scenario-21-server-mute-not-enforced);
- it did not rise because the kind was **unwatched**: its directive was already withdrawn
  (`reason="unwatched"`), so no new withdrawal is expected and the camera should already be off.
  The track stops on ANY empty-targets withdrawal. A camera that is on here is the client ignoring
  its empty targets. Route it to `client` and `security`;
- it DID rise: the client ignored its empty targets. That is a client fault (privacy), so route it to
  `client` and `security`.

Meanwhile MH's backstop still drops at ingress, so `server_muted` drops rise. No subscriber's video
slot receives the frames, but the device is still capturing, and that is the user-visible harm.

**Sub-arm: "Video not resuming after server unmute".** A server unmute restores capture **unless the
participant is client-muted** (R-18). The participant gets a non-suppressible notice when it happens.
**Check client mute state first**: holders' slot reads `avatar_client_muted` (no longer
`avatar_server_muted`) and the participant's own camera control is off. The remedy is that the
participant turns their camera on. If the slot reads `avatar_server_muted` after an `applied` unmute,
that is MC's half (MC Scenario 21). If the slot reads ACTIVE but nothing renders, go to
[Scenario 22](#scenario-22-video-slot-stuck-pending).

**Escalation**: `client` for Arm 1's optimistic rendering; `security` + `meeting-controller` for Arm 2;
`media-handler` for Arm 3 (and for A5 firing while MH's `server_muted` drops are flat); `meeting-controller` for Arm 4 and for a withdrawal that did not rise; `client` + `security` for a camera still capturing after a delivered withdrawal.

**Related**: [MC Scenario 21](mc-incident-response.md#scenario-21-server-mute-not-enforced);
[MC Scenario 15](mc-incident-response.md#scenario-15-media-generation-divergence);
`docs/observability/metrics/mh-service.md` (the `server_muted` drop reason).

---

### Scenario 20: Video Keyframe Storm

**Alert**: `MHVideoKeyframeStorm` (A1, warning; rule owned by observability, OBS-B)
**Severity**: Warning
**Runbook Section**: `#scenario-20-video-keyframe-storm`

**What this is.** Ingress video group starts per open publisher group stream
(`mh_media_stream_ingress_groups`, averaged over the window) have stayed above half the rate the
minimum keyframe interval allows (0.5 / `min(mc_media_video_keyframe_min_interval_seconds)`) for 10
minutes. Read as group lifetime: the fleet's mean group is shorter than twice the directed interval.
The denominator is the open-group gauge, not `mh_media_stream_sources_declared`: declared sources
include PAUSED, DROP_AT_INGRESS and unwatched streams that key nothing, so dividing by them would
hide a storm in proportion to idle sources. `min()` over MC instances is the false-positive-safe side
under an MC config skew. The alert reads `mh_media_group_starts_total{direction="ingress"}`, which MH counts
itself. A patched client can neither inflate nor hide that series, so it is the server-side
truth about how often publishers key.

**Context: there is no keyframe interval (R-9). Keyframes are event-driven (Amendment 4).**
- **The publisher keys any directed target that has no group in progress**: at a directed start,
  when a target is added, and after an abandonment it decided itself (its per-target backlog cap).
  One keyframe reaches every target of the stream, because there is one encoder (R-12). **It never
  re-keys for a stream MH refused or stopped (STOP_SENDING).** Whoever decides an abandonment
  repairs it, so MH requests a keyframe when it wants that stream again.
- **MH requests at most once per (source stream, counter)**, only for a source it has already seen,
  and never retries (R-10). Count: `mh_media_keyframe_requests_sent_total{trigger="initial"}`.
- **Receivers send one unpaced recovery request per loss event.** MH remaps slot to source stream and
  forwards at most one per (source stream, counter):
  `mh_media_keyframe_requests_sent_total{trigger="receiver_recovery"}`. Every request is counted by
  outcome on `mh_media_recovery_requests_received_total{outcome}`, and only `forwarded` ones are
  handed upstream (R-13). `forwarded` means accepted into the publisher's newest-only outbox, not
  written. It can exceed `requests_sent{trigger="receiver_recovery"}`, because a newer request
  replaces a queued one, and a write to a dead control stream counts on
  `mh_media_keyframe_request_write_failures_total` instead.
- **The publisher honours a request only for a stream it is sending to that handler**, with a
  counter at least its current one. Every keyframe is bounded by max(MC-directed
  `min_keyframe_interval_ms`, published as `mc_media_video_keyframe_min_interval_seconds` and
  configured as `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`; the client's compiled floor). A keyframe that
  would come too soon is held as the single deferral, never dropped (R-10–R-12). **So forwarded
  requests are bounded by groups, and groups are bounded by that interval.** A sustained rate near
  the bound means something keeps creating loss events or targets. It does not mean a counter is
  looping.

**Detection**

```promql
# The alert's shape: ingress group starts per open publisher group stream, vs the bound.
sum(rate(mh_media_group_starts_total{direction="ingress"}[10m]))
  / clamp_min(sum(avg_over_time(mh_media_stream_ingress_groups[10m])), 1)
0.5 / min(mc_media_video_keyframe_min_interval_seconds)
# Idle-source context (not the denominator): declared stream-per-group sources.
sum(mh_media_stream_sources_declared)

# What MH asked for, and why
sum by (trigger) (rate(mh_media_keyframe_requests_sent_total[10m]))
# What receivers asked for, by outcome
sum by (outcome) (rate(mh_media_recovery_requests_received_total[10m]))
# Abandonments that precede re-keys
sum by (reason) (rate(mh_media_group_resets_total[10m]))
```

All of these are pod-level and carry no identity. Nothing in this scenario needs a meeting,
participant, sender or stream id. Per-participant investigation, if it is needed at all, uses MC's
assignment state, never a per-stream dump. MC does not log slot occupancy. The publisher behind a
failing tile comes from user reports, and MC Scenario 18 Step 0 then gives that participant's
connected handlers and per-handler edge counts.

**Diagnosis: fork FIRST on MH pod age.**

```bash
kubectl get pods -n dark-tower -l app=mh-service -o custom-columns=POD:.metadata.name,STARTED:.status.startTime
```

**(a) A burst right after an MH roll or restart is EXPECTED in this story.** Every subscriber
reconnects and is held for a group start, so every directed target keys at once. Resume and
keyframe jitter after a mass reconnect is story 4's work (the ADR-0036 addendum's story-4 line).
**Do not tune it away.** If the rate decays to baseline within a few intervals of the pods becoming
Ready, close the alert as this arm. A burst on a pod that has NOT restarted is one of the arms below.

**(b) A lossy-path recovery loop (R-12): accepted, bounded by the minimum interval.** A path whose
capacity is below the fixed bitrate abandons groups and recovers again and again. The signature is
`mh_media_group_resets_total{reason="subscriber_backlog"}` rising alongside the group starts
(MH abandoning toward a slow subscriber, then requesting with `trigger="initial"`). A lossy UPLINK
re-keys on its own abandonment instead. That shows on the client series
`dt_client_media_keyframes_produced_total{source="abandonment"}` (client-media board, *Video
Keyframes Produced by Cause*). Both shapes are accepted in this story and bounded by the interval.
The real fix (bitrate adaptation, simulcast, a give-up threshold) is a later media-resilience story.
If `subscriber_backlog` is high **across the fleet**, this is also
[Scenario 21](#scenario-21-slow-video-subscriber-stream-credit-stall).

**(c) A recovery-request flood (R-13) cannot raise the keyframe rate.** One forward per (source
stream, counter) serves every receiver, so a flood moves the outcome counters, not group starts.
Read `mh_media_recovery_requests_received_total{outcome}`:
- high `duplicate`: many receivers lost the same group. That is shared loss, usually arm (b) or an
  uplink problem, and it is healthy dedupe;
- sustained `counter_ahead` or `source_mismatch`: a client or contract fault. A small
  `source_mismatch` tail right after a source change is normal;
- `unmapped_slot` or `malformed`: a client fault.

**There is no rate-limit drop for recovery requests and no recovery-rate key.** Junk costs MH a
fixed-size parse and a lookup, bounded by QUIC flow control.

**(d) A patched or faulty publisher keying without cause.** Group starts run above what forwarded
requests and target changes explain. Read the client series
`dt_client_media_keyframes_produced_total{source}`, with values `request`, `directive_start` and
`abandonment` (client-media board, *Video Keyframes Produced by Cause*). If `request` is small and
`abandonment` is flat while MH counts more ingress group starts than those causes add up to, the
publisher is keying outside the contract. **The publisher-side bound is enforced by the client**,
so a patched client can exceed it. It is visible here and as egress pressure
([Scenario 18](#scenario-18-media-egress-budget-exhaustion)). The client series is client-reported
and can be inflated, so it corroborates the cause. MH's own count is the proof.

**Resolution**
- **No runtime lever except the R-26 video-off lever**, `MC_MAX_VIDEO_RECEIVE_SLOTS=0`. It turns
  video off for every meeting and leaves audio untouched, and it costs an MC restart per pod
  ([`mh-deployment.md` §Operator levers for media](mh-deployment.md#operator-levers-for-media-video-off-and-nothing-else),
  [`mc-deployment.md` §Video-off lever](mc-deployment.md#video-off-lever)). Use it only if egress
  pressure from the storm threatens audio (Scenario 18).
- **MH has no keyframe-request or recovery tuning key. Do not look for one, and do not offer one.**
  The retry, aggregation and recovery-rate keys were removed with Amendment 4.
- Raising `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS` lowers the bound for a conforming client only, and it
  rolls MC (`mc-deployment.md` §Configuration Reference). It is a capacity decision, not an incident
  remedy. It does nothing against arm (d).
- Arm (a): wait it out. Arm (b): accepted, so file it as evidence for the media-resilience story if
  it is sustained. Arms (c) and (d): identify the client build. A dominant `client_version` on the
  client series points at a release.

**Escalation**: `client` for arms (c) and (d) (a request or keying contract fault; involve `security`
if a build appears deliberately patched); `media-handler` if group starts rise with no matching
cause on either side; operations for a sustained arm (b) across the fleet (capacity, Scenario 21).

**Related**: [Scenario 21](#scenario-21-slow-video-subscriber-stream-credit-stall),
[Scenario 22](#scenario-22-video-slot-stuck-pending),
[Scenario 18](#scenario-18-media-egress-budget-exhaustion);
`docs/observability/metrics/mh-service.md` (group starts, keyframe and recovery requests).

---

### Scenario 21: Slow Video Subscriber (Stream-Credit Stall)

**Alert**: `MHVideoSubscriberBacklogResets` (A2, warning; rule owned by observability, OBS-B). It is fleet-level only.
**Severity**: Warning
**Runbook Section**: `#scenario-21-slow-video-subscriber-stream-credit-stall`

**What this is.** Fleet-wide, `mh_media_group_resets_total{reason="subscriber_backlog"}` has run
above 10% of egress group starts for 15 minutes. One bad path cannot reach 10% of the fleet. **So
firing means a systemic capacity problem, and it is the leading indicator for egress exhaustion**
([Scenario 18](#scenario-18-media-egress-budget-exhaustion)). Per-path recovery is a panel, not a
page (R-12). One user on a slow link never pages.

**Context**
- MH forwards video **cut-through** (R-21). It reads the frame header, enforcing the maximum
  payload against the length field before anything is accepted, rewrites the header per subscriber,
  and passes payload bytes on as they arrive. MH holds only a header and the chunk in flight per
  incoming stream.
- Each subscriber stream has a capped backlog, `MH_EGRESS_STREAM_BACKLOG_BYTES` (published as
  `mh_media_egress_stream_backlog_limit_bytes`). Past the cap MH **resets that destination's group
  stream only**. It never stalls the publisher, the source's other destinations or anyone's audio.
  The subscriber returns to waiting for a group start, and MH recovers it with its own
  once-per-counter keyframe request (R-12).
- Resets count groups, never frames. They are **not a subset** of egress group starts, because a
  group can be abandoned before its stream opens. The 10% ratio is a fleet health ratio, not a
  "share of groups lost" (`docs/observability/metrics/mh-service.md`, `mh_media_group_resets_total`).

**Detection**

```promql
# The alert's shape, fleet-wide
sum(rate(mh_media_group_resets_total{reason="subscriber_backlog"}[15m]))
  / clamp_min(sum(rate(mh_media_group_starts_total{direction="egress"}[15m])), 1e-9)

# Per pod, to see whether one handler dominates
sum by (instance) (rate(mh_media_group_resets_total{reason="subscriber_backlog"}[15m]))
```

**Diagnosis: read the rows, in this order.**

| Signal | What it means | Next |
|---|---|---|
| `mh_media_group_resets_total{reason="subscriber_backlog"}` | Slow subscribers: a destination's backlog overflowed, its writer fell two groups behind, or its downlink channel was full. | Fleet-wide → capacity (Resolution). One pod → that pod's egress (Scenario 18 per-pod reading). |
| `mh_media_group_resets_total{reason="publisher_stall"}` | A publisher stalled mid-frame past `MH_STREAM_MID_FRAME_STALL_MS` (`mh_media_stream_mid_frame_stall_seconds`), so MH reset its downstream streams. | Uplink capacity or a client encoder stall. Sustained → `client`. |
| `{reason="publisher_reset"}`, `{reason="source_ended"}`, `{reason="edge_removed"}` | **Normal lifecycle**: the publisher reset its group, the source ended or left, MC removed the edge or the meeting ended. | Only a sudden fleet-wide jump matters, and it usually tracks an MC or MH roll. |
| `mh_media_frames_dropped_total{reason="stream_rate_limited"}` | Stream opens above `MH_STREAM_OPENS_PER_CONNECTION_PER_SEC`, refused at accept after only the fixed header is read. MH records the source's counter and repairs a declared source with its own keyframe request (`trigger="initial"`). **One per refused stream**, so it is a lower bound on frames. A conforming publisher opens at most one group stream per keyframe, so it stays under the limit while the limit is at least 1 / min interval. **Nothing validates that pairing at boot**: MH reads no keyframe interval, and the key is only range-checked. The shipped values (10 opens/s against a 500 ms interval, 2/s) leave the margin, but at MC's 100 ms floor (10/s) a conforming publisher plus target changes can reach it. | First confirm `mc_media_video_keyframe_min_interval_seconds` has not been lowered toward 0.1 and the configured limit was not lowered. Re-check the pairing whenever `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS` changes. Sustained non-zero with a sane config means a publisher churning group streams: `client`. |
| `mh_media_frames_dropped_total{reason="partial_frame_discard"}` | A publisher's group stream ended while a frame header was partly buffered. Counts on every non-clean end, MH's own aborts included. | Tracks publisher disconnects. Sustained without disconnects → `media-handler`. |

**Concurrency: there is NO event counter, by design. Do not hunt for one.** quinn's flow control
stops the peer opening streams past the declared limit, so the event cannot happen at MH and
nothing counts it. Triage it from the declared-limit config gauge
`mh_media_uni_streams_per_connection_limit` beside the concurrent-ingress-groups gauge
`mh_media_stream_ingress_groups`. On the publisher side, MAX_STREAMS starvation shows only as the
client's `dt_client_media_send_groups_stopped_total{outcome="open_stalled"}`.

**Memory: exhaustion is a counted reset, never an OOM kill.** R-21 validates backlog cap × streams
against `MH_POD_MEMORY_LIMIT_BYTES` (the downward-API limit) at startup, and MH refuses to boot if
the product does not fit. **Backlog in use** is `mh_media_egress_stream_backlog_bytes` (since
OBS-B: bytes charged across every installed edge's stream backlog on the pod, SAMPLED on the session
actor's 10 s pending tick, never moved per chunk; a draining writer whose edge already left the
routing snapshot is not counted, a short tail), beside the per-destination cap `mh_media_egress_stream_backlog_limit_bytes` and the
pod's charge bound (cap × `mh_media_egress_stream_ceiling`) on the MH Media dashboard. It is the
CHARGE, not memory: in-use near the bound is backlog pressure, not an OOM warning.

```promql
max by (instance) (mh_media_egress_stream_backlog_bytes)
max by (instance) (mh_media_egress_stream_backlog_limit_bytes * mh_media_egress_stream_ceiling)
```

The only memory alert is the existing `MHHighMemory`
(`container_memory_usage_bytes / container_spec_memory_limit_bytes` for `container="mh-service"`;
[Scenario 8](#scenario-8-resource-pressure)). If it fires together with this alert, the
startup check did not hold, and that is a `media-handler` defect.

**Resolution**
- **Fleet-wide `subscriber_backlog`** is a capacity problem: subscribers' paths cannot carry the
  fixed bitrate. The relief, in order:
  1. If egress admission is also refusing, work [Scenario 18](#scenario-18-media-egress-budget-exhaustion).
  2. The R-26 video-off lever, `MC_MAX_VIDEO_RECEIVE_SLOTS=0`. It keeps audio and costs an MC
     restart per pod ([`mh-deployment.md` §Operator levers for media](mh-deployment.md#operator-levers-for-media-video-off-and-nothing-else)).
  3. A lower-bitrate `media-codec-profile`. That is a config change that rolls MC and MH together,
     and it sheds media sessions on the MH pods (`mh-deployment.md` §Rollout With Media Flowing),
     so it is a planned change, not an incident step.
- **Do not raise `MH_EGRESS_STREAM_BACKLOG_BYTES` to stop resets.** It moves memory, not capacity:
  a deeper backlog on a path that cannot drain only delays the reset. The startup memory check
  may then refuse to boot.
- Repeated recovery on one path below the fixed bitrate is accepted in this story (R-12). The
  give-up threshold and "poor connection" state belong to a later media-resilience story.

**Escalation**: operations for capacity; `media-handler` if `MHHighMemory` fires with this alert or
`partial_frame_discard` rises without disconnects; `client` for `publisher_stall` or
`stream_rate_limited`.

**Related**: [Scenario 18](#scenario-18-media-egress-budget-exhaustion) (egress exhaustion, which
this alert leads); [Scenario 20](#scenario-20-video-keyframe-storm) arm (b) (the lossy-path loop
seen from the keyframe side).

---

### Scenario 22: Video Slot Stuck Pending

**Alert**: `MHVideoEdgeStuckPending` (A3, warning; rule owned by observability, OBS-B)
**Severity**: Warning
**Runbook Section**: `#scenario-22-video-slot-stuck-pending`

**What this is.** `mh_media_edge_pending_oldest_seconds` has exceeded
10 × `mc_media_video_keyframe_min_interval_seconds` for 5 minutes. The count is
`mh_media_edges_pending`. There is no MH retry interval to compare against any more: retries were
removed with Amendment 4.

**Context.** "Pending" means a stream edge whose subscriber is connected here and whose assigned
FORWARD source is connected here, but which has seen no group start yet. Pending is **MH-internal**
(R-4):
- **There is no client slot state for pending, and no give-up timeout while the source stays
  assigned.** MH's gauge is the only server-side detector. The client keeps showing what the slot
  showed before (R-4/R-5), so **users see a frozen picture or avatar, not a spinner**.
- **Nothing retries** (Amendment 4). A directed target with no group in progress gets a keyframe
  from the publisher within one minimum interval, and a seen source gets one MH request per counter.
  **Stuck pending is therefore a MISSING-EVENT BUG**: someone owed a keyframe or a request and
  never sent it. It is never a slow retry. The gauge and A3 exist to make that loud.
- An MH→MC switch-completion report does not exist until story 5. MC cannot see pending.

**Detection**

```promql
max(mh_media_edge_pending_oldest_seconds)
10 * max(mc_media_video_keyframe_min_interval_seconds)
sum(mh_media_edges_pending)
sum by (outcome) (rate(mh_media_edge_pending_exits_total[15m]))   # group_start should dominate

# The client half: ACTIVE video slots showing neither decodable video nor an avatar (no identity)
# 2026-10-26 (series identity, not traffic): client per-kind series carry content_kind beside media_kind; "main" = mic/camera (no screen share yet).
histogram_quantile(0.95, sum by (le) (rate(dt_client_media_slot_unfulfilled_duration_ms_bucket{media_kind="video", content_kind="main"}[5m])))
max by (media_kind, content_kind) (dt_client_media_slots_unfulfilled{media_kind="video", content_kind="main"})   # ONE browser's last value, not a fleet count; never read as healthy at 0
```

The fleet signal is the histogram. It is observed on EXIT only, so a slot stuck for a whole session
appears only at that session's teardown, and the p95 lags a live incident.

**If the client unfulfilled series rises together with MH pending, this is the scenario.** If MH
pending is flat, MH believes it is forwarding. The receive half is then
[MC Scenario 23](mc-incident-response.md#scenario-23-client-video-slot-unfulfilled).

**Diagnosis: fork in this order.**

**(a) Is the publisher directed?** An undirected source cannot answer a keyframe request (R-11).
Read MC's directive outcomes and withdrawals
([MC §send-directive wiring](mc-incident-response.md#send-directive-wiring)):
`mc_media_send_directives_total{outcome}` and
`mc_media_send_directive_withdrawals_total{media_kind="video", content_kind="main", reason}`. A withdrawal is healthy.
Pending is a bug here only when MC both holds the source in a live slot AND withdrew its video.
That is an MC disagreement between its slot table and its directive, so escalate to
`meeting-controller`.

**(b) Did the publisher key, or refuse?** A directed target with no group in progress gets a
keyframe within the minimum interval, unrequested (R-12). An unseen source correctly gets no MH
request. Read `dt_client_media_keyframes_produced_total{source}` and the client refusal reasons on
`dt_client_media_keyframe_requests_received_total{outcome}` (C-A1/C-A2). **No refusal should leave a
slot pending.**
- persistent `not_sending` ("not sending to that handler"): MC's directive and MH's edge disagree,
  or the start did not key. A request naming a stream number that is no published kind's directed
  stream lands on `dt_client_media_keyframe_requests_unattributed_total{outcome="not_sending"}`
  instead, so read both series;
- persistent `stale_counter`: the newer group is not reaching this handler. Either the publisher
  abandoned toward it with no `abandonment` keyframe (R-12), or MH refused it (fork e);
- `min_interval` is a deferral, not a refusal. **Pending past one interval means the deferred
  keyframe was lost.**

**(c) Capture failure not presented as muted.** R-19 requires a failed or revoked camera to present
as video-muted. A pending source whose client reports a camera failure
(`dt_client_media_capture_failures_total{media_kind="video", content_kind="main", outcome}`, including `ended`) is a
client bug: that source should present to others as video-muted (R-19), not as a live slot.

**(d) Recovery-request outcomes**, per wire-freeze ruling 2, on
`mh_media_recovery_requests_received_total{outcome}`:
- `source_mismatch`: a request that arrived after a source change. Normal in small numbers;
- `counter_ahead`: a counter higher than MH has seen. A client or contract fault.

A slot that receives nothing from its NEW source is MH's to recover (R-13), not the receiver's. So
pending on a fresh switch with no `initial` request from MH, once the source has been seen, is an MH
missing-event bug.

**(e) Ingress refused.** Read `mh_media_frames_dropped_total` with reason `stream_rate_limited`,
`unmatched_stream` or `transport_mismatch` (MH's attribution and stream-limit drops, MH-A1/MH-A2).
Also read `mh_media_frames_dropped_total{direction="ingress",reason="unknown_version"}`, a v2
sender's frames dropped at MH ingress (the client never sees them; see [`mh-deployment.md` §Skew detection](mh-deployment.md#skew-detection-one-row-per-one-component-skew-across-the-cutover)).
**The publisher does not re-key a stream MH refused or stopped (STOP_SENDING).** MH requests when it
wants the stream again. So **pending after a refusal that clears, with no MH request following, is
an MH missing-event bug**: `mh_media_keyframe_requests_sent_total{trigger="initial"}` should move.

**(f) A server-muted source showing as pending is an MC bug.** The slot should be
`avatar_server_muted`, with the directive withdrawn (R-16), and MH exits pending as
`paused_forward_nothing`. Go to [MC Scenario 21](mc-incident-response.md#scenario-21-server-mute-not-enforced).

**Resolution.** No operator lever repairs a missing event. The affected subscriber or publisher can
reload, which is a fresh join and a fresh directed start that keys. That clears the symptom and
destroys nothing an investigator needs, because the metrics carry no identity anyway. The durable
fix is the code path that owed the event. **Do not restart MH**: a replacement drains the pod and MC moves every meeting on it (a blip per meeting; only what is not moved by the drain deadline is shed), and it
recovers nothing a reload would not. The video-off lever
([`mh-deployment.md` §Operator levers for media](mh-deployment.md#operator-levers-for-media-video-off-and-nothing-else))
applies only if the defect is widespread.

**Escalation**: `meeting-controller` for (a) and (f); `client` for (b) and (c) and a client-side
`counter_ahead`; `media-handler` for (d) and (e) when MH owed the request.

**Related**: [MC Scenario 23](mc-incident-response.md#scenario-23-client-video-slot-unfulfilled),
[Scenario 20](#scenario-20-video-keyframe-storm), [Scenario 21](#scenario-21-slow-video-subscriber-stream-credit-stall).

---

### Scenario 23: Client Sending While Client-Muted

**Alert**: `MHPausedSourceIngress` (A4, warning; rule owned by observability, OBS-B)
**Severity**: Warning. Treat it as a **client privacy defect**.
**Runbook Section**: `#scenario-23-client-sending-while-client-muted`

**What this is.** `mh_media_paused_source_frames_total` has run above 1% of ingress
(`mh_media_frames_forwarded_total{direction="ingress"}`) for 15 minutes. **This is a CLIENT PRIVACY
DEFECT, not a mute-enforcement failure.** Under client mute nothing of that kind may leave the
device: no silence, no DTX, no video (R-17). MH learns client mute only from MC's policy, as the
stream's PAUSED disposition. **MH never drops on client mute** (R-17), which is why this is an
arrival counter and not a drop. The frames are forwarded, so a resume keyframe that beats MC's
policy update still reaches subscribers. **A short burst at unmute is expected** for exactly that
reason. Sustained arrivals mean a muted client is still transmitting.

**Detection**

```promql
sum(rate(mh_media_paused_source_frames_total[15m]))
  / clamp_min(sum(rate(mh_media_frames_forwarded_total{direction="ingress"}[15m])), 1e-9)

# Corroboration from the client side, both kinds (per-kind, no identity)
# 2026-10-26 (series identity, not traffic): client per-kind series carry content_kind beside media_kind; "main" = mic/camera (no screen share yet).
sum by (media_kind, content_kind, client_version) (increase(dt_client_media_mute_transitions_total[1h]))
sum by (media_kind, content_kind, client_version) (rate(dt_client_media_frames_sent_total[15m]))
```

**Diagnosis: forks.**
- **SDK version or stale bundle.** Split the client series by `client_version`. One version
  dominating means a build regression, or a tab running a pre-fix bundle that needs a reload.
- **The capture path did not stop.** A video client mute must stop the camera track. An audio client
  mute keeps the mic open and stops SENDING. If one kind is affected, the fault is that kind's mute
  path in the SDK.
- **A debug or test source wired into the send path.**
  - The R-7 self-check never reaches the encoder or a handler (`packages/web-app/tests/debugNeverExported.test.ts`).
    One that does is a client and security defect.
  - The R-27 test-pattern camera exists only in a `DT_TEST_PATTERN=1` dev build. There it replaces
    the camera, is sent, and must obey video mute exactly like the camera. A production-mode build
    refuses `DT_TEST_PATTERN=1` and the bundle test proves the pattern absent, so a fleet client
    sending it means a dev build was deployed. Identify the `client_version`; that is a client and
    security defect.

**Resolution.** **There is no operator remedy beyond identifying the build.** No operator path to an
individual's media exists, by design. Do not look for one and do not create one. Identify the
`client_version`, and roll the web-app bundle forward or back. A server mute
([Scenario 19](#scenario-19-server-mute-not-taking-effect-at-ingress)) is a host's moderation
action, not a remedy for this defect.

**Escalation**: `client` and `security`, always.

**Related**: [Scenario 19](#scenario-19-server-mute-not-taking-effect-at-ingress) (server mute; MH
DOES drop there); `docs/observability/metrics/mh-service.md` (`mh_media_paused_source_frames_total`).

---

### Scenario 24: Media-Dial Credential Rejections

**Alert**: `MHHighJwtValidationFailures` (the live rule is one aggregate failure ratio over both credential paths, `media_dial` and `service`, with no split; read this scenario's share with `{token_type="media_dial"}` in Diagnosis). Per-reason warnings, all on `mh_jwt_validations_total{token_type="media_dial"}`: `MHMediaDialTokenExpired` (`failure_reason="expired"` sustained); `MHMediaDialNoMeetingKey` (`failure_reason="no_meeting_key"` sustained; decided **before** the signature check, so unauthenticated and attacker-drivable — the ordering-bug reading only when it correlates with `mc_register_meeting_total{status="error"}` or legitimate clients failing to join, otherwise probing); `MHMediaDialWrongTokenType` (`failure_reason="wrong_type"` increase > 0); `MHMediaDialWrongHandler` (`failure_reason="aud_mismatch"` sustained, the `WRONG_HANDLER` rate). `expired`, `wrong_type` and `aud_mismatch` are post-signature (only a holder of the meeting's MC-minted key moves them).
**Severity**: warning
**Runbook Section**: `#scenario-24-media-dial-credential-rejections`

**Symptoms**: clients' dials close `DECLINED` (or `WRONG_HANDLER`) and redial or park; client
`dt_client_mh_connection_total{outcome=~"declined|wrong_handler"}` rises; affected participants have
no media on that handler while signalling works.

Clients dial MH with an MC-minted `media_dial` token (ADR-0039 §5). MH verifies it at accept, before holding anything, in a fixed order, against the Ed25519 key the meeting's `RegisterMeeting` installed. **AC and JWKS are not involved.** Every failure closes the session `DECLINED` except an `aud` mismatch, which closes `WRONG_HANDLER`. The reason is on the metric and in the `mh.webtransport.connection` WARN (`failure_reason`, `connection_id`; never the token or key), never on the wire.

```promql
sum by(failure_reason) (rate(mh_jwt_validations_total{token_type="media_dial",result="failure"}[5m]))
```

**Who can move each reason.** `no_meeting_key`, `malformed` and `signature_invalid` are decided before or at the signature check, so **any unauthenticated peer can drive them** with a forged token (and an MH restart moves `no_meeting_key` on its own): investigate them, never page or roll back on them alone. Only `expired` (including a future `iat`) and `wrong_type` need a token signed with the meeting's key; `aud_mismatch` does too, but it is routine for stale handler sets.

| `failure_reason` | What it means | First move |
|---|---|---|
| `expired` | `exp` passed (or `iat` in the future) beyond `MH_MEDIA_DIAL_CLOCK_SKEW_SECONDS`. | **Sustained = MC's TTL/3 token refresh push is broken** — the only signal for it: check `mc_media_handler_set_pushes_total{reason="token_refresh"}`. Otherwise clock drift: compare `date -u` on MC and MH nodes and fix NTP. **Do not raise the skew to mask drift** (bounded at 60 s). |
| `no_meeting_key` | The token's meeting is not registered on this MH (never registered, released, or the MH restarted). **Attacker-drivable** (checked before the signature). | **Sustained = a register-before-token ordering bug only once probing is excluded**: it must correlate with MC-side registration failures (`mc_register_meeting_total{status="error"}`) and with dials from real clients (MC issued a token before this handler acked `RegisterMeeting`); uncorrelated, it is probing — see `signature_invalid`. After an MH restart or a release it is expected and brief: clients hold tokens for a meeting this incarnation never held, and MC replaces the handler. An AC meeting token sent to MH also lands here (or on `signature_invalid`). |
| `signature_invalid` | The signature does not verify against the meeting's registered key. **Attacker-drivable** (a forged token naming a real meeting). | MC key / registration drift (the meeting re-registered with a different key — look for the `mh.session` WARN on a key change), a forged token, or an AC token. Steady low rate from unknown sources = probing: preserve logs, Security Team. |
| `wrong_type` | Signed with the meeting's key, `token_type` is not `media_dial`. | **Any increase is a defect** (token-type confusion at MC). Escalate to MC. |
| `aud_mismatch` | Fully valid token for ANOTHER handler (`aud` ≠ this MH's `handler_id`); closed `WRONG_HANDLER`. | Clients dialling with a stale handler set (address reused by a new incarnation, or a set not yet refreshed). Sustained = MC set pushes not reaching clients. |
| `malformed` | Oversized, non-`EdDSA` alg, unparseable, empty token or `sub`, or claim/key mismatch. | Client bug or probing; **unauthenticated peers can drive it at will**, so never page on it alone. |

**Pending-cap declines** are on `mh_media_session_starts_total{outcome="declined_pending_cap"}`, not here: a verified (meeting, participant) already had `MH_MAX_PENDING_CONNECTIONS_PER_PARTICIPANT` connections awaiting MC's bind answer. Expected near-zero; sustained = a client redial storm. MC is not asked.

**A participant who has left** and redials with a still-valid token passes this gate; MC answers `sender_id` 0 and MH closes `DECLINED` (`declined_no_sender_binding`, Scenario 15). That is expected.

**`WRONG_HANDLER` is sent only after FULL validation** (signature, claims, skew all passed; only `aud` names another handler). **Every other failure is `DECLINED`**, and the client reads `DECLINED` as "token dead", not "handler dead": it parks that handler, reports FAILED, and MC re-pushes its set with a fresh token (`mc_media_handler_set_pushes_total{reason="failed_report"}`).

**Resolution**: per the table — `expired` sustained → MC's refresh push (escalate `meeting-controller`) or NTP; `no_meeting_key` sustained and correlated → register-before-token ordering bug (escalate `meeting-controller`); `wrong_type` → escalate `meeting-controller` and `security`; `signature_invalid` / `malformed` from unknown sources → preserve logs, `security`. No MH-side lever: never raise `MH_MEDIA_DIAL_CLOCK_SKEW_SECONDS` to hide drift.

**Escalation**: `meeting-controller` (token minting, refresh, ordering), `security` (forged or confused tokens), `client` (dials with stale sets).

**Related**: [Scenario 2](#scenario-2-jwt-validation-failures) (service-token path), [Scenario 15](#scenario-15-media-sessions-declining--no-sender-binding), [Close codes](#close-codes-what-mh-told-the-client), [`mc-incident-response.md` Scenario 26](mc-incident-response.md#scenario-26-media-handler-migration-timed-out) (dials to a migration target).

---

### Scenario 25: Meetings Held Without Clients

**Alert**: dashboard first (MH Media - Meetings Without Clients). Warning `MHMeetingsHeldWithoutClients`:
`mh_media_meetings_without_clients` above zero on one MH (per instance) for longer than the
owner-silence timeout (`mh_media_meeting_owner_silence_timeout_seconds`, 900 s shipped) plus the
gauge's refresh tick and sampling (the current `for:` is in `mh-alerts.yaml` / `alerts.md`). The
gauge is a per-MH aggregate, so on a busy handler overlapping routine zero-client windows of
different meetings can hold it up: a steady churn of releases and teardowns means nothing is stuck.
**Severity**: Informational while under the owner-silence timeout; Warning past it
**Runbook Section**: `#scenario-25-meetings-held-without-clients`

**What this is.** MH holds registered meetings nobody is connected to. Brief non-zero values are
routine (a meeting before its first join, after its last leave). **Normally it is an MC that exited
without releasing its meetings** (crash, OOM or SIGKILL, or teardowns cut off at its shutdown
deadline; a graceful MC shutdown sends `EndMeeting` for every meeting and leaves MH nothing): MC ids are per process (`MC_ID` unset), so a restarted MC has a new id and answers its
predecessor's meetings' heartbeats with `FAILED_PRECONDITION` (owner mismatch) — or, when the MC pod
was replaced and its IP is gone, the heartbeats fail as transport errors. **MH takes no action on
either** (it never ends a meeting because MC is unreachable or disowns it) and keeps the meetings
until the **owner-silence release**: after `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` (default 900)
with no communication from the owning MC, MH ends the meeting, closing any sessions
`HANDLER_RELEASED`. **So the gauge self-clears within about 15 minutes of such an MC exit.** Those
meetings were already lost when their MC restarted (MC failure is out of scope); the release only
reclaims MH capacity. See [`mc-deployment.md` §MC rollout and MH meetings](mc-deployment.md#mc-rollout-and-mh-meetings-adr-0039-6).

**Diagnosis**

```promql
max by (instance) (mh_media_meetings_without_clients)
max(mh_media_meeting_owner_silence_timeout_seconds)
# Owner-mismatch answers, and heartbeat failures, from the MH side
sum by (outcome) (increase(mh_mc_heartbeat_verdicts_total{outcome="owner_mismatch"}[15m]))
sum by (outcome) (increase(mh_mc_heartbeats_total{outcome=~"error|timeout"}[15m]))
# `error` rises in both cases (FAILED_PRECONDITION is an `error` too). The discriminator is the
# verdict line above: owner_mismatch non-zero = in-place MC restart or IP reuse; flat = new MC pod IP.
# The releases that should follow ~15 min later
sum by (clients) (increase(mh_media_handler_releases_total{trigger="owner_silence"}[30m]))
```

- Did an MC crash, restart or roll in the last 15 minutes? (`kubectl get pods -n dark-tower -l app=mc-service`,
  pod ages and restart counts.) Yes → expected; wait for the release. (A graceful MC rollout leaves
  few or none: it sends `EndMeeting` first.)
- Logs: target `mh.grpc.heartbeat`, "MC heartbeats for this meeting failing; no action is taken …"
  (`kind=owner_mismatch` or a transport kind, `meeting_id`, `mc_id`); then, at the release, target
  `mh.session.owner_silence`, "Releasing a meeting whose owning MC has been silent …"
  (`meeting_id`, `mc_id`, `silence_seconds`).

**Resolution**
- **Act only if it persists past the owner-silence bound** (plus one 10 s actor tick): that means the
  owner-silence release is broken — **escalate to `media-handler`**.
- **Relief** while that is investigated: `./infra/kind/scripts/mh.sh up --owner <owner>` then
  `./infra/kind/scripts/mh.sh drain <k> --owner <owner>` for the affected MH. **A pod restart is a
  safe manual fence**: the drained MH takes its registrations with it. Live meetings on it are moved
  by their MC, but the **orphaned meetings that raised this scenario have no MC to move them**, so the
  drain runs to its deadline (~30 s) and sheds them: final `mh.drain` "Drain finished" line with
  `exit_reason=deadline` and `meetings_shed` ≈ the orphan count (plus the best-effort
  `mh_drain_meetings_shed_total`; no `mh_media_handler_releases_total` sample, and no MC
  `planned_transfer`/`timed_out`, because no MC is migrating them). **That is expected here, not a
  failed drain.**
- Persisting meetings whose heartbeats are **answered OWNED** are not owner-silence cases (their MC
  still owns them): that is an MC that never ended them — [`mc-incident-response.md` Scenario 20](mc-incident-response.md#scenario-20-meeting-teardown-failing--mh-budget-ratchet).

**Escalation**: `media-handler` (release not firing), `meeting-controller` (OWNED but empty meetings).

---

### Scenario 26: Owner-Silence Release With Clients Present

**Alert**: `MHOwnerSilenceReleaseWithClients` (warning;
`mh_media_handler_releases_total{trigger="owner_silence", clients="present"}` increase > 0).
**Severity**: Warning
**Runbook Section**: `#scenario-26-owner-silence-release-with-clients-present`

**What this is.** MH released a meeting on owner silence while **at least one client was still
connected**: people stayed attached for the whole silence window (default 15 min) to a meeting no MC
owned, and the release closed their sessions `HANDLER_RELEASED`. `clients="none"` after an MC crash
(or an MC rollout whose teardowns were cut off) is **routine** (Scenario 25), not this.

Possible causes: the owning MC restarted (its meetings were lost) but a client kept its media
transport to this MH open — no signalling to a live MC, yet the WebTransport session to MH survived;
or MH lost its path to a live MC for 15 minutes (that MC would itself have declared the handler dead
within N and moved the meeting off, so its clients should have left this MH long before).

**Diagnosis**

```promql
sum by (trigger, clients) (increase(mh_media_handler_releases_total{trigger="owner_silence"}[1h]))
sum by (outcome) (increase(mh_mc_heartbeats_total[1h]))
sum by (outcome) (increase(mh_mc_heartbeat_verdicts_total[1h]))
```

- The release log (target `mh.session.owner_silence`, `meeting_id`, `mc_id`, `silence_seconds`):
  is `mc_id` a live MC? (`mc_id` names the pod: `mc-<hostname>-<suffix>`.)
  - **Dead / restarted MC** → clients outlived their MC's session. Escalate to `client` (a client
    should drop media when its signalling session ends) and `meeting-controller`.
  - **Live MC** → the MH→MC path was broken for the whole window while clients stayed attached to
    MH: check MH's heartbeat errors for that `mc_id` and the NetworkPolicy MH→MC (TCP 50052); and
    check why that MC did not declare the handler dead ([`mc-incident-response.md` Scenario 24](mc-incident-response.md#scenario-24-media-handler-declared-dead-or-impaired)).
    Escalate to `meeting-controller` + `infrastructure`.

**Resolution**: none at runtime — the release has already happened and the affected clients
reconnect or rejoin. Fix the cause above.

**Escalation**: `meeting-controller`, `client`, `infrastructure` (path).

---

### Scenario 27: RegisterMeeting Refused or Meeting Not Released

**Alert**: No dedicated alert; reached from `MHMediaEgressEdgeHeadroomLow` (Scenario 18), MC's RegisterMeeting failures ([MC Scenario 12](mc-incident-response.md#scenario-12-registermeeting-coordination-failures)), or user reports.
**Severity**: warning

Moved from the retired [Scenario 13](#scenario-13-registermeeting-timeout--clients-kicked) (story 4 MH-1). The receipt-side view is `mh_grpc_requests_total{method="register_meeting"}` by `status` (MH Overview → "RegisterMeeting Receipts by Status"); since MH-1 a meeting MH does not hold surfaces client-side as DECLINED dials with `failure_reason="no_meeting_key"` ([Scenario 24](#scenario-24-media-dial-credential-rejections)).

> **Receipt is not application, and `accepted` is not success.** `RegisterMeeting` succeeding tells you the request arrived and parsed — nothing more. Whether forwarding policy actually took effect is a *separate* signal: `mh_media_policy_applies_total{outcome}` (dashboard panel **MH Overview → "Forwarding-Policy Applies by Outcome"**), and on the response it is `applied_generation`, never `accepted`. A meeting can register cleanly and still forward nothing — check that panel. On `apply_failed` the **prior** policy stays live, and the `reason` field on the `mh.session.policy` WARN log distinguishes the causes (a saturated or wedged config-apply mailbox vs the aggregate egress-edge bound); it is deliberately not a metric label.
>
> **"Some people can't hear anything in a meeting that's otherwise working, and it's the ones who joined most recently."** That report is this cause, and nothing else about it looks like a media-handler problem. MH reclaims a meeting's egress edges only when its MC calls `MediaHandlerService.EndMeeting` (story 2 R-20). As of story 2 task 12 MC calls it for every meeting it ends; permanently, for a meeting whose MC never *completes* it, a finished meeting's edges are never reclaimed, and headroom is freed only when a still-live meeting pushes a *smaller* policy. That unreclaimed floor rises with the rate such meetings end, and as it approaches the bound every registration that would **add** an edge starts failing: a new meeting, or an existing healthy meeting **admitting a new participant**. Everyone already talking keeps talking, because an equal-generation push short-circuits before the bound is tested and keeps reporting `applied` — so the panel shows `applied` and the refusal climbing together and the pod presents as healthy. `RegisterMeeting` still answers `accepted: true` and GC keeps placing meetings here, because this is a resource guard and is deliberately not advertised as capacity. New arrivals blame their own network and drop, so the affected population removes itself from the signal. **Discriminator for "leak, not load"**: `mh_media_meeting_teardowns_total{outcome="released"}` flat while meetings are known to be ending, and `mh_media_egress_edges` / `mh_media_registered_meetings` rising with pod uptime rather than with concurrent load.
>
> **Discriminator**: `reason=total_egress_edge_cap_exceeded` on the `mh.session.policy` WARN, with `installed_total_edges` and `installed_meeting_count` on the same line — a single fat policy and an accumulation of finished meetings look identical on the metric and are told apart only there. Onset is intermittent and worsens with uptime: it clears when some unrelated meeting happens to shrink and returns when it does not, which is what gets it misfiled as a transient.
>
> **Interim remedy: replace the pod.** `mh.sh up --owner <owner>`, then `mh.sh drain <its slot> --owner <owner>` (never a zero-grace force delete). The old MH drains on SIGTERM and MC moves each meeting make-before-break onto the new MH or a survivor (story 4 MC-F; a small blip per meeting; only what is not moved by the drain deadline is shed — [`mh-deployment.md` §MH drains on SIGTERM](mh-deployment.md#mh-drains-on-sigterm)), and the new MH starts with no unreclaimed edges. A **crash** (in-place container restart) is a NEW handler incarnation at the same address: MC declares the old one dead within N and replaces it ([`mc-incident-response.md` Scenario 24](mc-incident-response.md#scenario-24-media-handler-declared-dead-or-impaired)); pushes that reach the new incarnation for the old one's meetings are counted `mc_register_meeting_total{status="handler_replaced"}`. There is no periodic re-assert (ADR-0039 dropped it). **Raising `MH_MAX_TOTAL_EGRESS_EDGES` is not the remedy**: the ceiling is consumed by unreleased meetings at whatever rate they end, independent of concurrent load, so doubling it doubles time-to-onset and changes nothing else.
>
> **The durable fix has partly landed, and which part matters at 3am.** As of story 2 task 12, MC calls `EndMeeting` on the clean paths — a meeting that ends explicitly, and a meeting that empties — so the ratchet above no longer accumulates from ordinary meeting turnover. The residual is narrower and differently shaped: **a meeting whose MC never *completes* `EndMeeting`.** Scope and tracking: the `docs/TODO.md` entry "A meeting whose MC never sends `EndMeeting` is never reclaimed"; no absent-event alert detects it. A meeting that holds edges is caught by `MHMediaEgressEdgeHeadroomLow`'s LEAK fork ([Scenario 18](#scenario-18-media-egress-budget-exhaustion) Arm B), but a zero-edge registration has no alert (`docs/TODO.md` §Observability Debt, "No leading indicator for registered-meeting exhaustion"). So the checks below are manual. Settle **which cause** before restarting anything. `push_quiesce_bound_seconds` and `teardown_fence_hold_max_seconds` are structured fields on MC's `Configuration loaded successfully` startup line (`mc-deployment.md` §5) — read the live values off the pod rather than computing them.
>
> | You see | What happened | Read it as |
> |---|---|---|
> | `mc_media_end_meeting_total{outcome="unimplemented"}` non-zero | MH predates the RPC | Deploy or rollback order was reversed — MH rolls forward first, MC rolls back first. These meetings are never released by MC; MH reclaims them at its own restart |
> | `mc_media_end_meeting_total{outcome="rejected_ownership"}` non-zero | an MC that does not own the meeting tried to release it | MC defect, or a stale/misrouted MC. Recover with the re-register → `EndMeeting` → re-register order in the ownership-reject arm below |
> | `{outcome="superseded_by_successor"}` non-zero | a departing MC's best-effort shutdown teardown found the meeting already taken over | **Expected during a rolling MC deploy** — MH's ownership check doing its job, so do not open an incident. It is *consistent with* supersession rather than proof of it: an MC sending a wrong `mc_id` during shutdown lands on the same value and is indistinguishable at the recording site. **So this value rising outside a rollout window is worth a look** — confirm with mixed MC images per the `no_generation` check below |
> | `{outcome="unavailable_exhausted"}` non-zero | MC tried, MH was unreachable, retries ran out | These leak exactly like the crash path. Fix MH reachability; the meetings already lost are not recoverable by MC |
> | `{outcome="released"}` flat while meetings are known to be ending, **and** `mh_media_registered_meetings` rising with pod uptime rather than with load | nothing is calling `EndMeeting` at all | An MC that crashed, was killed mid-teardown, or was rolled back to a build without teardown. **Only this row is fixed by a pod restart** |
> | `mc_media_push_quiesce_total{outcome="timed_out"}` non-zero | MC released without confirming quiescence | The release went out with a push still in flight. A queued apply is refused MH-side; a registration that had **not yet landed** is only *ordered* by MC's wait, and that ordering has no in-tree control. **Confirm against `mh_media_registered_meetings` on that handler rather than assuming either outcome** |
>
> **MH's teardown counter is a UNION of MC's two rejection values; do not compare the like-named tokens.** MH cannot tell why an MC was tearing down, so `mh_media_meeting_teardowns_total{outcome="rejected_ownership"}` counts both populations — it corresponds to MC's `rejected_ownership` **plus** `superseded_by_successor`, not to the identically-named MC value alone. Compare against the sum. Comparing token to token shows a discrepancy that grows on every rolling deploy and looks exactly like lost messages between MC and MH.
>
> **The two races behind the last row, and what each can tell you.**
>
> **Race A — an apply MH had already queued.** It cannot resurrect a released meeting: MH refuses to install for a meeting it no longer holds, and that refusal is counted as `mh_media_released_meeting_apply_refusals_total`. **That counter is the evidence, and it is the only evidence.** `mh_media_policy_applies_total` is recorded only in MH's gRPC handler, which in this race has already timed out and returned, so `rejected_stale` does **not** move — do not look for it. `apply_failed` near a teardown is *consistent with* Race A but is not evidence of it: it also covers a full mailbox, an apply timeout, a dead actor and the aggregate egress-edge bound, and those have different remedies. **If `mh_media_released_meeting_apply_refusals_total` is flat, the `apply_failed` is one of those other causes — do not read it as a race.**
>
> **What a non-zero `mh_media_released_meeting_apply_refusals_total` means — and the thing it can never tell you.** It always means one thing: the proto's *accepted* deadline residual occurred and the guard caught it. The system working as designed. Informational, with deliberately no alert, because alerting would page on a design decision taken on purpose. That reading does **not** change with story 4 — the deadline race it counts survives unchanged, so the series is stable and needs no retirement. **What it cannot do is show a late registration resurrecting a released meeting**, and that limit is structural: a registration *re-registers* the meeting, so MH's guard passes it by construction and this counter stays FLAT. Story 4 brings that fence elsewhere: there is no periodic re-assert any more, and a meeting MC no longer holds gets a RELEASED verdict on its next heartbeat, which ends it on MH (`mh_mc_heartbeat_verdicts_total{outcome="released"}`, MC's `mc_media_handler_heartbeats_total{outcome="released_ended"}`). Do not read a flat value here as evidence either way.
>
> **Race B — a registration still in flight — has no signal at all, and it is the one that fails open.** MC waits `push_quiesce_bound_seconds` before releasing, which relies on a dropped client request cancelling MH's server-side handler: a foreign-library property. "The wait was performed" is not the same fact as "the late registration did not land", and there is no counter for the second. If that property regressed, nothing would turn red. **So do not conclude Race B is holding from any quiet signal, and in particular not from Race A's guard being active**: a non-zero `mh_media_released_meeting_apply_refusals_total` proves the guard caught a *queued* apply and says nothing about an in-flight *registration*, which re-registers the meeting and therefore passes that guard by construction — the same blindness described above. Since story 4 a resurrected registration is released by the next heartbeat's RELEASED verdict (MEETING_ENDED), so its lifetime is about one heartbeat interval. Silence here is what a working ordering and a regressed one look like alike. The only trace is `mh_media_registered_meetings` rising with pod uptime while teardowns stay flat — the row above. No alert covers it for a zero-edge registration (`docs/TODO.md` §Observability Debt, "No leading indicator for registered-meeting exhaustion"); an edge-holding one reaches `MHMediaEgressEdgeHeadroomLow`. That is why this row says confirm rather than conclude, and why the handler's registered-meetings gauge is the check that settles it.
>
> **Meeting-cap arm — new meetings refused, everything else looks fine.** MH refuses a NEW meeting id once it holds `MH_MAX_REGISTERED_MEETINGS` registrations (story 2 R-21): `RegisterMeeting` answers `RESOURCE_EXHAUSTED`, nothing is registered, and MC issues no dial tokens for an unregistered handler, so that meeting gets no media on this handler (any dial that does arrive is DECLINED at accept, `mh_jwt_validations_total{token_type="media_dial",failure_reason="no_meeting_key"}`). MC classifies no RegisterMeeting status: it shows only a generic push failure and bounded retries, so **MH's `mh_media_policy_applies_total{outcome="rejected_meeting_cap"}` is the discriminator**. Held meetings keep re-registering normally (the cap never refuses a meeting it already holds), which is why edge and stream signals stay healthy. Read `mh_media_registered_meetings` against `mh_media_registered_meetings_limit` (mh-media dashboard, "Registered Meetings vs Registration Cap"), then tell three causes apart: **leaked registrations** (meetings whose `EndMeeting` never arrived — occupancy rising with pod uptime, `released` teardowns flat), **an empty-meeting flood** (many registrations against flat `mh_media_egress_edges`), or **genuine load**. The first two are fixed by reclamation or a pod restart, not by raising the key: the value has a hard ceiling in code, and a larger cap only postpones the same exhaustion.
>
> **Ownership-reject arm — `EndMeeting` refused, the meeting is not released.** `mh_media_meeting_teardowns_total{outcome="rejected_ownership"}` counts an `EndMeeting` whose `mc_id` differs from the one that registered the meeting; MH answers `FAILED_PRECONDITION` and releases nothing, so that meeting keeps its registration, routes and edges. The `mh.session` WARN carries `registered_mc_id` and `caller_mc_id` (the gRPC status deliberately carries neither). Usual cause: a stale or misrouted MC. **This is also the order to recover a meeting another MC took over** — including the generation wedge in `docs/TODO.md` ("MH does not verify meeting ownership"), where a foreign registration carrying a high `policy_generation` leaves the owning MC's pushes stuck on `rejected_stale`. An `EndMeeting` from the owning MC is REFUSED in that state, which reads as "unrecoverable"; it is not. The order is:
> 1. **Re-register** the meeting from the owning MC. Its policy is still refused as stale, but any validation-passing registration takes over `mc_id`.
> 2. **`EndMeeting`** from that MC. This releases the meeting and FORGETS its applied generation.
> 3. **Re-register** again. The fresh policy now installs from generation 1.
>
> **Erroneous-release symptom.** If an `EndMeeting` releases a meeting that is still live — a misrouted call carrying the right `mc_id` — every participant of exactly that one meeting is dropped at once, handler-side, with no client-visible cause (MH closing a connection is not the client's meeting-ended signal). `mh_media_meeting_teardowns_total{outcome="released"}` rises by one at that moment, and the `mh.session` INFO "Meeting released by EndMeeting" names the meeting and `mc_id`. The meeting re-establishes when its MC registers it again.
>
> **`no_generation` is a ROLLOUT signal — expected during an MC rollout, actionable after one.** MC emits `policy_generation` >= 1 as of story task 13, so the healthy steady state is `applied` carrying the traffic and `outcome="no_generation"` at zero. **Do not open an incident while an MC rollout is in progress**: a not-yet-upgraded MC pod legitimately still sends 0, so the two series coexist and the mix shifts as pods cycle. **Sustained `no_generation` after the rollout completes is actionable and the owner is MC** — it means MC failed to compute a forwarding assignment. Check `mc_media_policy_pushes_total` and MC's `mc.register_meeting.trigger` logs, not this pod. Confirm which of the two you are looking at by asking whether **mixed MC versions are live right now** — that, not a rollout object's progress, is what the rule turns on:
> ```bash
> kubectl get pods -n dark-tower -l app=mc-service \
>   -o jsonpath='{.items[*].spec.containers[*].image}' | tr ' ' '\n' | sort -u
> ```
> More than one image means a rollout is in progress; one means it has completed. **Do not reach for `kubectl rollout status deployment/mc-service`** — there is no Deployment by that name (MC ships as `mc-0` and `mc-1`; `mc-service` is a Service, a PDB and a container name), and the `NotFound` it returns reads as "no rollout in progress", which would flip you to actionable during exactly the legitimate mixed-version window this note exists to protect. See `docs/observability/metrics/mh-service.md` §`mh_media_policy_applies_total`.

**Related**: [Scenario 18](#scenario-18-media-egress-budget-exhaustion) (edge headroom), [Scenario 24](#scenario-24-media-dial-credential-rejections) (DECLINED dials for an unheld meeting), [MC Scenario 12](mc-incident-response.md#scenario-12-registermeeting-coordination-failures).

---

### Close codes: what MH told the client

Every session MH ends **on purpose** closes with exactly one numeric `MhCloseCode`
(ADR-0039 §7), sent as a WebTransport session-close capsule with an EMPTY message
(`crates/mh-service/src/webtransport/close.rs`). A user report or a client metric that
names a code maps to MH's cause like this:

| Code (`code` label) | MH cause | Read on MH |
|---|---|---|
| `declined` | A `media_dial` credential rejection at admission (any reason except `aud_mismatch` — the client always sees the one generic DECLINED; includes a meeting MH does not hold), a pre-validation exit (unreadable or non-connect first message), a verified dial whose meeting was released before admission, the per-participant pending cap, a participant who has left (MC answers `sender_id` 0), or another decline with no code of its own. Means "token dead", not "handler dead": the client parks the handler and reports FAILED so MC re-pushes a token | `mh_jwt_validations_total{token_type="media_dial",result="failure"}` by `failure_reason` ([Scenario 24](#scenario-24-media-dial-credential-rejections)), `mh_media_session_starts_total{outcome=~"declined_pending_cap\|declined_no_sender_binding\|declined_sender_binding_out_of_range\|declined_mc_auth_rejected\|declined_mc_endpoint_unknown"}` |
| `binding_conflict` | MH refused the ordinal MC returned (held by another participant) | `mh_media_session_starts_total{outcome="declined_sender_binding_conflict"}` — [Scenario 15](#scenario-15-media-sessions-declining--no-sender-binding) |
| `mc_unavailable` | MH could not reach MC for a binding (after the ~48s retry budget and the close jitter) | `mh_media_session_starts_total{outcome="declined_mc_unavailable"}`, `mh_mc_notifications_total{status="error"}` |
| `handler_released` | MH released the meeting: `EndMeeting`, a RELEASED verdict (MH-2), the owner-silence release or the drain-deadline shed (MH-3) | Meetings: `mh_media_meeting_teardowns_total{outcome="released"}` (meeting ended), `mh_media_handler_releases_total{trigger}` (removals, and `trigger="owner_silence"`), `mh_drain_meetings_shed_total` (drain shed, best-effort — read MH's final `mh.drain` log line and MC's `planned_transfer`/`timed_out`). Sessions: `mh_webtransport_session_closes_total{code="handler_released"}` |
| `wrong_handler` | A fully valid `media_dial` token (signature, type, skew all passed) whose `aud` is another handler's `handler_id` — admission step (f) only. The client retries with backoff | `mh_jwt_validations_total{token_type="media_dial",failure_reason="aud_mismatch"}` — [Scenario 24](#scenario-24-media-dial-credential-rejections) |
| **no code** | Transport loss: MH crashed, the QUIC idle timeout fired, or the client/network ended it. MH also ends client closes, stream failures and shutdown (only sessions no held meeting owns: the drain sheds every held meeting with `handler_released` first) WITHOUT a code — with a QUIC close, never a capsule. The QUIC idle timeout is `MH_MAX_IDLE_TIMEOUT_MS` (≈ MC's N) | pod restarts, `mh_active_connections`, the client's loss reports |

- `mh_webtransport_session_closes_total{code}` counts closes MH **sent** (capsule written), one per
  close, recorded before MH waits for the peer. It is NOT added to `mh_media_session_starts_total` —
  the declines are the same events on both.
- **Reconcile only the 1:1 pairs**: `code="mc_unavailable"` vs `starts{outcome="declined_mc_unavailable"}`
  and `code="binding_conflict"` vs `starts{outcome="declined_sender_binding_conflict"}`. A shortfall
  is closes MH did not send (the connection was already gone — a client race — or the capsule write
  did not complete within the 1 s bound). Small amounts are expected; a sustained gap is worth a look.
  `code="declined"` has NO single comparator: it sums credential rejections, pre-validation exits,
  released-before-admission dials and the uncoded `declined_*` outcomes, so it normally sits ABOVE them.
- **`code="declined"` is driveable by unauthenticated peers** (anyone can send a malformed
  first message). Never page on it alone.
- **A coded close can take up to 1 s (`CLOSE_SESSION_LINGER`) before the QUIC connection
  ends** — the capsule write AND the wait for the peer share that one bound, so a peer withholding
  flow-control credit cannot stretch it — holding its `MH_MAX_CONNECTIONS` slot meanwhile. A capsule still unacknowledged at
  that bound (heavy loss) is lost, and the client then sees transport loss — safe.
- The client acts on these codes from story 4 task C1. Before C1, every code reads as an
  ordinary close to it.

---

## Diagnostic Commands

### Quick Health Check

```bash
kubectl port-forward -n dark-tower pod/<mh-pod> 8083:8083 &   # <mh-pod>: from ./infra/kind/scripts/mh.sh list
curl http://localhost:8083/health      # Liveness
curl http://localhost:8083/ready       # Readiness
kill %1

kubectl get pods -n dark-tower -l app=mh-service
kubectl logs -n dark-tower -l app=mh-service --tail=100 | grep -i error
```

### Metrics Analysis

```bash
kubectl port-forward -n dark-tower pod/<mh-pod> 8083:8083 &   # <mh-pod>: from ./infra/kind/scripts/mh.sh list

# All metrics
curl http://localhost:8083/metrics

# By subsystem
curl http://localhost:8083/metrics | grep mh_gc_registration
curl http://localhost:8083/metrics | grep mh_gc_heartbeat
curl http://localhost:8083/metrics | grep mh_webtransport
curl http://localhost:8083/metrics | grep mh_jwt_validations
curl http://localhost:8083/metrics | grep mh_mc_notifications
curl http://localhost:8083/metrics | grep mh_token_refresh
curl http://localhost:8083/metrics | grep mh_caller_type_rejected
curl http://localhost:8083/metrics | grep mh_active_connections

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
kubectl describe pod/<mh-pod> -n dark-tower | grep -A 5 "Limits:"   # <mh-pod>: from ./infra/kind/scripts/mh.sh list
kubectl get events -n dark-tower --sort-by=.lastTimestamp | grep -E 'mh-s[0-9]+'   # every MH Deployment/ReplicaSet/pod
```

### Network Debugging

```bash
# MH -> GC
kubectl exec -it pod/<mh-pod> -n dark-tower -- \
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
kubectl exec -it pod/<mh-pod> -n dark-tower -- \
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
./infra/kind/scripts/mh.sh list
./infra/kind/scripts/mh.sh up --owner <owner> && ./infra/kind/scripts/mh.sh drain <k> --owner <owner>   # per MH, one at a time; never rollout restart an mh-s<k>
kubectl logs -n dark-tower -l app=mh-service --tail=50 --prefix
```

**Rollback on failure**:
```bash
git checkout <previous-good-commit> && ./infra/kind/scripts/deploy.sh   # MH has no rollout history; mh.sh upgrade
```

**Impact**: the drained MH drains on SIGTERM and MC moves each of its meetings make-before-break onto the new MH or a survivor: a small blip per meeting, no rejoin. Only meetings MC cannot move by the drain deadline (`terminationGracePeriodSeconds − 5 s`) are shed (`mc_media_handler_migrations_total{trigger="planned_transfer", outcome="timed_out"}`).

---

### JWKS Cache Flush Procedure

**When to use**: After AC key rotation, when MH is validating MC's **service tokens** (MC->MH gRPC) against stale keys. Client `media_dial` admission never uses JWKS.

MH caches JWKS with a 5-minute TTL. There is no hot-flush endpoint — a new MH process forces a cold fetch:

```bash
./infra/kind/scripts/mh.sh up --owner <owner> && ./infra/kind/scripts/mh.sh drain <k> --owner <owner>   # per MH, one at a time
```

Alternatively, wait up to 5 minutes for the TTL to expire — service-token validation failures resolve automatically once the cache refreshes.

---

### Graceful Drain Procedure

**When to use**: planned maintenance, retiring a suspect MH, relieving an MH (Scenario 25), a likely
gray failure (`gc-incident-response.md` Scenario 15).

**Planned drain of one MH = `mh.sh drain <slot> --owner <owner>`** (ADR-0039 §7). On SIGTERM the MH reports DRAINING
to GC at once (GC stops selecting it), flags DRAINING to every MC, keeps forwarding while MC moves
its meetings, and exits at zero meetings or at the drain deadline
([`mh-deployment.md` §MH drains on SIGTERM](mh-deployment.md#mh-drains-on-sigterm)).

```bash
./infra/kind/scripts/mh.sh list --json | jq -c '.slots[] | {slot, state, owner, address}'
./infra/kind/scripts/mh.sh up --owner <owner>              # capacity first if it is the last serving MH
./infra/kind/scripts/mh.sh drain <k> --owner <owner>       # returns when the pod is gone
```

Expected signals: `mc_media_handler_declarations_total{trigger="planned_transfer"}` and
`mc_media_handler_migrations_total{trigger="planned_transfer", outcome="completed"}` rise by about the
meetings on that MH; `mh_media_handler_releases_total{trigger="planned_transfer"}` on the draining
pod; its final `mh.drain` "Drain finished" log line with `exit_reason=empty` (`meetings_shed=0`,
`shed_completed=true`). `exit_reason=deadline` with `shed_completed=false` means sessions ended
code-less: an MH defect, escalate to `media-handler`. (An MH holding orphaned meetings, Scenario 25,
legitimately runs to the deadline and sheds them.) Refusals
(`last-serving-mh`, `stuck-terminating`, `foreign-owner`, `mh-pool-busy`, `drain-timeout`):
`mh-deployment.md` §`mh.sh` refusals.

**Crash drill = a process kill**, not a delete: the container restarts in place (same slot, same pod
IP, new `handler_id`; GC supersedes the dead row, `gc_mh_registrations_total{outcome="superseded",match="both"}`)
and MC declares the old incarnation dead within N (`trigger="heartbeat_timeout"`). Run it on an MH
provisioned for the drill (`mh.sh up --owner drill-<you>`, then `mh.sh signal <k> KILL --owner drill-<you>`,
then `mh.sh drain <k> --owner drill-<you>`); `mh.sh signal` is owner-gated with no `--force`, so it
never touches a `baseline` MH. Steps and expected readings: `mh-deployment.md` §Never force-delete an MH pod.

**Never `kubectl delete pod --force --grace-period=0` an MH pod, and never edit GC's
`media_handlers` rows by hand** (GC marks a draining MH from its DRAINING load report; a forced
delete defeats the hostPort slot lock — `mh-deployment.md` §Never force-delete an MH pod).

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
