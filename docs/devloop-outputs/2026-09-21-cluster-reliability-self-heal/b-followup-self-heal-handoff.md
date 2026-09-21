# Handoff → Task (b): self-heal + docs + escalation routing

**Purpose.** This devloop was SPLIT (@team-lead ruling, 2026-09-21). Task **(a)** = #1 name-length + #2 kubeconfig only (see `main.md`). Task **(b)** = #3 self-heal + #4 docs + #5 SKILL routing + the `docs/TODO.md §2952` supersession. This file preserves the **fully Gate-1-reviewed design** for #3/#4/#5 so the (b) `/devloop` starts from it rather than re-deriving. **All six reviewers confirmed this design before the split** — nothing here is unreviewed, but (b)'s own panel should re-confirm since (a) may shift line numbers.

**Status at split:** design locked, all six reviewers confirmed, NOT yet implemented. @paired-operations had **already landed the #4/#5 doc edits in the working tree** (see "Doc edits already landed" below) — (b) must reconcile those.

**Guard-C symbol contract (locked with @paired-operations):** the landed docs cite `scripts/layer7.sh::__self_heal_cluster` and `crates/devloop-helper/src/commands.rs::probe_apiserver_reachable`. (b)'s code MUST define exactly those symbol names or `validate-doc-citations-symbol-resolves.sh` reds at Gate 2.

> **⚠ TOKEN-SPELLING SSoT for (b): `b-followup-ops-docs.patch`, NOT this prose.** The narrative below still shows some mid-review (v2) spellings in places (`SELF_HEAL PROBE=`, `ACTION=refused-by-helper`, `ATTEMPT=1/1`, `[CASE=…]`). The **settled** vocabulary is in the ops doc patch: `SELF_HEAL CASE=` with the **five-value** enum incl. `helper-unreachable`; `DETAIL=` (not `REASON=`/`CASE=`) as the optional sub-key; `ATTEMPT=<n>/<max>`; PID-keyed **write-before-act** recreate marker; the rc/termination-mode `helper-unreachable`-vs-`inconclusive` discriminator (wrap the `dev-cluster status` call in `timeout`; do NOT reuse `__cluster_ready`, which collapses rc). (b): **apply the patch and reconcile against it; do NOT re-derive tokens from this narrative** (that regresses to v2). @observability is sending a corrected token-surface block to fold in here.

---

## #3 Self-heal at `scripts/layer7.sh:509` (sole `cluster-unhealthy` emitter; ONE wiring site)

Replace the bare `precondition_fail cluster-unhealthy` with `__self_heal_cluster`. **The destructive recreate is a distinct host-side helper verb (`dev-cluster recreate`) that RE-CONFIRMS the kill at the ADR-0030 trust boundary — option (b), @team-lead ruling.** The semi-trusted container *classifies + requests*; the privileged helper *independently re-verifies control-plane-dead before it destroys anything*. This makes the S1/S2 safety controls **enforceable**, not advisory, and makes the destroy **audit-distinct by command name** (S6) — no self-attested reason arg.

### Shared probe fn `probe_apiserver_reachable(ctx) -> ApiserverReachability` (helper)
A real Rust `enum ApiserverReachability { Reachable, Unreachable, Unknown }` (@code-reviewer — not `Option<bool>`/stringly-typed; exhaustive match). POSITIVE + **kubeconfig-independent, HTTP-status-agnostic, no foreign error strings** (S1-residual fix — @security's preferred option, converges with S4):
1. `<runtime> inspect -f '{{.State.Running}}' <cluster>-control-plane` (runtime = `ctx.container_runtime.as_str()`) — control-plane **container** running? Definitive, kubectl-free (@security S2 second observation). Not running/absent ⇒ `Unreachable`.
2. if running: bounded **TCP connect** to the apiserver's **actual single binding — `HOST_GATEWAY_IP:HOST_PORT_K8S_API`** (@security's trap catch, verified against `kind-config.yaml.tmpl:136-137`: `apiServerAddress: ${HOST_GATEWAY_IP}` / `apiServerPort: ${HOST_PORT_K8S_API}` — **NO 127.0.0.1 binding and NO 6443 extraPortMapping**; the template's `extraPortMappings` are all 30xxx NodePorts). Host from `ctx.host_gateway_ip` (else `DEFAULT_HOST_GATEWAY_IP`), port = the `k8s_api` from the persisted `ports.json`/port-map (SAME value `commands.rs:861`'s `alloc.port(PortOffsets::K8S_API)` uses — S4-convergent). Connect succeeds ⇒ `Reachable` (listening — whatever `/readyz` would answer; a listening-but-wedged apiserver is `Reachable`, so escalate not destroy — @test-A5). Refused / no route ⇒ `Unreachable` (positive, prose-free).
3. can't determine port (ports.json missing) / inspect errored / connect timed out / unclassified ⇒ `Unknown`. **CRITICAL: an empty/absent port ⇒ `Unknown`, NEVER `Unreachable`** — aiming at a wrong/absent address and reading refusal as "dead" is how a healthy cluster gets destroyed (@security).

Mapping rule: **only `Unreachable` (container-down or TCP-refused at the real binding) licenses a destroy; everything indeterminate ⇒ `Unknown` → escalate, never destroy.** No kubectl, no kubeconfig, no stderr parsing in the reachability path. Used by BOTH `cmd_status` (container-side classification) and `cmd_recreate` (boundary re-confirmation). (`cmd_status` keeps its existing `kubectl get pods` for the *separate* `pods_healthy` field — reachability no longer derives from it.)

**Stale-comment trap fixed — these are (b)'s, NOT (a)'s (verified @dry-reviewer):** the comments at `commands.rs:788-795` (`rewrite_kubeconfig_server`) and `:829-835` (`generate_container_kubeconfig`) describe a non-existent **two-port** design ("apiServerPort bound to 127.0.0.1" + a separate "extraPortMappings hostPort for containerPort 6443"); neither exists in the template, and left in place they'd lead the next reader to target 127.0.0.1 → refused-on-healthy → destroy. Both describe the **container** kubeconfig, whereas (a)'s #2 is the **host** `$KUBECONFIG` in `setup.sh:create_cluster()` — (a) touches neither function, so these ride with (b) alongside the TCP-probe work that gives them a reason. Correct to the single `HOST_GATEWAY_IP:HOST_PORT_K8S_API` binding.

**Dead allocation removed (@security + @dry-reviewer verified dead repo-wide):** `PortOffsets::K8S_API_HOST` (`ports.rs:46-47`), exported as `HOST_PORT_K8S_API_HOST` (`ports.rs:521-522`, asserted `:838`) but **never referenced by the template** — residue of the defunct two-port design. Remove it + its export + the test ref. Safe: `STRIDE` (`ports.rs:18`) and `base_port` (`:232`) are offset-independent, so removing offset 104 shifts no allocation (@security verified). One-line note marks offset **104 RETIRED, not freed for reuse** (@dry-reviewer).

`apiserver_reachable` in `cmd_status` = that enum serialized additively as a STRING (`reachable|unreachable|unknown`) in status JSON; client prints `Apiserver reachable: <state>`. Three existing COUPLED lines + NDJSON shape untouched (backward-compatible). **Shell handles all three states in explicit arms** (@code-reviewer — no `[[ != true ]]` lumping `unknown` with `unreachable`; `unknown` → `probe-inconclusive`, never recreate).

### Container-side dispatch (from the FRESH bounded probe — exhaustive, explicit else)
| Probe verdict | `SELF_HEAL CASE=` | `ACTION=` | Then |
|---|---|---|---|
| `Apiserver reachable: unreachable` (or `Cluster exists: false`) | `apiserver-unreachable` | `recreate` | `dev-cluster recreate` (helper re-confirms host-side) → re-verify `__wait_cluster_ready` |
| exists=true ∧ reachable ∧ pods healthy ∧ **setup NOT in progress** | `kubeconfig-stale` | `restore-kubeconfig` | `dev-cluster restore-kubeconfig` (**restore ONLY**) → re-verify |
| exists=true ∧ reachable ∧ pods NOT healthy | `rollout-wedged` | `none` | `precondition_fail cluster-unhealthy` (existing pod-logs recipe — never recreate a cluster whose apiserver answers) |
| helper ANSWERED (rc 0) but reachable=`unknown`, `Setup in progress: true`, or ANY unmatched combo (explicit else) | `probe-inconclusive` | `none` | `precondition_fail cluster-self-heal-probe-inconclusive` |
| helper COULDN'T be reached — `dev-cluster status` `rc != 0` or `timeout` fired (rc 124) | `helper-unreachable` | `none` | `precondition_fail helper-unreachable` (**reuse** existing REASON) |

**Split rule (helper-unreachable vs probe-inconclusive) by termination mode, not prose:** `dev-cluster status` rc is 0 whenever the query ran (`layer7.sh:~188`). Probe **captures rc AND output separately** (does NOT reuse `__cluster_ready`, which collapses rc into the readiness verdict): `rc != 0`/`timeout`(124) ⇒ `helper-unreachable`; `rc == 0` but positive tokens unmatchable ⇒ `probe-inconclusive`. `status` call is `timeout`-wrapped so a wedged helper can't hang the self-heal — every reached path emits a `SELF_HEAL CASE=` line.

### `dev-cluster recreate` verb (helper — the enforcement point), reqs 1-4
1. **Host-side independent re-confirmation:** calls `probe_apiserver_reachable` ITSELF at the moment of action; destroys only if the fresh host-side observation is `Unreachable`. A wrong/stale container-side classification is **insufficient** — if the helper finds the control-plane alive it REFUSES with a distinct result (→ shell escalates `cluster-self-heal-probe-inconclusive`, does not destroy). **Load-bearing call-site comment: this re-confirmation must NOT be "optimized away" as redundant with the shell probe — it is the enforcement of the whole safety argument at the ADR-0030 boundary.**
2. **Containment invariant (relied-upon):** destroys ONLY `ctx.cluster_name` (per-slug helper-process state); **no client-supplied target arg.** Worst case "one devloop destroys its own cluster, never another's." Comment so a future "let recreate take a cluster name" refactor trips a reviewer.
3. **Evidence bundle, host-side, BEFORE the destroy:** triggering status JSON verbatim, `kind get clusters`, control-plane container inspect (IS the S2 second observation — captured before deciding), attempt count + elapsed-from-timeout → `${DEVLOOP_TMP:-/tmp/devloop}/self-heal-evidence-<ts>/` beside `layer-7.log`. **Dir `0700`, files `0600`.** Host-side only, each item bounded-timeout (no hanging `kubectl logs`). Raw `pod_error` kept verbatim (kubectl stderr → cert subjects not secrets — @security confirmed; if ever secret-bearing, truncate not drop). `EVIDENCE=<path>` on the `SELF_HEAL` line; capture failure ⇒ `EVIDENCE=capture-failed:<reason>` (never silent, never aborts recovery).
4. **Bounded, PID-keyed marker:** `${DEVLOOP_TMP:-/tmp/devloop}/self-heal-recreate.attempted` records the `helper.pid` (`runtime_dir/helper.pid`, `main.rs:55`) live when the recreate was attempted. On entry, counts ONLY if recorded PID == current `helper.pid`; different/absent ⇒ stale ⇒ ignored. Gets both cases right without fresh-vs-reattach classification: (a) reattach to a live helper → same PID → within-session suppression holds (disk-loop → escalate not re-destroy); (b) host crash → helper restarted → new PID → marker ignored → cluster gets its one legitimate recreate. Read/write **fails closed** (unwritable/unreadable → no second destroy) and is **written BEFORE the recreate** (write-then-act; comment the constraint). Present-and-current ⇒ escalate `cluster-self-heal-failed` `DETAIL=recreate-already-attempted-this-session`. `restore-kubeconfig` NOT marker-gated. Honest bound: **≤1 automated recreate per helper lifetime.**

After a successful recreate the verb's setup rebuilds+deploys current-tree images, so no separate `rebuild-all`; shell re-verifies with `__wait_cluster_ready`.

### `restore-kubeconfig` verb (helper — non-destructive, case B), S3 + S4
Reaches **no** teardown/delete/create path (state explicitly). Refuses loudly if `!cluster_already_exists` or `ports.json` missing. Derives the apiserver port from the **live cluster** (persisted `ports.json`/port-map k8s_api), **never** a fresh `allocate_ports` (a restarted helper re-allocating could hand a different slot → kubeconfig pointing at another devloop's apiserver — @security S4). Reuses the shared `generate_container_kubeconfig`, refactored to take the k8s_api port (u16) so setup + restore share one impl.

**S3 atomic kubeconfig write** (inside `generate_container_kubeconfig`): write `kubeconfig.tmp` with `.create_new(true).mode(0o600)` then `fs::rename` — `.mode()` applies only on create, so truncate-in-place would PRESERVE a looser mode on the restore path + can leave a partial key on a crash. Rename carries the temp's mode + is atomic. **NOTE for (b): if (a) already added a setup.sh `write_kubeconfig()` for #2, that is the HOST kubeconfig — distinct from this container kubeconfig atomic write; do not conflate.**

**Finding 5 — ONE home per distinct operation, no `__recreate_cluster()`:** the self-heal's `cmd_recreate` (Rust, host-side, atomic under the write mutex) is the sole home of the **guarded** recreate; the pre-existing infra-change branch at `layer7.sh:479-487` is left **as-is** (unconditional `teardown` + `__dev_cluster_setup`). Semantically different ops — infra-change MUST destroy a healthy cluster (blueprint changed); `cmd_recreate` MUST refuse. Reciprocal one-line comments at BOTH sites recording the inverse preconditions so a future DRY pass doesn't "fix" the non-duplicate.

**Never mask / SCOPED:** a CrashLoop from the diff's own code ⇒ `rollout-wedged` ⇒ escalate, never recreate. Recreate authority rests on a positive `Unreachable` re-confirmed host-side, not the 300s-timeout's ambiguous meaning. **Gate-3 note: `is_write=true`/mutex + audit are NOT the safety control — the host-side re-confirmation is; the containment invariant bounds blast radius.**

### Emitted anchors (verbatim — one-line `SELF_HEAL` tuples)
- `SELF_HEAL CASE=<apiserver-unreachable|kubeconfig-stale|rollout-wedged|probe-inconclusive|helper-unreachable> ACTION=<recreate|restore-kubeconfig|none>` — FINAL 5-value enum. Emitted **once the (timeout-bounded) probe returns a verdict** (NOT first-thing-on-entry), on every reached path incl. decline paths. Positive control: `cluster-unhealthy` *with* a preceding `SELF_HEAL CASE=rollout-wedged` = ran and declined; *with no* `SELF_HEAL CASE=` line = did not run (regression).
- `SELF_HEAL RESULT=<recovered|failed> ACTION=<recreate|restore-kubeconfig> ATTEMPT=<n>/<max> EVIDENCE=<path|capture-failed:reason|none> [DETAIL=<recreate-failed|restore-failed|recreate-refused-cluster-alive|recreate-already-attempted-this-session>]` — after fix + re-verify. **`DETAIL=` NOT `REASON=`** (REASON= is the STATUS-line enum key; keep it single-vocabulary).
- New REASON tokens (via `precondition_fail`): `cluster-self-heal-failed`, `cluster-self-heal-probe-inconclusive`. Bare `cluster-unhealthy` RESERVED for `rollout-wedged`. Escalation `fix` text: "the automated recreate/restore already ran and failed — inspect `${DEVLOOP_TMP}/self-heal-evidence-*` + `/tmp/devloop/helper.log`, check host disk/resources, escalate to host/operator" — NOT "run teardown+setup."
- Audit: `recreate` + `restore-kubeconfig` are audit-distinct by command name via existing generic `audit_log.log_command` (`main.rs:340`) — no special-case arm, no bypass.

### Helper changes (`crates/devloop-helper/**`, additive, `auth.rs` untouched)
`protocol.rs` (two variants `Recreate` + `RestoreKubeconfig`, parse arms rejecting a service arg, `name`/`args`/`is_write`/`Display`); `commands.rs` (`probe_apiserver_reachable`, `cmd_recreate`, `cmd_restore_kubeconfig`, `apiserver_reachable` in `cmd_status`, refactor `generate_container_kubeconfig` to take a port + atomic write); `main.rs` (dispatch arms). Both new verbs `is_write=true` (mutex serialization — concurrency, not authority).

---

## #4 Docs — @paired-operations owns (ALREADY LANDED, see below)
`devloop-validation.md` §3/§6.7/§8/§11 + the `scripts/layer-all.sh:25` comment fix (that one is INFRA-owned; belongs to whoever does the code). §8 rows: `FAILURE_TRIAGE` (`layer-all.sh:312`), `FMT_MODE=…SOURCE=…`/`FMT_APPLIED=` (→ §6.2; `FMT_LANE` is a phantom — catalogue the real anchors), `FAILED_GUARD_NAMES` (`run-guards.sh:293`), PLUS the new self-heal anchors + `CLUSTER_NAME_TOO_LONG` (whose TOKEN emits in (a) but whose §8 catalogue ROW ships HERE in (b) per @team-lead Option A — **NOT** already cataloged; (b) must add it). Keyed on `DETAIL=` + the 5-value CASE enum. §6.7 recovery-recipe rows distinct from `helper-unreachable`; drift-note adds the two new REASONs to §3's L7 enum list.

## #5 Escalation routing — @paired-operations owns (ALREADY LANDED)
`.claude/skills/devloop/SKILL.md §Recovery`: a host-infra precondition the self-heal can't resolve escalates to the **operator/host**, not the `operations` agent; records the dead-cluster recovery path (`dev-cluster recreate`/`restore-kubeconfig`) + the `cluster-self-heal-*` tokens; headless → `.devloop-escalation.json` reason `host-op-needed`.

## TODO.md §2952
"Devloop cluster self-heal after a host crash (2026-09-16)" is the direct predecessor. Mark **resolved/superseded** pointing at the devloop output — do this in (b) (it's the self-heal work). (a) does NOT touch TODO.md.

---

## Doc edits ALREADY LANDED by @paired-operations (in the working tree at split time)
- `docs/runbooks/devloop-validation.md` — §3 (2 new REASONs), §6.7 (rewritten `cluster-unhealthy` + `helper-unreachable` rows, 2 new lane rows, recovery-recipe prose + CASE→ACTION table), §8 (4 self-heal rows keyed on `DETAIL=` + 4 legacy-token rows + the `CLUSTER_NAME_TOO_LONG` row), §11 changelog.
- `.claude/skills/devloop/SKILL.md §Recovery` — the host-infra-escalation subsection.

**✅ RECONCILIATION RESOLVED (@team-lead Option A) — ALL of #4 rides with (b); (a) touches no doc files.** The landed #4/#5 doc edits were `git restore`d out of the tree and preserved in `b-followup-ops-docs.patch`; (b) re-applies them from that patch alongside the self-heal code, so the Guard-C cites (`scripts/layer7.sh::__self_heal_cluster`, `crates/devloop-helper/src/commands.rs::probe_apiserver_reachable`) resolve when they land. **The `CLUSTER_NAME_TOO_LONG` §8 row is part of that set — its TOKEN emits in (a), but its catalogue ROW lands here in (b); (a) authors NO §8 row. (b) MUST add this row (it is NOT already cataloged).** There is no (a)/(b) partition left to decide.

---

## Tests for #3 (full list, @test-confirmed)
- **Rust unit** — `protocol.rs`: `recreate` + `restore-kubeconfig` parse to variants, reject a service arg. `commands.rs` `probe_apiserver_reachable` (TCP+inspect, kubeconfig-free): container-absent/not-running ⇒ `Unreachable`; container-running + TCP-refused ⇒ `Unreachable`; container-running + TCP-connect-succeeds ⇒ `Reachable` (HTTP-status-agnostic); can't-determine-port/inspect-error/connect-timeout ⇒ `Unknown` (NOT `Unreachable`). **Probe-TARGET pin:** a healthy-cluster fixture must yield `Reachable` (aiming at the defunct 127.0.0.1 reds a test). `cmd_recreate` re-confirmation: given a `Reachable` re-probe, REFUSES to destroy (asserts no teardown). **Marker (PID-keyed):** recorded-PID ≠ current `helper.pid` ⇒ ignored (recreate proceeds); same PID ⇒ suppressed (no second teardown); unreadable ⇒ fails closed.
- **`scripts/layer7.test.sh`** — extend the frozen fake `dev-cluster` (copy wording from the real client): add `recreate` + `restore-kubeconfig` verbs (distinct markers) + `Apiserver reachable: <state>` status line honoring `FAKE_APISERVER_REACHABLE` (three-valued); keep `*)` catch-all LOUD (exit 97); update the COUPLED cross-ref comment both ends for the 4th line. Each case asserts a `ran.status` (probe-consulted) marker + the specific `SELF_HEAL CASE=` line:
  1. `apiserver-unreachable` → `recreate` fires **exactly once** (append-counter `==1`), re-check green → `SELF_HEAL RESULT=recovered`.
  2. `kubeconfig-stale` — minimal pair with #1 (flip only `FAKE_APISERVER_REACHABLE`): `restore-kubeconfig` marker PRESENT AND recreate/teardown `assert_no_marker`. A7 trigger: gate budget=0 with initially-not-ready→then-green status.
  3. bounded → recreate re-check fails → escalate loudly (`assert_org_precondition_lane`-style: PRECONDITION_FAILURE + `cluster-self-heal-failed` + stderr banner + no suite ran), recreate exactly once.
  4. `rollout-wedged` → escalates `cluster-unhealthy` with no recreate/restore marker; assert `SELF_HEAL CASE=rollout-wedged` present TOGETHER with `cluster-unhealthy`.
  5. `probe-inconclusive` (reachable=`unknown`) → escalates `cluster-self-heal-probe-inconclusive` and `assert_no_marker` on BOTH recreate AND restore. Plus `helper-unreachable` (rc!=0/timeout).
  - **Routing/wiring proof:** run end-to-end through `run_layer7` — force `__wait_cluster_ready` to time out at `:509` so the gate invokes `__self_heal_cluster`; assert the `SELF_HEAL CASE=` line emits (proves wired-and-called, not just callable).

## Classification (b)-rows
`scripts/layer7.sh` (`__self_heal_cluster` + dispatch), `infra/devloop/dev-cluster` (recreate + restore-kubeconfig client verbs + `Apiserver reachable:` line + COUPLED cross-ref), `crates/devloop-helper/src/{protocol,commands,main,ports}.rs`, `scripts/layer7.test.sh`, `scripts/layer-all.sh:25`, `docs/TODO.md §2952`, and the operations-owned `docs/runbooks/devloop-validation.md` + `.claude/skills/devloop/SKILL.md`. None are GSAs.

## Reviewer confirmations captured at split (all six CONFIRMED the #3/#4/#5 design)
- @security: S1 tri-state, S2 host-side second-observation, S3 atomic 0600, S4 port-from-live-cluster, S5→evidence bundle, S6 audit-by-verb-name, Q1 PID-marker; TCP-target-trap catch + dead-allocation-inert verification; Gate-3 checklist recorded above.
- @observability: one-line tuples, tri-state, exhaustive else, helper-unreachable enum + bounded probe, `DETAIL=` rename, PID-keyed marker, write-before-act ordering, §8 absence-rule.
- @test: A1-A6 + A7 trigger, `ran.status` marker, minimal pairs, probe-target pin, Unknown⇒no-destroy, end-to-end routing proof.
- @code-reviewer: tri-state enum (not Option<bool>), shell 3 explicit arms, exhaustive matches.
- @dry-reviewer: Finding 5 (one home per op, reciprocal comments), dead-allocation deletion verified, offset-104-retired note.
- @paired-operations: verb names, `DETAIL=` grep, 5-value enum, Q1/Q2, doc drop-ins (landed), Guard-C symbol contract.

---

## SETTLED TOKEN SURFACE (authoritative — supersedes any v2 spellings in the narrative above; from @observability, 2026-09-21)

### Greppable tokens (settled)
Two anchors, each a single line carrying a correlated tuple under a stable prefix (one line per event, not one token per fact — §8 rows quote one greppable line and the pairing is the diagnostic fact). In-tree precedent: `FAILURE_TRIAGE LAYER=%d LOG=%s STDERR_LOG=%s RESULT=%s` (`scripts/layer-all.sh:312`).

```
SELF_HEAL CASE=<apiserver-unreachable|kubeconfig-stale|rollout-wedged|probe-inconclusive|helper-unreachable> ACTION=<recreate|restore-kubeconfig|none>
SELF_HEAL RESULT=<recovered|failed> ACTION=<…> ATTEMPT=<n>/<max> EVIDENCE=<path|capture-failed:<reason>> [DETAIL=<recreate-failed|restore-failed|recreate-refused-cluster-alive|recreate-already-attempted-this-session>]
```

- **Emission rule:** the `CASE=` line is emitted **once the probe returns a verdict, on every reached path including no-action ones** — NOT on entry (the verdict isn't known until the probe completes).
- **`DETAIL=`, not `REASON=` and not `CASE=`** as the sub-key. `REASON=` is the pipeline STATUS-line closed vocabulary (`scripts/lang/_common.sh:188`); reusing it mixes two token families under one grep. Reusing `CASE=` would make it mean "which case" on one line and "why the fix failed" on the other.
- **No `CASE=healthy` line** exists (a healthy cluster passes the `:509` gate, never reaches the block).
- **Absence rule (for §8's `cluster-unhealthy` row):** a `cluster-unhealthy` log preceded by `SELF_HEAL CASE=rollout-wedged` = self-heal ran and correctly declined; a `cluster-unhealthy` log with NO `SELF_HEAL CASE=` line = self-heal did not run = a regression. **Sound only if BOTH hold: `helper-unreachable` is in the enum, AND the probe is bounded** — else a hung/failed probe emits nothing and is indistinguishable from "never ran." Treat both as preconditions of the row.

### Probe classification — split on TERMINATION MODE, not content
`scripts/layer7.sh:~188`: `dev-cluster status` rc is 0 whenever the query ran regardless of health, so rc is the transport signal:
- **rc != 0, or `timeout` fired (124)** → query never ran → `CASE=helper-unreachable ACTION=none` → `precondition_fail helper-unreachable`.
- **rc == 0, output present, positive tokens unmatchable** → answered but unclassifiable → `CASE=probe-inconclusive ACTION=none` → `precondition_fail cluster-self-heal-probe-inconclusive`.
- **Wrap the `dev-cluster status` call in `timeout`** (the `/readyz` `--request-timeout=5s` covers only the new positive determination, not this call).
- **Do NOT reuse `__cluster_ready` for classification** (`scripts/layer7.sh:205,216` collapse rc into the readiness verdict — fatal here; it destroys the discriminator). Capture rc + output separately; leave the existing predicate untouched.

### Once-per-recreate marker — PID-keyed, write-before-act
Needed because a Layer-7 `PRECONDITION_FAILURE` is retried once with a fresh in-process budget.
- **Key to `helper.pid`** (`crates/devloop-helper/src/main.rs:55`): the marker counts only if the recorded PID == the current one; differing/absent PID = stale-and-ignored. Reattach to a live helper → same PID → suppression holds; host crash → helper restarts → new PID → marker ignored → the cluster gets its one legitimate recreate (the motivating WSL-crash case, resolved as a fact not a fresh-vs-reattach heuristic).
- **Write the marker BEFORE the recreate.** This ordering IS the fail-closed property: write-then-act means an unwritable `${DEVLOOP_TMP}` refuses the recreate and escalates loudly; act-then-write leaves a failed marker-write free to destroy again (the disk-loop). Site comment required (write-before-act reads as needless and a tidy-up refactor reverses it).
- `${DEVLOOP_TMP}` = `/tmp/devloop` (`scripts/lang/_common.sh:44`), the container side of the **persistent** per-slug host mount — does not clear between runs, which is why the PID key is load-bearing.
- **Drop** the "fallback: rely on the bounded outer retry, document ≤2 recreates" option — it permits two automated destructions where the panel settled on one.

### Reconciliation (settled): refusal carries in `DETAIL=`, not `ACTION=`
Handoff narrative had `ACTION=refused-by-helper`; the settled form is `RESULT=failed ACTION=recreate DETAIL=recreate-refused-cluster-alive`. `ACTION=` enumerates what was *attempted* (a recreate WAS attempted; the helper's host-side gate declined it); refusal is an outcome, so it belongs in `DETAIL=`. The Gate-1 constraint (a refusal must read as a refusal, not a generic failure) is about being *distinguishable*, satisfied by the distinct `DETAIL=` value. **Test case (d) asserts `DETAIL=recreate-refused-cluster-alive`, not an `ACTION=` value.** (b) implementer owns this vocabulary — confirm rather than inherit silently.
