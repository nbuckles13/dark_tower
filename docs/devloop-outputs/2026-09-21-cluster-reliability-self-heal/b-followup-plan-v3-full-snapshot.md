# Devloop Output: KIND cluster launch/creation prerequisites — name-length + kubeconfig (task (a))

**Date**: 2026-09-21
**Task**: **SPLIT (operator ruling) — this devloop is task (a): #1 name-length fail-loud (4 sites) + #2 kubeconfig-on-reuse ONLY** — the launch/creation prerequisites. The self-heal (#3), docs (#4), SKILL routing (#5), and the `docs/TODO.md §2952` supersession move to **task (b)**, a scheduled follow-up `/devloop` in this session right after (a) lands. The full Gate-1-reviewed design for (b) is preserved in **`b-followup-self-heal-handoff.md`** (nothing lost — (b) starts from it).
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/devloop-improvements-and-story-2-taskd`
**Duration**: ~TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `4b0f9bfeaa9dbe8894fe6ffaeb56eb9853eb8d8b` |
| Branch | `feature/devloop-improvements-and-story-2-taskd` |
| Lead Model | `claude-opus-4-8[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `planning` |
| Implementer | `implementer` (infrastructure) |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `paired-operations` (paired — owns runbook + SKILL edits, expanded reviewer role) |
| Semantic Guard | `not spawned` (shell-only diff; no credential/lifetime/Rust-idiom check surface per scripts/guards/semantic/checks.md) |

---

## Task Overview

### Objective
The devloop KIND cluster must come up reliably or fail loudly — never silently fail or mask a broken cluster. Five workstreams:
1. **Name-length fail-loud** — reject a cluster name whose derived `<cluster>-control-plane` node name would exceed the 63-char DNS-label cap (real cluster-name limit = 49), at launch in `devloop.sh` and defense-in-depth in `setup.sh`. No truncation, no hash.
2. **Kubeconfig-on-reuse** — `setup.sh` must write `$KUBECONFIG` on the cluster-*reuse* path, not only on create.
3. **Three-case self-heal probe** — at `layer7.sh` `cluster-unhealthy`, probe the real cause via kind/control-plane container (not kubectl-in-container), then apply exactly one matching fix: apiserver-unreachable → teardown+recreate once; kubeconfig-missing/stale → restore kubeconfig only (never recreate a healthy cluster). LOUD, BOUNDED (once), SCOPED to env-precondition class, ROBUST probe.
4. **Docs** — `devloop-validation.md` §8 catalogue rows for four tokens (FAILURE_TRIAGE, FMT_APPLIED, FMT_LANE, FAILED_GUARD_NAMES); §6.7 cluster-unhealthy recovery recipe.
5. **Escalation routing** — devloop SKILL §Recovery: a host-infra precondition the self-heal can't resolve escalates to the operator/host, not the operations reviewer agent.

### Scope
- **Service(s)**: none (infra tooling: devloop launcher, KIND setup, layer7 env-test gate, in-container dev-cluster helper, runbook, devloop SKILL)
- **Schema**: No
- **Cross-cutting**: Yes — validation-pipeline infra; operations owns runbook + SKILL edits (pulled in via paired)

### Debate Decision
NOT NEEDED — no cross-service architectural boundary change; ownership and approach are set by the task. Design lives within existing devloop/KIND/layer7 boundaries.

---

## Cross-Boundary Classification

<!-- Implementer finalizes in Planning; preliminary routing recorded by Lead at setup. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `infra/devloop/devloop.sh` | Mine | — | #1 slug-length check at launch (cap 41) |
| `infra/kind/scripts/setup.sh` | Mine | — | #1 CLUSTER_NAME cap 49; #2 write_kubeconfig helper |
| `infra/kind/scripts/teardown.sh` | Mine | — | #1 CLUSTER_NAME cap 49 (was byte-identical 63 dup of setup.sh) |
| `crates/devloop-helper/src/error.rs` | Mine (scope addition, approved) | — | #1 `ValidSlug::new` slug cap 41 (reachable bypass via `main.rs:45`) |
| `scripts/layer7.sh` | Mine | — | #3 `__self_heal_cluster` at :509 (probe + dispatch to `dev-cluster recreate`/`restore-kubeconfig`); infra-change branch :479-487 left as-is (no `__recreate_cluster` extraction — Finding 5) |
| `crates/devloop-helper/src/ports.rs` | Mine (scope addition, approved) | — | #3 remove dead `K8S_API_HOST` allocation/export (@security) |
| `infra/devloop/dev-cluster` | Mine | — | #3 `recreate` + `restore-kubeconfig` client verbs; `Apiserver reachable:` status line + COUPLED cross-ref |
| `crates/devloop-helper/src/protocol.rs` | Mine (scope addition, approved) | — | #3 `Recreate` + `RestoreKubeconfig` variants + parse arms |
| `crates/devloop-helper/src/commands.rs` | Mine (scope addition, approved) | — | #3 `probe_apiserver_reachable`, `cmd_recreate` (host-side re-confirm + evidence bundle + cross-invocation marker), `cmd_restore_kubeconfig`, `apiserver_reachable` in `cmd_status`, atomic kubeconfig write |
| `crates/devloop-helper/src/main.rs` | Mine (scope addition, approved) | — | #3 dispatch arms for recreate + restore-kubeconfig |
| `scripts/layer-all.sh` | Mine | — | #4 fix stale `:25` debt comment (points at §8 now) |
| `docs/runbooks/devloop-validation.md` | Not mine, Domain-judgment | operations | #4 §8 rows + §6.7 recovery recipe |
| `.claude/skills/devloop/SKILL.md` (§Recovery routing) | Not mine, Domain-judgment | operations | #5 escalate host-infra to operator/host |
| `docs/TODO.md` | Mine (cleanup) | — | supersede §2952 self-heal entry |
| `scripts/setup.test.sh` | Mine | — | #1 name-length (49) tests |
| `scripts/layer7.test.sh` | Mine | — | #3 self-heal 4-case tests |
| `crates/devloop-helper/src/{protocol,commands,error}.rs` `#[cfg(test)]` | Mine | — | Rust unit tests for the additions |

**Scope: APPROVED by @team-lead** — `crates/devloop-helper/**` (`restore-kubeconfig` verb + additive `apiserver_reachable` field) is Mine/infrastructure, strictly additive/backward-compatible, no `auth.rs` touch, non-GSA. The `error.rs` `ValidSlug` cap is also in-loop (ADR-0024 §6.3 in-scope Rust edit — it's a reachable name-length bypass, part of completing #1 "everywhere"). **Gate-2 now includes Rust compile/clippy/unit-test** (layers 1/4/5) in addition to the shell self-tests. None of the touched paths match a GSA criterion.

---

## Planning

> **Plan v3** — locked after full Gate-1 review + three @team-lead scope rulings. Deltas from v2: **#3 self-heal recreate is now a distinct host-side helper verb** (`dev-cluster recreate`), not `teardown`+reason-arg — the destructive decision is **re-confirmed by the privileged helper at the ADR-0030 trust boundary** (option (b)); `apiserver_reachable` is **tri-state** (`true`/`false`/`unknown`; only positive evidence → `false`, indeterminate → escalate, never destroy); an **evidence bundle** is captured host-side before any destroy; the recreate bound **survives the Gate-2 outer retry** via a cross-invocation marker; kubeconfig write is **atomic (temp+rename, explicit 0600)**; restore derives the apiserver port from the **live cluster**, never a fresh allocation; tokens are **one-line `SELF_HEAL CASE=… ACTION=…`** tuples; #2 keeps the `setup.sh` **host-`$KUBECONFIG`** fix (lead retracted the pushback). v2 history retained below where unchanged.

> **v3 blockquote note on `> Plan v2` markers below:** the mechanism text is updated in place to v3; the v2 tables/prose that changed are superseded by the v3 subsections. Read the v3 subsections as authoritative.

### Mechanism restatement (why each fix, in mechanism-language)

**#1 Name-length — ONE invariant chain, FOUR sites, bounds DERIVED from 63.** KIND derives the control-plane node name `<cluster>-control-plane` (one node; `infra/kind/kind-config.yaml.tmpl:18` declares exactly one `role: control-plane`), a Kubernetes DNS **label** capped at 63. The invariant chain: `node ≤ 63 ⟺ cluster ≤ 49 ⟺ slug ≤ 41`, from `-control-plane`=14 and `devloop-`=8. A literal `49` at a *slug* site is a bug: a 45-char slug passes but yields a 67-char node name — the failure surviving behind a green check (@dry-reviewer, @security). The four sites, each bounding the name it actually holds, each writing the SAME prefix/suffix arithmetic (shell and Rust can't share a constant — no shared home imports both — so the *derivation* is the SSoT, written at each site):
  | Site | Validates | Cap | Derivation written at the site |
  |---|---|---|---|
  | `infra/devloop/devloop.sh:92` (has NO length check today) | `TASK_SLUG` | **41** | `63 − ${#"-control-plane"} − ${#"devloop-"}` |
  | `crates/devloop-helper/src/error.rs:110` `ValidSlug::new` (reachable via `main.rs:45` `format!("devloop-{slug}")` without re-entering devloop.sh's check) | slug | **41** | `DNS_LABEL_MAX(63) − "-control-plane".len() − "devloop-".len()` |
  | `infra/kind/scripts/setup.sh:62` | `CLUSTER_NAME` (may be non-devloop, e.g. `dark-tower`) | **49** | `63 − ${#"-control-plane"}` |
  | `infra/kind/scripts/teardown.sh:17` (byte-identical `63` dup of setup.sh) | `CLUSTER_NAME` | **49** | same as setup.sh |
  - Shell sites: `NODE_SUFFIX="-control-plane"; CLUSTER_NAME_MAX=$(( 63 - ${#NODE_SUFFIX} ))`; devloop.sh also `CLUSTER_PREFIX="devloop-"; SLUG_MAX=$(( CLUSTER_NAME_MAX - ${#CLUSTER_PREFIX} ))`. Rust: `const` from `"-control-plane".len()` etc. No literal `49/41/14` anywhere.
  - **teardown.sh → derived 49, same message as setup.sh** (@dry-reviewer confirmed this is the preferred resolution — identical *and* correct beats divergent-and-annotated).
  - **Finding 4 (@dry-reviewer) — the derivation's INPUTS are themselves SSoT.** The `devloop-` prefix and `-control-plane` suffix the arithmetic subtracts must not be independent literals from the ones that build the names: Rust `const CLUSTER_PREFIX = "devloop-"` consumed by BOTH `main.rs:45`'s `format!` and `error.rs`'s cap; bash `CLUSTER_PREFIX="devloop-"` consumed by devloop.sh's cap derivation AND `devloop.sh:698`. `NODE_SUFFIX="-control-plane"` carries a one-line cross-ref to `kind-config.yaml.tmpl:18` (the single-node fact that makes it the longest suffix — a second control-plane node would make it `-control-plane2`=15 and silently drop the cap to 48). Scoped to cluster-name sites only, not the incidental container/network-name prefixes.
  - Token carries its data (@observability #4): `CLUSTER_NAME_TOO_LONG NAME=<name> LEN=<n> MAX=<49|41>` (so the operator doesn't recompute; and seeing it from `setup.sh` rather than `devloop.sh` means the launcher check was bypassed — itself a finding). Rust: `InvalidSlug` message naming the slug cap + the 63 node-name origin. **No truncation/hash** anywhere (would collide two devloops onto one cluster — isolation breach, @security #4).

**#2 Kubeconfig-on-reuse — HOST `$KUBECONFIG` (lead retracted the pushback; defect confirmed).** There are **two distinct kubeconfigs** and the task text conflated them: (i) the **container** kubeconfig `/tmp/devloop/kubeconfig`, written unconditionally by the helper `generate_container_kubeconfig` (`commands.rs:724`, after the create/reuse branch rejoins) — already covered on both paths; its *staleness at suite time* is owned by #3 case B (`restore-kubeconfig`). (ii) the **host `$KUBECONFIG`** context that `setup.sh:create_cluster()` gets ONLY as an implicit side-effect of `kind create cluster` — the two reuse `return 0` paths (`setup.sh:358` AUTO_YES, `:370` interactive "Using existing cluster") skip it, so a caller whose active kubeconfig lacks `kind-<name>` gets `kubectl` silently falling back to `localhost:8080` → "connection refused" → healthy cluster read as broken (a **masked failure** — @dry-reviewer notes the absence of a loud signal is itself the finding). **Fix:** one `write_kubeconfig()` helper (`kind export kubeconfig --name "$CLUSTER_NAME"`, honoring `$KUBECONFIG`) called from create + **both** reuse paths — makes the implicit explicit (SSoT, three call sites, never inlined). A note at `write_kubeconfig` states these are deliberately distinct artifacts from the helper's container kubeconfig so a future DRY pass doesn't collapse them. Repro attempt + evidence recorded in the Implementation Summary either way (@dry-reviewer: "already satisfied" with no repro reads like "didn't check").

**#3 Self-heal at `scripts/layer7.sh:509`** (sole `cluster-unhealthy` emitter; ONE wiring site — @paired-operations confirmed). Replace the bare `precondition_fail cluster-unhealthy` with `__self_heal_cluster`. **The destructive recreate is a distinct host-side helper verb (`dev-cluster recreate`) that RE-CONFIRMS the kill at the ADR-0030 trust boundary — option (b), @team-lead ruling.** The semi-trusted container *classifies + requests*; the privileged helper *independently re-verifies control-plane-dead before it destroys anything*. This makes the S1/S2 safety controls **enforceable**, not advisory, and makes the destroy **audit-distinct by command name** (S6) — no self-attested reason arg.

  - **Shared probe fn `probe_apiserver_reachable(ctx) -> ApiserverReachability` (helper).** A real Rust `enum ApiserverReachability { Reachable, Unreachable, Unknown }` (@code-reviewer — not `Option<bool>`/stringly-typed; exhaustive match). POSITIVE + **kubeconfig-independent, HTTP-status-agnostic, no foreign error strings** (this is the S1-residual fix — @security's *preferred* option, which also converges with S4):
    1. `<runtime> inspect -f '{{.State.Running}}' <cluster>-control-plane` (runtime = `ctx.container_runtime.as_str()`) — control-plane **container** running? Definitive, kubectl-free (@security S2 second observation). Not running/absent ⇒ `Unreachable`.
    2. if running: bounded **TCP connect** to the apiserver's **actual single binding — `HOST_GATEWAY_IP:HOST_PORT_K8S_API`** (@security's trap catch, verified against `kind-config.yaml.tmpl:136-137`: `apiServerAddress: ${HOST_GATEWAY_IP}` / `apiServerPort: ${HOST_PORT_K8S_API}` — there is **NO 127.0.0.1 binding and NO 6443 extraPortMapping**; the template's `extraPortMappings` are all 30xxx NodePorts). Host from `ctx.host_gateway_ip` (else `DEFAULT_HOST_GATEWAY_IP`), port = the `k8s_api` value read from the persisted `ports.json`/port-map (the SAME value `commands.rs:861`'s `alloc.port(PortOffsets::K8S_API)` already uses — S4-convergent). Connect succeeds ⇒ `Reachable` (listening — whatever `/readyz` would answer; a listening-but-wedged apiserver is `Reachable`, so escalate not destroy — SAFE direction, @test-A5). Connection refused / no route ⇒ `Unreachable` (positive, prose-free).
    3. can't determine the port (ports.json missing) / inspect errored / connect timed out / anything unclassified ⇒ `Unknown`. **CRITICAL: an empty/absent port must map to `Unknown`, NEVER `Unreachable`** — aiming a probe at the wrong/absent address and reading the refusal as "dead" is exactly how a healthy cluster gets destroyed (@security).
    Mapping rule: **only `Unreachable` (container-down or TCP-refused at the real binding — both positive signals) licenses a destroy; everything indeterminate ⇒ `Unknown` → escalate, never destroy.** No kubectl, no kubeconfig, no stderr parsing in the reachability path — so the #2↔#3 collision (a missing host `kind-<name>` context misfiring as unreachable) **cannot occur**. Used by BOTH `cmd_status` (container-side classification) and `cmd_recreate` (the boundary re-confirmation). (`cmd_status` keeps its existing `kubectl get pods` for the *separate* `pods_healthy` field — reachability no longer derives from it.)
    - **Stale-comment trap fixed IN THIS DIFF (@security, fix-don't-defer):** `commands.rs:788-795` (`rewrite_kubeconfig_server`) and `:829-835` (`generate_container_kubeconfig`) both describe a non-existent **two-port** design ("apiServerPort bound to 127.0.0.1" + a separate "extraPortMappings hostPort for containerPort 6443"). Neither exists in the template. Left in place they'd lead the next reader to target 127.0.0.1 → refused-on-healthy → destroy. Corrected to the single `HOST_GATEWAY_IP:HOST_PORT_K8S_API` binding.
    - **Dead allocation removed (@security + @dry-reviewer verified dead repo-wide):** `PortOffsets::K8S_API_HOST` (`ports.rs:46-47`), exported as `HOST_PORT_K8S_API_HOST` (`ports.rs:521-522`, asserted `:838`) but **never referenced by the template** — residue of the defunct two-port design. Remove it + its export + the test ref. Safe: `STRIDE` (`ports.rs:18`) and `base_port` (`:232`) are offset-independent, so removing offset 104 shifts no allocation (@security verified). A one-line note marks offset **104 as RETIRED, not freed for reuse** (@dry-reviewer — the table reads as a stable registry; an unexplained gap invites a future reuse). Adds `ports.rs` to the changeset.
  - **`apiserver_reachable` in `cmd_status`** = that enum, serialized additively as a STRING (`reachable|unreachable|unknown`) in status JSON; client prints `Apiserver reachable: <state>`. The three existing COUPLED lines + NDJSON shape untouched (backward-compatible). **Shell handles all three states in explicit arms** (@code-reviewer — no `[[ != true ]]` that lumps `unknown` with `unreachable`; `unknown` routes to `probe-inconclusive`, never recreate).

  Container-side dispatch (from the FRESH bounded probe — **exhaustive, explicit else**, @observability #2):
  | Probe verdict | `SELF_HEAL CASE=` | `ACTION=` | Then |
  |---|---|---|---|
  | `Apiserver reachable: unreachable` (or `Cluster exists: false`) | `apiserver-unreachable` | `recreate` | `dev-cluster recreate` (helper re-confirms host-side; see below) → re-verify `__wait_cluster_ready` |
  | exists=true ∧ reachable ∧ pods healthy ∧ **setup NOT in progress** | `kubeconfig-stale` | `restore-kubeconfig` | `dev-cluster restore-kubeconfig` (**restore ONLY**) → re-verify |
  | exists=true ∧ reachable ∧ pods NOT healthy | `rollout-wedged` | `none` | `precondition_fail cluster-unhealthy` (existing pod-logs recipe — never recreate a cluster whose apiserver answers) |
  | helper ANSWERED (rc 0) but `Apiserver reachable: unknown`, `Setup in progress: true`, or ANY unmatched combo (explicit else) | `probe-inconclusive` | `none` | `precondition_fail cluster-self-heal-probe-inconclusive` |
  | helper COULDN'T be reached — the `dev-cluster status` call `rc != 0` or the `timeout` wrapper fired (rc 124) | `helper-unreachable` | `none` | `precondition_fail helper-unreachable` (**reuse** the existing REASON — @paired-operations; same host fix: restart via `devloop.sh`) |

  **Split rule (@observability/@paired-operations):** `helper-unreachable` vs `probe-inconclusive` is decided by the **call's termination mode, not prose** — `dev-cluster status` documents (`layer7.sh:~188`) that its rc is 0 whenever the query ran. So the probe **captures rc AND output separately** (it does NOT reuse `__cluster_ready`, which collapses rc into the readiness verdict): `rc != 0` or `timeout`(124) ⇒ `helper-unreachable` (couldn't ask); `rc == 0` but positive tokens unmatchable ⇒ `probe-inconclusive` (answered, unclassifiable). The `status` call is `timeout`-wrapped so a wedged helper can't hang the self-heal — every reached path emits a `SELF_HEAL CASE=` line, which is what makes the absence-rule a clean single-meaning detector.

  - **`dev-cluster recreate` verb (helper — the enforcement point), @team-lead reqs 1-4:**
    1. **Host-side independent re-confirmation:** calls `probe_apiserver_reachable` ITSELF at the moment of action; destroys only if that fresh host-side observation is `Unreachable`. A wrong/stale container-side classification is **insufficient** — if the helper finds the control-plane alive it REFUSES with a distinct result (→ shell escalates `cluster-self-heal-probe-inconclusive`, does not destroy). **A load-bearing comment sits at this call site (@security): this re-confirmation must NOT be "optimized away" as redundant with the shell probe — it is the enforcement of the whole safety argument at the ADR-0030 boundary.**
    2. **Containment invariant (relied-upon; @security):** destroys ONLY `ctx.cluster_name` (per-slug helper-process state); **no client-supplied target arg**. Worst case remains "one devloop destroys its own cluster, never another's." Written into the plan so a future "let recreate take a cluster name" refactor trips a reviewer.
    3. **Evidence bundle, host-side, BEFORE the destroy** (@observability spec; supersedes @security S5): the triggering status JSON verbatim, `kind get clusters`, control-plane container inspect (this IS the S2 second observation — captured before deciding, one impl for both), attempt count + elapsed-from-timeout → `${DEVLOOP_TMP:-/tmp/devloop}/self-heal-evidence-<ts>/` beside `layer-7.log`. **Dir created `0700`, files `0600`** (@security BN1 — same runtime dir as the 0600 kubeconfig, shared `/tmp`). Host-side only, each item bounded-timeout (no hanging `kubectl logs`). The status JSON's raw `pod_error` is kept verbatim for diagnostics; it is kubectl stderr (TLS/connection errors → cert subjects, not secrets — @security BN2 confirmed clean); if any path is ever found that could carry a token/key, **truncate, don't drop**. Surface `EVIDENCE=<path>` on the `SELF_HEAL` line; capture failure ⇒ `EVIDENCE=capture-failed:<reason>` (never silent, never aborts recovery).
    4. **Bounded, keyed to the helper PID so a stale marker can't suppress a legitimate later recreate** (@team-lead Q1 + @security Q1-residual + @observability structural fix): a marker `${DEVLOOP_TMP:-/tmp/devloop}/self-heal-recreate.attempted` that **records the `helper.pid` (`runtime_dir/helper.pid`, `main.rs:55`) live when the recreate was attempted.** On entry, the marker counts ONLY if its recorded PID equals the current `helper.pid`; a different (or absent) PID ⇒ stale ⇒ ignored. This gets both cases right **without any fresh-vs-reattach classification** (which @observability showed is exactly ambiguous for the motivating WSL-crash-then-relaunch): (a) reattach to a *live* helper → same PID → marker valid → within-session suppression holds (the disk-loop case: a cluster that dies again post-recreate is a persistent-host-cause, escalate not re-destroy — @security's Q1 residual, fixed); (b) host crash → helper **necessarily** restarted → new PID → marker ignored → the cluster gets its one legitimate recreate. A host crash cannot leave the same helper PID running, so PID identity IS the "same session" predicate, as a fact not a heuristic (no timestamp threshold to pick wrong). Marker read/write **fails closed** (unwritable/unreadable → do NOT proceed to a second destroy — @paired-operations Gate-3), and is **written BEFORE the recreate, not after** (@observability): write-then-act means an unwritable `${DEVLOOP_TMP}` refuses the recreate and escalates loudly (an unwritable pipeline dir is its own operator-lane problem); act-then-write would let the marker write fail silently and the next attempt destroy again — the exact disk-loop the marker prevents, via the failure path. A one-line comment at the site records the write-before-act constraint so a tidy-up refactor doesn't reverse it. If present-and-current → escalate `cluster-self-heal-failed` `DETAIL=recreate-already-attempted-this-session`. `restore-kubeconfig` (non-destructive) is NOT marker-gated. This drops the earlier "devloop.sh clears it" line entirely (no `devloop.sh` marker-clear needed). §8 documents the PID-staleness rule so a reader hitting the token knows what resets it. Honest bound (§6.7): **≤1 automated recreate per helper lifetime.**
    After a successful recreate the verb's setup rebuilds+deploys current-tree images (the diff's), so no separate `rebuild-all` needed; shell re-verifies with `__wait_cluster_ready`.
  - **`restore-kubeconfig` verb (helper — non-destructive, case B), @team-lead + S3 + S4:** reaches **no** teardown/delete/create path (state it explicitly). Refuses loudly if `!cluster_already_exists` (won't write a kubeconfig for a cluster that isn't there) or if `ports.json` is missing. Derives the apiserver port from the **live cluster** (the persisted `ports.json`/port-map k8s_api port), **never** a fresh `allocate_ports` — a restarted helper re-allocating could hand a different slot and point kubectl at another devloop's apiserver (@security S4). Reuses the shared `generate_container_kubeconfig`, refactored to take the k8s_api port (u16) so setup + restore share one impl.
  - **S3 atomic kubeconfig write** (inside `generate_container_kubeconfig`, so setup + restore both get it): write `kubeconfig.tmp` with `.create_new(true).mode(0o600)` then `fs::rename` over the target — `.mode()` applies only on create, so the current truncate-in-place would PRESERVE a looser existing mode on the restore path and can leave a partial key on a crash (which then reads as broken → chains into recreate). Rename carries the temp file's mode + is atomic.
  - **Never mask / never mislead (SCOPED, @security S1):** a CrashLoop from the diff's own code ⇒ `rollout-wedged` ⇒ escalate, never recreate. The recreate authority rests on a **positive `Unreachable` re-confirmed host-side**, not on the 300s-timeout's ambiguous meaning. Note in the plan (Gate-3): `is_write=true`/the mutex and the audit entry are NOT the safety control — the host-side re-confirmation (req 1) is; the containment invariant (req 2) bounds the blast radius.
  - **Finding 5 (@dry-reviewer) — ONE home per distinct operation, no `__recreate_cluster()`:** v2 proposed extracting a bash `__recreate_cluster()` shared by the infra-change branch and the self-heal. v3's option-(b) verb makes that a *second* home for the teardown→setup sequence (bash + Rust) — worse. Resolution: **do NOT create `__recreate_cluster()`.** The self-heal's `cmd_recreate` (Rust, host-side, atomic under the write mutex) is the sole home of the **guarded** recreate; the pre-existing infra-change branch at `layer7.sh:479-487` is left **as-is** (its original inline `teardown` + `__dev_cluster_setup`). These are **semantically different operations, and unifying them would be a bug:** the infra-change branch is an *unconditional* destroy (the kind blueprint changed → it MUST rebuild a *healthy* cluster), whereas `cmd_recreate` *refuses to destroy a live cluster* (the safety re-confirmation). Routing the infra-change branch through `cmd_recreate` would make it refuse to rebuild after a config change — breaking that feature. So each distinct operation has exactly one home; the two share only a two-line client tail, not an invariant. Bonus: `cmd_recreate` holds the mutex across teardown→setup (race-free), the atomicity @dry-reviewer noted. Self-heal re-verify reuses `__wait_cluster_ready` (no 4th poll loop). **This means I touch neither `layer7.sh:479-487` nor add a bash recreate helper** — smaller diff. **Reciprocal one-line comments (@dry-reviewer): at `layer7.sh:479-487` AND at `cmd_recreate`, recording that the two teardown→setup pairs deliberately do NOT share a helper because their preconditions are INVERSE** (infra-change MUST destroy a healthy cluster; `cmd_recreate` MUST refuse to) — so a future DRY pass doesn't "fix" the duplicate and land the bug where infra-change silently refuses to rebuild after a config change.

**Emitted anchors (verbatim — one-line `SELF_HEAL` tuples per @observability; ALL self-heal lines):**
  - `SELF_HEAL CASE=<apiserver-unreachable|kubeconfig-stale|rollout-wedged|probe-inconclusive|helper-unreachable> ACTION=<recreate|restore-kubeconfig|none>` — the **FINAL 5-value PROBE/CASE enum** (@paired-operations to lock §8 to exactly these). Emitted **once the (timeout-bounded) probe returns a verdict** — NOT "first thing on entry" (the verdict isn't known on entry; @observability/@paired-operations corrected this) — on every reached path incl. the decline paths. **Positive-control property (put verbatim in the §8 `cluster-unhealthy` row, @observability):** `cluster-unhealthy` *with* a preceding `SELF_HEAL CASE=rollout-wedged` = self-heal ran and correctly declined; `cluster-unhealthy` with **no** `SELF_HEAL CASE=` line = self-heal did not run (a regression). Because the probe is `timeout`-bounded and a hang/failure still emits `CASE=helper-unreachable`, "no `SELF_HEAL` line" collapses to a single meaning: the block was bypassed.
  - `SELF_HEAL RESULT=<recovered|failed> ACTION=<recreate|restore-kubeconfig> ATTEMPT=<n>/<max> EVIDENCE=<path|capture-failed:reason|none> [DETAIL=<recreate-failed|restore-failed|recreate-refused-cluster-alive|recreate-already-attempted-this-session>]` — after the fix + re-verify. **`DETAIL=` (NOT `REASON=`)** — @observability: `REASON=` is the closed key of the pipeline's STATUS-line enum (`_common.sh:188` / §3), and putting a second vocabulary under it forces a §3 disclaimer and mixes two enums under one grep. `DETAIL=` keeps `REASON=` meaning exactly one thing. It distinguishes never-attempted (rollout-wedged, no RESULT line) from attempted-and-failed from refused-by-reconfirmation from already-attempted-this-session.
  - New REASON tokens (kebab, via `precondition_fail`): `cluster-self-heal-failed`, `cluster-self-heal-probe-inconclusive`. Bare `cluster-unhealthy` RESERVED for `rollout-wedged`. Escalation `fix` text names probed-state + fix-tried + post-state and says "the automated recreate/restore already ran and failed — inspect `${DEVLOOP_TMP}/self-heal-evidence-*` + `/tmp/devloop/helper.log`, check host disk/resources, escalate to host/operator" — **not** "run teardown+setup."
  - **Audit (@security S6 / @observability):** the `recreate` and `restore-kubeconfig` verbs are audit-distinct by command name via the existing generic `audit_log.log_command` (`main.rs:340`) — no special-case arm added, no bypass.

**Helper changes (`crates/devloop-helper/**`, approved — strictly additive, `auth.rs` untouched):** `protocol.rs` (two variants `Recreate` + `RestoreKubeconfig`, parse arms rejecting a service arg, `name`/`args`/`is_write`/`Display`); `commands.rs` (`probe_apiserver_reachable`, `cmd_recreate`, `cmd_restore_kubeconfig`, `apiserver_reachable` in `cmd_status`, refactor `generate_container_kubeconfig` to take a port + atomic write); `main.rs` (dispatch arms). Both new verbs `is_write=true` (mutex serialization — concurrency, not authority).

**#4 Docs — DIVISION OF LABOR settled: @paired-operations makes the runbook + SKILL edits directly (they own the prose + drafted drop-in content); I take ONLY `scripts/layer-all.sh:25`.** So `docs/runbooks/devloop-validation.md` and `.claude/skills/devloop/SKILL.md` move OUT of my changeset into @paired-operations' (the Classification table keeps them as operations-owned; the "who edits" is now operations, cleaner). Content locked with them:
  - `devloop-validation.md` §8 rows: `FAILURE_TRIAGE` (`layer-all.sh:312`), `FMT_MODE=…SOURCE=…`/`FMT_APPLIED=` (→ §6.2; **`FMT_LANE` phantom** — catalogue the real anchors, note the mis-name), `FAILED_GUARD_NAMES` (`run-guards.sh:293`) — PLUS the **new** self-heal anchors (`SELF_HEAL CASE=…`, `SELF_HEAL RESULT=…`, `cluster-self-heal-failed`, `cluster-self-heal-probe-inconclusive`, `CLUSTER_NAME_TOO_LONG`) so no partial invariant. §6.7 recovery-recipe rows + the drift-note (add the two new REASONs to §3's L7 enum list). Keyed on `DETAIL=` (not `REASON=`) + the 5-value CASE enum.
  - `scripts/layer-all.sh:25` stale debt comment: **I take this edit** (infra file) — replace "owed to Devloop D / tracked in docs/TODO.md" with a pointer to §8. @paired-operations does NOT touch it (avoids a double-edit).
  - (No `devloop.sh` marker-clear — superseded by the PID-keyed marker in #3 req 4, which is self-invalidating on a helper restart. devloop.sh is still touched for #1 only.)

**#5 Escalation routing — @paired-operations edits `.claude/skills/devloop/SKILL.md §Recovery` directly** (their drafted §Recovery drop-in is accepted; they confirm the current anchor since Devloop A recently touched the file). A host-infra precondition the self-heal can't resolve escalates to the **operator/host**, not the `operations` agent; records the dead-cluster recovery path + the `cluster-self-heal-*` tokens.

**Also — TODO.md §2952** "Devloop cluster self-heal after a host crash (2026-09-16)" is the direct predecessor of this whole task (it names the apiserver-unreachable→recreate-once fix, the §6.7 recipe, and the SKILL routing — predating the name-length and kubeconfig findings). Mark **resolved/superseded**, pointing at this devloop output. No split follow-up (all five items land here).

### Tests (@test — hermetic, positive controls; full A1-A6 list adopted)
- **`scripts/setup.test.sh`** (A1/A2) — CLUSTER_NAME **49 ACCEPTED / 50 REJECTED**. Reject: `assert_no_marker` the kind/psql create stub ran (not rc+token alone). Accept: `assert_absent CLUSTER_NAME_TOO_LONG` (positive-control twin). Same for `teardown.sh`.
- **`devloop.sh` launch-time** — slug **41 accepted / 42 rejected**; reject exits non-zero **before** creation — stub create verb marker asserted **absent** (the property, not rc).
- **Rust unit tests** — `error.rs`: `ValidSlug::new` rejects 42, accepts 41. `protocol.rs`: `recreate` + `restore-kubeconfig` parse to their variants, reject a service arg. `commands.rs` **`probe_apiserver_reachable` tri-state (A5 — the safety crux, pin the fail direction; probe is now TCP+inspect, kubeconfig-free):** container-absent/not-running ⇒ `Unreachable`; container-running + TCP-connect refused ⇒ `Unreachable`; container-running + **TCP-connect succeeds ⇒ `Reachable`** (a listening apiserver, whatever HTTP status `/readyz` would give — this is @test's "answered/up ⇒ never teardown", now HTTP-status-agnostic since there's no HTTP call); can't-determine-port / inspect-error / connect-timeout ⇒ `Unknown` (**NOT** `Unreachable`). **Probe-TARGET pin (@security):** a healthy-cluster fixture must yield `Reachable`, so a probe aimed at the wrong address (the defunct 127.0.0.1) reds a test rather than silently destroying clusters in the field (a correct classifier pointed at the wrong place is the vacuity case). `cmd_recreate` re-confirmation: given a `Reachable` re-probe, the verb REFUSES to destroy (returns the refuse result; asserts no teardown). **Marker (PID-keyed):** recorded-PID ≠ current `helper.pid` ⇒ ignored (recreate proceeds); same PID ⇒ suppressed (asserts no second teardown); unreadable marker ⇒ fails closed (no destroy).
- **`scripts/layer7.test.sh`** — extend the frozen fake `dev-cluster` (copy wording **from the real client**, @dry-reviewer/@test-Vacuity-4): add `recreate` + `restore-kubeconfig` verbs (each a distinct marker) and an `Apiserver reachable: <state>` status line honoring `FAKE_APISERVER_REACHABLE` (three-valued); **keep the `*)` catch-all LOUD (exit 97)** (A6); update the COUPLED cross-ref comment both ends for the 4th line. Each case asserts a **`ran.status` (probe-consulted) marker** + the SPECIFIC `SELF_HEAL CASE=` line (answers "probe read status vs assumed"; strongest anti-vacuity control):
  1. `apiserver-unreachable` → `recreate` verb fires **exactly once** (append-counter, assert `==1` like `BUSY_CNT` `:542`, not `≥1`), re-check green → `SELF_HEAL … RESULT=recovered`.
  2. `kubeconfig-stale` — **minimal pair with #1** (identical fixture, flip only `FAKE_APISERVER_REACHABLE`): `restore-kubeconfig` marker **PRESENT** (the positive control that stops "teardown-absent" being vacuous) **AND** recreate/teardown `assert_no_marker` (cluster NOT destroyed). Trigger note (A7): the knob that makes `__wait_cluster_ready` time out while the fresh probe reads green is documented in the test (e.g. gate budget=0 with an initially-not-ready then-green status sequence).
  3. bounded (A4) → recreate re-check fails → escalate loudly: `assert_org_precondition_lane`-style (STATUS=PRECONDITION_FAILURE + `cluster-self-heal-failed` + stderr banner + **no suite ran**), recreate **exactly once**.
  4. `rollout-wedged` → escalates `cluster-unhealthy` with **no** recreate/restore marker (SCOPED), and **assert the `SELF_HEAL CASE=rollout-wedged` line is present TOGETHER with `cluster-unhealthy`** (@observability(b) — pins the decline path that's most likely to lose its emission).
  5. `probe-inconclusive` (reachable=`unknown`) — escalates `cluster-self-heal-probe-inconclusive` and **`assert_no_marker` on BOTH recreate AND restore** (@test: Unknown-but-destroy is the destroy-on-uncertainty path; the third leg of the safety triangle alongside refused-on-Reachable and recreate-on-Unreachable). Plus `helper-unreachable` (status `rc != 0`/timeout → reuses `helper-unreachable` REASON).
  - **Routing / wiring proof (@test new-gap):** case (a) (or a dedicated case) runs **end-to-end through `run_layer7`** — force `__wait_cluster_ready` to actually time out at the `:509` gate so the gate itself invokes `__self_heal_cluster`, then assert the `SELF_HEAL CASE=` line emits. You can't force the gate to time out without routing through it, so this proves the wiring (not just that the function works when called directly) — closes the "wired but never called → green suite over dead code" class. Case (b)'s A7 trigger (gate-timeout-with-green-status) is driven the same end-to-end way.
- All in-loop; none deferred (the false-recreate case #2 is the dangerous one and ships here).

### Confirmations requested (all six confirming → @team-lead "Plan approved")
1. **@security:** S1 tri-state + S2 second-observation now enforced host-side inside `cmd_recreate` (option b); S3 atomic temp+rename 0600; S4 port-from-live-cluster + refuse-if-absent; S6 audit-distinct by verb name; evidence bundle captured before destroy; cross-invocation marker for Q1. Containment invariant written. Good to confirm?
2. **@observability:** one-line `SELF_HEAL CASE=/RESULT=` tuples with `EVIDENCE=`/`REASON=`/`ATTEMPT=`; tri-state `apiserver_reachable`; exhaustive explicit-else dispatch; `helper-unreachable` in the enum + bounded probe call; §8 absence-rule sentence. Confirm?
3. **@paired-operations:** `dev-cluster recreate` + `dev-cluster restore-kubeconfig` are the verb names; the CASE/RESULT/REASON spellings above; Q1 (re-verify → always OK or ESCALATE, no bare-cluster-unhealthy after RESULT=recovered) and Q2 (REASON= on ESCALATE) both answered. Your §6.7 + §Recovery drop-ins accepted (thank you). FAILED_GUARD_NAMES source received.
4. **@dry-reviewer:** Finding 4 (prefix/suffix const SSoT) folded in; #2 recorded as host-`$KUBECONFIG` with repro evidence. Confirm?
5. **@code-reviewer:** setup.sh single-63 SSoT + tri-state robust classification folded in. Confirm?
6. **@test:** A1-A6 + A7 trigger + `ran.status` marker + minimal-pair (a↔b) all adopted. Confirm?

### WAIT — I will NOT implement until @team-lead sends "Plan approved".

---

## Pre-Work

None.

---

## Implementation Summary

TBD

---

## Files Modified

```
TBD
```

---

## Devloop Verification Steps

TBD (Gate 2 — `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, interactive fail-fast)

---

## Code Review Results

TBD (Gate 3)

---

## Accepted Deferrals

- (none yet)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Start commit: `4b0f9bfeaa9dbe8894fe6ffaeb56eb9853eb8d8b`
2. Review: `git diff 4b0f9bf..HEAD`
3. Soft reset: `git reset --soft 4b0f9bf`
4. Hard reset: `git reset --hard 4b0f9bf`
5. No schema/manifest changes — `git reset` is sufficient.

---

## Issues Encountered & Resolutions

TBD

---

## Lessons Learned

TBD
