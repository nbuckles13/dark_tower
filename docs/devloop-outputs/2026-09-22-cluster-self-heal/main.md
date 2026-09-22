# Devloop Output: Devloop cluster self-heal (task b)

**Date**: 2026-09-22
**Task**: Task (b) of the split "cluster reliability + self-heal" — #3 three-case self-heal probe + #4 docs (§8 tokens, §6.7 recipe) + #5 SKILL §Recovery routing + TODO §2952 supersession. Implements the fully Gate-1-reviewed design in `docs/devloop-outputs/2026-09-21-cluster-reliability-self-heal/b-followup-self-heal-handoff.md`.
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/devloop-improvements-and-story-2-taskd`
**Duration**: ~1 working session; Gate-3 review extended by the F4 vocabulary oscillation (5 decision reversals, zero partial applications) and a Lead-Gate-2 clippy catch requiring one fix + re-run.

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `8b71901` (task (a) landed) |
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
| Operations | `paired-operations` (paired — owns §6.7/§8 runbook + SKILL §Recovery content) |
| Semantic Guard | `semantic-guard` (SPAWNED — cluster-admin kubeconfig material + evidence bundle + audit logging; credential-leak check surface) |

---

## Task Overview

### Objective
Implement the devloop KIND-cluster **self-heal** — the destructive-recovery half of "cluster comes up reliably or fails loud", split out of task (a) because it hardened into a safety-critical subsystem. The complete, Gate-1-reviewed design is in **`docs/devloop-outputs/2026-09-21-cluster-reliability-self-heal/b-followup-self-heal-handoff.md`** — that is the spec; this devloop implements it and re-confirms (line numbers may have shifted after (a)).

Core pieces (see handoff for full detail):
1. **#3 self-heal** at `scripts/layer7.sh` `cluster-unhealthy`: a host-side probe (control-plane container inspect + bounded TCP to the real apiserver binding) yielding **tri-state** `Reachable|Unreachable|Unknown`; exhaustive case dispatch; a **host-side `dev-cluster recreate` verb** that independently RE-CONFIRMS control-plane-dead before destroying (option B — trust-boundary enforcement) and REFUSES on a live cluster; `restore-kubeconfig` (case B, non-destructive); a pre-teardown **evidence bundle** (host-side, `EVIDENCE=`); a **PID-keyed write-before-act recreate marker** (bounded across the Gate-2 outer retry); S3 (atomic 0600 kubeconfig write) + S4 (port-from-live-cluster) fixes.
2. **#4 docs** (`docs/runbooks/devloop-validation.md`): §8 token rows (settled `SELF_HEAL CASE=/RESULT=`, `DETAIL=` sub-key, five-value enum; the four legacy fmt/diagnostics tokens; `CLUSTER_NAME_TOO_LONG`) + §6.7 recovery recipe.
3. **#5 SKILL** (`.claude/skills/devloop/SKILL.md §Recovery`): host-infra preconditions escalate to operator/host, not the operations reviewer agent.
4. **TODO §2952** ("Devloop cluster self-heal after a host crash") — mark resolved/superseded, pointing at this devloop.

**Already done in (a) — do NOT redo:** #1 name-length (4 sites), #2 kubeconfig-on-reuse, teardown charset-only, the kind-config back-reference NOTEs + single-control-plane count-guard. The dead `K8S_API_HOST` deletion + stale two-port comments in `commands.rs` (deferred to b in (a)'s classification) DO land here (they're #3 territory).

### Scope
- **Service(s)**: none (infra tooling: layer7 self-heal, the host-side `devloop-helper` crate, in-container `dev-cluster` client, runbook, devloop SKILL)
- **Schema**: No
- **Cross-cutting**: Yes — destructive recovery (security safety surface); operations owns the runbook §6.7/§8 + SKILL §Recovery content (paired).

### Debate Decision
NOT NEEDED — design already Gate-1-reviewed (handoff); no new cross-service boundary.

---

## Cross-Boundary Classification

<!-- Implementer finalizes in Planning against the post-(a) tree. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/layer7.sh` | Mine | — |
| `scripts/layer7.test.sh` | Mine — self-heal cases + fake dev-cluster (helper Rust `#[cfg(test)]` lives in the crate rows) | — |
| `scripts/layer-all.sh` | Mine — infra (stale debt comment deletion) | — |
| `infra/devloop/dev-cluster` | Mine | — |
| `crates/devloop-helper/src/commands.rs` | Mine | — |
| `crates/devloop-helper/src/protocol.rs` | Mine | — |
| `crates/devloop-helper/src/main.rs` | Mine | — |
| `crates/devloop-helper/src/ports.rs` | Mine | — |
| `crates/devloop-helper/src/auth.rs` | Mine — P8 delta, atomic secret write for the auth token | — |
| `crates/devloop-helper/src/fs_atomic.rs` | Mine — P8 delta, NEW shared `atomic_write_secret` | — |
| `docs/TODO.md` | Mine — cleanup (§2952 supersession) | — |
| `docs/runbooks/devloop-validation.md` | Not mine, Domain-judgment (§8/§6.7) | operations |
| `.claude/skills/devloop/SKILL.md` | Not mine, Domain-judgment (§Recovery) | operations |
| `docs/devloop-outputs/2026-09-21-cluster-reliability-self-heal/b-followup-self-heal-handoff.md` | Mine — supersession banner (@team-lead directed) | — |
| `docs/devloop-outputs/2026-09-21-cluster-reliability-self-heal/b-followup-ops-docs.patch` | Not mine, Domain-judgment — superseded patch deletion (re-apply trap) | operations |

**GSA check:** none of these match a Guarded Shared Area criterion (the client↔helper unix-socket control protocol is dev-tooling, not production wire-format/auth-routing/forensics/schema). Operations pulled into plan+review via `--paired-with=operations` for the two Domain-judgment doc/SKILL edits. Security is a required reviewer (destructive teardown/recreate = safety surface) but NOT as a GSA co-owner.

---

## Planning

Grounded against the post-(a) tree (HEAD `8b71901`); all self-heal-touched files intact (only kind-config.yaml.tmpl's apiserver binding shifted to :143-144 from (a)'s NOTE block — binding unchanged: `HOST_GATEWAY_IP:HOST_PORT_K8S_API`, no 127.0.0.1/6443). Guard-C symbols: shell `__self_heal_cluster`, Rust `probe_apiserver_reachable`.

### Deltas against the handoff spec (post-(a) RW-mount fact — @team-lead ruled to adopt)

The handoff design predated the finding that `devloop.sh:583` bind-mounts the helper `runtime_dir` into the container **read-write** at the same uid (`--userns=keep-id`, :680), so container `${DEVLOOP_TMP}` == host `ctx.runtime_dir` — 0700/0600 give the container no protection. Two locked-spec controls moved:

- **Delta 1 (BOUNDED):** the once-per-lifetime recreate bound moves from the PID-keyed **file** marker (container-`rm`-able) into helper **process memory** — an `AtomicBool` on `Context`, check-and-set (`compare_exchange`) *after* the host re-confirm returns `Unreachable` and *before* the destroy. Per-process == per-lifetime by construction; outer Gate-2 retry re-enters the same helper (suppressed); host crash restarts it (one legitimate recreate). No PID key, no control-carrying file. An optional on-disk breadcrumb licenses nothing.
- **Delta 2 (SCOPED destroy license):** `reachability_from_signals` — the control-plane container `inspect` is the **sole** positive `Unreachable` (destroy) license. TCP step returns only `Reachable` (connect Ok) or `Unknown` (refused / host-net-unreachable / timeout / would-block / port-absent). Closes the ports.json-in-mount + `DEFAULT_HOST_GATEWAY_IP` fallback attack that could induce `ECONNREFUSED→Unreachable→destroy`. Flips one handoff test row (`container-running + TCP-refused` ⇒ Unknown ⇒ escalate). WSL-crash heal still fires (crashed host ⇒ control-plane container not running ⇒ inspect ⇒ Unreachable).

### Other folded constraints
- **P3** evidence bundle = closed allowlist in code (fresh host status snapshot, `kind get clusters`, control-plane inspect, meta; NO kubeconfig/`kubectl config view`/`kubectl logs`); comment that 0700/0600 protects from other host users, NOT the container; not tamper-evident.
- **P4/S3** atomic secret write: temp in SAME dir, `create_new(true).mode(0o600)` (O_EXCL defeats a symlink swap), flush, `fs::rename`; pre-existing temp fails LOUDLY (unique `<name>.tmp.<pid>`, no silent remove-and-retry).
- **P5** parse arms for both verbs reject ANY non-empty arg (service or skip_observability) with positive unit tests.
- **P6/S4** restore-kubeconfig: port from live ports.json only; refuse loudly (explicit early return) if `!cluster_exists` or ports.json missing/unparseable; never `allocate_ports`.
- **P7** comments: host-side re-confirm makes the *container's assertion* never authority; containment invariant bounds worst case; no over-claim.
- **P8** (ruled): extract ONE `atomic_write_secret` (`fs_atomic.rs`), apply to kubeconfig + `auth.rs::write_token` (both secret); one-line "not secret-bearing" comments at `write_pid_file`/`acquire_startup_lock`.
- **Security pt.1/DRY pt.2** refusal + outcome authoritative in **structured** `CommandResult.data.self_heal` (outcome/attempt/max/evidence-leaf), rendered by the client as `SELF_HEAL HELPER_OUTCOME=… ATTEMPT=… EVIDENCE_LEAF=…` (see Round-3); layer7 keys on the structured field. `RECREATE_REFUSED=` DROPPED. `capture-failed:<reason>` is a **bounded enum**, no `{e}` interpolation.
- **Security pt.3** coreutils `timeout`-wrap every host subprocess in evidence capture AND the probe's `inspect`; layer7 `timeout`-wraps `status`/`recreate`/`restore-kubeconfig`. `timeout` absent ⇒ `capture-failed:timeout-unavailable` / probe⇒Unknown.
- **Observability O2/O3** helper reports the evidence LEAF; shell composes container spelling `${DEVLOOP_TMP}/<leaf>` (topology stays in the shell). **O4** ATTEMPT `<n>/<max>` max from the Rust bound const (SSoT, helper-reported). **O6** every `SELF_HEAL` line emitted BEFORE `precondition_fail` (which exits) — site comment + test pairing.
- **DRY pt.1** generalize `__dev_cluster_setup` → `__dev_cluster_write <verb>` (busy-detect regex in one home); new verbs route through it.
- **DRY/dead-alloc** delete `K8S_API_HOST` const+export+test key + its `ports.rs:46` two-port doc comment; delete the export by hand (`:838` is presence-only).
- **Ops** `layer-all.sh:25-26` comment DELETED outright (rows now exist); `docs/TODO.md §2952` marked resolved → this devloop output. O1 (`/readyz` v2 leftover) + O2 host↔container path prose are @paired-operations' doc fixes (my code has no `/readyz`). Do NOT re-apply `b-followup-ops-docs.patch` — @paired-operations already staged their content (+3 corrections) in the working tree; leave the two doc files alone.

### Round-2 folds (Gate-1 second pass)
- **DRY new finding** `setup_in_progress` (`commands.rs:1042`) is blind to the new verb — during a `recreate` the in-flight op is `"recreate"`, so a status served mid-recreate reports `setup_in_progress=false` and matches dispatch row 1. Fix: `matches!(b.op.as_str(), "setup" | "recreate")` + revise the `:1035-1042` comment.
- **Obs rider 1** anchor the `Apiserver reachable:` value greps (`unreachable` ⊃ `reachable`): `grep -qE 'Apiserver reachable:[[:space:]]+<state>[[:space:]]*$'`, three arms + explicit else.
- **Obs rider 2** the probe's own bound is strictly < the shell `timeout` on `dev-cluster status` (comment both sites); a probe failure degrades to `apiserver_reachable: unknown` with `cmd_status` rc=0 and the three existing lines unchanged (additive field, `__cluster_ready` unaffected).
- **Obs rider 3** fix `__wait_cluster_ready`'s `waited` to real wall-clock via `layer_now` (~3 lines) so the emitted "within Ns" string is true (pre-existing, inside changeset).
- **Obs/ATTEMPT** settled: `n` = destroy attempts CHARGED against the bound at emit time (helper reports `bound.load() as 0|1`); `max` = literal `1` (see Round-4 — NO tunable const). recreated ⇒ 1/1, suppressed ⇒ 1/1, refused ⇒ 0/1; restore reports the actual bound state (no 1/1 filler). Assert `DETAIL=recreate-refused-cluster-alive`↔`ATTEMPT=0/1` and `DETAIL=recreate-already-attempted-this-helper-lifetime`↔`1/1` (guards the compare_exchange ordering).
- **Obs** the client-rendered `SELF_HEAL HELPER_OUTCOME=… ATTEMPT=… EVIDENCE_LEAF=<leaf|capture-failed:…>` transport line needs one §8 row (@paired-operations); shell VALIDATES the leaf (`^self-heal-evidence-…`, no `/`/`..`) before composing `${DEVLOOP_TMP}/<leaf>`, else `capture-failed:malformed-leaf` (see Round-3 for the settled spelling).
- **Security R1** P5 written as allowlist (reject unless ONLY token+command populated — mirror `cancel`'s reject-service+reject-skip_observability), with a comment that any new `Request` field must extend the guard.
- **Security R2** evidence `kubectl get pods` pinned to default table output; comment excludes `-o yaml`/`-o json`/`kubectl describe pod` (env/secret-ref exposure) as part of the closed allowlist.
- **Security non-blocking 2 (folding)** `atomic_write_secret` temp suffix = CSPRNG (via `ring`, already a dep) not `<pid>`, removing the container-precreate self-DoS.
- **DRY** one-line comment that `cmd_recreate` calling `cmd_teardown`/`cmd_setup` directly is correct (they run inside the already-held write slot; routing through the dispatcher would self-`Busy`); reciprocal back-pointer on the container-kubeconfig side to `setup.sh:393-397`'s host `write_kubeconfig` (host vs container kubeconfig — do not collapse).
- **Test A1-A6** add cases for `recreate-already-attempted-this-helper-lifetime` (counter stays ==1) + `restore-failed`; Rust units for S3 (0600 + looser-mode file REPLACED at 0600) and S4 (port-from-ports.json, allocate_ports NOT called); wire every new FAKE_* into BOTH the `env -i` passthrough AND `reset_case` unset, counter file under `$MARKERS`; inspect-ERROR⇒None⇒Unknown unit; real-`timeout`(124) helper-unreachable sub-case; assert full RESULT= line arity.
- **Inspect→Option<bool> mapping** — ⚠️ SUPERSEDED by Round-4 (R3-final two-license). The Round-2 form here mapped `ran non-zero non-timeout ⇒ Some(false) [absent]` — REVERSED in Round-4: absent ⇒ `Unknown` (License 2 via `cluster_already_exists()==Ok(false)`, not inspect exit codes). Use the Round-4 bullet as the authoritative mapping; this line is kept only to record what changed.

### Round-3 folds (Gate-1 third pass — token surface + doc reconciliation SETTLED)
- **Helper transport line RESPELLED (obs RULING 1, dry, code-reviewer):** the client renders `SELF_HEAL HELPER_OUTCOME=<recreated|restored|recreate-refused-cluster-alive|recreate-already-attempted-this-helper-lifetime|recreate-failed|restore-failed> ATTEMPT=<n>/<max> EVIDENCE_LEAF=<leaf|capture-failed:<reason>>` — ONE `SELF_HEAL ` prefix family (so `grep 'SELF_HEAL '` returns the whole story), `EVIDENCE_LEAF=` DISTINCT from the `RESULT=` line's composed `EVIDENCE=` (leaf-vs-path never collide on one key), `ATTEMPT=` identical on both lines. The 4 failure values are BYTE-IDENTICAL to the `DETAIL=` set (frozen in Round-4). Supersedes the `SELF_HEAL_HELPER OUTCOME=` / `EVIDENCE=` spelling at lines 106/119.
- **`RECREATE_REFUSED=` DROPPED entirely (obs RULING 2, dry BLOCKER A):** no `KEY=`-shaped breadcrumb. Any human context at the refusal site goes to host-side `helper.log` as PROSE with no `=`, saying what the re-confirm OBSERVED (container state), not restating the outcome enum.
- **On-disk recreate breadcrumb DROPPED (obs, code-reviewer):** no file at all — the `AtomicBool` is the sole bound. Removes the `.attempted`-file confusion class entirely.
- **THREE `SELF_HEAL` §8 rows (@paired-operations owns):** `CASE=`, `RESULT=`, `HELPER_OUTCOME=` — all operator-visible in `layer-7.stderr.log`. `RECREATE_REFUSED=` gets NO row (dropped).
- **`capture-failed:<reason>` CLOSED enum (final, for §8):** `bundle-dir-unwritable`, `timeout-unavailable`, `no-evidence-line`, `malformed-leaf`, `unrecognized-reason`. Two producers (Rust helper: `bundle-dir-unwritable`/`timeout-unavailable`; shell: `no-evidence-line`/`malformed-leaf`/`unrecognized-reason`). Shell splits on `capture-failed:` BEFORE leaf-regex, validates `<reason>` against the closed set, unknown ⇒ `capture-failed:unrecognized-reason` (closed-by-construction). `ANCHOR (DRY):` comments (colon form) tie Rust site ↔ shell site ↔ §8 row; @paired-operations adds the reciprocal doc→code pointer.
- **Obs: probe-bound arithmetic** — `cmd_status` now nests inspect-`timeout` + TCP-connect-timeout; their SUM (+margin) must be < the shell `timeout` on `dev-cluster status`. Derive/comment the arithmetic at both sites so a healthy-but-slow cluster never trips `helper-unreachable`.
- **DRY: `__dev_cluster_write` must CAPTURE AND PRINT on BOTH attempts** — printing preserves the `setup.sh` `REASON=insufficient-disk` relay (`devloop-validation.md:685`); the capture-global is repopulated on the busy-retry (else a retry-success is classified from the busy attempt's text).
- **code-reviewer:** helper-side outcome tokens are a typed `#[serde(rename_all=…)]` enum (compile-checked, not `json!` string literals); `AtomicBool` bound is a field on the Arc-shared `Context` (never cloned per-connection); reachability match spells out `Some(false)` is the SOLE `Unreachable`.
- **Obs SSoT catch — no tunable `MAX` const:** the `ATTEMPT=<n>/<max>` `max` is emitted as the LITERAL `1` (helper `data.self_heal.max = 1`), NOT a `SELF_HEAL_MAX_RECREATE` const — the `AtomicBool` bound is structurally 1 (a bool permits exactly one transition), so a standalone const would be a decorative second encoding that can drift (set to 2 ⇒ `ATTEMPT=1/2` while the code still permits one). Load-bearing comment at the emit site: "the `AtomicBool` permits exactly one transition; `max` is structurally 1, not a tunable."
- **Test A1b/A2 (load-bearing):** `restore-failed` case (FAKE_RESTORE_RC=1); `atomic_write_secret` units — mode 0600, `create_new`/O_EXCL rejects pre-existing target+symlink, pre-existing temp fails LOUD (assert error, no clobber), rename replaces a looser-mode file at 0600, BOTH callers route through it; S4 unit — restore derives port from ports.json, `allocate_ports` NOT invoked. Evidence-bundle allowlist unit (excludes kubeconfig material) — security's call.
- **Doc reconciliation (dry BLOCKER B / obs) — @paired-operations owns, already folding:** Delta 1 removed the PID-marker file (was described at runbook `:689`/`:708`/`:817`); Delta 2 removed TCP's destroy authority (`:708` "OR apiserver positively unreachable", changelog `:1083`); + the newly-common `probe-inconclusive` sub-case (container up, apiserver crashed inside → `<runtime> logs <cluster>-control-plane`). I leave the two doc files alone; they confirm the reconciliation.

### Round-4 folds (Gate-1 final pass — inspect mapping tightened + vocabulary frozen)
- **R3-FINAL: TWO-LICENSE destroy model — supersedes the Round-2/3 single-license mapping (security R3-final + @team-lead blessed, after test found the daemon-down collision):** `cmd_recreate`'s destroy gate is **TWO-armed** — destroy is licensed iff **(a)** `probe_apiserver_reachable(ctx) == Unreachable` (container stopped) **OR (b)** `cluster_already_exists(&ctx.cluster_name) == Ok(false)` (positively absent). Anything else ⇒ refuse.
  - **License 1 (inspect):** pure `inspect_signal(spawned_ok, exit_code, stdout) -> Option<bool>`: `rc==0 && stdout=="false"`⇒Some(false)⇒Unreachable (SOLE Unreachable source); `rc==0 && stdout=="true"`⇒Some(true)⇒proceed to TCP; EVERYTHING else (spawn-err, timeout 124/137, any non-zero rc **incl. container-absent** Docker-1/Podman-125, rc0+garbage stdout)⇒None⇒Unknown. Drops the `"no such container"`/exit-nonzero⇒Some(false) branch — it needed a foreign runtime error-string match (fails open on a reword) AND conflated container-absent with runtime-broken (Docker exits 1 for BOTH → would destroy on a daemon hiccup; the re-confirm can't catch it — same probe, same daemon, one dependency).
  - **License 2 (`cluster_already_exists() == Ok(false)`):** already-correct at `commands.rs:1353-1372` (rc≠0⇒`Err`, rc0 parsed as positive list membership — no error strings, wedged `kind`⇒`Err`). **The `Err` arm MUST stay distinguishable from `Ok(false)`** (no `unwrap_or(false)` — a refactor collapsing it turns every daemon outage into a destroy license); site comment marks it safety-load-bearing. License 2 is safe though License 1 is strict: an absent cluster has **nothing to destroy** (`kind delete` idempotent, zero blast radius); a stopped container may sit on a recoverable cluster. Comment the two-argument asymmetry.
  - **Why License 2 must be in `cmd_recreate`'s gate, not only the shell dispatch:** the shell's `Cluster exists: false` row routes to `recreate`, but a one-armed (`Unreachable`-only) re-confirm would then REFUSE the absent cluster ⇒ a heal path that always declines. The two-armed gate opens it.
  - WSL heal untouched (kind never `--rm` → crashed host = control-plane container stopped-but-existing = rc0/"false" ⇒ License 1). Tests (@test's final matrix): `inspect_signal` four rows (WSL fixture ⇒ STOPPED rc0/"false"; absent rc≠0 ⇒ Unknown; rc0+garbage ⇒ Unknown; spawn/timeout ⇒ Unknown) **PLUS `cmd_recreate` two-armed gate cases** (the license proof, distinct from the classifier): probe `Unreachable`⇒proceeds; `cluster_already_exists()==Ok(false)`⇒proceeds; `==Err` (daemon down)⇒refuses; probe `Reachable`/`Unknown` with `Ok(true)`⇒refuses. The `Err`⇒refuses case pins the `Err`/`Ok(false)` distinction against an `unwrap_or(false)` collapse. **PLUS a shell `FAKE_CLUSTER_EXISTS=false`⇒recreate⇒recovered case** proving ROUTING (dispatch row 1's `Cluster exists: false` OR-clause reaches the verb) — with a comment scoping it as routing-NOT-license (the fake replays `FAKE_RECREATE_OUTCOME`, doesn't model the gate) and naming the Rust `cmd_recreate` cases as the license proof, so a future reader can't cut the Rust cases as "redundant" with the green shell case.
- **Vocabulary FROZEN (obs refinements 1-3; Gate-3 added the refusal split + a drift DETAIL — see Code Review Results):**
  - `DETAIL=` set (6): `recreate-failed`, `restore-failed`, `recreate-refused-cluster-alive`, `recreate-refused-inconclusive`, `recreate-already-attempted-this-helper-lifetime`, `recreate-unrecognized-outcome`. (`…-this-helper-lifetime` renamed from `…-this-session`; `recreate-refused-inconclusive` added at Gate 3 for the host-couldn't-determine refusal [Security F2, obs ruled the spelling]; `recreate-unrecognized-outcome` is the SHELL-emitted drift detector for an unknown `HELPER_OUTCOME=` [dry-F2] — DETAIL-only, not a helper value.)
  - `HELPER_OUTCOME=` set (7), BYTE-IDENTICAL to `DETAIL=` for failures + the two success values: `recreated`, `restored`, `recreate-refused-cluster-alive`, `recreate-refused-inconclusive`, `recreate-already-attempted-this-helper-lifetime`, `recreate-failed`, `restore-failed` (does NOT include `recreate-unrecognized-outcome` — the helper never emits that; the shell does, when it doesn't recognize the helper's outcome).
  - `capture-failed:` set: `bundle-dir-unwritable` (RENAMED from `bundle-dir` — carries a verdict), `timeout-unavailable`, `no-evidence-line`, `malformed-leaf`, `unrecognized-reason` (framed in §8 as a DRIFT DETECTOR — its presence means the Rust enum and shell allow-list diverged, a defect to fix, not a condition).
- **Security R1 upgrade (compiler-enforced allowlist):** the parse guard exhaustively DESTRUCTURES `Request` — `let Request { token: _, command: _, service, skip_observability } = req;` — then rejects unless both are empty, so a future `Request` field fails to COMPILE at this site until classified. Comment explains the awkward form so a tidy-up doesn't revert it to `.is_none()`.
- **Test A2 (fs_atomic direct units) + evidence allowlist (@team-lead):** direct `fs_atomic` tests for the adversarial paths — `create_new`/O_EXCL rejects a pre-existing target AND a symlink (doesn't follow); a pre-existing TEMP fails LOUDLY (assert error, existing file untouched); both callers route through the primitive. Evidence allowlist test = **set-equality on bundle item NAMES** (not a content grep) + content checks keyed on `client-key-data`/`client-certificate-data`/`-----BEGIN` absent (note `certificate-authority-data` is NOT secret — a CA cert, public).
- **DRY: correct the superseded spelling at plan :106/:119 in place** (done) rather than rely on a supersession note (that control already lost once in this task — the `/readyz` residue propagated out of the handoff prose despite its bold SSoT warning).
- **Evidence capture is best-effort/non-fatal (confirmed to @paired-operations):** a `capture-failed:<reason>` NEVER aborts recreate/restore — recovery proceeds and just annotates the leaf; capture is bounded-timeout, host-side, non-fatal.

---

## Pre-Work

None (task (a) committed at `8b71901`).

---

## Implementation Summary

Implemented the self-heal subsystem per the frozen plan:

- **Helper crate** (`crates/devloop-helper/`): new `fs_atomic.rs` (`atomic_write_secret`: temp+`create_new`(O_EXCL)+rename, CSPRNG suffix); `protocol.rs` gains `Recreate`/`RestoreKubeconfig` variants with the compiler-enforced empty-arg allowlist (exhaustive `Request` destructure); `commands.rs` gains the pure `inspect_signal`/`reachability_from_signals` classifiers + `probe_apiserver_reachable`, the two-armed `cmd_recreate` (host re-confirm via `recreate_licensed`, in-memory `AtomicBool` bound charged after re-confirm/before destroy, pre-destroy closed-allowlist evidence bundle) and non-destructive `cmd_restore_kubeconfig` (port from live `ports.json`, refuse-loudly), the typed `SelfHealOutcome` token enum, `apiserver_reachable` in `cmd_status`, the `setup_in_progress` `"setup"|"recreate"` fix, `generate_container_kubeconfig` refactored to take the port + atomic write, and the stale two-port comments corrected; `auth.rs::write_token` routed through `atomic_write_secret`; `main.rs` dispatch arms + `recreate_bound` field + non-secret-write comments; `ports.rs` dead `K8S_API_HOST` deleted.
- **Client** (`infra/devloop/dev-cluster`): `recreate`/`restore-kubeconfig` verbs + usage/valid-commands; `Apiserver reachable:` status line (coupled cross-ref updated); the `SELF_HEAL HELPER_OUTCOME=… ATTEMPT=… EVIDENCE_LEAF=…` transport line rendered from structured `data.self_heal`.
- **layer7.sh**: `__self_heal_cluster` (Guard-C symbol) at the sole `cluster-unhealthy` site, `__self_heal_recreate`/`__self_heal_restore_kubeconfig`, `__self_heal_compose_evidence` (leaf validation + closed `capture-failed:` enum, ANCHOR (DRY)), the one-home `__dev_cluster_write` (busy-tolerant, capture-and-print both attempts, optional `timeout`), tail-anchored reachability greps, `__wait_cluster_ready` wall-clock fix, the Finding-5 reciprocal non-collapse comment.
- **Tests**: helper Rust units (inspect_signal 4-row + garbage/error rows; reachability cells; two-armed gate incl. `Err`⇒refuses pin; identity-mapping pin; `atomic_write_secret` adversarial paths; S4 port-from-ports.json; parse arms); `layer7.test.sh` SH1-SH10 (apiserver-unreachable/recovered, kubeconfig-stale, recreate-reverify-fail, rollout-wedged, probe-inconclusive, refused, already-attempted, restore-failed, absence-routing, timeout-124 helper-unreachable) with the fake `dev-cluster` extended.
- **Ride-alongs**: `layer-all.sh:25-26` stale comment deleted; `docs/TODO.md §2952` marked resolved. Operations-owned docs (`devloop-validation.md`, `SKILL.md`) left untouched (staged by @paired-operations).

Local validation: `cargo test -p devloop-helper` (all pass, incl. new), clippy + fmt clean; `bash scripts/layer7.test.sh` → 241 passed / 0 failed; Guard-C symbol-resolution guard OK.

---

## Files Modified

```
crates/devloop-helper/src/fs_atomic.rs        (NEW)
crates/devloop-helper/src/commands.rs
crates/devloop-helper/src/protocol.rs
crates/devloop-helper/src/main.rs
crates/devloop-helper/src/ports.rs
crates/devloop-helper/src/auth.rs
infra/devloop/dev-cluster
scripts/layer7.sh
scripts/layer7.test.sh
scripts/layer-all.sh                          (stale comment)
docs/TODO.md                                  (§2952 supersession)
docs/devloop-outputs/2026-09-21-.../b-followup-self-heal-handoff.md  (supersession banner)
docs/devloop-outputs/2026-09-21-.../b-followup-ops-docs.patch        (DELETED — superseded re-apply trap; @paired-operations git rm; content in git 8b71901 + the two staged doc files)
```

Operations-owned, untouched by me (staged by @paired-operations): `docs/runbooks/devloop-validation.md`, `.claude/skills/devloop/SKILL.md`.

---

## Devloop Verification Steps

**Gate-2 GREEN — Lead-verified from real runs** (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, Lead-owned per ADR-0030 implementer-can't-self-validate):

| Layer | Result | Notes |
|-------|--------|-------|
| L1 compile | OK | cargo (workspace + dt-guard + dt-story) / buf / nx-typecheck |
| L2 fmt | OK | cargo-fmt / buf-format / nx-format |
| L3 guards | OK | guards-passed + all shell self-tests: `layer7.test.sh` **241/0**, `setup.test.sh` 92/0, `layer-all.test.sh` 84/0, `run-story` 453/0; cross-boundary scope + classification clean; Guard-C doc-citation symbols resolve |
| L4 test | N/A-over-green | `devloop-helper` **394/0** + all crates 0-failed (the `layer4-summary N/A` is a per-lang aggregation label; the Rust lane ran green) |
| L5 lint | OK | cargo clippy + nx-lint |
| L6 audit | N/A | SKIPPED-NO-DIFF no-dep-changes — no dependencies added |
| L7 env-tests + browser-E2E | **OK** | clean-cluster run: env-tests passed + browser-e2e **10/10**; `LAYER=7 RESULT=OK DURATION=1390s` |

**L7 history (honest record):** two earlier L7 attempts went red on `cluster-setup-failed`/`AlreadyExists` — an ENVIRONMENT issue, NOT the diff: `setup.sh` reuses an existing Calico-laden cluster and its non-idempotent `kubectl create` fails `AlreadyExists`, compounded by shared-helper concurrency from parallel runs (my own interrupted standalone `layer7.sh` runs — a validation mistake, since the sandbox has a live helper). A full teardown → fresh setup passed L7 cleanly. Task (b) touches no `setup.sh`/Calico. The pipeline correctly classified the reds as operator-lane ("environment problem, not a code regression").

**Gate-2 RE-RUN after Gate-3 edits (Lead-owned):** the Gate-3 fixes invalidated the first Gate-2 pass. The Lead re-run caught a **real L5 clippy failure** my `--all-targets` clippy had masked — `EVIDENCE_BUNDLE_ITEMS` (the B2 test aggregate) was dead in the BIN build (production writes via the individual `EV_*` consts, not the slice). FIXED with `#[cfg(test)]` on the aggregate (NOT `#[allow(dead_code)]`); the drift guarantee is preserved (the `EV_*` consts are the production SSoT the test reads via the aggregate). Verified locally: `cargo clippy -p devloop-helper -- -D warnings` RC=0, `bash scripts/layer5.sh` `STATUS=OK layer5-summary`, helper 184/0, fmt clean. Awaiting the Lead's full Gate-2 re-run for the authoritative pass.

---

## Code Review Results (Gate 3)

- **@observability** — verified code (not plan): completeness code→row for all CASE/DETAIL/HELPER_OUTCOME/capture-failed anchors, capture-failed reachability + split-before-validate ordering, three-way `ANCHOR (DRY):`, literal `max=1`, both absence-rule preconditions, probe-bound inequality, emission ordering, anchored greps. **One minor finding (FIXED):** `layer7.test.sh` SH7's ATTEMPT↔DETAIL pairing was two separate assertions (didn't prove co-location); made it one combined needle symmetric with SH6 (`ATTEMPT=1/1 EVIDENCE=none DETAIL=recreate-already-attempted-this-helper-lifetime`). Hermetic re-run 241/0.
- **@security** — destructive path built as specified; `fs_atomic.rs` better than asked (symlink-at-temp-path test). Two findings, both FIXED:
  - **F1** evidence `control-plane-inspect.txt` was an unbounded full `inspect` object dump inside the closed allowlist (no live leak today, but auto-captures anything later landing in the kind node's env/mounts). Fixed to a bounded `-f` format (`State.Status/Running/ExitCode/OOMKilled/StartedAt/FinishedAt`) with a "do NOT restore the wildcard" comment.
  - **F2** `recreate-refused-cluster-alive` fired for EVERY unlicensed refusal incl. `Unknown`/`Err` (runtime down), asserting "cluster ALIVE, check kubeconfig" — wrong + misrouting in the most likely refusal. Split: `reach == Reachable` ⇒ `recreate-refused-cluster-alive` (kept); else ⇒ new `recreate-refused-inconclusive` (obs ruled the spelling — reuses the subsystem's could-not-determine word) with fix text → host container runtime + helper.log, NOT kubeconfig. Both still fail closed → `cluster-self-heal-probe-inconclusive`.
- **@observability F3 + @paired-operations** — the KEPT `recreate-refused-cluster-alive` arm ALSO blamed container-visibility/kubeconfig, but reachability is host-side container-inspect (kubeconfig-independent) and the same probe runs at classify + re-confirm — so a refusal is the helper's own probe DISAGREEING WITH ITSELF (a flap/race), not container-visibility. FIXED: reworded to the race, drop the `restore-kubeconfig` recommendation, point at `dev-cluster status` re-run + helper.log. (obs's separate kubeconfig-stale-naming question answered — see reviewer thread.)
- **@dry-reviewer** — 5 findings, all FIXED: F1 (Finding-5 reciprocal back-pointer `cmd_recreate`→infra-change branch); F2 (unknown `HELPER_OUTCOME=` was silently `recreate-failed` → now a loud drift arm `DETAIL=recreate-unrecognized-outcome`, symmetric with `capture-failed:unrecognized-reason`); F3 (`fs_atomic.rs` false-complete enumeration → dropped the list, per-site "not secret-bearing" notes at kind-config + write_port_map_shell); F4 (identity test renamed + made exhaustive-by-construction w/ set-equality — see below); F5 (`900` default single-sourced to `SELF_HEAL_WRITE_TIMEOUT_DEFAULT`).
- **@code-reviewer** — FIXED: `cmd_recreate` hardcoded `skip_observability=false`, ignoring the persisted `observability_deployed` SSoT → now reads it from live ports.json before teardown, honoring the original opt-out. (Bound-coverage note deferred to @test F1 = B1.)
- **@test** — 2 findings FIXED: F1/B1 the AtomicBool once-per-lifetime bound was behaviorally untested → extracted pure `decide_recreate(reach, exists, bound) -> RecreateDecision`; two hermetic tests pin (a) once-per-lifetime (2nd ⇒ AlreadyAttempted) and (b) refusal-doesn't-charge / charge-after-reconfirm. F2/B2 evidence-bundle allowlist test → `EVIDENCE_BUNDLE_ITEMS` named const (one source, both capture + test), set-equality + credential-marker (no client-key/cert-data/PEM; not certificate-authority-data).
- **A3 (identity test)** — `test_self_heal_outcome_failure_tokens_match_detail_set` → renamed `test_self_heal_outcome_wire_tokens_exhaustive`: an exhaustive `wire()` match compile-forces a token for any NEW variant (not a vacuity-prone literal list), serde↔hand-map agreement, + set-equality against the frozen 7-member `HELPER_OUTCOME=` set.
- **@security F1** (evidence inspect unbounded, above) FIXED. Validation: clippy + fmt clean; devloop-helper 184/0; hermetic `layer7.test.sh` 249/0 (SH6b inconclusive, SH6c drift). New DETAIL tokens looped to @observability (vocab bless) + @paired-operations (§8/§6.7).
- **@observability F5** (F3 half-applied) FIXED: the `RecreateRefusedClusterAlive` **enum doc comment** (the definition site) still said "kubeconfig problem"; corrected to the host-side-observations-disagree/flap framing with an explicit negation. **@dry nit** FIXED: normalized the classification-comment case so `grep "not secret-bearing"` returns all 5 classified write sites. `test_self_heal_outcome_wire_tokens_exhaustive` confirmed correct as-is (pins the 7 `HELPER_OUTCOME=` enum variants; `recreate-unrecognized-outcome` is shell-only, correctly not a variant).
- **@observability F4** (`CASE=kubeconfig-stale` names a cause the host-side detection path can't observe — it fires on a ready-late race, not a stale container kubeconfig) — **RESOLVED-DEFERRED (docs-only)**. Landed: an option-neutral dispatch-site comment recording what the arm actually detects + `restore-kubeconfig` = prophylactic refresh; a `docs/TODO.md` deferral entry; §6.7 reframe (@paired-operations). The token is NOT renamed: `CASE=` is a **Gate-1-frozen** enum, so renaming it at Gate 3 is a panel/lead decision, and under conflicting [FINAL] rulings (Lead A / owner B) I defaulted to the conservative non-scope-expanding option (matches @observability's latest, RESOLVED-DEFERRED). The rename to `ready-late` is tracked for when the CASE enum is next touched. `ACTION=restore-kubeconfig` and safety behavior unchanged.
- **Final vocab:** `CASE=` 5 (unchanged: apiserver-unreachable, kubeconfig-stale, rollout-wedged, probe-inconclusive, helper-unreachable), `DETAIL=` 6, `HELPER_OUTCOME=` 7 (⊂ DETAIL for failures), `capture-failed:` 5.

---

## Accepted Deferrals

- **`CASE=kubeconfig-stale` → `ready-late` rename** (@observability F4, RESOLVED-DEFERRED). The self-heal arm is named for a stale-container-kubeconfig cause its host-side-only detection path cannot observe (it fires on a ready-late race). Deferred because `CASE=` is a Gate-1-frozen enum — renaming a frozen value at Gate 3 is a panel decision, not a unilateral one. The operator-facing misstatement is removed WITHOUT the rename (dispatch-site comment + §6.7 reframe), so what remains is a legibility wart, not a defect. Tracked in `docs/TODO.md` (§Rename `CASE=kubeconfig-stale` → `ready-late`); rename when the CASE enum is next touched. **Burden of proof met:** no correctness/safety impact; `ACTION=restore-kubeconfig` behavior unchanged; the follow-up is scoped + zero-grep-guarded.

---

## Rollback Procedure

1. Start commit: `8b71901`
2. Review: `git diff 8b71901..HEAD`
3. Soft/hard reset to `8b71901`.
4. No schema/manifest changes.

---

## Issues Encountered & Resolutions

- **RW-mount premise break (planning).** Security found the container↔helper `${DEVLOOP_TMP}` mount is read-write at the same uid, invalidating two self-heal controls. Resolved via Delta 1 (bound moved from an on-disk marker to an in-helper-process `AtomicBool`) and Delta 2 (destroy license taken off `ports.json`/gateway-IP defaults reachable from that mount) — the destroy now rests only on observations the container cannot forge.
- **Destroy-license daemon-down collision (Gate-1).** Test found Docker exits 1 for both container-absent and daemon-down, so an exit-code-based license would destroy on a daemon hiccup, and `cmd_recreate`'s host re-confirm shares the daemon with the shell probe (one observation twice, not defence-in-depth). Resolved with the two-positive-license model (inspect `rc0/"false"` OR `cluster_already_exists()==Ok(false)`; everything else ⇒ Unknown ⇒ refuse; `Err` kept distinct from `Ok(false)`).
- **"Prose asserts what the code establishes" defect class (Gate-3).** Six instances in one diff — every one an emitted string/comment asserting a causal story the code never made (`/readyz`, "within Ns", `-this-session`, the `max` const, the `recreate-refused-cluster-alive` container-visibility misattribution, and `CASE=kubeconfig-stale` naming an unobservable cause), all beside correct logic. Surfaced almost entirely by the question "who actually made this observation?" (every cluster observation here is host-side). Fixed in-diff except the frozen-CASE rename (deferred).
- **F4 five-reversal oscillation (Gate-3).** The `kubeconfig-stale`→`ready-late` A/B decision reversed five times between Lead and the vocabulary owner, driven by crossing async messages and each party treating its own state as the decision's state. Resolved on the governance principle that `CASE=` is a Gate-1-frozen enum (panel-owned; a Gate-3 rename is a panel follow-up, not a unilateral call) → docs-only fix + tracked TODO. Cost zero partial applications only because the implementer and paired-operations never acted on an unsettled ruling.
- **L7 environment flakes (Gate-2).** `setup.sh` non-idempotently `kubectl create`s Calico on cluster reuse (AlreadyExists), compounded by shared-helper concurrency when multiple agents self-ran the pipeline. Resolved for this loop by full teardown → fresh setup (L7 then passed 10/10); the underlying `setup.sh` idempotency and helper-serialization gaps are follow-ups.
- **L5 dead-const, caught by the Lead-owned Gate-2 re-run.** The B2 evidence-allowlist const was referenced only under `#[cfg(test)]`, so the bin-target clippy (`-D warnings`) failed on an unused const — invisible to the implementer's `--all-targets` clippy and to the hermetic test run. Fixed by `#[cfg(test)]`-gating the const (not `#[allow(dead_code)]`), preserving the real-capture drift guard; re-run went fully green.

---

## Lessons Learned

- **Look at the thing, not the account of the thing.** Line numbers, teammate summaries, and self-reported check results all go stale or mislead; nearly every real catch this loop (the `/readyz` prose, the `-this-session` token, the container-visibility misattribution, the §2952 "duplicate" that wasn't, the L5 dead const) came from reading the actual file/emitter rather than its description.
- **Gate-2 is the Lead's to run, and never on report.** The Lead initially accepted a subagent's *narration* of a phantom pipeline for ~2h; the two real Gate-2 failures (the phantom itself, then the L5 clippy dead-const) were only found by the Lead running `layer-all.sh` in the real environment. Per ADR-0030 the implementer can't self-validate; a narrated pass is not a pass.
- **The coordination control has two halves that only work together.** Lead holds its ruling until the decision owner signals convergence; the owner marks a ruling `[FINAL]` only *after* the decider has gone silent. Either alone reproduces the F4 thrash (Lead ruling on intermediate positions; owner marking its own convergence FINAL while rulings still arrive).
- **Executor discipline is the backstop when the decision layer thrashes.** Five F4 reversals reached the tree as zero partial applications solely because the implementer and paired-operations refused to act on any unsettled ruling — including the implementer correctly declining a frozen-enum rename from the vocabulary owner. Name it as a practice to keep, not slowness.
- **Name the *kind* of objection.** An owner's "I can't do this unilaterally" (authority/standing) looks identical to "this is wrong" (merit) in a one-line message; conflating them cost three extra exchanges. Distinguish authority objections from merit objections explicitly.
- **A deferral reported as "tracked" is not tracked until it is filed.** Close-out surfaced a deferral all three reviewers cited as tracked-elsewhere with no TODO entry, and a duplicate deferral entry with conflicting owners — both would have shipped silently. Verify the file at verdict time.
- **A guard/checklist for the recurring defect class is the highest-value follow-up** — "does every operator-facing string assert only what the code established?" plus the emitted-token↔§8-row catalogue guard (filed).
