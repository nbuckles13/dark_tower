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
3. [Rollback](#rollback)
4. [References](#references)

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

Use this checklist after any deployment that touches MH WebTransport server code (handshake, JWT validation, `await_meeting_registration` timeout) or MC↔MH coordination wiring (`RegisterMeeting`, MH→MC notifications, `MediaConnectionFailed` reporting). For routine deployments that do not touch these paths, the general MH monitoring section is sufficient.

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

# Client-reported all-MH-failed events in the bake window (target: 0)
sum(increase(mc_media_connection_failures_total{all_failed="true"}[30m]))

# Is traffic flowing? (sanity check; mirrors join-flow precedent's rate spot-check)
sum(increase(mh_webtransport_connections_total{status="accepted"}[5m]))
```

- [ ] `mh_webtransport_connections_total{status="accepted"}` rate / total >95% (handshake success SLO from R-36)
- [ ] `mh_jwt_validations_total{result="success"}` rate / total >99% (JWT success SLO from R-36)
- [ ] `mh_register_meeting_timeouts_total` increase over 30m = 0 (healthy MC→MH coordination)
- [ ] `mc_register_meeting_total{status="success"}` rate / total >95% (MC RegisterMeeting RPC SLO; emitter labels are `success|error`, see `crates/mc-service/src/observability/metrics.rs:340`)
- [ ] `mh_mc_notifications_total{status="success"}` rate / total >95% (MH→MC delivery SLO)
- [ ] `sum(mh_active_connections) > 0` once test traffic is flowing (proof clients are connecting)
- [ ] `mc_media_connection_failures_total{all_failed="true"}` increase over 30m = 0 (any non-zero is a P1 — clients are losing all MH paths)
- [ ] No new MH alerts firing: `MHHighJwtValidationFailures`, `MHHighWebTransportRejections`, `MHWebTransportHandshakeSlow`
- [ ] No new MC alerts firing: `MCMediaConnectionAllFailed`

### 2-hour check

- [ ] WebTransport handshake success rate trend stable (no downward drift toward 95%)
- [ ] JWT validation success rate trend stable (no downward drift toward 99%)
- [ ] `mh_register_meeting_timeouts_total` increase over the last 2 hours = 0
- [ ] `mc_media_connection_failures_total{all_failed="true"}` increase over 2h = 0
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
sum(increase(mc_media_connection_failures_total{all_failed="true"}[2h]))
```

### 4-hour check

```promql
# WebTransport handshake P95 latency (track against handshake SLO trend).
# Uses `[5m]` to match the existing MHWebTransportHandshakeSlow alert at
# infra/docker/prometheus/rules/mh-alerts.yaml:135-149.
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
# Cumulative coordination-failure counts since deploy (target: 0)
sum(increase(mh_register_meeting_timeouts_total[24h]))
sum(increase(mc_media_connection_failures_total{all_failed="true"}[24h]))

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
- [ ] `mc_media_connection_failures_total{all_failed="true"}` increase over 24h = 0
- [ ] 24-hour averaged WebTransport handshake success rate still >95%
- [ ] 24-hour averaged JWT validation success rate still >99% (catches slow JWKS-cache or token-rotation regressions that don't show up at 30-min)
- [ ] No upward trend in WebTransport rejection rate over the past 24h
- [ ] No new `MHHighJwtValidationFailures` flapping pattern across the day-long window
- [ ] `mh_active_connections` follows expected diurnal/load pattern (no clamping at 0, no unexplained spikes)
- [ ] *Additional (beyond R-36)*: WebTransport handshake P95 trailing-1h trend is flat (slow-leak detection — JWKS or connection-pool drift)
- [ ] *Additional (beyond R-36)*: `mc_register_meeting_duration_seconds` P95 trailing-1h trend is flat (leading indicator for the timeout counter; see `mc-service/src/observability/metrics.rs:341`)

### Rollback criteria

Trigger an immediate rollback if any of the following hold (verbatim from user story §operations):

- `mh_webtransport_connections_total` non-accepted ratio > 10% sustained for 10 minutes. The `and sum(rate(...)) > 0` guard prevents phantom rollbacks when there is no traffic (without it, a single rejected connection during low-traffic periods would compute `A / 0 = +Inf > 0.10` and fire). Mirrors the pattern at `infra/docker/prometheus/rules/mh-alerts.yaml:115-123` (`MHHighWebTransportRejections`).

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
# Rollback command (mh-service)
kubectl rollout undo deployment/mh-service -n dark-tower

# Active WebTransport sessions on rolled-back pods will be severed during pod
# replacement. Per assumption 4 of the MH QUIC story, MH state is in-memory
# only — clients reconnect with fresh JWTs to the rolled-back pods naturally.
# No data migration or auth-state cleanup is required to roll back; this is a
# pure binary replacement.
```

If the deploy bundled MC changes alongside MH (e.g. an MC-side `RegisterMeeting` client revision), also roll back MC:

```bash
kubectl rollout undo deployment/mc-service -n dark-tower
```

See `docs/runbooks/mc-deployment.md` §"Post-Deploy Monitoring Checklist: MC↔MH Coordination (RegisterMeeting + Notifications)" for the MC-side perspective on the same checklist.

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
