# Devloop Output: Phase-1f Prometheus gate (IFS bug) + GC→collector egress netpol

**Date**: 2026-06-30
**Task**: Fix the two env-test-pipeline bugs uncovered after the `.dockerignore` keystone (commit `d54143c`) finally let Layer 7 reach Phase-1f / the suite against a live cluster: (D) the Phase-1f Prometheus readiness gate failing deterministically, and (D.1) the GC telemetry forward returning 502.
**Specialist**: infrastructure (paired-with: operations)
**Mode**: Agent Teams (v2) — follow-on fix (item D / D.1 from `docs/TODO.md` §Devloop Container Resource Hygiene)
**Branch**: `feature/browser-client-join-task-57`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `d54143c` (the envtest-infra-reliability commit that uncovered these) |
| Branch | `feature/browser-client-join-task-57` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `validation (gate 2)` |
| Implementer | `team-lead` (Lead-driven root-cause + fix; user-directed direct fix) |

---

## Origin

The `2026-06-30-envtest-infra-reliability` devloop's `.dockerignore` keystone fixed
the disk exhaustion and — for the first time ever — let Layer 7 reach Phase-1f and
the env-test suite against a real cluster. That exposed two latent bugs (neither
introduced by that diff; both UNCOVERED by it):

### D — Phase-1f Prometheus `/-/ready` gate: IFS word-split bug (`scripts/layer7.sh`)
`layer7.sh:35` sets `IFS=$'\n\t'` (no space). `__wait_http_ready` ran
`until $HTTP_PROBE "$url"` with an **unquoted** `$HTTP_PROBE` (`"curl -fsS -o /dev/null
--max-time 5"`). Under that IFS the string does NOT word-split into argv — the whole
string is treated as a single command name → **exit 127** (command-not-found) on every
probe → 300s timeout → `PRECONDITION_FAILURE observability-prometheus-not-ready` against
a perfectly healthy Prometheus.

**Evidence (concurrent in-container probe-loop + instrumented run):** during the failing
poll window an identical `curl …:20300/-/ready` from the same container returned 200
**93/93**; the instrumented probe logged `verbose-curl rc=0` (200 OK) vs `$HTTP_PROBE
rc=127`. Latent for 6 weeks because `layer7.test.sh` injected a single-token fake probe
(no spaces → no IFS issue) and the real-curl path never ran live until the `.dockerignore`
fix.

**Fix:** array-split in `__wait_http_ready` — `local -a probe; IFS=' ' read -r -a probe
<<<"$HTTP_PROBE"; until "${probe[@]}" "$url"` — mirrors the existing `layer7.sh:409`
`IFS=' ' read -r -a env_test_cmd` idiom. Plus a `layer7.test.sh` regression case (multi-word
probe under strict IFS — the exact gap that hid the bug).

### D.1 — GC telemetry forward → 502: missing GC egress netpol (`infra/services/gc-service/network-policy.yaml`)
With Phase-1f fixed, the suite ran `31_gc_telemetry` live → 6/11 failed with **502**
(GC→collector forward failed). The otel-collector INGRESS netpol admits `gc-service:4318`,
but GC's EGRESS netpol allowed only AC/MC/Postgres/DNS — **no `→ otel-collector:4318` rule**.
Task #10 (telemetry proxy) added the forward + the collector-ingress allow but missed the
symmetric GC-egress allow (zero-trust requires both). **Fix:** add the egress rule
(namespace `dark-tower`, `app: otel-collector`, TCP 4318). Proven: all **11/11**
`31_gc_telemetry` tests pass live.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/layer7.sh` (MODIFY: array-split `$HTTP_PROBE` in `__wait_http_ready`) | Mine | — (infrastructure owns the validation pipeline; operations reviews) |
| `scripts/layer7.test.sh` (MODIFY: multi-word-probe-under-strict-IFS regression case) | Mine | — |
| `infra/services/gc-service/network-policy.yaml` (MODIFY: add GC→otel-collector:4318 egress) | Mine | — (infrastructure owns per-service netpols) |

(No Guarded Shared Areas — no `proto/**`, `crates/common/src/jwt.rs`, `db/migrations/**`.)

---

## Validation

Gate 2 to produce a real `GATE2=PASS` (the first of this effort): with these fixes,
Layer 7 reaches Phase-1f → passes → runs the suite → all env-tests green (telemetry WIP
stashed for this devloop's run; validated separately at 11/11 in the telemetry devloop).
Committed legitimately (no `--no-verify`).
