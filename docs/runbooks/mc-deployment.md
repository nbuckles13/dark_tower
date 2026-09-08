# MC Service Deployment Runbook

**Service**: Meeting Controller (mc-service)
**Version**: Phase 4a (ADR-0010 Implementation)
**Last Updated**: 2026-03-27
**Owner**: Operations Team

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

- [ ] **MC and the browser SDK ship together** (wire-lockstep)
  - The ADR-0036 signalling reshape is a breaking wire change: `ClientMessage`
    tag 8 changed message type, and stream assignments moved from tag 5 to 12.
    A split deploy fails in the decodes-successfully-wrong-semantics direction,
    not with a loud error — so MC and the SDK build (`@darktower/sdk-core`
    consumers) must be deployed as one coordinated release, not independently.
  - **MH is NOT in the coupled set for the `signaling.proto` reshape**:
    `MhClientMessage` is unchanged and no `mh-service` source is in it, so MH
    deploys independently. **This changes when the `internal.proto` reshape
    (story task 4) lands** — that is the MC↔MH contract, and MH joins the
    coupled set then; revisit this line at that point.
  - Note: `## Rollback Procedure` below covers rolling back the *repo*, not a
    *deployment* — a deployment rollback must revert MC and the SDK together.
  - **Identity-key handling adds NO deploy-ordering constraint** beyond the
    tag-reshape lockstep above. MC admits a joiner that sends no
    `identity_public_key` (length 0 is the contract's NO KEY PUBLISHED state), so
    an un-updated SDK still joins; it is counted on
    `mc_join_identity_key_presence_total{presence="absent"}`, not as a failure.
    What the admission work adds is a new *observable*:
    `error_type=identity_key_invalid` on `mc_session_join_failures_total` means a
    client sent a **wrongly-encoded** key (PEM/JWK/base64 rather than the raw 32
    bytes), never that a client has not implemented the field.


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
kubectl get deployment mc-0 mc-1 -n dark-tower

# Check current pod status
kubectl get pods -n dark-tower -l app=mc-service

# Check current resource utilization
kubectl top pods -n dark-tower -l app=mc-service

# Check active meetings (critical - do not deploy if high)
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
watch -n 10 'kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 2>/dev/null & sleep 1; curl -s http://localhost:8080/metrics | grep mc_meetings_active; kill %1 2>/dev/null'

# Proceed when active meetings reach acceptable level (e.g., <10)
```

### 3. Update Container Image

> **Apply the manifests FIRST if the release changes `infra/services/mc-service/**`.**
> MC reads fifteen required env vars via `ConfigError::MissingEnvVar` with no Rust
> default, and each is injected by a per-key `configMapKeyRef` that lives in the
> **Deployment**, not the ConfigMap. A release that adds a required key and is
> shipped with `kubectl set image` alone puts **both MC pods into
> `CrashLoopBackOff`** — new image, old pod template, no reference to the new key.
> This is not hypothetical: it is what happened on 2026-09-03 when the five
> ADR-0036 `MC_AUDIO_*` / `MC_MAX_RECEIVE_*` keys were introduced. Both pods fail
> identically, so the PDB offers no protection and it is a full signalling-plane
> outage. See §Config-failure triage for the two signals and their opposite first
> steps.
>
> ```bash
> # Does this release touch the manifests? If yes, Option B is the ONLY safe route.
> git diff --name-only <PREVIOUS_TAG>..<NEW_TAG> -- infra/services/mc-service/
> ```

**Option A: Using kubectl (direct deployment) — image-only releases**

Safe **only** when the diff above is empty. `set image` changes the image and
nothing else; it cannot add a `configMapKeyRef`.

```bash
# Set new image version
export NEW_VERSION="v1.2.3"  # Replace with actual version tag

# Update BOTH instances -- mc-0 alone is half of MC
kubectl set image deployment/mc-0 \
  mc-service=mc-service:${NEW_VERSION} \
  -n dark-tower
kubectl set image deployment/mc-1 \
  mc-service=mc-service:${NEW_VERSION} \
  -n dark-tower

# Verify image updated on both
kubectl describe deployment mc-0 -n dark-tower | grep Image:
kubectl describe deployment mc-1 -n dark-tower | grep Image:
```

**Option B: Using kubectl apply -k (declarative, Kustomize) — always safe**

Applies the ConfigMaps, both Deployments and the image together, which is the
coupled set §Config-failure triage requires. Use this whenever the release
touches `infra/services/mc-service/**`, and prefer it otherwise.

```bash
# Update the image tag in BOTH per-instance Deployments:
#   infra/services/mc-service/mc-0-deployment.yaml
#   infra/services/mc-service/mc-1-deployment.yaml
# Change image tag: mc-service:latest → mc-service:v1.2.3
# (There is no `deployment.yaml` -- MC's workloads are per-instance files.)

# Apply the overlay, NOT the base. The Kind overlay strategic-merge-patches
# OTEL_ENABLED="true" into mc-service-config; applying the base directly would
# silently revert it (see §Config-failure triage, Recovery).
kubectl apply -k infra/kubernetes/overlays/kind/services/mc-service/

# Verify change on both instances
kubectl describe deployment mc-0 -n dark-tower | grep Image:
kubectl describe deployment mc-1 -n dark-tower | grep Image:
```

### 4. Rolling Update Monitoring

Deployments update pods via rolling strategy (maxSurge=1, maxUnavailable=0 for zero-downtime).

**Monitor rollout:**

```bash
# Watch pod status (Ctrl+C to exit)
kubectl get pods -n dark-tower -l app=mc-service -w

# Check rollout status
kubectl rollout status deployment/mc-0 -n dark-tower

# Monitor logs from new pod
kubectl logs -f deployment/mc-0 -n dark-tower
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
kubectl logs deployment/mc-0 -n dark-tower --tail=50

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

**Verify the EFFECTIVE config, not the ConfigMap.** A *missing* required key is
loud (CrashLoop, `logs --previous` names it — §Config-failure triage). A
**wrong-but-valid** one is silent: nothing fails, no counter moves, and the two
MC pods can quietly disagree. The `Configuration loaded successfully` line is
the only record of what the process actually received, and the ConfigMap cannot
be its own evidence — the ConfigMap being out of step with what the Deployment
injects *is* the failure mode these keys create.

```bash
# What each pod actually loaded (structured fields on the startup line):
kubectl logs deployment/mc-0 -n dark-tower | grep "Configuration loaded successfully"
kubectl logs deployment/mc-1 -n dark-tower | grep "Configuration loaded successfully"
```

- [ ] `max_receive_slots`, `max_receive_capability_declarations`, `audio_codec`,
      `audio_max_bitrate_bps`, `audio_frame_rate_hz` are present on the line and
      match `infra/services/mc-service/configmap.yaml`
- [ ] **mc-0 and mc-1 report identical values.** They read one shared ConfigMap,
      so a difference means one Deployment's `configMapKeyRef` block is stale —
      the guard-green split-brain the ConfigMap banner warns about.
- [ ] No `audio frame rate other than 20 ms frames` WARN, unless you set one
      deliberately

> **That WARN will not appear in the error grep above.** §Logs review greps
> `error|panic|fatal`; this one is a `WARN`. `mc-service` emits it whenever
> `MC_AUDIO_FRAME_RATE_HZ` is not 50 Hz, because `mh-service` sizes its datagram
> send buffer in bytes against 20 ms frames and will then **understate** its
> reported held latency. A non-50 value is legal (ADR-0036 §3's 40 ms
> signature-overhead mitigation) but is a coordinated change with media-handler,
> not a unilateral one. Grep for it explicitly:
>
> ```bash
> kubectl logs deployment/mc-0 -n dark-tower | grep -i "frame rate"
> kubectl logs deployment/mc-1 -n dark-tower | grep -i "frame rate"
> ```

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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &

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

> **What is in the coupled set, and what may NOT be reverted alone.**
> Since the ADR-0036 keys landed, MC's config is a hard startup dependency:
> **the ConfigMap, BOTH Deployments and the image roll together, and none of them
> may be reverted independently.** Concretely, for a rollback:
>
> - **`kubectl rollout undo` is safe** and is the recommended route. It reverts
>   the *whole pod template*, not just the image, so the old image lands with the
>   old `env` block — self-consistent by construction. Reverting only the image
>   (`kubectl set image` to an older tag) is also safe: surplus env is ignored.
> - **Do NOT revert the ConfigMap on its own.** A `git revert` of
>   `infra/services/mc-service/configmap.yaml` followed by an apply leaves both
>   Deployments holding a `configMapKeyRef` for a key that no longer exists. This
>   fails **latently**: `kustomization.yaml` uses `resources:`, not
>   `configMapGenerator:`, so there is no content hash and the apply does not roll
>   the pods. The running pods keep working, and the next restart — a node drain,
>   an eviction, an unrelated deploy, hours or days later — comes up
>   `CreateContainerConfigError` with **empty logs**, disconnected in time from the
>   change that caused it.
> - **Do not revert the manifests without the image, or apply the image against
>   stale manifests.** Both MC pods fail identically at startup, so the PDB offers
>   no protection: this is a full signalling-plane outage, not a partial one.
>
> Full triage for both failure signals — which have **opposite first steps** —
> and the verified recovery sequence are in §Config-failure triage below.

**Step 1: Identify previous version**

```bash
# Find previous image version -- read BOTH histories. Revision numbers are
# per-Deployment; mc-0 revision 7 and mc-1 revision 7 are not the same release.
kubectl rollout history deployment/mc-0 -n dark-tower
kubectl rollout history deployment/mc-1 -n dark-tower

# Get image from previous revision
kubectl rollout history deployment/mc-0 -n dark-tower --revision=<MC0_REVISION>
kubectl rollout history deployment/mc-1 -n dark-tower --revision=<MC1_REVISION>
```

**Step 2: Rollback Deployment**

> **Roll back BOTH instances, and treat that as one step.** §Update Container
> Image says the same thing for the forward direction ("mc-0 alone is half of
> MC"); it matters more here, because the state you land in when you stop after
> `mc-0` is a **mixed-version signalling plane** — mc-0 on the old image, mc-1 on
> the new — and it is not self-announcing. Each instance holds its own sessions
> and advertises its own client-facing URL, so GC keeps assigning meetings to
> both: roughly half of new meetings get the behaviour you just rolled back
> **away from**, and the symptom is intermittent and per-meeting rather than a
> clean regression. Do not stop between the two commands to "see if that fixed
> it" — a half-rolled pair cannot answer that question.

```bash
# Rollback to previous revision -- BOTH instances.
kubectl rollout undo deployment/mc-0 -n dark-tower
kubectl rollout undo deployment/mc-1 -n dark-tower

# Or rollback to specific revision. Revision numbers are PER-DEPLOYMENT and are
# not guaranteed to line up between mc-0 and mc-1 -- read each one's own history
# (Step 1, repeated for mc-1) rather than reusing a number across both.
kubectl rollout undo deployment/mc-0 -n dark-tower --to-revision=<MC0_REVISION>
kubectl rollout undo deployment/mc-1 -n dark-tower --to-revision=<MC1_REVISION>

# Monitor rollback -- both, and do not declare success on one.
kubectl rollout status deployment/mc-0 -n dark-tower
kubectl rollout status deployment/mc-1 -n dark-tower

# Confirm both landed on the SAME image before calling it done.
kubectl get pods -n dark-tower -l app=mc-service \
  -o custom-columns=POD:.metadata.name,INSTANCE:.metadata.labels.instance,IMAGE:.spec.containers[0].image
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
| `REDIS_URL` | **Yes** | Redis connection URL for session state. Supplied from the `mc-service-secrets` Secret. | None | `redis://:pw@redis.dark-tower:6379` |
| `MC_BINDING_TOKEN_SECRET` | **Yes** | Base64 master secret for binding-token HMAC (ADR-0023). From `mc-service-secrets`. | None | `openssl rand -base64 32` |
| `AC_ENDPOINT` | **Yes** | Auth Controller endpoint for OAuth token acquisition (ADR-0010). | None | `http://ac-service.dark-tower:8082` |
| `MC_CLIENT_ID` | **Yes** | OAuth client ID for MC's client-credentials flow to AC. | None | `meeting-controller` |
| `MC_CLIENT_SECRET` | **Yes** | OAuth client secret. From `mc-service-secrets`. | None | `<secret>` |
| `AC_JWKS_URL` | **Yes** | AC JWKS endpoint for meeting-token validation. Validated at load: must start `http://` or `https://`. | None | `http://ac-service.dark-tower:8082/.well-known/jwks.json` |
| `MC_TLS_CERT_PATH` | **Yes** | PEM cert for the WebTransport server. File existence is checked at startup, so a *malformed or empty* Secret is a load failure rather than a runtime error. A **missing** `mc-service-tls` never reaches this check — the mount blocks first (`FailedMount`, no logs). | None | `/etc/mc-tls/tls.crt` |
| `MC_TLS_KEY_PATH` | **Yes** | PEM private key for the WebTransport server. Existence checked at startup, with the same caveat as `MC_TLS_CERT_PATH` above. | None | `/etc/mc-tls/tls.key` |
| `MC_GRPC_ADVERTISE_ADDRESS` | **Yes** | Address GC uses to reach this pod's gRPC. Built from the downward-API `POD_IP`. | None | `http://$(POD_IP):50052` |
| `MC_WEBTRANSPORT_ADVERTISE_ADDRESS` | **Yes** | Address GC hands clients for this pod. **Per-instance** — from `mc-0-config` / `mc-1-config`, not the shared ConfigMap. | None | `https://127.0.0.1:4433` |
| `MC_MAX_RECEIVE_SLOTS` | **Yes** | Max receive slots one client may declare in a `ReceiveCapability` (ADR-0036 §6). Over-cap rejects the **whole declaration**. The value itself is validated `1..=64` at load — a cap whose own value is unbounded is not a cap. | `8` (in ConfigMap) | `8` |
| `MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS` | **Yes** | Per-connection budget on *accepted* capability declarations. Bounds client-driven O(N) meeting-actor work; over-budget declarations are rejected and counted. | `64` (in ConfigMap) | `64` |
| `MC_AUDIO_CODEC` | **Yes** | Codec MC directs clients to produce for main audio. Parsed to the `Codec` proto enum; unrecognised values, `unspecified`, and video codecs all fail at startup. | `opus` (in ConfigMap) | `opus` |
| `MC_AUDIO_MAX_BITRATE_BPS` | **Yes** | Max audio bitrate MC directs. Top of the 32–48 kbps band `mh-service` sizes its datagram buffer against; validated against that band at load. | `48000` (in ConfigMap) | `48000` |
| `MC_AUDIO_FRAME_RATE_HZ` | **Yes** | Audio frames per second MC directs (50 Hz = 20 ms frames). Same physical quantity as `mh-service`'s `AUDIO_FRAME_DURATION_MS = 20` in the reciprocal unit; changing it moves MH's "32 frames = 640 ms" latency budget. | `50` (in ConfigMap) | `50` |
| `MC_REGION` | No | Geographic region for this MC. | `us-east-1` | `local` |
| `GC_GRPC_URL` | No | Global Controller gRPC endpoint. | `http://localhost:50051` | `http://gc-service.dark-tower:50051` |
| `MC_WEBTRANSPORT_BIND_ADDRESS` | No | WebTransport (QUIC/UDP) bind address. | `0.0.0.0:4433` | `0.0.0.0:4433` |
| `MC_GRPC_BIND_ADDRESS` | No | gRPC bind address for GC communication. | `0.0.0.0:50052` | `0.0.0.0:50052` |
| `MC_HEALTH_BIND_ADDRESS` | No | Health/metrics bind address. | `0.0.0.0:8081` | `0.0.0.0:8081` |
| `MC_ID` | No | Instance identifier. Auto-generated when unset. | `mc-$HOSTNAME-<uuid8>` | `mc-0` |
| `MC_MAX_MEETINGS` | No | Maximum concurrent meetings. | `1000` | `1000` |
| `MC_MAX_PARTICIPANTS` | No | Maximum participants across all meetings. | `10000` | `10000` |
| `MC_BINDING_TOKEN_TTL_SECONDS` | No | Binding token TTL (ADR-0023). | `30` | `30` |
| `MC_CLOCK_SKEW_SECONDS` | No | Clock skew allowance (ADR-0023). | `5` | `5` |
| `MC_NONCE_GRACE_WINDOW_SECONDS` | No | Nonce grace window (ADR-0023). | `5` | `5` |
| `MC_DISCONNECT_GRACE_PERIOD_SECONDS` | No | Participant disconnect grace period (ADR-0023). | `30` | `30` |
| `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS` | No | QUIC max-idle-timeout. **Fail-loud parse**: non-numeric or `0` is rejected at load rather than falling back to the library default (`0` = infinite idle timeout, which leaves crash/network-loss departures undetected). | `10` | `10` |
| `OTEL_ENABLED` | No | Enable MC's OTel SDK span export (R-55). Gated by this explicit flag, NOT by presence of `OTLP_ENDPOINT`. Unrecognised values fail at load. | `false` | `true` |
| `OTLP_ENDPOINT` | No | OTLP-gRPC collector endpoint for MC's own span export. `http://` scheme required. Only consumed when `OTEL_ENABLED=true`. | None | `http://otel-collector.dark-tower:4317` |
| `OTEL_SAMPLE_RATE` | No | Head-sampling ratio in `[0.0, 1.0]`; `init_otel` validates. | `1.0` | `1.0` |
| `DEPLOYMENT_ENVIRONMENT` | No | `deployment.environment` resource attribute (ADR-0011). | `development` | `production` |
| `RUST_LOG` | No | Logging level. Set as a literal in the Deployment, not via ConfigMap. | `info` | `info,mc_service=debug` |

**The fifteen Required rows are the CrashLoop list.** Each is loaded via
`ConfigError::MissingEnvVar` in `crates/mc-service/src/config.rs`, so absence
fails config load before the server binds: the pod enters `CrashLoopBackOff`
and the reason is the first line of the container log. `MC_TLS_CERT_PATH` and
`MC_TLS_KEY_PATH` additionally fail when the *file* is absent — but **that check
is only reachable once the container starts**, so it is the signal for a Secret
that exists and is malformed or empty, **not** for a Secret that is missing. A
missing `mc-service-tls` never gets that far: the volume is not `optional:`, so
kubelet cannot complete the mount and the pod stays Pending in
`ContainerCreating`. See §Config-failure triage, which lists all three signals.

> **Maintenance.** This table is verified against `crates/mc-service/src/config.rs`.
> It previously documented six variables that did not exist in any MC build
> (`GC_REGISTRATION_URL`, `MC_CAPACITY`, `WEBTRANSPORT_BIND_ADDRESS`,
> `HTTP_BIND_ADDRESS`, `ACTOR_MAILBOX_SIZE`, `GC_HEARTBEAT_INTERVAL_SECS`)
> while omitting every genuinely required one — an inverted `Required` column
> on the artifact an operator reads at 2am. `dt-guard env-config` checks
> manifests against `config.rs` but does **not** read this runbook, so nothing
> catches drift here automatically. Re-verify against `config.rs` whenever a
> key is added or removed.

> **`MC_AUDIO_*` has no observable effect until the client honours the send
> directive** (story task 19). MC emits the directive today and the SDK still
> uses its own encoder settings, so tuning these three changes the wire message
> and nothing an operator can measure. Stated here because a live-looking knob
> that does nothing generates support questions for a whole story.

### Config-failure triage: three signals, and `kubectl logs` is wrong for all three

Five required keys mean two distinct startup failures with **opposite first
steps**; a third, the TLS Secret, produces neither and is listed with them
because it presents at the same moment and reaches the same operator. What they
have in common is the trap: **the standard `kubectl logs` reflex is wrong for
every one of them.**

| Cause | Signal | Where the answer is |
|---|---|---|
| Deployment has a `configMapKeyRef` the ConfigMap lacks (new Deployment applied against an old ConfigMap) | `CreateContainerConfigError` — **no container ever ran** | `kubectl describe pod`. `logs` is **empty** and `exec` is impossible — an operator reading only those concludes "the pod is wedged" when `describe` names the missing key |
| New image with an old pod template lacking the new `env` entries (`kubectl set image`, a partial apply) | `CrashLoopBackOff` | `kubectl logs --previous` — `ConfigError::MissingEnvVar` names the variable |
| `mc-service-tls` Secret absent (fresh namespace, `apply -k` only — it is created imperatively and is outside the coupled set) | `FailedMount`; pod Pending in `ContainerCreating` | `kubectl describe pod`. The `mc-tls` volume is not `optional:`, so kubelet blocks the mount and the container **never starts** — there is no config load, no `MC_TLS_CERT_PATH` error and no log at all. See §Kubernetes Secrets |

**Three signals, and `kubectl logs` is empty or misleading for all three.** That
is the generalisation worth carrying rather than the rows: the standard first
reflex is wrong every time. `describe pod` is what names the artifact for the
first and third; `logs --previous` is what names it for the second, and plain
`logs` is empty for both of those. The three are also distinguishable *before*
you read anything, by pod phase alone — Pending/`ContainerCreating` is the
Secret, `CreateContainerConfigError` is the ConfigMap key, and a pod that runs
and exits is the stale pod template.


**Rollout ordering.** The manifest change and the code change roll **together**.
The ConfigMap must never be reverted independently of the image, and the image
must never be applied against a stale ConfigMap. Note that
`infra/services/mc-service/kustomization.yaml` uses `resources:`, not
`configMapGenerator:` — so there is **no content hash on the ConfigMap and
editing it does not roll the pods**. Env is read once at process start, so a
ConfigMap edit takes effect only on the next restart, and a wrong one is latent
until then. Both MC pods fail identically at startup in either case, so the PDB
offers no protection: this is a full signalling-plane outage, not a partial one.

**Recovery — applying the ConfigMap alone is NOT enough.** This was observed on
2026-09-03 during the validation of the change that introduced these five keys,
so the sequence below is what actually worked rather than what ought to:

```bash
# 1. Confirm the cause before changing anything.
kubectl get pods -n dark-tower -l app=mc-service          # CrashLoopBackOff?
kubectl logs -n dark-tower deployment/mc-0 --previous | head -5
#   -> Error: MissingEnvVar("MC_MAX_RECEIVE_SLOTS")

# 2. Apply the OVERLAY, which carries the whole coupled set in one object
#    stream: the ConfigMap supplies the value, and the Deployment supplies the
#    configMapKeyRef that injects it. Applying only the ConfigMap leaves the
#    pods crashing on the SAME key, because the running pod template still has
#    no reference to it -- which reads as "my fix did nothing" and invites a
#    second, wrong diagnosis.
#
#    APPLY THE OVERLAY, NOT THE BASE. `kubectl apply -f
#    infra/services/mc-service/configmap.yaml` looks like the obvious move and
#    is wrong: the Kind overlay strategic-merge-patches OTEL_ENABLED="true" into
#    mc-service-config (infra/kubernetes/overlays/kind/services/mc-service/
#    configmap-otel-patch.yaml), and `apply -f` on the base rewrites
#    last-applied-configuration to the base content, so the three-way merge
#    flips OTel export back OFF and drops the `environment: kind` /
#    `managed-by: dark-tower` labels. Step 3's rollout then makes that take
#    effect -- i.e. the documented recovery for a signalling-plane outage would
#    disable MC's own span export at the moment you most need it, silently.
#    This is the same overlay `dev-cluster deploy mc` and
#    infra/kind/scripts/setup.sh use. Selectors are unaffected
#    (includeSelectors: false), so there is no second, louder symptom to catch
#    the mistake -- the pods come back healthy and the procedure reads as having
#    worked, while the traces you would use for the post-incident review are
#    gone.
#
# 2a. Devloop / Kind cluster -- go through the OVERLAY.
kubectl apply -k infra/kubernetes/overlays/kind/services/mc-service/
#     (equivalently, and preferred on a devloop cluster: dev-cluster deploy mc)

# 2b. A cluster deployed from the base with NO overlay -- here the base IS the
#     deployed artifact and the -f form is correct.
kubectl apply -f infra/services/mc-service/configmap.yaml
kubectl apply -f infra/services/mc-service/mc-0-deployment.yaml
kubectl apply -f infra/services/mc-service/mc-1-deployment.yaml

# 3. Roll. A ConfigMap edit alone does not restart anything (no content hash --
#    see above), and env is read once at process start.
kubectl rollout restart deployment/mc-0 deployment/mc-1 -n dark-tower
kubectl rollout status deployment/mc-0 -n dark-tower
kubectl rollout status deployment/mc-1 -n dark-tower
```

**On a devloop cluster, a rebuild does NOT apply manifests.** `dev-cluster
rebuild` / `rebuild-all` build the image, load it and `rollout restart`
(`crates/devloop-helper/src/commands.rs`); they never `kubectl apply`. Layer 7's
infra-change detector watches `infra/kind/` only (`scripts/layer7.sh`), never
`infra/services/`. So a diff that changes `infra/services/mc-service/**` and the
image together produces the new image against the **old** ConfigMap and pod
template by default — the CrashLoop above is the guaranteed outcome, not bad
luck. Run `dev-cluster deploy mc` (which does `kubectl apply -k` the overlay)
after any change under `infra/services/mc-service/`.

**The symptom may not look like a config problem at all.** In the 2026-09-03
occurrence the first error surfaced was a Layer 7 `PRECONDITION_FAILURE` on
`port 24500 (prometheus) is already in use`. That was downstream noise: cluster
`setup` re-ran *because* mc-0 and mc-1 were unhealthy, and the re-run collided
with ports the existing cluster still held. Chasing the port would have been an
hour spent nowhere. **If cluster setup re-runs unexpectedly, check MC pod health
before believing any port or resource-conflict message it produces.**

### Kubernetes Secrets

**Secret: `mc-service-secrets`** (namespace: `dark-tower`) — supplies three env
vars by `secretKeyRef`:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: mc-service-secrets
  namespace: dark-tower
type: Opaque
stringData:
  REDIS_URL: "redis://:<password>@redis.dark-tower:6379"
  MC_BINDING_TOKEN_SECRET: "<openssl rand -base64 32>"
  MC_CLIENT_SECRET: "<oauth client secret>"
```

**Secret: `mc-service-tls`** (namespace: `dark-tower`) — a *separate* Secret,
mounted as the `mc-tls` volume at `/etc/mc-tls` (mode `0400`), not injected as
env vars. Keys are `tls.crt` and `tls.key`; `MC_TLS_CERT_PATH` /
`MC_TLS_KEY_PATH` point at the mount paths.

> **It is NOT in the kustomization and `apply -k` will not create it.**
> `infra/services/mc-service/kustomization.yaml` lists `secret.yaml`, which
> defines only `mc-service-secrets`. `mc-service-tls` is created *imperatively*
> by `infra/kind/scripts/setup.sh::create_mc_tls_secret` — which runs
> `scripts/generate-dev-certs.sh`, then `kubectl create secret tls` — so it is
> outside the ConfigMap + Deployments + image coupled set and survives every
> `apply -k`. **On a fresh namespace it must be created before the pods start.**
>
> **The signal is `FailedMount`, and there are NO LOGS.** The `mc-tls` volume is
> not marked `optional:`, so a missing Secret blocks the mount outright: both MC
> pods sit Pending in `ContainerCreating`, the process **never starts**, and
> there is no `MC_TLS_CERT_PATH` error and no container log to read. Only
> `kubectl describe pod` names the Secret. Do **not** go looking for a cert-load
> failure — `Identity::load_pemfiles` failing is a *different* problem that
> requires the Secret to exist and be malformed. See §Config-failure triage.
>
> ```bash
> # Does it exist, and is the cert still valid? (No exec needed -- the runtime
> # image is distroless and has no openssl.)
> kubectl get secret mc-service-tls -n dark-tower
> kubectl get secret mc-service-tls -n dark-tower \
>   -o jsonpath='{.data.tls\.crt}' | base64 -d | openssl x509 -noout -dates
> ```

### Kubernetes ConfigMap

**ConfigMap: `mc-service-config`** (namespace: `dark-tower`) — shared across
instances. See `infra/services/mc-service/configmap.yaml` for the deployed
source of truth.

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: mc-service-config
  namespace: dark-tower
data:
  MC_GRPC_BIND_ADDRESS: "0.0.0.0:50052"
  MC_HEALTH_BIND_ADDRESS: "0.0.0.0:8081"
  MC_WEBTRANSPORT_BIND_ADDRESS: "0.0.0.0:4433"
  MC_REGION: "local"
  GC_GRPC_URL: "http://gc-service.dark-tower:50051"
  AC_JWKS_URL: "http://ac-service.dark-tower:8082/.well-known/jwks.json"
  MC_TLS_CERT_PATH: "/etc/mc-tls/tls.crt"
  MC_TLS_KEY_PATH: "/etc/mc-tls/tls.key"
  OTEL_ENABLED: "false"
  OTLP_ENDPOINT: "http://otel-collector.dark-tower:4317"
  OTEL_SAMPLE_RATE: "1.0"
  DEPLOYMENT_ENVIRONMENT: "development"
  # ---- ADR-0036 media signalling (all REQUIRED; absent = CrashLoop) ----
  # Documented defaults live in infra/services/mc-service/configmap.yaml, not as
  # Rust constants: the load path has no unwrap_or, so THAT file is the only
  # statement of the intended value. THIS BLOCK IS A COPY — do not edit it to
  # change a value; edit the ConfigMap and roll (see the ordering banner there).
  # Same convention as the mc-alerts.yaml pointer further down this file.
  MC_MAX_RECEIVE_SLOTS: "8"                    # validated 1..=64 at load
  MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS: "64"
  MC_AUDIO_CODEC: "opus"
  MC_AUDIO_MAX_BITRATE_BPS: "48000"            # top of mh-service's 32-48 kbps band
  MC_AUDIO_FRAME_RATE_HZ: "50"                 # 50 Hz = 20 ms frames
```

**Per-instance ConfigMaps: `mc-0-config`, `mc-1-config`** — carry only
`MC_WEBTRANSPORT_ADVERTISE_ADDRESS`, which necessarily differs per pod. Both
Deployments use per-key `configMapKeyRef` (no `envFrom`), so **every new key
needs an entry in both `mc-0-deployment.yaml` and `mc-1-deployment.yaml`**.
`dt-guard env-config` enforces this **only for keys `config.rs` declares
required** via `ConfigError::MissingEnvVar` — its per-workload check
(`missing_in_manifest`) is built from those literals. `orphan_configmap_key`
does **not** cover it: that rule fires only when *no* workload references the
key at all, so one of two references satisfies it. A non-required key wired
into one Deployment and forgotten on the other is therefore **guard-green while
the two pods run different values**, which is why these five keys are required
with no Rust default. A *required* key referenced by only one CrashLoops that
pod alone, which presents as "the meeting works about half the time" depending
on which pod GC assigned.

### OpenTelemetry (R-55) and Break-glass

MC's OTel SDK is initialized in `main.rs` via `init_otel`, gated by the explicit
`OTEL_ENABLED` flag (see the four `OTEL_*` / `DEPLOYMENT_ENVIRONMENT` env vars
above). The prod base ConfigMap (`infra/services/mc-service/configmap.yaml`)
ships `OTEL_ENABLED="false"`; the Kind overlay
(`infra/kubernetes/overlays/kind/services/mc-service/configmap-otel-patch.yaml`)
strategic-merge-patches it to `"true"` for the dev cluster.

**`init_otel` is fail-hard at startup:** with `OTEL_ENABLED=true`, if the
collector at `OTLP_ENDPOINT` is unreachable the pod fails readiness. Enabling
therefore requires ALL of: `OTEL_ENABLED=true` **and** the MC→collector `:4317`
egress NetworkPolicy rule (present in `network-policy.yaml`) **and** a reachable
collector.

**Break-glass / escape hatch** — if the collector is unavailable and MC must
boot without OTel:

1. Set `OTEL_ENABLED=false` in the `mc-service-config` ConfigMap (or, in Kind,
   drop the `configmap-otel-patch.yaml` from the overlay `kustomization.yaml`).
2. Restart **both** MC replicas — MC runs as two singleton Deployments
   (`mc-0`, `mc-1`), so restart each:
   ```bash
   kubectl rollout restart deployment/mc-0 deployment/mc-1 -n dark-tower
   kubectl rollout status deployment/mc-0 -n dark-tower
   kubectl rollout status deployment/mc-1 -n dark-tower
   ```
3. MC boots with the OTel layer off and no collector dependency. Unlike a
   StatefulSet, a bad `OTEL_ENABLED=true` rollout does not cause an outage:
   the old ReplicaSet pods keep serving until the new ones pass readiness, so
   `kubectl rollout undo deployment/mc-0` (and `mc-1`) is also a valid revert.

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

> **Registration is gRPC, not HTTP.** MC calls `RegisterMc` on GC's gRPC server at
> `GC_GRPC_URL` (`crates/mc-service/src/grpc/gc_client.rs::register`). There is
> **no `GC_REGISTRATION_URL`** — that variable is one of the six phantoms named in
> the §Environment Variables maintenance note above and appears in no MC build.
> Do not `curl` a registration endpoint; there isn't one.

**Causes:**
- GC service not running or not healthy
- `GC_GRPC_URL` incorrect in the `mc-service-config` ConfigMap
- NetworkPolicy blocking MC → GC traffic
- GC rejecting registration (capacity, region mismatch)

**Resolution:**

```bash
# Check GC service is running
kubectl get pods -n dark-tower -l app=gc-service

# Verify the gRPC endpoint MC was actually given, read from the pod spec rather
# than the ConfigMap -- the ConfigMap being out of step with what the Deployment
# injects is itself a failure mode, so the artifact under suspicion cannot also
# be the evidence. No shell needed (the runtime image may be distroless).
kubectl set env deployment/mc-0 --list -n dark-tower | grep GC_GRPC_URL
kubectl set env deployment/mc-1 --list -n dark-tower | grep GC_GRPC_URL

# What MC logged at startup, including the effective config line
kubectl logs deployment/mc-0 -n dark-tower --tail=100 | grep -i "register\|gc"
kubectl logs deployment/mc-1 -n dark-tower --tail=100 | grep -i "register\|gc"

# Confirm GC is actually serving gRPC on 50051
kubectl get svc gc-service -n dark-tower -o jsonpath='{.spec.ports}'
```

**Fix:**
1. Ensure GC service is running and ready
2. Correct `GC_GRPC_URL` in `infra/services/mc-service/configmap.yaml`, then apply
   **and roll** — see §Config-failure triage; a ConfigMap edit alone does not
   restart anything
3. Adjust NetworkPolicy to allow MC → GC egress on **TCP:50051**
   (`infra/services/mc-service/network-policy.yaml`); MC's gRPC *ingress* from GC
   is TCP:50052

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
kubectl logs deployment/mc-0 -n dark-tower --tail=100 | grep -i "tls\|cert\|webtransport"
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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &

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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &

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
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &

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

# Step 5: Verify join metrics incremented.
# PORT 8081 and Deployment `mc-0`, not `mc-service:8080`: MC_HEALTH_BIND_ADDRESS
# is 0.0.0.0:8081 (configmap.yaml) and the pods declare containerPort 8081 —
# there is no listener on 8080, and there is no Deployment named `mc-service`
# (that string is a Service, a PDB and a container name). A `8080:8080` forward
# to it yields connection-refused, which looks identical to "the metric did not
# increment" — and this step is the designated producer for the post-deploy
# "Forwarding policy confirmed live" gate below, so a broken command here makes
# that gate permanently unrunnable rather than merely red.
kubectl port-forward -n dark-tower deployment/mc-0 8081:8081 &
curl -s http://localhost:8081/metrics | grep -E "mc_session_joins_total|mc_session_join_duration|mc_media_policy_pushes_total"
kill %1

# Expected:
# mc_session_joins_total should have incremented
# mc_session_join_duration_seconds should show recent observation
# mc_media_policy_pushes_total{outcome="match"} should have incremented — this join
#   programmed the assigned MH with the meeting's forwarding policy and the handler
#   echoed back the generation MC sent (ADR-0036 §8). This is the producer for the
#   post-deploy "Forwarding policy confirmed live" gate, so running Test 5 answers
#   that gate without a second port-forward.
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
- `MCMediaConnectionAllFailed` - Every client media connection attempt failing

`infra/docker/prometheus/rules/mc-alerts.yaml` is the source of truth; this list
is a convenience copy. **Verify against that file before relying on a name here**
— nothing checks this list, and it previously named two page alerts,
`MCHighLatency` and `MCHighMessageDropRate`, that have never existed in the rule
file. Believing a page alert covers you when no rule exists is worse than knowing
you have no alert. (`MCHighJoinLatency`'s own description still refers to "the
aggregate `MCHighLatency` page alert" — that reference is also stale; join
latency is covered at `severity: info` only.)

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
# Rollback command -- BOTH instances, as one step. Stopping after mc-0 leaves a
# mixed-version signalling plane: GC keeps assigning meetings to both, so about
# half of new meetings still get the behaviour you are rolling back away from,
# intermittently and per-meeting. See §How to Rollback for the full ordering.
kubectl rollout undo deployment/mc-0 -n dark-tower
kubectl rollout undo deployment/mc-1 -n dark-tower

# Note: Active sessions on old pods will drain gracefully (60s).
# New joins will route to rolled-back pods once they register with GC.
```

### Post-Deploy Monitoring Checklist: MC↔MH Coordination (RegisterMeeting + Notifications)

Use this checklist after any deployment that touches the MC↔MH coordination path: `RegisterMeeting` RPC client (`crates/mc-service/src/grpc/mh_client.rs`), `MhConnectionRegistry`, or MH→MC notification handling. This is the MC-side companion to the MH-side post-deploy checklist; the canonical full checklist (with all four windows — 30-min, 2-hour, 4-hour, 24-hour — and rollback criteria) lives at:

- `docs/runbooks/mh-deployment.md` §"Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination"

Open that section first if you are deploying mh-service or both services together. The MC-specific spot-checks below let an MC-only engineer (e.g. deploying only an MC client revision) verify the MC half of coordination without flipping runbooks.

**Quick MC-side gates** — for the canonical PromQL, see `docs/runbooks/mh-deployment.md` §"Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination" → "30-minute check". Do not duplicate the queries here; thresholds and emitter-label conventions are owned in one place to avoid silent divergence:

- `mc_register_meeting_total{status="success"}` rate / total > 95% (canonical query in MH runbook). Emitter-label note: `status="success|error"` (NOT `failure`); see `crates/mc-service/src/observability/metrics.rs::record_register_meeting` and call sites in `crates/mc-service/src/grpc/mh_client.rs::register_meeting` — the inherent `impl MhClient` method (NOT the `MhRegister` trait declaration nor its trait-impl; the three success/error/timeout call sites all live in the inherent impl body).

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
- [ ] `mc_participant_mh_status_total` **failed-share < 0.20** over 30m (R-60; canonical ratio query in MH runbook 30-min check). This is a *ratio*, NOT a `{state="failed"}` increase == 0 check — the counter increments on any single per-MH client hiccup, so a bare `== 0` false-fails every deploy. Any breach → investigate the client→MH media plane per `mc-incident-response.md` §"Scenario 11: Media Connection Failures".
- [ ] No new `MCMediaConnectionAllFailed` alerts firing (`infra/docker/prometheus/rules/mc-alerts.yaml`)
- [ ] No mc-service pod restarts since deploy completed
- [ ] **Forwarding policy confirmed live** (ADR-0036 §8): `increase(mc_media_policy_pushes_total{outcome="match"}[30m]) > 0`. **If this is zero the gate is NOT green — it is unrun**: no meeting was programmed in the window, which is also what a broken deploy looks like. Run §Smoke Tests → [Test 5: Join Flow](#test-5-join-flow-webtransport--signaling) to drive one, then re-check. **Do not tick this on an absent series.**
- [ ] **No unconfirmed pushes**: `increase(mc_media_policy_pushes_total{outcome!~"match|handler_id_mismatch"}[30m]) == 0`. **`handler_id_mismatch` is excluded deliberately**: `handler_id` is a per-incarnation token (MH derives it from `HOSTNAME` plus a fresh UUID at process start, and `MH_HANDLER_ID` is unset in all three manifests), so any deploy that rolls — or merely restarts — an MH pod produces that outcome **by construction**. A gate written "no non-`match` outcomes" false-fails every such deploy, and a gate that cries wolf stops being read, including for the real `no_applied_generation` it exists to catch. **Three sites hold this one expression** — this checklist, the metrics catalog entry, and story task 21's alert rule — and they are **one decision with one revert trigger**: remove the exclusion when `2026-09-02-mh-stable-handler-id` lands, in all three places together.
- [ ] Read `mc_media_generation_divergence` **only after** the gate above has fired. It is the magnitude, never the detection signal, and it is written **only on a registration push** — with the ADR-0036 §8 re-assert cadence deferred, it does not observe a handler restart.

> **Why these two PromQL expressions are inline here, against the section preamble.** That preamble says not to duplicate queries, and the rule is about two divergent copies of one query drifting apart. These two series are **MC-only with no MH-side counterpart**, so there is no second copy and nothing to diverge from — unlike the gates above, they are canonically owned here. Do not move or delete them to satisfy the preamble.
>
> **Scope of a green `match`**: it covers **only meetings programmed after the rollout**. Meetings already live on a pod when it rolled were programmed by the previous process, and there is no re-assert cadence to reprove them. Do not read `match > 0` as "forwarding is healthy fleet-wide".
- [ ] Cross-check the MH-side checklist (link above) for the full set of MH-side checks (handshake, JWT, timeout, MH→MC delivery success rate, active connections)

**Rollback (MC half)**: same as the join-flow rollback above — `kubectl rollout undo deployment/mc-0 -n dark-tower` and `kubectl rollout undo deployment/mc-1 -n dark-tower` (MC ships as two per-ordinal Deployments; there is no Deployment named `mc-service` — that string is a Service, a PDB and a container name. Form per `docs/runbooks/mh-deployment.md`). **Carve-out — do NOT roll MC back for sustained `no_applied_generation`.** If `mc_media_policy_pushes_total{outcome="no_applied_generation"}` is climbing with retries exhausted after a deploy, MH is below the ADR-0036 §8 contract and **the remedy is to roll MH forward, not MC back**: rolling MC back returns it to `policy_generation: 0` registrations, which MH installs nothing for — the pre-change media blackhole, not a fix. If the issue is on the MH side (handshake, JWT, RegisterMeeting timeouts), follow the rollback criteria + `mh-service` rollback documented in `docs/runbooks/mh-deployment.md` §"Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination" → "Rollback criteria".

**Carve-out #2 — do NOT roll MC back on a media-session decline spike.** Since
`NotifyParticipantConnectedResponse` gained `sender_id`, MC and MH are a two-sided
contract. An MC binary predating that field never sets it, and because the field is a
bare `uint32` (not `optional`) a still-new MH decodes the absence as `0` — the reject
value — and **declines every media session**. So `kubectl rollout undo deployment/mc-0`
after this contract has landed is a **total media blackout**, not a mitigation.

If `mh_media_session_starts_total{outcome="declined_no_sender_binding"}` is climbing,
**the remedy is to roll MH back (or MC forward), never MC back.** Deploy order is forced:
**MC forward first, MH back first.** Full triage at
`docs/runbooks/mh-incident-response.md` §"Scenario 15: Media Sessions Declining — No
Sender Binding"; ordering at `docs/runbooks/mh-deployment.md` §"Cross-service ordering:
MC and MH are coupled by the sender_id binding contract".

**This is the same mechanism as the `no_applied_generation` carve-out above**, one
contract later: in both cases rolling MC back returns it to a state MH cannot consume,
and in both cases the intuitive action is the destructive one. Recorded separately rather
than left to be inferred from the first.

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
