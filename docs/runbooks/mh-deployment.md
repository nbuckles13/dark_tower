# MH Service Deployment Runbook

**Service**: Media Handler (mh-service)
**Owner**: Operations Team
**Last Updated**: 2026-05-01

---

## Overview

This runbook covers the post-deploy monitoring procedure for the Media Handler service, with particular focus on deployments that touch the WebTransport server or MC↔MH coordination paths (e.g. the MH QUIC connection user story: client WebTransport handshake, JWT validation, `RegisterMeeting` provisional-accept, MH→MC participant notifications).

A deeper deployment-procedure section (manifest layout, rolling-update steps, smoke tests) lives alongside the existing service runbooks (`gc-deployment.md`, `mc-deployment.md`) and is tracked as a follow-up. The post-deploy monitoring checklist below is the primary deliverable for the MH QUIC story (R-36) and is what an on-call engineer follows after deploying mh-service to verify the WebTransport + coordination paths are healthy.

For active-incident triage (e.g. "an alert is firing right now"), see the companion runbook `docs/runbooks/mh-incident-response.md`. For the metric definitions referenced below, see the canonical metrics catalog at `docs/observability/metrics/mh-service.md` and `docs/observability/metrics/mc-service.md`.

---

## Table of Contents

1. [Deployment Procedure](#deployment-procedure)
2. [Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination](#post-deploy-monitoring-checklist-mh-webtransport--mcmh-coordination)
3. [Required environment keys](#required-environment-keys)
4. [Both pods CrashLoop immediately after apply](#both-pods-crashloop-immediately-after-apply)
5. [Rollback](#rollback)
6. [References](#references)

---

## Deployment Procedure

> **Status**: Stub. The full step-by-step deployment procedure (manifest layout, rolling-update sequencing, smoke tests, GC registration verification) is not yet documented for mh-service. Until it is, follow the structure of `docs/runbooks/mc-deployment.md` §Deployment Steps, substituting `mh-service` for `mc-service`.

The MH service is deployed via Kustomize:

```bash
kubectl apply -k infra/kubernetes/overlays/kind/services/mh-service/
```

Pre-deployment checks: GC reachable, MC reachable, JWKS endpoint reachable, MH WebTransport TLS secret provisioned. Post-rollout, run the post-deploy monitoring checklist below.

---

## Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination

Use this checklist after any deployment that touches MH WebTransport server code (handshake, JWT validation, `await_meeting_registration` timeout) or MC↔MH coordination wiring (`RegisterMeeting`, MH→MC notifications). For routine deployments that do not touch these paths, the general MH monitoring section is sufficient.

This checklist implements the post-deploy verification required by user story `docs/user-stories/2026-04-12-mh-quic-connection.md` §operations (R-36). Required windows: 30-min, 2-hour, 4-hour, 24-hour.

> **Asymmetry note**: success-rate gates use the affirmative (`{result="success"}` / `{status="accepted"}`) over total. Rollback rules use the negative (`{result="failure"}` / `{status!="accepted"}`) over total. The two are mathematically dual; they're written different ways here so each block reads naturally for its purpose (verification gate vs. rollback floor).
>
> **Rollback applies throughout**: the rollback criteria below remain authoritative across all four windows. If any rollback PromQL evaluates true at any point — at 30-min, 2-hour, 4-hour, 24-hour, or in between — the deploy must be rolled back regardless of which window's checklist you happen to be working through.
>
> **Sparse-traffic note**: during the 30-min check immediately after deploy, ratio queries can return empty vectors (Grafana renders this as "No data") if there is no traffic yet. Treat that as "no traffic yet, re-run in a few minutes" — not as a failure. The rollback criteria below use explicit `and sum(rate(...)) > 0` guards so phantom rollbacks cannot fire under no-traffic conditions; do NOT remove those guards if you adapt the queries elsewhere.

> **Why no `or vector(0)` denominator guard?** It looks like a sparse-traffic guard but is actually unsafe in rollback contexts: when no series match, `sum(rate(...))` returns an empty vector and `or vector(0)` falls through to scalar `0`, so the division becomes `A / 0 = +Inf`. Then `+Inf > 0.10` evaluates true and triggers a phantom rollback. The dashboard queries below intentionally produce a clean "No data" empty vector instead, and the rollback queries use the alert-style `and ... > 0` guard.

### 30-minute check

```promql
# WebTransport handshake success rate (target: >95%, R-36 §operations).
# Empty result under no traffic is intentional — see Sparse-traffic note above.
sum(rate(mh_webtransport_connections_total{status="accepted"}[5m]))
/
sum(rate(mh_webtransport_connections_total[5m]))

# JWT validation success rate (target: >99%, R-36 §operations)
sum(rate(mh_jwt_validations_total{result="success"}[5m]))
/
sum(rate(mh_jwt_validations_total[5m]))

# RegisterMeeting timeouts in the bake window (target: 0)
sum(increase(mh_register_meeting_timeouts_total[30m]))

# Forwarding-policy apply failures in the bake window (target: 0).
#
# RegisterMeeting SUCCEEDING DOES NOT MEAN POLICY APPLIED. The RPC answers
# `accepted: true` and records mh_grpc_requests_total{status="success"} on
# apply_failed, rejected_stale and rejected_invalid alike — correct RPC
# semantics, but it means every other RegisterMeeting gate in this file is
# blind to a build that refuses all policy. See mh-incident-response.md
# Scenario 13.
#
# Cumulative-zero, same shape and justification as the timeout counter above:
# no rate threshold is invented here (a useful rate depends on MC's re-assert
# cadence, which lands at story task 13, and alerting is task 21's scope), and
# zero is unambiguously correct for all three values in either era.
#
# `no_generation` is DELIBERATELY EXCLUDED: it is the expected steady state for
# the whole task-11 -> task-13 window, so including it would fail every deploy
# until MC starts emitting policy_generation >= 1. The gate stays valid
# unchanged when the shape inverts at task 13.
sum(increase(mh_media_policy_applies_total{outcome=~"apply_failed|rejected_invalid|rejected_stale"}[30m]))

# MC RegisterMeeting RPC success rate (target: >95%, R-36 §operations)
sum(rate(mc_register_meeting_total{status="success"}[5m]))
/
sum(rate(mc_register_meeting_total[5m]))

# MH→MC notification delivery success rate (target: >95%, R-36 §operations)
# Uses the MH-side counter — only the sender knows whether the gRPC call landed.
# The MC-side `mc_mh_notifications_received_total` has no status label and only
# counts arrivals, so it cannot distinguish "delivered" from "delivery failed".
sum(rate(mh_mc_notifications_total{status="success"}[5m]))
/
sum(rate(mh_mc_notifications_total[5m]))

# Active connections across all MH pods (target: >0 once traffic flows)
sum(mh_active_connections)

# Is traffic flowing? (sanity check; mirrors join-flow precedent's rate spot-check)
sum(increase(mh_webtransport_connections_total{status="accepted"}[5m]))

# Client-reported media-connection FAILED share (R-60; MC-side metric). Gate: < 0.20.
# This is the canonical query for both mh-deployment.md and mc-deployment.md — the MC
# runbook references it rather than duplicating (single home avoids silent divergence).
# Widen the window to [2h] / [24h] for the longer gate tiers below.
# NOTE: this is a RATIO, not a `{state="failed"}` increase == 0 check. The deleted
# mc_media_connection_failures_total{all_failed} was a rare total-failure counter where
# ==0 was reasonable; mc_participant_mh_status_total{state="failed"} increments on ANY
# single per-MH client hiccup, so ==0 would false-fail every deploy. clamp_min guards the
# no-traffic case (0/1 = 0 → passes; no traffic is not a failure). Mirrors the
# MCMediaConnectionAllFailed page alert, which fires at >0.80 sustained 5m.
sum(increase(mc_participant_mh_status_total{state="failed"}[30m]))
/
clamp_min(sum(increase(mc_participant_mh_status_total{state=~"connected|failed"}[30m])), 1)
```

- [ ] `mh_webtransport_connections_total{status="accepted"}` rate / total >95% (handshake success SLO from R-36)
- [ ] `mh_jwt_validations_total{result="success"}` rate / total >99% (JWT success SLO from R-36)
- [ ] `mh_register_meeting_timeouts_total` increase over 30m = 0 (healthy MC→MH coordination)
- [ ] `mh_media_policy_applies_total{outcome=~"apply_failed|rejected_invalid|rejected_stale"}` increase over 30m = 0 (forwarding policy actually took effect — **receipt is not application**; the `mc_register_meeting_total` gate below cannot see this). `no_generation` is excluded and is the expected value until story task 13.
- [ ] `mc_register_meeting_total{status="success"}` rate / total >95% (MC RegisterMeeting RPC SLO; emitter labels are `success|error`, see `crates/mc-service/src/observability/metrics.rs::record_register_meeting`)
- [ ] `mh_mc_notifications_total{status="success"}` rate / total >95% (MH→MC delivery SLO)
- [ ] `sum(mh_active_connections) > 0` once test traffic is flowing (proof clients are connecting)
- [ ] `mc_participant_mh_status_total` **failed-share < 0.20** over 30m (R-60 client→MH media-plane health; run the canonical ratio query above). A breach means clients are reaching MC signaling but failing the MH media connection — triage per `mc-incident-response.md` §"Scenario 11: Media Connection Failures".
- [ ] No new MH alerts firing: `MHHighJwtValidationFailures`, `MHHighWebTransportRejections`, `MHWebTransportHandshakeSlow`
- [ ] No new MC alerts firing: `MCMediaConnectionAllFailed` (pages at >0.80 failed-share for 5m — the media-plane paging line above the 0.20 gate)

### 2-hour check

- [ ] WebTransport handshake success rate trend stable (no downward drift toward 95%)
- [ ] JWT validation success rate trend stable (no downward drift toward 99%)
- [ ] `mh_register_meeting_timeouts_total` increase over the last 2 hours = 0
- [ ] `mh_media_policy_applies_total{outcome=~"apply_failed|rejected_invalid|rejected_stale"}` increase over the last 2 hours = 0
- [ ] `mc_participant_mh_status_total` failed-share still < 0.20 over 2h (no upward drift toward the 0.80 `MCMediaConnectionAllFailed` paging line)
- [ ] No mh-service or mc-service pod restarts since deploy completed (`kubectl get pods -n dark-tower -l app=mh-service` — `RESTARTS` column should match pre-deploy baseline)
- [ ] Logs show no repeated error patterns related to WebTransport, JWT, or RegisterMeeting (cross-reference `mh-incident-response.md` Scenarios 2, 5, 10 if anything looks off)

```promql
# 2-hour ratio re-checks (same `[5m]` window as 30-min so the dashboard panel
# matches the existing alerts at infra/docker/prometheus/rules/mh-alerts.yaml;
# the trend stability check is "do these readings still match what we saw at
# 30-min", which is read off a panel duration, not the rate window).
sum(rate(mh_webtransport_connections_total{status="accepted"}[5m]))
/
sum(rate(mh_webtransport_connections_total[5m]))

sum(rate(mh_jwt_validations_total{result="success"}[5m]))
/
sum(rate(mh_jwt_validations_total[5m]))

# Cumulative-zero counters use the per-window increase
sum(increase(mh_register_meeting_timeouts_total[2h]))
sum(increase(mh_media_policy_applies_total{outcome=~"apply_failed|rejected_invalid|rejected_stale"}[2h]))

# Client-reported media-connection failed share over 2h (R-60; gate < 0.20).
# Same shape as the 30-min canonical query, window widened to [2h].
sum(increase(mc_participant_mh_status_total{state="failed"}[2h]))
/
clamp_min(sum(increase(mc_participant_mh_status_total{state=~"connected|failed"}[2h])), 1)
```

### 4-hour check

```promql
# WebTransport handshake P95 latency (track against handshake SLO trend).
# Uses `[5m]` to match the existing alert defined at
# infra/docker/prometheus/rules/mh-alerts.yaml § "MHWebTransportHandshakeSlow".
histogram_quantile(0.95,
  sum by(le) (rate(mh_webtransport_handshake_duration_seconds_bucket[5m]))
)
```

- [ ] All MH and MC alerts clear and stable (no flapping)
- [ ] `mh_webtransport_handshake_duration_seconds` P95 stable (not drifting upward toward an SLO boundary)
- [ ] WebTransport rejection rate steady (no upward trend in `mh_webtransport_connections_total{status="rejected"}` or `{status="error"}`)
- [ ] JWT failure rate steady, no new `failure_reason` label values appearing (cross-reference `mh-incident-response.md` §"Scenario 2: JWT Validation Failures" for the `failure_reason` taxonomy)
- [ ] No anomalous patterns in MH→MC notification failures (`mh_mc_notifications_total{status="error"}` rate flat)

### 24-hour check

This is the long-tail window where slow leaks show up — JWKS cache eviction interacting with token rotation, MC↔MH connection-pool drift, gradual handshake-latency creep under sustained load. The 24-hour cadence is required by R-36 (the join-flow post-deploy precedent goes 15-min/1-hour/4-hour; this checklist extends to 24-hour because QUIC connection state is more long-lived than the join handshake).

```promql
# Cumulative coordination-failure counts since deploy (target: 0).
# The policy-apply arm matters most at this tier: the aggregate egress-edge
# bound is consumed by finished meetings over uptime, so `apply_failed` from
# that cause surfaces at the 24-hour window rather than at 30 minutes
# (mh-incident-response.md Scenario 13).
sum(increase(mh_register_meeting_timeouts_total[24h]))
sum(increase(mh_media_policy_applies_total{outcome=~"apply_failed|rejected_invalid|rejected_stale"}[24h]))

# Client-reported media-connection failed share over 24h (R-60; gate < 0.20).
sum(increase(mc_participant_mh_status_total{state="failed"}[24h]))
/
clamp_min(sum(increase(mc_participant_mh_status_total{state=~"connected|failed"}[24h])), 1)

# 24-hour averaged success rates — should still match 30-min readings
sum(rate(mh_webtransport_connections_total{status="accepted"}[24h]))
/
sum(rate(mh_webtransport_connections_total[24h]))

sum(rate(mh_jwt_validations_total{result="success"}[24h]))
/
sum(rate(mh_jwt_validations_total[24h]))

# Latency-trend slow-leak checks — additional (beyond R-36).
# Catches gradual P95 drift before the timeout/rejection counters fire:
# JWKS cache eviction, QUIC connection-pool fragmentation, or MC→MH
# RegisterMeeting RPC slowdown that hasn't yet crossed the 15s timeout.
# Trailing `[1h]` rate means this reads "P95 over the last hour, evaluated
# at the 24h mark".
histogram_quantile(0.95,
  sum by(le) (rate(mh_webtransport_handshake_duration_seconds_bucket[1h]))
)
histogram_quantile(0.95,
  sum by(le) (rate(mc_register_meeting_duration_seconds_bucket[1h]))
)
```

- [ ] `mh_register_meeting_timeouts_total` increase over 24h = 0
- [ ] `mc_participant_mh_status_total` failed-share still < 0.20 over 24h (catches a slow media-plane regression — MH-edge reachability or TLS drift — that stays under the paging line but degrades a growing share of clients)
- [ ] 24-hour averaged WebTransport handshake success rate still >95%
- [ ] 24-hour averaged JWT validation success rate still >99% (catches slow JWKS-cache or token-rotation regressions that don't show up at 30-min)
- [ ] No upward trend in WebTransport rejection rate over the past 24h
- [ ] No new `MHHighJwtValidationFailures` flapping pattern across the day-long window
- [ ] `mh_active_connections` follows expected diurnal/load pattern (no clamping at 0, no unexplained spikes)
- [ ] *Additional (beyond R-36)*: WebTransport handshake P95 trailing-1h trend is flat (slow-leak detection — JWKS or connection-pool drift)
- [ ] *Additional (beyond R-36)*: `mc_register_meeting_duration_seconds` P95 trailing-1h trend is flat (leading indicator for the timeout counter; see `crates/mc-service/src/observability/metrics.rs::record_register_meeting`)

### Rollback criteria

Trigger an immediate rollback if any of the following hold (verbatim from user story §operations):

- `mh_webtransport_connections_total` non-accepted ratio > 10% sustained for 10 minutes. The `and sum(rate(...)) > 0` guard prevents phantom rollbacks when there is no traffic (without it, a single rejected connection during low-traffic periods would compute `A / 0 = +Inf > 0.10` and fire). Mirrors the pattern at `infra/docker/prometheus/rules/mh-alerts.yaml § "MHHighWebTransportRejections"`.

  ```promql
  (
    sum(rate(mh_webtransport_connections_total{status!="accepted"}[10m]))
    /
    sum(rate(mh_webtransport_connections_total[10m]))
  ) > 0.10
  and
  sum(rate(mh_webtransport_connections_total[10m])) > 0
  ```

- `mh_jwt_validations_total` failure ratio > 20% sustained for 5 minutes:

  ```promql
  (
    sum(rate(mh_jwt_validations_total{result="failure"}[5m]))
    /
    sum(rate(mh_jwt_validations_total[5m]))
  ) > 0.20
  and
  sum(rate(mh_jwt_validations_total[5m])) > 0
  ```

- Any `mh_register_meeting_timeouts_total` increment, sustained for 10 minutes (i.e. timeouts are continuing to fire — not a single transient blip). `sum(increase(...))` aggregates across pods so a per-pod label split in the future doesn't change behavior:

  ```promql
  sum(increase(mh_register_meeting_timeouts_total[10m])) > 0
  ```

The 20%/5m JWT floor is intentionally looser than the existing `MHHighJwtValidationFailures` warning alert (which fires at >10%/5m, see `infra/docker/prometheus/rules/mh-alerts.yaml`). The alert is for "investigate"; this rollback floor is for "abort the deploy regardless of cause". Do not tighten the rollback floor to alert thresholds — that conflates investigation with rollback.

```bash
# Rollback command (mh-service). BOTH instances — MH is deployed as two
# per-instance Deployments, `mh-0` and `mh-1`, each with its own NodePort
# Service and Kind port mapping. There is NO Deployment named `mh-service`;
# targeting one returns NotFound.
kubectl rollout undo deployment/mh-0 -n dark-tower
kubectl rollout undo deployment/mh-1 -n dark-tower

# Roll back BOTH. Rolling one leaves a split-version pair: GC keeps placing
# meetings across both, so which version a participant lands on depends on
# placement rather than on any decision you made.

# Active WebTransport sessions on rolled-back pods will be severed during pod
# replacement. Per assumption 4 of the MH QUIC story, MH state is in-memory
# only — clients reconnect with fresh JWTs to the rolled-back pods naturally.
# No data migration or auth-state cleanup is required to roll back; this is a
# pure binary replacement.
```

#### Rollback ordering: the image may roll back alone, the manifests may not

MH reads five **required** environment variables (see
[Required environment keys](#required-environment-keys)). That makes the image
and the manifests orderable rather than independent:

- **Forward — manifests, then image.** Already satisfied: the ConfigMap keys
  landed ahead of the code that requires them, so the tree was never in a state
  where a running image demanded a key no manifest supplied.
- **Backward — the image may roll back alone.** This is the normal rollback and
  the two `kubectl rollout undo` commands above are the whole of it. Older MH
  code ignores environment variables it does not read, so extra keys are inert.
- **Backward — the manifests must NOT roll back alone.** Reverting the ConfigMap
  to a pre-transport-parameter state while the new image is running strips five
  variables that image requires. `Config::from_env()` fails, **both** pods enter
  `CrashLoopBackOff`, and MH accepts no connections at all.

  That last case is a **join-path outage, not a media-only degradation** — a
  participant cannot complete a join if no MH will accept them. **Page, do not
  ticket.** If you must revert the ConfigMap, revert the image in the same
  action.

> **Do not deploy MH with `kubectl apply -f`.** `MH_TERMINATION_GRACE_SECONDS`
> is not a ConfigMap key: it is written into each Deployment's env at build time
> by the kustomize `replacements:` block in
> `infra/services/mh-service/kustomization.yaml`, from **that instance's own**
> `spec.template.spec.terminationGracePeriodSeconds`. The literal checked into
> `mh-{0,1}-deployment.yaml` is a `"0"` sentinel. `kubectl apply -f` bypasses the
> replacement and applies the sentinel verbatim — and because `0` is not a valid
> grace, **both pods refuse to start**. Always `kubectl apply -k`.
>
> That refusal is deliberate: a sentinel that reached a running pod silently
> would give MH a drain window nobody chose, which is the failure the derivation
> exists to prevent. Failing to start is the safe reading of an invalid value.

If the deploy bundled MC changes alongside MH (e.g. an MC-side `RegisterMeeting` client revision), also roll back MC:

```bash
# Same shape as MH: MC is two per-instance Deployments, `mc-0` and `mc-1`.
# There is no Deployment named `mc-service` — that string is a Service, a PDB,
# a NetworkPolicy, a ServiceMonitor and a *container* name, which is the trap.
kubectl rollout undo deployment/mc-0 -n dark-tower
kubectl rollout undo deployment/mc-1 -n dark-tower
```

See `docs/runbooks/mc-deployment.md` §"Post-Deploy Monitoring Checklist: MC↔MH Coordination (RegisterMeeting + Notifications)" for the MC-side perspective on the same checklist.

---

## Required environment keys

MH refuses to start if any of these is missing or invalid. There is no default
for any of them: a value nobody chose, running forever because nothing failed,
is the defect this list exists to prevent.

The **Enforced by** column is the load-bearing one. Three keys begin `MH_MAX_`
and mean unrelated things; the layer that enforces a bound is what tells them
apart, and it is also what tells you where to look when one trips.

| Key | Source | Kind value | What it bounds — and **who enforces it** |
|---|---|---|---|
| `MH_MAX_CONCURRENT_UNI_STREAMS` | ConfigMap `mh-service-config` | `64` | How many unidirectional QUIC streams a **peer** may have open toward MH. **Enforced by quinn**, inside MH's process, at stream-open. Ingress only — it does not bound MH's egress, which the subscriber's own advertised limit does. A peer at the limit **stalls** (QUIC withholds stream credit) rather than MH rejecting, so there is **no MH-side metric for it**: diagnose from the publisher, not from an MH dashboard. |
| `MH_DATAGRAM_BUFFER_AUDIO_FRAMES` | ConfigMap `mh-service-config` | `32` | The QUIC datagram **send** buffer, per connection, expressed in **frames of 20 ms Opus** and converted to bytes once at startup. **Enforced by quinn.** A latency ceiling, not a capacity guarantee — quinn's unchosen default is ~1 MiB, i.e. roughly 89 seconds of queued audio on a realtime path. (`configmap.yaml` says ~93 s for the same default: identical arithmetic, differing only in the assumed frame size — ADR-0036 §1's rounded ~225 B versus the 236 B the code sums from the wire format. Neither is wrong; do not reconcile them by editing one to match.) Must stay strictly **above** MH's own application-level egress queue bound, or MH refuses to start. |
| `MH_KEEPALIVE_INTERVAL_MS` | ConfigMap `mh-service-config` | `10000` | QUIC connection-level keepalive. **Enforced by quinn.** This is what refreshes the NAT binding of a **muted** participant, who by definition sends no media — without it an intermediary can reap the path and unmute is not instantaneous. Validated at startup to sit at or below one third of MH's 30 s idle timeout, so one lost keepalive is not fatal. |
| `MH_MAX_CONNECTIONS` | ConfigMap `mh-service-config` | `500` | Maximum concurrent WebTransport connections. **Enforced by mh-service itself**, at accept, before allocating handler resources. A **resource-exhaustion guard, never a capacity figure, never advertised to GC.** It sits far above expected peak, so rejections attributed to it mean a connection **flood or leak** — not demand that has outgrown the deployment. Scaling out is the wrong response. Visible as `mh_webtransport_connections_total{status="rejected"}`. |
| `MH_TERMINATION_GRACE_SECONDS` | **Not a ConfigMap key** — written into each Deployment's env at build time by the kustomize `replacements:` block | `35` | The pod's own termination grace, from which MH derives its post-cancellation settle window. **Enforced by mh-service** at startup (it refuses to start if the grace leaves no room for the shutdown margin). **Never hand-type this.** It is derived from that instance's own `spec.template.spec.terminationGracePeriodSeconds`, so the two cannot drift; the literal in the deployment file is a `"0"` sentinel. |

One non-required key is listed **for contrast only**, because without it there is
no way to see why the two `MH_MAX_` keys above are not it:

| Key | Source | Kind value | What it bounds — and who enforces it |
|---|---|---|---|
| `MH_MAX_STREAMS` *(not new; not required)* | ConfigMap `mh-service-config` | `100` | **Advertised to GC** as capacity and **enforced only at GC placement** — never on MH's data path. It is also known to be the wrong *unit*: the quantity MH actually needs to bound is an egress bandwidth budget, and a stream count is standing in for it. Its correction is a later story. Do not "fix" it here. |

Table contributed at the request of the observability review, which found that
this runbook — unlike GC's, AC's and MC's — had no configuration section at all.

---

## Both pods CrashLoop immediately after apply

Symptom: after a deploy, `mh-0` and `mh-1` both enter `CrashLoopBackOff` and MH
accepts no connections. **Page, not ticket** — this is a join-path outage, not a
media-only degradation, because a participant cannot complete a join if no MH
will accept them.

Entry point — the container is already dead, so read the previous container's
logs, not the current one's:

```bash
kubectl logs -n dark-tower deployment/mh-0 --previous
kubectl logs -n dark-tower deployment/mh-1 --previous
```

### First, partition on how much output there is

The **absence** of MH's structured startup line is itself a diagnostic, and it
splits the failure space three ways before you read any error text:

| What you see | What it means |
|---|---|
| **No JSON log lines at all**, just a `Debug`-style dump | MH died *before* the tracing subscriber existed. Exactly two things run that early: configuration load, and the OpenTelemetry initialiser. See the ambiguity note below. |
| `Starting Media Handler`, then the structured `Configuration loaded successfully` line, then it stops | Configuration was **valid and fully logged**. The failure is downstream — token acquisition against an unreachable AC, GC registration, or a bind failure — and all of those emit structured errors. Read them. |
| **No Rust output at all** | Not a boot refusal. Look at the image, the entrypoint, or scheduling (`kubectl describe pod`). |

> **Ambiguity inside the first row, and it will mislead you if you do not know
> about it.** A rejected configuration and an **unreachable OTLP collector**
> produce the *identical* signature — zero structured log lines and a `Debug`
> dump — because the collector probe deliberately runs before the subscriber is
> initialised and fails hard rather than silently dropping spans.
>
> The only discriminator is the **type** of the dumped error: a `ConfigError::*`
> variant versus an OpenTelemetry initialisation error. **Read the error's type,
> not just its message** — the instinct is to skim the message, and the message
> alone will send you to inspect a ConfigMap that is perfectly fine.
>
> (This ordering is pre-existing and deliberate: the OTel layer needs the
> endpoint from configuration, and the tracing subscriber can only be
> initialised once. It is recorded here because five required keys make the
> configuration branch much more likely than it used to be.)

### Then, the three causes in likelihood order

1. **A required ConfigMap key is missing or renamed.** The error names the exact
   variable. Check it against
   [Required environment keys](#required-environment-keys) and against
   `infra/services/mh-service/configmap.yaml`. Most likely after a ConfigMap
   edit or a partial revert.
2. **`MH_TERMINATION_GRACE_SECONDS` reads `0`.** The kustomize replacement did
   not run — almost always because the deploy used `kubectl apply -f` on the
   deployment files instead of `kubectl apply -k` on the overlay. Re-deploy with
   `apply -k`. See the sentinel note under
   [Rollback criteria](#rollback-criteria).
3. **The manifests were rolled back under a newer image.** The image requires
   five variables the reverted ConfigMap no longer supplies. Roll the image back
   to match, or restore the ConfigMap. See
   [Rollback ordering](#rollback-ordering-the-image-may-roll-back-alone-the-manifests-may-not).

Every startup refusal names the offending variable. Where the remediation
location travels with it differs by refusal kind, and it is worth knowing which
you are looking at, because in a CrashLoop the message is the only diagnostic an
operator gets:

- The four startup **validations** — egress-queue ordering, termination grace,
  keepalive ratio, and the media latency sample ratio's `0.0..=1.0` range —
  embed the remediation **inline**: the offending value, the bound it was
  compared against, and which ConfigMap key or pod-spec field to change.
  - The fourth is **present-but-invalid**, not missing, so likelihood cause 1
    below ("a required ConfigMap key is missing or renamed") does not cover it:
    `MH_MEDIA_LATENCY_SAMPLE_RATIO` is optional, and an *absent* key is a clean
    fallback to the code default. Only a present value that does not parse, or
    parses outside `0.0..=1.0`, refuses — deliberately, because neither end
    fails safe (above 1.0 silently observes every frame on the per-frame path;
    below 0.0 silently observes none, which reads as a healthy quiet path).
- A **missing variable** refuses with the variable name alone
  (`Missing required environment variable: MH_...`). That is by design, not an
  omission: its remediation is the
  [Required environment keys](#required-environment-keys) table above, which
  names the source of every one of them. Look the variable up there.

Deliberately unchanged, so nobody "improves" it: the missing-variable error
carries the variable as a **plain string literal**, and `dt-guard env-config`
discovers MH's thirteen required variables by matching exactly that shape in
`crates/mh-service/src/config.rs`. Restructuring it to carry a remediation field
would blind that guard on **all thirteen** while it kept reporting clean.

---

## Rollback

For the MH-WebTransport / MC↔MH-coordination deploy path, see [Rollback criteria](#rollback-criteria) above. For other rollback scenarios (general service restore, configuration regression), follow the same `kubectl rollout undo` pattern; deeper operational steps will be filled in alongside the deployment-procedure stub.

---

## References

- **User story**: `docs/user-stories/2026-04-12-mh-quic-connection.md` (R-36, §operations)
- **Companion runbook (active incidents)**: `docs/runbooks/mh-incident-response.md`
- **MC-side post-deploy checklist (companion)**: `docs/runbooks/mc-deployment.md` §"Post-Deploy Monitoring Checklist: MC↔MH Coordination (RegisterMeeting + Notifications)"
- **Metrics catalog**: `docs/observability/metrics/mh-service.md`, `docs/observability/metrics/mc-service.md`
- **Alert rules**: `infra/docker/prometheus/rules/mh-alerts.yaml`, `infra/docker/prometheus/rules/mc-alerts.yaml`
- **ADR-0011**: Observability Framework
- **ADR-0029**: Dashboard / counter conventions (counters vs rates; Category A vs B PromQL)

---

**Document Version**: 1.0
**Last Reviewed**: 2026-05-01
**Next Review**: 2026-06-01
