# Devloop Output: T-0 Env-test MH discovery by label

**Date**: 2026-11-06
**Task**: Env-tests discover MHs by label with fixture-owned port-forwards; retire layer7.sh Phase 1i and ENV_TEST_MH_{0,1}_* (story 4 task #4, R-39)
**Specialist**: test
**Mode**: Agent Teams (v2) — full, Gate-1 present; --paired-with=infrastructure
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~95m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `ebdb20e7b465e384f41ad48e433edca399684769` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `spawned` |
| Implementing Specialist | `test` |
| Tier | `full` |
| Iteration | `1` |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `not spawned (no check surface: test fixtures + layer7 shell, no credential types)` |
| Paired Infrastructure | `spawned` |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
     - Tier (ADR-0037 §D2): set the Tier row at setup from the manifest tier; under `light`, Step 5 SKIPS the Gate-1 plan round and writes a `### Gate 1 — SKIPPED (tier=light; reason: …)` marker instead of pending rows. A resumed devloop reads the tier from THIS Tier row (not the manifest, not the confirmations table); no Tier row ⇒ full (fail-safe).
-->

---

## Task Overview

### Objective
Env-tests and Layer 7 stop assuming fixed MH instances mh-0/mh-1 for runtime access. Every MH a test talks to is discovered by `app=mh-service` from the live pods, through a fixture-owned `kubectl port-forward` on local port 0. Lands before I-2 (R-39, runtime-discovery half).

### Scope
- **Service(s)**: env-tests crate, scripts/layer7.sh (+ self-test), runbook. No service code.
- **Schema**: No
- **Cross-cutting**: No (test harness + Layer 7 lane only)

### Debate Decision
NOT NEEDED - governed by ADR-0039 §7 and the 2026-10-28 test-gates debate (Round 3, Fixture migration).



---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/env-tests/src/fixtures/mh_grpc.rs` | Mine | — |
| `crates/env-tests/src/fixtures/kube.rs` | Mine | — |
| `crates/env-tests/src/fixtures/injection.rs` | Mine | — |
| `crates/env-tests/src/fixtures/metrics.rs` | Mine | — |
| `crates/env-tests/src/fixtures/deployment_config.rs` | Mine | — |
| `crates/env-tests/src/fixtures/media.rs` | Mine | — |
| `crates/env-tests/src/fixtures/egress_admission.rs` | Mine | — |
| `crates/env-tests/src/cluster.rs` | Mine | — |
| `crates/env-tests/tests/01_mh_deployment_config.rs` (LivePod.instance → Option) | Mine | — |
| `crates/env-tests/tests/26_mh_quic.rs` | Mine | — |
| `crates/env-tests/tests/27_mc_slot_placement.rs` | Mine | — |
| `crates/env-tests/tests/28_mh_egress_admission.rs` | Mine | — |
| `crates/env-tests/tests/29_mh_meeting_teardown.rs` | Mine | — |
| `crates/env-tests/tests/35_mc_server_mute_teardown.rs` | Mine | — |
| `crates/env-tests/tests/37_mh_stream_attribution.rs` | Mine | — |
| `crates/env-tests/tests/38_mh_video_forwarding.rs` | Mine | — |
| `crates/env-tests/tests/39_mh_video_keyframes.rs` | Mine | — |
| `crates/env-tests/README.md` | Mine | — |
| `crates/env-tests/Cargo.toml` | Mine | — |
| `scripts/layer7.sh` | Not mine, Domain-judgment | infrastructure (paired) |
| `scripts/layer7.test.sh` | Not mine, Domain-judgment | infrastructure (paired) |
| `docs/runbooks/devloop-validation.md` | Not mine, Minor-judgment | operations |
| `docs/specialist-knowledge/observability/INDEX.md` (pointer `gauge_by_live_pod` → `gauge_by_pods`) | Not mine, Mechanical | observability |
| `docs/TODO.md` (stale `__reap_stale_mh_forwards` pointer → historical ref) | Not mine, Mechanical | infrastructure |
| `docs/specialist-knowledge/infrastructure/INDEX.md` (stale `__start_mh_grpc_forward` pointer) | Not mine, Mechanical | infrastructure |
| `docs/specialist-knowledge/test/INDEX.md` | Mine | — |
| `docs/devloop-outputs/2026-11-06-env-test-mh-discovery-by-label/main.md` | Mine | — |

---

## Planning

### Problem, in mechanism language

Today Layer 7 (Phase 1i) is the only thing that knows which MH pods exist. It resolves `instance=mh-{0,1}`, binds the helper port-map numbers `.ports.mh_{0,1}_grpc` on 127.0.0.1, and hands env-tests four env vars. Env-tests then hardcode the count `2` (`injection::both_handlers`) and pin Prometheus through `kube::MH_INSTANCES` (`gauge_by_live_pod(…, MH_INSTANCES)` in tests 28/38/39, plus `MH_INSTANCE_COUNT` in 28). I-2 turns those helper offsets into MH hostPort slots and replaces mh-0/mh-1 with N slot pods. Both assumptions (fixed names/count, and a forward on a port-map number) break at that point.

**Wider than the prompt names** (completing the invariant, all in-scope runtime uses):
- `tests/35_mc_server_mute_teardown.rs` also calls `mh_grpc::handlers()` four times. It migrates automatically, but it needs the `ReleaseOnDrop::record` signature change below.
- The runtime Prometheus pins `gauge_by_live_pod(…, MH_INSTANCES)` in tests 28, 38, 39 and `MH_INSTANCE_COUNT` (28:145/573) are runtime instance assumptions. They move to label discovery. Test 01's uses stay: it pairs each instance with its per-instance ConfigMap (OUT OF SCOPE, I-2).
- The Phase-1i REASON tokens number FOUR: `mh-pod-unresolved` is also emitted only by `__start_mh_grpc_forward`, so it is retired too, along with the `DEVLOOP_MH_FORWARD_BUDGET` / `DEVLOOP_MH_FORWARD_PROBE` seams.
- **A lifetime hazard the prompt does not name.** `ReleaseOnDrop` records `grpc_url` strings and releases them in its own Drop. With fixture-owned forwards, test 35's `for handler in mh_grpc::handlers()` drops the forwards at the end of the loop, so the end-of-test release would dial a dead port and leak registrations. Fix by type: `record(&Handler, …)` stores an `Arc` of that handler's forward. A forward then lives until every registration made through it has been released.

### Design

**kube.rs (one kubectl shape, one pure predicate)**
- `live_pods_json(instance)` delegates to a new `pods_json(selector)`. Both go through `kubectl_get_json`. There is one call shape and no second function family.
- Pure `is_terminating(pod)` (`metadata.deletionTimestamp` present) and `is_ready(pod)` (`status.conditions[type=Ready].status == "True"`). `resolve_generation` (kube.rs:164) and `deployment_config::newest_live_pod` (:439) switch to `is_terminating`.
- Pure `partition_serving(pods: &Value, selector) -> Result<PodPartition, String>`, where `PodPartition { serving: Vec<LivePod>, terminating: Vec<String>, not_ready: Vec<String> }`. `serving` = non-terminating AND Ready AND has `status.podIP`. Err on a missing `items` array or a Ready pod with no IP.
- `MH_SELECTOR = "app=mh-service"` (the ADR-0039 §7 template invariant).
- Impure `live_mh_pods() -> Vec<LivePod>`, plus the pure `require_all_serving(&PodPartition) -> Result<Vec<LivePod>, String>`. It returns a loud `PRECONDITION` when: zero pods; ANY pod is Terminating ("a Terminating MH mid-suite is a precondition, not a silent subset"); or ANY non-terminating pod is not Ready. **Stricter than the prompt's "exclude not-Ready" — please confirm.** Excluding a crash-looping or starting MH silently shrinks "every live MH" to a subset. The old code enforced exactly two; MC/GC may still route a meeting to a pod that is NotReady at that moment; and "fail loudly, never mask" applies. Selection still partitions not-Ready out of `serving` (the hermetic test the prompt asks for). The wrapper refuses to proceed on a non-empty `not_ready`/`terminating`. **One lookup** feeds both the forward target (pod name) and `pod_ip`, the same JSON object, as in `__start_mh_grpc_forward`.
- `LivePod.instance` doc changes to "the identity the pod was resolved by: the `instance` label for per-instance reads, the pod NAME for label-discovered pods". Discovered pods set `instance = name`, so nothing reads the `instance` label at runtime.

**metrics.rs**: new `gauge_by_pods(prom, metric, &[LivePod], timeout)`. It keeps the distinct-IP check and has the same keying. `gauge_by_live_pod(instances)` resolves instances and then delegates, so there is one loop. Tests 28/38/39 call `gauge_by_pods(…, &live_mh_pods())`. In test 28, `MH_INSTANCE_COUNT` becomes the discovered pod count, resolved once per test and passed down. The `distinct_pod_ips` unit test drops the mh-0/mh-1 names.

**mh_grpc.rs: fixture-owned forward**
- `PortForward` (private fields): `kubectl -n dark-tower port-forward --address 127.0.0.1 pod/<pod> :50053`, with stdin null and stdout/stderr piped. It uses the ambient context, like every other env-test kubectl call (no second context path).
- A stdout drain thread sends the FIRST line over an mpsc channel. It then keeps reading to EOF and keeps a bounded tail (kubectl prints `Handling connection for 50053` on every connection, so an undrained pipe would block kubectl mid-suite). A stderr drain thread keeps a bounded tail in memory for messages.
- Bounded waits, hardcoded constants with comments, no config key: `FORWARD_LINE_BOUND` (30s, `recv_timeout`) for the line, then `FORWARD_ACCEPT_BOUND` (30s) for readiness. Readiness is `try_wait()` FIRST (an exited forward fails fast), then a TCP connect to `127.0.0.1:<parsed>` through a probe shared with `cluster.rs` (extract its `TcpStream::connect_timeout` kernel to `pub(crate) fn tcp_accepts`, so there is no third probe).
- Pure `parse_forwarding_line(line) -> Result<u16, String>`. It accepts exactly `Forwarding from 127.0.0.1:<port> -> 50053` with port != 0. Anything else is Err.
- Failure tokens, distinct per the vacuity rule, as panics naming the pod, the elapsed time and the captured kubectl stdout/stderr tail: `PRECONDITION (mh-forward-no-line)` (timeout/EOF before a line), `PRECONDITION (mh-forward-malformed-line)`, `PRECONDITION (mh-forward-exited)`, `PRECONDITION (mh-forward-not-accepting)`, `PRECONDITION (mh-pods-unserving)` (from discovery). These are panic text for grep, not Layer-7 REASON tokens.
- `Drop`: `kill()` then `wait()` (no zombie). Reader threads end on EOF.
- `Handler { name: pod name, grpc_url, pod_ip, forward: Arc<PortForward> }`, owned per test. Never in a static/OnceLock (statics never drop). `handlers() -> Vec<Handler>` = `live_mh_pods()` → one forward each.
- `ReleaseOnDrop::record(&Handler, meeting_id, mc_id)` holds the `Arc<PortForward>` (see the hazard above). Callers: injection.rs, 29, 35.
- `required_env` goes. Its only callers were the ENV_TEST_MH reads. Its message names Layer 7 step (i).
- **Port-map constraint (module doc, per paired-infra D):** the local port comes ONLY from kubectl's output for a `:50053` (local 0) request. Nothing in env-tests reads `ports.json`, `HOST_PORT_MH_*`, `mh_N_grpc` or `ENV_TEST_MH_*`. Port 0 draws from the kernel's `ip_local_port_range` (32768-60999 measured in the devloop container). The helper allocates 20000-29999 (`crates/devloop-helper/src/ports.rs` `PORT_RANGE_START`, `NUM_SLOTS*STRIDE`), and the two ranges are disjoint. On the host, Kind already holds those numbers, so the kernel can never hand them out. There is deliberately no hardcoded "not 20000-29999" assertion (it would duplicate the helper's SSoT) and no ports.json read (it would re-couple env-tests to the map I-2 repurposes).
- Accepted residual, recorded where the reaper's rationale lived: a SIGKILLed or aborted test binary never runs Drop and orphans its kubectl children. Port 0 means an orphan can never collide with the next run's forward. The cost is a leaked process plus an apiserver stream until the container exits. PR_SET_PDEATHSIG was rejected: it fires on the death of the spawning THREAD, not the process, so it could kill a live forward created on a tokio worker or blocking-pool thread that exits early.

**injection.rs**: `both_handlers()` (exactly 2) is deleted. `injectable_handlers` calls `mh_grpc::handlers()`, which already carries ≥1 / no-Terminating / no-NotReady. A renamed wrapper with an identical body would be a DRY alias, so the rename is that "all live MHs" now *is* `mh_grpc::handlers()`, documented as such. The callers' meaning is unchanged: every injection goes to every live MH. Docs say "every live MH pod" instead of "both".

**Comments**: media.rs:157, egress_admission.rs:14/59, 27:22, 29:29/79, Cargo.toml's tonic comment ("Layer 7's per-pod port-forward"), README prerequisite 4 (rewritten: the fixture starts its own forwards; it needs only kubectl access to dark-tower).

**layer7.sh (paired-infra A/B)**: delete :504-679 (`__MH_FORWARD_PIDS`, `__stop_mh_grpc_forwards`, `__reap_stale_mh_forwards`, `__mh_forward_accepting`, `__start_mh_grpc_forward`), the call site :1581-1617 including the else-branch message, the `MH_FORWARD_PROBE`/`DEVLOOP_MH_FORWARD_PROBE` seams and `DEVLOOP_MH_FORWARD_BUDGET`, the step-duration name `mh-grpc-forwards`, and all four tokens. `KUBECTL_BIN` stays (MC pods / codec profile), with its comment reworded. Reword the comments at :34 and :353. Target after the edit: zero hits for `1i|mh_._grpc|MH_FORWARD|ENV_TEST_MH|mh-grpc|mh-pod-unresolved` across layer7.sh, layer7.test.sh and the runbook.

**layer7.test.sh (paired-infra C)**: delete the Phase-1i block (~717-800), the `FAKE_MH_PODS`/`FAKE_FORWARD_EXIT`/`FAKE_PORT_OCCUPIED`/`FAKE_MH_TERMINATING`/`FAKE_MH_PROBE` fake-kubectl branches and pass-throughs (only if no MC-pod/codec-profile block uses them; I will check), the `mh_N_grpc` keys and the "absent mh_1_grpc" fixture, and the `sel-env-needs-1i-*` group. `STEP=mh-grpc-forwards` present → `assert_absent` on the default and env-only runs. New positive-controlled negative: the env-suite stub records all four `ENV_TEST_MH_*` as `<unset>` AND its `ran.env-suite` marker exists.

**Runbook (operations 1/2/6)**: remove all four tokens from the L7 list at :169. Replace the four Phase-1i rows with ONE triage row keyed on the retired token names (so a 3am grep of an old token lands there): "MH forward/discovery fixture PRECONDITION panic inside the env-test suite → lands as `FAIL`/`env-tests-failed` (implementer lane) but is ENVIRONMENT; first commands `kubectl -n dark-tower get pods -l app=mh-service -o wide` (Ready/Terminating), then the panic's captured kubectl output; an MH restart DURING the suite surfaces as a gRPC connection error in the test, not as a forward failure."

### Security constraints (confirmed by security)
- Both the pod listing (`pods_json(selector)` → `kubectl_get_json(["pods","-n",NAMESPACE,"-l",sel])`) and the forward argv pin `-n dark-tower` explicitly. Neither relies on the context's default namespace.
- `grpc_url` is built as `http://127.0.0.1:{parsed u16}`, from the parsed integer and the literal address, never from echoed kubectl text.
- Captured kubectl tails are bounded. The MC token and client secret are never placed in the forward's argv or buffers.
- `handlers()` doc note for T-1/I-5 (per paired-infra): the strict no-NotReady/no-Terminating rule stays. A test that runs after a pool kill must WAIT for the replacement to be Ready before resolving handlers, rather than loosening this rule.

### Design decision: refuse not-Ready, do not merely exclude (deliberate deviation from the prompt)
The prompt says selection "excludes ... not-Ready". The plan does exclude: `partition_serving` puts not-Ready pods in their own bucket, never in `serving`. Separately, `require_all_serving` REFUSES a non-empty `not_ready` (or `terminating`) bucket with a PRECONDITION panic. The reason: silently excluding a pod would turn "every live MH" into a subset that hides a crash-looping or still-starting MH. That breaks fail-loudly, and it loosens the old exactly-two strictness. Infrastructure confirmed every MH is Ready before Phase 2 (deploy.sh `rollout status` plus the Phase-1 health gate), so the refusal fires only on a real anomaly. Agreed by paired-infrastructure, dry-reviewer, security and code-reviewer. Two separate hermetic tests pin the behaviours: exclusion by `partition_serving`, and refusal by `require_all_serving`.

### `LivePod` identity (code-reviewer option a)
`LivePod.instance` becomes `Option<String>`: `Some(label)` for a per-instance read (`live_pod(instance)`, used by test 01), `None` for a label-discovered pod. A new `fn display_id(&self) -> &str` returns the instance if set, else the pod name. Keys and messages use it: `gauge_on_live_pod` PRESENCE text, `gauge_by_*` keys `"<display_id> (<ip>)"`, the `distinct_pod_ips` message. Test 01:732 reads `pod.instance` and is updated accordingly. No 28/38/39 assertion indexes by a literal `"mh-N (...)"` key; I'll verify with grep during implementation.

### Gate-1 amendments (test, observability, code-reviewer)
- **Bounded eventual discovery (test 1).** `live_mh_pods()` becomes `async`. It re-lists until `require_all_serving` is Ok (zero Terminating, zero NotReady, at least one serving), bounded by `ConsistencyCategory::K8sResourceUpdate` (60s) through `eventual::assert_eventually`. At the bound it panics PRECONDITION with the partition as it stood on the LAST snapshot. This covers old-ReplicaSet pods still inside their grace period after a converge, which `rollout status` does not wait for, and a starting pod that becomes Ready. It still fails closed. The pure `require_all_serving` stays the per-snapshot verdict. `mh_grpc::handlers()` becomes `async` as well (callers in 29, 35 and injection already run in async contexts).
- **One snapshot per test (obs 2).** A test that both injects and pins (38, 39; 26 already pins via `handler.pod_ip`) derives its pins from its handlers through `mh_grpc::pins(&[Handler]) -> Vec<LivePod>`, taken from the same lookup. Only pin-only tests (28) call `live_mh_pods()`, once per test, and pass the result down.
- **Test 28 ALERT-JOIN per pod (obs 1).** For each discovered pod, `instance_for_pod_ip(&joined, &pod.ip)` must be Some, AND `joined.len() == pods.len()`. The panic lists unmatched pods and extra instances separately.
- **Forward-up stderr line (obs 3).** `mh-forward: pod=<name> ip=<ip> local=127.0.0.1:<port> ready_in=<ms>ms`.
- **`connect()` message (test 5).** It names the fixture forward and the pod, not "the Layer-7 port-forward". It notes that a TCP accept only proves kubectl is listening, and that the gRPC handshake is the real readiness check.
- **Hermetic additions (test 3).** (a) mixed 2 Ready + 1 NotReady → 2 serving, and refusal names the NotReady pod; (b) no `status.conditions` (Pending) → not-Ready; (c) Ready=False → not-Ready; (d) distinct, separately asserted Err substrings for zero / terminating / not-ready / no-IP; (e) a trailing `\r`/whitespace on the forwarding line is ACCEPTED after trimming (kubectl line endings), and this is pinned.
- **Retirement drift guard (test 6, optional, taken).** A layer7.test.sh static assertion that layer7.sh contains zero `ENV_TEST_MH_` / `mh-grpc` / `mh-pod-unresolved` tokens.
- **`display_id()` (code-reviewer).** Returns the instance label when `instance` is Some, else the pod name. Test 01's and `gauge_by_live_pod`'s key text is therefore unchanged.

### Lane change (operations 1), stated explicitly
Phase-1i failures were `PRECONDITION_FAILURE` (exit 2, OPERATOR lane, no attempt consumed). The fixture's PRECONDITION panics are inside `cargo test`, so they land as `FAIL`/`env-tests-failed` (exit 1, IMPLEMENTER lane, consuming the attempt). This follows the existing in-test PRECONDITION precedent (deployment_config.rs, injection.rs, kube.rs). It is intentional: discovery is per test now, so there is no pre-suite step left to own it. The runbook row says so.

### Tests
- Hermetic, in-module: `parse_forwarding_line` (good line; `[::1]` line refused; malformed/garbage refused; port 0 refused; wrong remote port refused). `partition_serving` / `require_all_serving` (Terminating excluded from serving and refused by the wrapper; not-Ready excluded and refused; zero pods → Err with a PRECONDITION message; a Ready pod without an IP → Err; a positive control with 3 serving pods → all three returned, proving there is no fixed count). The `is_terminating`/`is_ready` predicates.
- layer7.test.sh as above.
- Live: targeted `cargo test -p env-tests --features all` for 26, 28, 29, 35, 37, 38, 39 against the cluster. Assertion content is unchanged.

### Rollback (operations 7)
One commit, no service/config/infra runtime change, so rollback is a plain revert. I-2 has NOT landed (it depends on this task). Reverting this AFTER I-2 lands would bring back forwards onto repurposed slot offsets, so the safe-revert unit is "this commit, only while I-2 is absent".

---

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Paired Infrastructure | confirmed |

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`. Plan approved by Lead.

## Pre-Work

None

---

## Implementation Summary

Implemented as planned, including the Gate-1 amendments.
- `kube.rs`: added `pods_json(selector)`, `is_terminating`/`is_ready`, `partition_serving`, `require_all_serving` and async `live_mh_pods()` (60s bounded wait). `resolve_generation` and `newest_live_pod` now use `is_terminating`.
- `deployment_config::LivePod.instance` is now `Option<String>`, plus `display_id()`.
- `mh_grpc.rs`: added `PortForward` (drain threads, bounded line/accept waits, kill+wait on Drop), `parse_forwarding_line`, async `handlers()` and `pins()`. `ReleaseOnDrop::record(&Handler, ..)` holds an `Arc` of the forward. `required_env` is removed.
- `metrics::gauge_by_pods` added; `gauge_by_live_pod` delegates to it. `cluster::tcp_accepts` is the one TCP probe.
- `injection::both_handlers` is removed.
- Tests: 28/38/39 pin via discovered pods (38/39 via `pins(&handlers)`), and 28's ALERT-JOIN is checked per pod. Test 35's pin-only loops use `live_mh_pods()`.
- `layer7.sh` Phase 1i, its 4 tokens, its seams and its step name are deleted. `layer7.test.sh` has the positive-controlled `<unset>` negative plus a static drift guard. The runbook has one replacement triage row. The infra INDEX pointer is updated.

**Gate-3 review fixes (iteration 1):**
- Restored the lost `\` continuations in `require_all_serving` (security, ops, DRY, code-reviewer, infra, test).
- The malformed-line panic now leads with `PRECONDITION (mh-forward-malformed-line)` (obs, code-reviewer).
- New `metrics::instance_is_pod` is the one pin rule, used by `instance_for_pod_ip` and by test 28's `extra` filter, with a unit test (obs, DRY, code-reviewer).
- `handlers()` and `live_mh_pods()` run kubectl and the forward starts on `spawn_blocking`, concurrently, re-raising panics via `join_blocking` (code-reviewer, infra, test).
- `PortForward.child` is a plain `Child` (no Mutex). The accept bound is measured from the announcement.
- On every failure path, `fail()` kills and waits the child, then joins both drains before formatting, so stderr is complete (test F1).
- A `start_program` seam has 4 hermetic lifecycle tests: port returned and child reaped on Drop; error+exit → no-line with the stderr text; malformed; announce-then-exit → exited, fast (test F3).
- Test 35 and test 01 resolve `live_mh_pods()` once per test. Test 01's 4 Prometheus pins moved to `gauge_by_pods`; its per-instance ConfigMap reads stay for I-2 (test F6/F7, DRY F2).
- `layer7.test.sh` has positive controls for both absence checks (662/0).
- `docs/TODO.md` reaper pointer is now historical (ops F1).
- `metrics::gauge_by_live_pod` is deleted: it had no callers left, and keeping it would invite instance-name pinning. The INDEX pointers (obs, infra) now name `gauge_by_pods` (DRY F4).
- **Handed to the Lead (not hand-editable here):** a future task's prompt in the story's dt-story manifest block (`docs/user-stories/2026-10-31-survive-handler-loss.md:2907`) still says `gauge_by_live_pod`. It should say `gauge_by_pods`. The block is marked do-not-hand-edit and has no edit verb. If it is left unfixed, a task following it hits a loud compile error, not a silent regression.

Re-verified live: 01, 26, 28, 29, 35, 37, 38 and 39 are all green, with no orphan forwards. `layer-fast.sh` is green.

**Not changed, by finding:** test 28 / `egress_admission::size_s9_meeting`'s `2 * ceiling` is GC's per-meeting selection cap (`mh_selection.rs`, "up to 2"), not the MH pod count. The doc now says so.

**Verified live** (KUBECONFIG=/tmp/devloop/kubeconfig, current mh-0/mh-1 cluster): 26 (11 passed, 2 pre-existing ignored), 28, 29, 35 (5), 37, 38, 39 are all green through the new discovery. No kubectl port-forward process is left after the runs. `layer7.test.sh` 659/0. `layer-fast.sh` green.

---

## Files Modified

```
 crates/env-tests/Cargo.toml                        |   2 +-
 crates/env-tests/README.md                         |   2 +-
 crates/env-tests/src/cluster.rs                    |  12 +-
 crates/env-tests/src/fixtures/deployment_config.rs |  19 +-
 crates/env-tests/src/fixtures/egress_admission.rs  |   4 +-
 crates/env-tests/src/fixtures/injection.rs         |  31 +-
 crates/env-tests/src/fixtures/kube.rs              | 321 +++++++++++-
 crates/env-tests/src/fixtures/media.rs             |   4 +-
 crates/env-tests/src/fixtures/metrics.rs           |  67 ++-
 crates/env-tests/src/fixtures/mh_grpc.rs           | 540 ++++++++++++++++++++-
 crates/env-tests/tests/01_mh_deployment_config.rs  |  30 +-
 crates/env-tests/tests/26_mh_quic.rs               |   6 +-
 crates/env-tests/tests/27_mc_slot_placement.rs     |   2 +-
 crates/env-tests/tests/28_mh_egress_admission.rs   |  76 +--
 crates/env-tests/tests/29_mh_meeting_teardown.rs   |  15 +-
 .../env-tests/tests/35_mc_server_mute_teardown.rs  |  35 +-
 crates/env-tests/tests/37_mh_stream_attribution.rs |  10 +-
 crates/env-tests/tests/38_mh_video_forwarding.rs   |  13 +-
 crates/env-tests/tests/39_mh_video_keyframes.rs    |  15 +-
 docs/TODO.md                                       |   4 +-
 docs/runbooks/devloop-validation.md                |   9 +-
 docs/specialist-knowledge/infrastructure/INDEX.md  |   4 +-
 docs/specialist-knowledge/observability/INDEX.md   |   2 +-
 docs/specialist-knowledge/test/INDEX.md            |   2 +-
 scripts/layer7.sh                                  | 227 +--------
 scripts/layer7.test.sh                             | 246 ++--------
 26 files changed, 1074 insertions(+), 624 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `crates/env-tests/src/fixtures/mh_grpc.rs` | Fixture-owned PortForward (port 0, parsed line, bounded waits, kill+wait Drop), label-discovered handlers |
| `crates/env-tests/src/fixtures/kube.rs` | `pods_json`, `is_terminating`/`is_ready`, `partition_serving`/`require_all_serving`, bounded `live_mh_pods` |
| `crates/env-tests/src/fixtures/metrics.rs` | `gauge_by_pods`, `instance_is_pod`; `gauge_by_live_pod` removed |
| `scripts/layer7.sh` / `layer7.test.sh` | Phase 1i, its 4 REASON tokens, seams and ENV_TEST_MH_* removed; absence assertions with positive controls |

---

## Devloop Verification Steps

Gate 2 was run by the Lead on the final tree: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → `TOTAL_RESULT=OK` (1642s). Log: `/tmp/gate2-1.log`.

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 5s |
| 2 Format | OK | 4s |
| 3 Guards | OK | 49s |
| 4 Test | OK | 162s |
| 5 Lint | OK | 1s |
| 6 Audit | OK (cargo audit: SKIPPED-NO-DIFF no-dep-changes) | 7s |
| 7 Env-tests (live cluster: Rust env-tests + browser E2E `browser-e2e-passed`) | OK | 1414s |

Fast check before review: `layer-fast.sh` `TOTAL_RESULT=OK`. `scripts/layer7.test.sh`: 662 passed, 0 failed.

---

## Code Review Results

### Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | Loopback-only forward, namespace pinned, strict line parse, no creds in argv/tails. Sent one non-security note (string continuations), also raised by others |
| Test | RESOLVED-FIXED | 7 | 7 | 0 | Drain-before-format on failure, spawn_blocking, hermetic `sh`-fake lifecycle tests, layer7.test.sh positive controls, test 35 single snapshot, test 01 runtime pins |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 | `metrics::instance_is_pod` is now the single pin rule; malformed-line token shape matches the runbook |
| Code Quality | RESOLVED-FIXED | 6 | 6 | 0 | String continuations, token shape, blocking-in-async, unneeded Mutex, accept-bound origin, duplicated pin rule |
| DRY | RESOLVED-DEFERRED | 5 | 4 | 1 | Deferred: the story manifest prompt names the deleted `gauge_by_live_pod` (the runner-owned block can't be edited mid-run) |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | TODO.md reaper pointer made historical; string continuations |
| Paired Infrastructure | RESOLVED-FIXED | 2 | 2 | 0 | String continuations; forwards started concurrently via spawn_blocking |
| Semantic Guard | — | — | — | — | Not spawned (no check surface) |

Details of each fix are under the implementer's "Gate-3 review fixes" section above.

**DRY extraction opportunities**: None.

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

Examples:

```
- `docs/TODO.md` §Observability Debt — orphan recording-site audit follow-up
- `docs/TODO.md` §Cross-Service Duplication (DRY) — extract record_token_refresh_metrics
```

or:

```
- (none surfaced in this devloop)
```

- `docs/TODO.md` §Story Workflow Follow-ups — story-4 manifest prompt names deleted gauge_by_live_pod

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `ebdb20e7b465e384f41ad48e433edca399684769`
2. Review all changes: `git diff ebdb20e7..HEAD`
3. Soft reset (preserves changes): `git reset --soft ebdb20e7`
4. Hard reset (clean revert): `git reset --hard ebdb20e7`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit (layer7.sh Phase-1i deletion + fixture forward + consumer migration move together; a partial revert leaves env-tests reading unset ENV_TEST_MH_* or two forward owners). Safe ONLY while I-2 is absent: reverting after I-2 lands would bring back forwards onto the repurposed MH slot offsets. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: Lost string continuations
**Problem**: An edit script dropped the `\` continuations in three `require_all_serving` messages, which left runs of spaces mid-sentence. Five reviewers flagged it.
**Resolution**: The continuations are restored, and a grep of the whole diff found no other cases.

### Issue 2: Blocking work inside async fixtures
**Problem**: `PortForward::start` and the kubectl listing blocked the current-thread runtime while live sessions were open.
**Resolution**: Both now run on `spawn_blocking`, concurrently, and `join_blocking` re-raises any panic with its original message.

### Issue 3: `team-lead` address did not resolve
**Problem**: Teammates' messages to `team-lead` were undeliverable.
**Resolution**: They were routed to `main`, the Lead session, with no impact on the gates.

---

## Lessons Learned

1. Moving a pre-suite Layer-7 step into a fixture moves its failures from the operator lane (exit 2) to the implementer lane (exit 1). State the lane change explicitly in the runbook.
2. When a forward's lifetime moves to the fixture, every Drop-time consumer of its URL (`ReleaseOnDrop`) must hold the forward alive.
3. A runner-owned manifest prompt can reference a symbol that a devloop deletes, and nothing currently lets a devloop amend it.

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```
