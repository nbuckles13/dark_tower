# Devloop Output: Gate zero — UDP hostPort reachability + NetworkPolicy enforcement on the DNAT path (Kind)

**Date**: 2026-11-06
**Task**: Story 4 task I-1 (R-27): env-test proving UDP hostPort reachability and MH NetworkPolicy enforcement on the hostPort/portmap DNAT path
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present (paired with test)
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~2h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `7c0d41d069a2191e28918b08fb24529f533bddad` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (infrastructure) |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `paired-test` (paired, replaces test slot) |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | not spawned (no check surface: test-only env-test code, no credential/Debug/secret material) |

---

## Task Overview

### Objective
See task prompt (run-story task #1, story `docs/user-stories/2026-10-31-survive-handler-loss.md`, ADR-0039 §7 / Implementation Guidance "First").

### Scope
- **Service(s)**: env-tests (Kind cluster); no product behaviour change
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED — ADR-0039 already decides the gate and its fallback.

---

## Cross-Boundary Classification

| File | Owner | Classification | Notes |
|------|-------|----------------|-------|
| `crates/env-tests/tests/41_gate_zero_hostport_policy.rs` (new) | infrastructure + test (paired) | Own | Own binary, `#![cfg(feature = "resilience")]`, `Edge-sum: 0` header |
| `crates/env-tests/src/canary.rs` | infrastructure (CanaryPod) | Own | Manifest-based deploy path; graceful (non-force) cleanup becomes the ONLY delete path (fixes the class for `40_resilience.rs` too); `BUSYBOX_IMAGE` const |
| `crates/env-tests/src/fixtures/kube.rs` | test | Minor-judgment | Result-returning `kubectl_get_json()` reader; paired-test reviews |
| `crates/env-tests/src/fixtures/gate_zero.rs` (new) | infrastructure + test (paired) | Own | Pure gate parts: policy derivation, selector/collision/endpoint checks, manifest, log matcher, timing budget; unit-tested |
| `crates/env-tests/tests/40_resilience.rs` | infrastructure + test | Mechanical | `..CanaryConfig::default()` for the new `name_prefix` field |
| `crates/env-tests/README.md` | test | Mechanical | Test-file listing line |
| `crates/env-tests/src/fixtures/deployment_config.rs` | test | Mechanical | Doc comment: `live_pods_json` is the per-instance wrapper over `kubectl_get_json` |
| `crates/env-tests/src/fixtures/mod.rs` | test | Mechanical | module registration |
| `docs/user-stories/2026-10-31-survive-handler-loss.md` (Design section) | Lead / story | Mechanical | Gate outcome record, as the task instructs |
| `docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md` (line 81) | test | Mechanical | Point the Gate zero line at the test file + fn |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | infrastructure | Own | Pointer to the gate test |

---

## Planning

### Premise defect (escalated to Lead)
The task says the canary uses "two EXISTING, currently dead kind mappings" (containerPort 30830, 30530) so no recreate is needed. Both are `protocol: TCP` in `infra/kind/kind-config.yaml.tmpl` and in the live rendered `/tmp/devloop/kind-config.yaml` (30830->29020/TCP, 30530->29021/TCP). Kind/podman publish per protocol, so UDP to `gateway:29020/29021` never reaches the node: the allowed arm would false-FAIL as "hostPort broken" and push I-2 onto the fallback for the wrong reason. The static `kind-config.yaml` has neither mapping. All existing UDP mappings (30433-30436) are live MC/MH NodePorts; reusing them would contaminate live traffic (rejected).

### Chosen approach: node-direct (no recreate) + crossover
- The test sends from the devloop container to the canary's node (`.status.hostIP` of the canary pod, read live; reachable from the container: verified TCP 10.89.2.8:6443) at UDP hostPort 30830 / 30530. That packet enters the node's PREROUTING exactly as a host-published one does and meets the SAME Calico `portmap` DNAT (`CNI-HOSTPORT-DNAT`) and the SAME Calico policy evaluation the slots will use. Calico CNI conflist includes `portmap` (verified in `calico-config`).
- Not exercised: the outer Kind/podman host-publish hop (host gateway -> node) for UDP. That hop is already carried in production-shape today by the MH/MC UDP NodePort mappings (26_mh_quic), and I-2 must recreate anyway for UDP slot mappings. **Obligation recorded for I-2**: its slot env-test must send host-gateway -> slot host port end to end. Recorded in the story Design section with the outcome.
- Alternative for Lead: flip the two template mappings to UDP + cluster recreate now (needs host-side recreate; contradicts the task's "no recreate").
- Crossover (paired-test proposal, replaces any unpolicied control pod): Phase A canary hostPort 30830->4434/UDP, 30530->9999/UDP; Phase B fresh canary with the swap 30830->9999, 30530->4434. PASS = each hostPort both delivers (to 4434) and drops (to 9999), so the only variable is the container port, i.e. the policy. Silence on 9999 can never be attributed to the mapping.

### Canary
- Labels = EXACTLY live `networkpolicy/mh-service` `.spec.podSelector.matchLabels` (read live; policy missing -> fail). Allowed port derived from the live ingress rules (UDP 4434, `from: []`); assert 9999 is in no ingress rule. No `instance` label (no RS adoption; no live-pod fixture collisions); assert `ownerReferences` empty.
- Name `gate-zero-canary-<nonce>` (not `mh-` prefixed); container `gate-zero-canary`; image `busybox:1.36` (shared const). Declares only the two UDP ports; positively assert no 8083/50053 in the rendered manifest (Prometheus `mh-service` job keeps app=mh-service + port 8083 -> MHDown).
- `readinessProbe: exec [false]`; wait for Running + `started=true`; assert pod IP absent from mh-service EndpointSlices as ready/serving. `terminationGracePeriodSeconds: 1`. PDB selects it (harmless: we delete, never evict) -- documented.
- Listeners: busybox `udpsvd` (multi-peer; `nc -lu` binds to the first peer) per port, one shared script const: log `gate-zero-canary rx port=<p> <payload>` and echo.

### Sequence (per phase)
Preflight: no leftover `gate-zero-canary-*` pod; no pod declares hostPort 30830/30530; no Service uses nodePort 30830/30530 -> else fail with the delete command. Deploy, wait (bounded; FailedScheduling reported distinctly). Self-send control `ctl-<nonce>-<port>` on loopback for BOTH listeners, both must log. Allowed arm: K spaced datagrams `ext-<nonce>-<hostport>-4434-<i>`; need echo AND log line within T_allow. Drop arm: same K/spacing, window bound >= several x allowed latency; assert no echo, no log line; then re-read log and require the 9999 control line (log read still works). Cleanup: `kubectl delete pod <n> -n dark-tower --wait=true --timeout=30s`, assert NotFound; Drop backstop prints that command only.

### Failure classes (greppable `GATE0 <token>` + typed error enum)
`precondition` (policy object missing, node unreachable, port collision, leftover canary), `control-failed arm=allowed|drop`, `hostport-delivery-failed` (allowed arm silent with controls green; prints node IP, hostPort, protocol, portmap conflist hint, and states that the runner->node UDP leg is NOT independently controlled, so the Lead must diagnose before adopting the fallback), `policy-not-enforced port=9999`, `cleanup-failed`. Failures dump the canary log tail. PASS prints one summary line for both phases (story evidence).

### Security additions (S1-S7)
- Claim (1) "host-side end to end over UDP hostPort" is NOT reported as PASS by this gate: recorded as "publish-layer UDP leg not exercised by gate zero", covered by composition (26_mh_quic on 30434) + the I-2 obligation. Claim (2) policy-on-DNAT is what this gate decides.
- Live policy: assert `matchExpressions` empty; the set of dark-tower NetworkPolicies whose podSelector matches the canary labels is exactly `{mh-service}` (a later default-deny fails loudly).
- Bracketing: drop window starts after the allowed arm echoes; a final allowed-arm datagram must echo after the drop bound. Drop bound = max(floor, multiple x observed allowed RTT); several drop datagrams across the window. Per-datagram nonces.
- No Service selecting the canary labels sets `publishNotReadyAddresses`; canary IP absent from READY endpoints of every selecting Service. No scrape annotations.
- `activeDeadlineSeconds` (a few minutes) so a leaked canary self-terminates. No shipped manifest is touched; canary spec lives only in test code. Node IP from the canary pod's `.status.hostIP` (the node actually holding the hostPort), PRECONDITION reader in `fixtures/kube.rs`; hostPorts/container ports come from the one manifest builder.

### Reviewer implementation notes (adopted)
- Every `GATE0` line carries `phase=A|B hostport=<..> container_port=<..>`; bracketing re-check has its own token `bracket-failed`; node reachability precondition (bounded TCP connect to `<hostIP>:6443`) has its own token `node-unreachable`, run before phase A. (obs, paired-test)
- PASS line states scope: `policy-on-DNAT: PASS (phases A+B); publish-layer UDP leg: NOT EXERCISED (I-2 obligation)`. (obs)
- All CanaryPods get `terminationGracePeriodSeconds: 1` (kubectl-run path moved to the manifest builder); delete timeout a named const well above it. (paired-test)
- `activeDeadlineSeconds` derived from the sum of a phase's bounded waits + margin, with a hermetic assert deadline > worst-case phase duration. (paired-test)
- CanaryPod name prefix is a parameter (default `canary`). The whole type moves to `tokio::process` (Drop is the one sync exception, commented). (dry, code-reviewer)
- New kubectl readers live in `fixtures/kube.rs` and return `Result`; the gate maps them to `GateZeroError::Precondition` so every failure carries a `GATE0` token. (dry, code-reviewer)
- One const each for hostPorts, drop port, image, prefix; K / spacing / drop-floor as named consts with rationale. (code-reviewer)
- Conntrack: a fresh unconnected UDP socket bound to port 0 per phase AND per arm (per datagram on the drop arm); the source port is logged next to each nonce. Phase B starts only after Phase A's NotFound assertion. (ops O1)
- Drop backstop: eprintln the exact `kubectl delete pod <name> -n dark-tower --wait=true --timeout=30s` AND fire a non-blocking, non-forced `kubectl delete pod <name> -n dark-tower --wait=false --ignore-not-found` (result only logged). Never --force / --grace-period=0, never blocks. (ops O3)
- Leftover preflight names a Failed leftover as an aborted prior run (activeDeadline fired) and prints the graceful delete command. activeDeadlineSeconds >= 2x the worst-case single-phase duration (scheduling + waits + drop windows + delete), derived from the same consts. (ops O4)

### Also
- CanaryPod: new manifest deploy (`kubectl apply -f -`), reuse name gen / wait / Drop; existing `--grace-period=0 --force` delete replaced with graceful delete for ALL CanaryPods (complete the invariant; 40_resilience uses it).
- New code uses `tokio::process`. Hermetic unit tests for policy derivation (9999-allowed fixture errors), log matcher.
- No config keys, no metrics, no retirements. No layer7 export needed (no host-side port is used).

### Lead decision on premise defect (Gate 1): Option A — node-direct, no recreate
Rationale: the gate exists to choose hostPort slots vs the fallback (StatefulSet-per-slot + per-slot NodePort). The un-exercised leg (Kind/podman UDP host-publish, host gateway -> node) is IDENTICAL under both designs — the fallback's NodePorts are also reached through Kind UDP mappings — so it does not discriminate between them, and it is already exercised in production shape today by 26_mh_quic (UDP 30434). Option A therefore fully decides the architectural question this task owns (hostPort DNAT delivery + policy enforcement on that path, both from the live node), crosses no safety line, and keeps the task's no-recreate constraint. The publish-leg proof for slot mappings is an explicit I-2 obligation recorded in the story Design section (I-2 recreates anyway). Option B would mutate shared cluster topology + devloop-helper port allocation for a leg that I-2 must re-prove on its own mappings.

### Gate 1 confirmations
| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (conditional on A) |
| Test (paired) | confirmed (conditional on A) |
| Observability | confirmed |
| Code Quality | confirmed (conditional on A) |
| DRY | confirmed |
| Operations | confirmed (A) |
| Semantic Guard | not spawned |

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`.

---

## Pre-Work

None

---

## Implementation Summary

**Gate result: PASS** (live devloop Kind cluster, 2026-11-06). Policy enforcement on the UDP hostPort / portmap DNAT path is proven in both crossover phases; publish-layer UDP leg not exercised (I-2 obligation recorded in the story Design section). I-2 proceeds on hostPort slots.

- `crates/env-tests/tests/41_gate_zero_hostport_policy.rs` (new, own binary, `resilience`): live policy derivation, policy/Service/collision/leftover preflight, crossover phases A/B, controls, allowed arm, per-datagram drop arm, bracket, post-window log read, never-ready/never-endpoint asserts, asserted cleanup.
- `crates/env-tests/src/fixtures/gate_zero.rs` (new): pure parts + 15 hermetic unit tests (policy derivation incl. drop-port-admitted/range/portless refusals, selector/Service/EndpointSlice/collision checks, manifest ports, log matcher, crossover, deadline > 2x worst-case phase, distinct GATE0 tokens).
- `crates/env-tests/src/canary.rs`: whole type on `tokio::process`; manifest deploy path (`kubectl create -f -`); every canary goes through it with `terminationGracePeriodSeconds: 1` + `activeDeadlineSeconds`; graceful asserted cleanup is the ONLY delete path (force delete removed); Drop prints the command and fires a non-blocking non-forced delete; `BUSYBOX_IMAGE`, `canary_name()`, `graceful_delete_command()`.
- `crates/env-tests/src/fixtures/kube.rs`: `kubectl_get_json()` (Result-returning).
- `crates/env-tests/tests/40_resilience.rs`: `..CanaryConfig::default()` (new `name_prefix` field).
- Docs: story Design (gate result, scope, I-2 obligation), test-gates.md Gate zero line, env-tests README, infrastructure INDEX.

Post-review live run (2026-11-06, `cargo test -p env-tests --features resilience --test 41_gate_zero_hostport_policy -- --nocapture`; verbatim output in §Devloop Verification Steps): `GATE0 PASS policy-on-DNAT: PASS (phases A+B; ...)` — phase A pod gate-zero-canary-6ba03948 rtt 11.64 ms, phase B pod gate-zero-canary-f3fdadf7 rtt 11.67 ms, 5 drop datagrams over 5 s each. Earlier pre-review evidence: `GATE0 PASS policy-on-DNAT: PASS (phases A+B; networkpolicy/mh-service labels={"app": "mh-service", "component": "media"}, allowed UDP 4434 delivered and UDP 9999 dropped via EACH hostPort [30830, 30530]); publish-layer UDP leg: NOT EXERCISED (I-2 obligation)`; RTT ~12 ms, drop window 5 s x 5 datagrams per phase; no leftover canary afterwards. Discriminating-power check (planning-time manual prototype, pod WITHOUT the MH labels, same hostPorts, same listener script): UDP 9999 via hostPort WAS delivered and echoed — so the drop observed in the gate is the policy, not the path.

Busybox findings baked into the listener: `nc -lu` binds to its first peer (use `udpsvd`); busybox `read` consumes a datagram one byte at a time, truncating it (read whole datagrams with `dd bs=2048 count=1`).

---

## Devloop Verification Steps

Live gate run (post-review code), devloop Kind cluster `devloop-mh-failure-transfer` (KUBECONFIG=/tmp/devloop/kubeconfig):

```
cargo test -p env-tests --features resilience --test 41_gate_zero_hostport_policy -- --nocapture
# full output: /tmp/devloop/gate-zero-run.log (run-local, not committed)
```

Verbatim result line:

```
GATE0 PASS policy-on-DNAT: PASS (phases A+B; networkpolicy/mh-service labels={"app": "mh-service", "component": "media"}, allowed UDP 4434 delivered and UDP 9999 dropped via EACH hostPort [30830, 30530]); publish-layer UDP leg: NOT EXERCISED (I-2 obligation). phase=A node=10.89.2.8 pod=gate-zero-canary-6ba03948: hostport 30830 -> 4434 delivered (rtt 11.644746ms, bracketed), hostport 30530 -> 9999 dropped (5 datagrams over 5s); phase=B node=10.89.2.8 pod=gate-zero-canary-f3fdadf7: hostport 30530 -> 4434 delivered (rtt 11.665482ms, bracketed), hostport 30830 -> 9999 dropped (5 datagrams over 5s)
test result: ok. 1 passed
```

Also: `40_resilience` live 3/3 (graceful-delete change); no canary pods left (`kubectl get pods -A | grep -c canary` = 0); `cargo test -p env-tests --lib` 273 passed; `./scripts/layer-fast.sh` TOTAL_RESULT=OK. The gate was run four times in total during the loop (pre- and post-review code), PASS every time.

---

## Gate 2 — Full Validation (Lead)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → `TOTAL_RESULT=OK` (L1 OK 6s, L2 OK 5s, L3 OK 49s, L4 OK 161s, L5 OK, L6 OK, L7 OK 1452s). Layer 7 ran `tests/41_gate_zero_hostport_policy.rs`: `gate_zero_mh_policy_enforced_on_udp_hostport_dnat_path ... ok` (27.06s), confirming the recorded PASS independently; `40_resilience` 3/3 ok; browser E2E passed.

---

## Code Review Results

### Gate 3 verdicts
| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | Calico GNP/NP-empty precondition; drop-tag leak check on any port |
| Test (paired) | RESOLVED-FIXED | 5 | 5 | 0 | recv-error-as-silence; Calico policy; deadline budget; failed-start asserted cleanup; single-node probe assumption |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | log-read-failure evidence; phase tag on preconditions; recv error |
| Code Quality | RESOLVED-FIXED | 8 (+Q1) | 8 | 0 | thiserror, helper extraction, NotFound via --ignore-not-found, PASS backed by real post-fix run |
| DRY | RESOLVED-FIXED | 7 | 7 | 0 | kubectl_get_json kernel, selector_selects, string_map, canary_pod_manifest, wget helper, ports from live policy |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | failed-start asserted cleanup; Drop doc accuracy; runnable portmap hint |
| Semantic Guard | — | — | — | — | not spawned (no check surface) |

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `7c0d41d069a2191e28918b08fb24529f533bddad`
2. `git diff 7c0d41d0..HEAD`
3. Safe-revert unit: the whole devloop commit (test-only code + docs; no shipped manifest, config key, or metric touched)

---

## Issues Encountered & Resolutions

None yet.
