# Devloop Output: Speed up scripts/setup.test.sh (Layer 3 back near ~90s)

**Date**: 2026-09-30
**Task**: From docs/TODO.md — `scripts/setup.test.sh` takes ~7 minutes and makes Layer 3 ~6.5 minutes
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/test-speed-up`
**Duration**: ~4h (16:20–20:30), 2 review iterations; Gate 2 layer-all 1432s (L7 933s)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `b9b0693b8cab52c183848a066cf9a7f615dbb802` |
| Branch | `feature/test-speed-up` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `2` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned (no check surface: test-suite + shell script perf)` |

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
Bring Layer 3 back near its pre-ADR-0038-step-3 cost by removing the process-spawn blow-up in `scripts/setup.test.sh` (docs/TODO.md entry "`scripts/setup.test.sh` takes ~7 minutes…"), without dropping any assertion.

### Scope
- **Service(s)**: none (Kind scripts + their hermetic self-test)
- **Schema**: No
- **Cross-cutting**: No (infrastructure-owned scripts and test only)

### Debate Decision
NOT NEEDED - test-suite redesign + script performance within one owner's files; no contract change.

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
| `scripts/setup.test.sh` | Mine | — |
| `infra/kind/scripts/provision.sh` | Mine | — |
| `infra/kind/scripts/lib/common.sh` | Mine | — |
| `infra/kind/scripts/deploy.sh` | Mine | — |
| `infra/kind/scripts/teardown.sh` | Mine | — |
| `infra/devloop/devloop.sh` (Gate-3 CQ F2 + Ops iteration 2: eager-setup existence check → `cluster_exists`; orphan scan no longer reads a failed listing as "no orphans"; not a blueprint input) | Mine | — |
| `docs/TODO.md` | Mine | — |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |
| `docs/devloop-outputs/2026-09-30-setup-test-speedup/main.md` | Mine | — |

---

## Planning

### Measurement first (this container, i9-14900KF, 32 cores, idle)

Baseline, each Layer-3 self-test run alone from the repo root (`dt-guard`/`dt-story` not built, so `guards` and three guard self-tests fail fast and are under-counted):

| Script | Baseline |
|---|---|
| `setup.test.sh` | **66.8s** |
| `layer7.test.sh` | 34.4s |
| `validate-frame-vectors.test.sh` | 27.5s |
| `lang/ts/fmt.test.sh` | 15.6s |
| `dev-web.test.sh` | 6.6s |
| everything else | < 3.1s each |

`setup.test.sh` by section (instrumented copy, `$EPOCHREALTIME` at every `(X)` header):

| Section | Time | What it spends it on |
|---|---|---|
| (D8) deploy.sh `main` | **36.5s** | 19 end-to-end `main` runs × ~13 REAL `kubectl kustomize` renders of the SAME env root (0.18s each) — `first_party_repos` ×2, `env_root_workloads <repo>` per repo in `deployed_refs` and in the prune, `wait_for_env_root`, `render_env_overlay` |
| (P6c) provision classification | 10.4s | e2e runs + fixed `sleep 1` after each of 4 python listeners + 3 × 1s probe timeouts |
| (D6) ref resolution | 4.4s | same repeated root render (5 per call) |
| (D) header render/D1–D3 | 3.9s | real renders (irreducible; each asserts on a different render) |
| (P10) tty `N` | 2.1s | `script` + a setup provision |
| all other (P) | ~4s | ~75 e2e `provision.sh` runs at ~0.1s each + ~30 `make_ptree` copies |

**The TODO's profile (463s, group (P) 394s, ~3.5s per provision run) does not reproduce here: one stubbed provision run is 0.1s.** A ~35× gap on a fork-bound workload means the profiling host had far slower process creation (load, nested runtime). The mechanism is the same in both — cost ∝ processes spawned — so the fix is too; but on THIS host the largest term is (D8)'s repeated kustomize render, not (P). I fix both. @team-lead: if you know the conditions of the 463s measurement, I'll re-measure there; my before/after will be from this host.

**Environment-independent metric (Lead ask).** No strace in the container, so process creation is counted from the kernel's `processes` counter in `/proc/stat` (all forks/clones system-wide during the run, idle host; kubectl's Go threads count too). Baseline `setup.test.sh`: **50,661 / 53,723 forks, 61.2s / 61.2s** (two runs, 576 passed). Executed-label baseline: 529 traced `assert_*` labels (+ direct `PASS=` sites) — `labels.before`.

### Mechanism restated

The class is not "provision.sh is slow": **a test (or a converge) re-derives the same expensive thing per call instead of once** — the whole provision pipeline per assertion group in (P), the same root render per repo in deploy.sh (also in production: ~13 renders ≈ 2.3s per real deploy), a `date` fork per log line, a `sha256sum` pipeline per input, a `grep` per existence check. Fixing it completely means both scripts and both test groups.

### Changes

**1. Group (P) redesign (setup.test.sh).** New helper `psrc '<snippet>'` runs a snippet in a subshell with the tree copy's `provision.sh` SOURCED (its existing `BASH_SOURCE==$0` guard already suppresses `main`; no new seam). Failure-path direct calls run under the production mechanism itself — `trap __provision_exit_trap EXIT; pstep <fn>` — so the STEP/REASON line comes from the real trap, not a re-implementation. `make_ptree` builds a pristine template ONCE and each reset is one `cp -a`. Seeding "a cluster with a recorded blueprint" is a direct `blueprint_decide; record_blueprint` against the stubs instead of a whole `provision --yes`.

End-to-end runs are kept exactly where the behaviour IS `main`'s orchestration (ordering, decision→action mapping, trap + one-line invariant, mode dispatch, tty detection): fresh build, no-op, `--blueprint` purity, one sensitivity rebuild (kind-config, named cluster — also the exact-name destroy), 4 stage failures + 1 recovery, mid-build edit, unreadable refuse, unnamed-destroy refused, unnamed create, TCP port-held (ordering: no create), runtime-incapable one-line, prerequisite-missing ×2, tty `N`, `--check` ×3 + renewal ×2. **~23 e2e runs (from ~75).** Everything else becomes a direct call:

| Old case (P) | New form |
|---|---|
| P1 fresh, P2 no-op | e2e, unchanged assertions |
| P3 determinism / lists-every-input / no key / no input lines | direct `blueprint_render` ×2 — **stdout only**. The old case compared `2>&1` output including a timestamped `detect_container_runtime` log line: it flakes whenever the two runs straddle a second (it failed once in my instrumented run). |
| P3 sensitivity ×5 + deploy-only-edit no-op | direct `blueprint_decide` against a seeded record → `BP_REASON`/`BP_CHANGED` (exactly that section); mutation reverted by restoring the file rather than re-provisioning. kind-config case stays e2e (rebuild + delete action). provider via real `detect_container_runtime` with only `docker` on PATH |
| P4 `--blueprint` pure | e2e |
| P5 stage failure ×4 (reached, classified line, one line, no record) | e2e ×4, unchanged; + `ran.readyz` asserted on the post-create stages / absent on `create` (folds P6c's pre-create case, run with `STUB_READYZ=fail`) |
| P5 next-run-sees-missing ×4 / recovers ×4 | direct `blueprint_decide` → `missing` ×4; e2e recovery for `record` (cluster exists, no record → delete + rebuild, rc 0); state after calico/secret/record failures asserted IDENTICAL (so one recovery covers all three); `create` recovery ≡ P1 fresh (no cluster) |
| P6 mid-build edit | e2e |
| P6b Calico tampered / verified / https-only / pin shape | direct `pstep install_calico` (with the trap) |
| P6c post-create apiserver, runtime-unreachable, hang ×2 | direct `trap …; PROVISION_CLUSTER_CREATED=…; pstep <failing fn>` — the real trap and the real shared classifier |
| P6c unreadable-classified + P7 unreadable | ONE e2e (was two identical runs) |
| P6c unnamed operator-declined + P9 unnamed refused | ONE e2e (was two identical runs) |
| P6c TCP port-held | e2e (never creates); listener binds port 0 itself and reports it (no pick-then-rebind race, no fixed `sleep 1` — poll for readiness) |
| P6c port-free control, UDP held / wildcard / other-address | direct `pstep check_host_ports` |
| udp_port_bound tables ×4 | direct (unchanged) |
| runtime-incapable classified + one line + probe shape | e2e |
| node probe passes / no image / hang / success-never-probes | direct `pstep create_cluster` under the trap |
| prerequisite-missing, no-timeout | e2e (they fail at step 1, cheap) |
| P7 kind-list-fail unreadable | direct `blueprint_decide` |
| P8 no record rebuilds + deletes | folded into the P5 recovery e2e; garbage record → direct decide |
| P9 exact-name destroy + once | folded into the kind-config sensitivity e2e (named cluster) |
| P10 tty `N` | e2e via `script` (seeded directly) |
| P11 renewal --check / --yes | e2e ×2 (seeded directly) |
| P12 --check missing / stale / match | e2e ×3 (seeded directly) |
| P13, P14 | unchanged (static / recipe) |

The full old-name → new-name map goes in Implementation Summary; every existing assertion NAME is kept (so `grep` of the old list against the new output proves nothing dropped — I'll run exactly that diff and record it).

**2. `_ts` without a fork (lib/common.sh).** The log functions format the time with bash's builtin `printf -v ts '%(%H:%M:%S)T' -1` (no `$(…)` subshell, no `date`). Output byte-identical. Needs bash ≥ 4.2; deploy.sh already requires ≥ 4 (associative arrays).

**3. One `sha256sum` for the blueprint (provision.sh).** `blueprint_render` hashes kind-config + the 4 PROVISION_INPUTS + present TLS certs in ONE `sha256sum -- …` call, results taken by position. **The render stays byte-identical** (same labels, same values) — no format change, `BLUEPRINT_FORMAT` stays `dt-blueprint v1`; I'll prove it by diffing old vs new `--blueprint` output with provision.sh/common.sh held equal. Fail-loud improvement: today `$(sha256_of missing)` inside `echo` renders an EMPTY hash silently (the failed substitution doesn't trip `set -e`); the single call fails the render if any input can't be hashed. `cluster_exists` (common.sh) matches the listing with a bash pattern instead of a `grep -qx` fork.

**4. deploy.sh: render the env root once per converge.** New `cache_renders` step at the top of `main` renders `ENV_ROOT` and `MIGRATE_BASE` once; `root_repos` / `first_party_repos_of` / `env_root_workloads` read the cached render. They FAIL LOUDLY if it was not primed (one code path, no silent re-render fallback); the harness snippets (D2, D6, D8) call `cache_renders` like `main` does. The wrapper and migration-Job renders stay live (they depend on per-run refs). Effect in production: ~13 → 2 root renders per deploy (~2s). deploy.sh is NOT a blueprint input — no rebuild.

**5.** Mark the TODO entry resolved; INDEX pointer for `cache_renders` / `psrc`.

### Reviewer input folded into the plan (Gate 1)

**Security**
- S1: group (B)'s credential-containment asserts are not touched (that group is outside this change). `prov-ac-db-url-derived` and the four stage failures stay end-to-end. The positive controls against vacuity: every e2e failure case asserts that its injected stage was REACHED (`prov-fail-<stage>-reached`) and that no call was unmodelled. The fresh-build case asserts that `secrets.calls` is non-empty before checking its content.
- S2a/b, O3: the batched `sha256sum -- <paths>` call has its rc checked explicitly, not inside a `$(…)` that `echo` would swallow. The number of digests must equal the number of paths, or the render returns 1. Digests are taken by POSITION; filenames are never parsed back out. A leading `\` (GNU's escape marker) is stripped from the digest field and the name is never used. sha256sum's lines are never echoed through.
- S2c, O2: one digest per labelled input, same labels, same order, never an aggregate hash. Equivalence is checked by a new case: the old per-file formula (`sha256sum < f`) is computed in the test for every labelled input and must equal the value on each render line. This is byte-identical by construction, and one test proves it per file.
- S2d: the Calico integrity check is left untouched.
- S(b), S2 test: new e2e case where one PROVISION_INPUT is unreadable (the file is removed from the tree copy). Expected: rc ≠ 0, `ERROR: … could not hash`, `PROVISION_FAILED REASON=step-failed STEP=blueprint_decide`, no `BLUEPRINT` line, and no delete, create or record marker. `--check` mode with the same tree must give rc ≠ 0 and no BLUEPRINT line with an empty digest. Propagation today: `BP_MANIFEST="$(blueprint_render)" || return 1` → `pstep blueprint_decide` fails → `set -e` exits → EXIT trap. It can never reach `bp_emit rebuild`.
- S(a): `cluster_exists` uses a quoted RHS: `[[ $'\n'"${out}"$'\n' == *$'\n'"${CLUSTER_NAME}"$'\n'* ]]`. New direct cases: a listing with `dt-foo-x` but not `dt-foo` → 1; a name containing glob metacharacters is not possible (validate_cluster_name) but is still tested as literal; last line without a trailing newline → 0; `kind get clusters` failing → rc 2 (direct).
- S(c): the no-key / no-input-content `assert_absent` checks run on stdout+stderr combined; determinism is compared on stdout only.
- S(d), cache_renders: the cache lives in shell variables only. No file. The "not primed" error names the render (`ENV_ROOT`/`MIGRATE_BASE`), never its content.
- S3: log_warn/log_error still go to stderr.
- S4: all fixtures stay under the existing `mktemp -d` `$WORK` with its cleanup trap. The tree copy's `secret.yaml` is a copy under `$WORK`; the repo file is only read, as today.

**Observability**
- O1: the kept e2e cases assert the EMITTED line for every ACTION/REASON pair asserted today: none/match (P2), rebuild/missing (P1, recovery), rebuild/changed (kind-config e2e), refuse/unreadable (unreadable e2e), check/changed (P12 stale, P11), check/missing (P12). check/match rc 0 (P12).
- O4: `printf -v __dt_ts '%(%H:%M:%S)T' -1` inside each log_* (no `$(…)`). New pin case: `log_info x` output matches `^\[[0-9]{2}:[0-9]{2}:[0-9]{2} INFO\] x$` after the colour codes are stripped, and `log_error` goes to stderr only.
- O5: before/after wall times for setup.test.sh and a full `layer3.sh` go in Implementation Summary.

**Code quality**
- CQ1: the `first_party_repos` call site (deploy.sh:500) no longer masks failure. It uses separate captures, each with `|| return 1`, and the pre-existing `|| true` is gone. New case: an unprimed reader fails `first_party_repos`. The readers are gated on an explicit `RENDERS_CACHED=true` flag, not on a non-empty cache.
- CQ2: `mapfile` + `${line%% *}`. There must be exactly N lines, each digest must match `^[0-9a-f]{64}$`, or the render fails. Labels and paths come from ONE pair of arrays built together.
- CQ3: quoted-RHS exact match; rc 0/1/2 unchanged. Near-miss cases: `foo` vs `foo-2`, `xfoo`.
- CQ4: `_ts` stays the single place the format lives. It sets `__dt_ts` via `printf -v`, and the log functions read it. `echo -e` semantics are kept. A comment records the bash ≥ 4.2 floor.
- CQ5: the helper sets `set --` before sourcing. Its header says snippets run under the production `set -euo pipefail`, so an rc must be captured as `|| rc=$?`.

**DRY**
- D1 (scope add): `teardown.sh` uses `cluster_exists` with the 0/1/2 shape from deploy.sh:1191: 2 → log_error + exit 1, never "absent". New case: kind list fails → rc ≠ 0, no `ran.kind_delete`. teardown.sh is not a PROVISION_INPUT, so this adds no rebuild.
- D2: ONE generic `src_run <script> <snippet>` helper (the `RUN_*` idiom as a function). `psrc` is that helper bound to the tree copy's provision.sh. I'll migrate the existing deploy.sh `RUN_*` sites that I'm touching anyway (the ones that must now call `cache_renders`).
- D3: `sha256_of` is deleted once it has no callers.

**Test** (all acked)
- T-a/b/c: the "state identical" check hashes the WHOLE `$PSTATE` + tree copy (`find | sort` over contents). Positive control: `pcluster` is in `clusters` and there is no `rec.hash`. It compares real failure outcomes. Create-failure state must equal the pristine seeded state. P8's labels (RECORDED=none, delete marker) run on the record-failure recovery e2e.
- T1: a label is not kept if it can no longer fail. Instead it is retired, with a mapping to the case that carries it: `prov-calico-tampered-no-record` → P5 e2e; `prov-sensitive-{kind-version,lib-file,tls-cert,provider}-deleted` → the kind-config e2e (main's changed→delete mapping is section-independent). Re-scoped labels are renamed for what they check (`prov-port-free-passes-check`, `prov-udp-other-address-passes-check`). Implementation Summary gets a "now asserts" column.
- T2: the label diff comes from EXECUTED output (every PASS/FAIL label, both runs). Old and new assertion counts are recorded. `_test_helpers.sh` prints only failures by default, so I'll capture labels via a harness-local trace of the assert functions' first argument (the scratch copy only, not committed).
- T3: one fresh `bash -c` per call; `set --` before `source`; the header notes `set -euo pipefail` and the `[[ -t 0 ]]` AUTO_YES logic.
- T4: `pstep` is never called under `if`/`||`/`&&`/`!`. The subshell rc is asserted ≠ 0, "exactly one PROVISION_FAILED" is asserted on every direct classification case, and the hang cases keep their SECONDS bound.
- T5: every sensitivity mutation is preceded by a direct `match` positive control.
- T6: listener readiness uses a bounded poll that FAILS loudly on timeout. It never runs without a listener.
- T7: go-red demonstrations of the six named mutations, on a scratch copy, recorded in main.md.
- T8: new coverage:
  - missing input → render fails, naming the file;
  - absent TLS crt → `missing` with rc 0, and positional alignment is checked against `sha256sum < file` for every hashed label;
  - defensive parse;
  - byte-identical diff;
  - `cluster_exists` exact / near-miss (`pcluster-2`, `xpcluster`, substring) / list-fail → 2;
  - `_ts` shape `\[[0-2][0-9]:[0-5][0-9]:[0-5][0-9] (INFO|WARN|ERROR|STEP)\]`;
  - unprimed `cache_renders` consumer fails loudly;
  - D8 counts `kubectl kustomize <ENV_ROOT>` calls through the stub and asserts exactly ONE root render per `main`.
- Staleness: main.md confirms that nothing between `cache_renders` and its consumers writes under ENV_ROOT/MIGRATE_BASE. The wrapper and Job renders are written to mktemp dirs OUTSIDE the tree.
- T9: `script` / `openssl` absent stays a FAIL, not a skip.

**DRY D2 (decided): migrate ALL** `RUN_*` sites (RUN_GUARD, RUN_RENDER, RUN_JOB, RUN_APPLY, RUN_CT, RUN_RES, RUN_MIG, RUN_DEPLOY, RUN_GUARDED, RUN_EMI, plus the inline `bash -c 'source …'` one-offs) to the one `src_run <script> <snippet> [args…]` helper. Snippet args arrive as `ARGS[@]`, because `set --` clears the positionals before the source.

**Operations / Observability (review-time)**
- Ops3/O(d): main.md will record the old-vs-new render diff, the kept-assertion-names diff, and the first-provision line against a record made before this change (`CHANGED=` should name exactly the two files).
- Ops2: a comment on `cache_renders` states it holds one snapshot per converge.
- O(c): I'll grep tests, runbooks and the layer7 route tables for any STEP= value that a root-render failure used to report.
- Ops4: the commit message must state the one-time rebuild, which wipes cluster DB state (for the Lead, who commits).

### Operator-visible effect
`provision.sh` and `lib/common.sh` are PROVISION_INPUTS: after this lands, each existing cluster's next `provision` prints `BLUEPRINT ACTION=rebuild REASON=changed CHANGED=file:infra/kind/scripts/provision.sh,file:infra/kind/scripts/lib/common.sh` and rebuilds ONCE. All script edits are batched in this devloop so it is one rebuild. deploy.sh changes cause none.

### Out of scope (checked) — ruled out of scope by @team-lead (pre-date step 3; target is the pre-step-3 baseline)
- `layer7.test.sh` (34s): dominated by deliberate REAL `timeout`/`sleep` expiries (the point of those cases), not re-derivation.
- `lang/ts/fmt.test.sh` (16s): runs the REAL nx→prettier chain by design (its header says why a stub would be vacuous).
- `validate-frame-vectors.test.sh` (27s): one whole-guard run per case against a synthetic tree — the same *shape* as old (P), but the guard is a black box whose only interface is its CLI, so there is no sourced decision function to call instead; speeding it would be a guard-internals change owned by that guard's author.

All three pre-date step 3 and were inside the pre-step-3 ~86s; the regression is (P)+(D). Not touched here.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Timings and process counts (this devloop container, idle, dt-guard/dt-story built)

| Measure | Before (b9b0693b) | After |
|---|---|---|
| `setup.test.sh` wall | 61.2s / 61.2s (2 runs) | 24.8s (final tree; 26.4s at first cut) |
| `setup.test.sh` process creations (`/proc/stat` `processes` delta) | 50,661 / 53,723 | 16,046 |
| `setup.test.sh` assertions | 576 passed (529 traced `assert_*` + 47 direct `PASS=`) | 653 passed (606 + 47) |
| Layer 3 (`./scripts/layer3.sh`) wall, standalone | 175s (Lead, b9b0693b) | **131s** (RESULT=OK) |
| Layer 3 inside `layer-fast.sh` | — | 134s |

**The ~90s target is NOT met: Layer 3 is 131s.** What remains is outside this devloop's ruled scope: `layer7.test.sh` 34s, `validate-frame-vectors.test.sh` 27s, `lang/ts/fmt.test.sh` 16s, `dev-web.test.sh` 7s, `setup.test.sh` 26s, guards 5s, everything else ~15s. `setup.test.sh` went from ~35% of Layer 3 to ~20%; reaching ~90s needs the three tests the Lead ruled out of scope.

Remaining `setup.test.sh` cost (by section): (D8) deploy main 10.8s (19 e2e converges × 4 real renders + 2 bounded 1s hangs), (P6c) 3.6s (three bounded 1s probe timeouts — the bound IS the assertion), (D) header renders 3.0s (each asserts on a different render), (D7) 2.8s, (P10) tty `script` 2.0s.

### Scripts

| Item | Before | After |
|---|---|---|
| `lib/common.sh:_ts` + `log_*` | `$(_ts)` → `date` fork per log line | `_ts` sets `__dt_ts` via `printf -v … '%(%H:%M:%S)T' -1` (the one home of the format); output byte-identical; bash ≥ 4.2 noted |
| `lib/common.sh:cluster_exists` | `grep -qx` fork | quoted-RHS exact-line bash match; rc 0/1/2 unchanged |
| `provision.sh:blueprint_render` | one `sha256sum < f \| cut` pipeline per input; a missing input rendered `sha256:` (EMPTY) silently | ONE `sha256sum -- <paths>`; rc checked, digest count == path count, each digest `^[0-9a-f]{64}$`, labels/paths built together, digests taken by position; unhashable input fails the render. `sha256_of` deleted |
| `deploy.sh` root derivations | `kubectl kustomize` of the env root in `preload_third_party_images`, `first_party_repos` (×2 per converge, + migrate base), `env_root_workloads` per repo (deployed_refs, prune) and in `wait_for_env_root`, `render_env_overlay` → ~13 renders per converge | `step cache_renders` (after prerequisites) renders root + migrate base ONCE into shell variables; `cached_render` fails loudly (naming the render, never printing it) unless `RENDERS_CACHED=true`; `first_party_repos` checks each derivation (the old `\|\| true` masked a failed root render whenever the migrate base yielded repos); `<<< "$(env_root_workloads …)"` here-string captures (which swallowed failure) replaced by checked captures |
| `devloop.sh` eager-setup existence check (Gate 3, CQ F2) | inline `kind get clusters \| grep -q`; a failed listing printed "No Kind cluster found" | sources `lib/common.sh` (definitions only; no name clashes) and uses `cluster_exists`; rc 2 prints a WARNING naming the failed listing, then runs provision, which refuses on `unreadable` |
| `teardown.sh` existence check | inline `kind get clusters \| grep -q`; a failed listing read as "absent" → WARN + exit 0 with the cluster still running | `cluster_exists` with the 0/1/2 case shape of deploy.sh; rc 2 → `log_error` + exit 1 |

**Byte-identical render (Ops3/O2).** Old `blueprint_render` (from `git show HEAD`) and new, sourced side by side over the SAME files: `diff` empty, both with every cert present and with `mc-webtransport.crt` absent (the `missing` line stays in `TLS_LEAVES` order). `BLUEPRINT_FORMAT` unchanged (`dt-blueprint v1`).

**First provision after this lands (Ops3).** Render of the pre-change tree vs this tree, same inputs: exactly two lines differ — `file:infra/kind/scripts/provision.sh` and `file:infra/kind/scripts/lib/common.sh`. Operators see `BLUEPRINT ACTION=rebuild REASON=changed … CHANGED=file:infra/kind/scripts/provision.sh,file:infra/kind/scripts/lib/common.sh` once per existing cluster, and that rebuild wipes the cluster's DB state (Ops4 — for the commit message). deploy.sh / teardown.sh are not PROVISION_INPUTS.

**cache_renders staleness (T8).** Nothing a converge writes lands under ENV_ROOT or MIGRATE_BASE: deploy.sh's only writes are `mktemp` files/dirs (`dt-iid.*`, `dt-env-root.*`, `dt-migrate.*`, `kind-image.*.tar`). **O(c):** no test, runbook or layer7 route keys on the STEP of a root-render failure; layer7 routes on REASON (`step-failed` → tree), which a `cache_renders` failure keeps.

### Test file (`scripts/setup.test.sh`)

- **`src_run [-u VAR]… <script> <snippet> [args…]`** — the ONE source-and-call helper (fresh `bash -c`, `set --` before `source`, args as `ARGS[@]`, documented set -e / AUTO_YES semantics). Every former `RUN_*` string and inline `bash -c 'source …'` site uses it (D2). One deliberate exception, commented: `lib-common-is-definitions-only` asserts on what SOURCING prints, which src_run discards.
- **Group (P)**: tree template built once (`cp -a` per reset); `psrc`, `ptrap` (`trap __provision_exit_trap EXIT; pstep <fn>` — the production trap; never under if/||/&&/!), `decide`, `seed_record` (real `blueprint_decide` + `record_blueprint`), `prender`, `pstate_sum`. End-to-end `provision.sh` runs: 27 (was ~75): P1, P2, unhashable-input ×2, P4, P5 ×4 + 1 recovery, P6, unreadable, unnamed-refused, unnamed-create, TCP port-held, runtime-incapable, prerequisite ×2, kind-config rebuild, tty N, P11 ×2, P12 ×3.
- **Port listeners** bind port 0 and report it through a file (no pick-then-rebind race, no fixed `sleep 1`); bounded readiness poll that FAILS loudly and skips nothing silently.

#### Labels: old → new (every label that disappeared; T1/T2)

Built from EXECUTED output (every `assert_*` label traced in both runs, whole file, all groups): 529 → 606 traced labels (re-traced on the final tree, after the iteration-2 fixes). **Unlabeled passes** (direct `PASS=$((PASS + 1))` sites, 47 of the old 576), counted per group by instrumenting every increment in both versions: B2 1, B3 17, B4 1, B5 1, B6 1, B7 1, C 2, D 3, E 13, F 7 — **identical before and after** (47 = 47); none of them is in (P). **The only 20 labels gone are all in (P)**; nothing outside (P) disappeared.

| Old label | Now | What it asserts now |
|---|---|---|
| `prov-sensitive-{kind-version,lib-file,tls-cert,provider}-rebuilds` | `prov-sensitive-<s>-changed` | direct `blueprint_decide` → `REASON=changed` (preceded by a `-baseline-match` positive control) |
| `prov-sensitive-{kind-version,lib-file,tls-cert,provider}-deleted` | retired → `prov-sensitive-kind-config-deleted` (e2e) | main's changed → rebuild + delete mapping is section-independent; the per-section part is `…-names-exactly-that-section` (kept) |
| `prov-deploy-only-edit-is-noop` | `prov-deploy-only-edit-matches` | decide → `match` (main's match → none is `prov-noop-decision`) |
| `prov-deploy-only-edit-no-delete` | retired → `prov-noop-no-delete` | match never deletes (P2, e2e) |
| `prov-fail-create-next-run-recovers` | `prov-fail-create-state-equals-pristine` | the post-create-failure state (whole `$PSTATE` + tree fingerprint) == pristine, which P1 builds from rc 0 |
| `prov-fail-{calico,secret}-next-run-recovers` | `prov-fail-<s>-state-equals-record-failure-state` + `prov-fail-record-state-is-listed-unrecorded` + `prov-fail-record-next-run-recovers` | the three post-failure states are identical (positive control: cluster listed, no record); the one recovery e2e covers all three |
| `prov-calico-tampered-no-record` | retired → `prov-fail-calico-no-record` | a failed calico stage leaves no record (P5, e2e); the tampered case is now a direct `install_calico` under the trap |
| `prov-classify-pre-create-never-apiserver` | folded into `prov-fail-create-classified-line` | the P5 create run now runs with `STUB_READYZ=fail`; `…-pre-create-no-apiserver-token` / `…-readyz-not-asked` assert on that run |
| `prov-garbage-record-rebuilds` | `prov-garbage-record-is-missing` | decide → `missing` (missing → rebuild: `prov-no-record-rebuilds`) |
| `prov-kind-list-fail-no-create` | retired → `prov-unreadable-no-create` | unreadable → refuse is cause-independent; `prov-kind-list-fail-is-unreadable` (decide) kept |
| `prov-port-free-creates` | `prov-port-free-passes-check` | `check_host_ports` rc 0 on a freed port |
| `prov-udp-port-held-never-creates` | retired → `prov-port-held-never-creates` (TCP, e2e) | a held port stops main before create regardless of protocol |
| `prov-udp-other-address-creates` | `prov-udp-other-address-passes-check` | `check_host_ports` rc 0 |

New or renamed labels: 97 (20 gone, 606 − 529 + 20). New coverage: one-digest-per-label equivalence, absent-cert alignment, unhashable input (direct + e2e + `--check`), `cluster_exists` exact / near-miss / literal / list-fail, log line shape + stream, `cache_renders` unprimed / primed, one root render per `main`, teardown list-failure, post-create readyz asked, one-line invariants on every direct classification case, `prov-fresh-secrets-created` (positive control for the DB-URL assertion), `prov-check-match`.

#### Go-red demonstrations (T7) — each mutation on a scratch repo copy, whole suite run

| Mutation | Fails (besides harness noise) |
|---|---|
| drop one PROVISION_INPUT from `blueprint_render` | `prov-render-lists-every-input`, `prov-render-digest-per-label`, `…-absent-cert-still-aligned`, `…-names-exactly-that-section` ×5 |
| `changed_sections` prints empty | `prov-sensitive-*-names-exactly-that-section` ×5, `prov-real-renewal-names-cert` |
| skip the record-hash regex | `prov-garbage-record-is-missing` |
| drop `classify_env_failure`'s apiserver answer | `prov-classify-post-create-apiserver`, `prov-classify-readyz-hang-token`, `deploy-apiserver-unreachable-*`, `deploy-classify-readyz-hang-token` |
| drop the UDP branch of `check_host_ports` | `prov-udp-port-held-{rc,classified,names-port,one-line}`, `prov-udp-wildcard-holder-overlaps` |
| drop the Calico sha comparison | `prov-calico-tampered-{rc,says-so,not-applied}` |
| reverse digest positions | `prov-render-digest-per-label`, `…-absent-cert-still-aligned`, 3 × `…-names-exactly-that-section`, `prov-real-renewal-names-cert` |
| unquote `CLUSTER_NAME` in `cluster_exists` | `cluster-exists-name-is-literal-not-glob` |
| remove the `RENDERS_CACHED` check | `cache-unprimed-names-the-render`, `cache-unprimed-env-root-workloads-fails` |
| env_root_workloads re-renders the root | `deploy-renders-the-root-once`, `cache-unprimed-env-root-workloads-fails` |
| swallow sha256sum's failure | `prov-render-unhashable-input-names-file` (the count check still fails the render — defence in depth) |

("Harness noise": `prov-one-render-definition` also failed in the provision.sh mutations because the demo left a `provision.sh.orig` beside the file — a correct detection of a second `blueprint_render()` definition, not a gap.)

**The go-red pass found a vacuous assertion in my own first draft**: `prov-render-digest-per-label` read render lines with the file's `IFS=$'\n\t'`, so `read label val` never split, every line was skipped, and the check passed on a misaligned render. Fixed (`IFS=' ' read`), then re-demonstrated red.

## Files Modified

```
{Output of: git diff --stat HEAD}
```

### Key Changes by File
| File | Changes |
|------|---------|
| `path/to/file.rs` | {Brief description} |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS
**Duration**: ~4h (16:20–20:30), 2 review iterations; Gate 2 layer-all 1432s (L7 933s)

### Layer 2: cargo fmt
**Status**: PASS
**Duration**: ~4h (16:20–20:30), 2 review iterations; Gate 2 layer-all 1432s (L7 933s)

### Layer 3: Simple Guards
**Status**: ALL PASS
**Duration**: ~4h (16:20–20:30), 2 review iterations; Gate 2 layer-all 1432s (L7 933s)
`setup.test.sh`: 653 passed, 0 failed.

### Layer 4: Unit Tests
**Status**: N/A aggregate (cargo-test-passed, nx-test-passed)
**Duration**: ~4h (16:20–20:30), 2 review iterations; Gate 2 layer-all 1432s (L7 933s)

### Layer 5: All Tests (Integration)
**Status**: PASS
**Duration**: ~4h (16:20–20:30), 2 review iterations; Gate 2 layer-all 1432s (L7 933s)

### Layer 6: Clippy / audit
**Status**: OK (N/A aggregate) on HEAD 56ed295a (the operator's pnpm-overrides commit cleared the ambient advisories that failed earlier runs on b9b0693b; this diff touched no TS dependency).
**Duration**: ~4h (16:20–20:30), 2 review iterations; Gate 2 layer-all 1432s (L7 933s)

### Layer 7: Env-tests
**Status**: PASS/FAIL
**Duration**: ~4h (16:20–20:30), 2 review iterations; Gate 2 layer-all 1432s (L7 933s)
**Output**: {Wall-clock time for dev-cluster rebuild + env-test run; pass/fail summary; log path}

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 1 found, 1 fixed, 0 deferred

- Render-containment check (P3 `m_all`) had no positive control, and its needle was the key name rather than the secret. Fixed with `prov-render-all-streams-is-a-render` (rc 0, format line, 7 digests), plus `prov-render-no-postgres-password`. That check's needle is the password VALUE read from the tree copy, and it is guarded by `prov-render-password-needle-read`. Go-red: a render that leaks the value to stderr fails `prov-render-no-postgres-password`; a render that fails early fails `prov-render-all-streams-is-a-render`. setup.test.sh: 639 passed.

### Test Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 1 found, 1 fixed, 0 deferred

- T-R1 (stale numbers): re-traced executed labels on the final tree: 599 traced + 47 unlabeled = 646. The same 20 labels are gone as before (the table is unchanged). All numbers in this file are updated.

### Observability Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 1 found, 1 fixed, 0 deferred

- F1: `STEP=cache_renders` had no e2e assertion. Added D8 case `deploy-root-render-failure-*` (new D4-stub knob `STUB_KUSTOMIZE_FAIL=<dir>`; every other render stays real). It asserts the `DEPLOY_FAILED REASON=step-failed STEP=cache_renders WORKLOADS=-` line, that there is exactly one such line, that the stub failure was reached, and that nothing was applied or built.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 3 found, 3 fixed, 0 deferred

- F1: the TLS loop cursor in `blueprint_render` is now set explicitly (`i=$(( 1 + ${#PROVISION_INPUTS[@]} ))`). Go-red: setting it to 1 fails 7 assertions, including `prov-render-digest-per-label`. The render is still byte-identical to the old one.
- F2: option (a). `devloop.sh` uses `cluster_exists`. New static guard `cluster-exists-is-the-one-existence-check`: every executed `kind get clusters 2>` under `infra/` is `cluster_exists` or devloop.sh's prefix listing. Go-red: the old devloop.sh fails it.
- F3: new `psrc_rt` helper, which is psrc with the runtime detected. It replaces the five repeated prefixes; `m_all` keeps its explicit form.

### Iteration 2 (after rebase onto 56ed295a)

- **T-R2 / DRY (same guard):** `cluster-exists-is-the-one-existence-check` matched only `kind get clusters 2>`, so an inline copy without a redirect passed. It now matches the command wherever it is not quoted (`(^|[^'])kind get clusters`; log messages quote it), lists one entry per occurrence, and expects exactly `devloop.sh` (the orphan scan) + `lib/common.sh`. Positive control `cluster-exists-guard-catches-inline-copy`: a teardown.sh copy with an appended `if kind get clusters | grep -q "^x$"` IS reported, and its quoted log message is not. Go-red in-tree: that inline line inserted into teardown.sh fails the guard.
- **Ops (devloop.sh `detect_orphan_clusters`):** `kind get clusters 2>/dev/null | grep … || true` read a failed listing as "no orphans". Now the listing is captured on its own, and a failure prints a WARNING and skips the advisory scan (rc 0). Tested in group (C): devloop.sh has no source guard, so the REAL function body is extracted with `sed` (and asserted extracted) and run against PATH stubs. Cases: `orphan-scan-list-fail-{rc0-advisory,warns}`, plus the positive control `orphan-scan-listing-{rc0,reports-orphan,prefix-only}` (with a listing, the same function reports the prefixed orphan). Go-red: HEAD's devloop.sh fails `orphan-scan-list-fail-warns`.
- T-R3: `orphan-scan-fn-extracted` is a single conjunction (starts with the function header, ends with `\n}`, contains `kind get clusters`) — no `&&`/`||` precedence trap.
- setup.test.sh: 653 passed, 0 failed (606 traced labels + 47 unlabeled; the same 20 labels gone as in the table).

### DRY Reviewer
**Verdict**: RESOLVED-FIXED (iteration 2; CLEAR at iteration 1)

**True duplication findings**: None at review. Gate-1 items D1 (teardown.sh → `cluster_exists`), D2 (one `src_run` helper), and D3 (`sha256_of` deleted) all landed. DRY noted the TODO wording "~13 renders → 1" should say 2; the Lead fixed it in `docs/TODO.md`.

**Extraction opportunities**: None

### Operations Reviewer
**Verdict**: RESOLVED-FIXED (iteration 2; CLEAR at iteration 1)
**Findings**: 1 found, 1 fixed, 0 deferred

No findings. All four plan asks landed. Rollback is `git revert`, which costs one more one-time cluster rebuild. The commit message carries the rebuild notice.

### Semantic Guard Reviewer
Not spawned: the diff touches no check surface (shell test-suite and script performance; no credential types, Debug/Display impls, or client credential lifetime).

### Gate 3 Summary

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | render-containment positive control |
| Test | RESOLVED-FIXED | 4 | 4 | 0 | T-R1 stale counts; T-R2 guard needle + positive control; T-R3 extraction-check precedence; T-R4 main.md counts |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | STEP=cache_renders e2e |
| Code Quality | RESOLVED-FIXED | 3 | 3 | 0 | TLS cursor, devloop.sh, psrc_rt |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | iter 2: guard needle widened (same issue as T-R2) |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | iter 2: `detect_orphan_clusters` warns on listing failure; covered by the extracted-function-body test in setup.test.sh |

**Lead note (scope)**: the ~90s Layer 3 target is not met (175s → ~131s on the devloop container). The remainder is three pre-step-3 self-tests, which the Lead ruled out of scope at Gate 1. A new open entry for them is filed in `docs/TODO.md` §Infrastructure Validation in Devloops. This is a scope decision, not a deferred finding.

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

Examples:

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: `{the whole commit | <unit + safe direction>}` (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

### Issue 2: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

{Add more issues as needed, or "None" if no issues}

---

## Lessons Learned

1. {Key takeaway 1}
2. {Key takeaway 2}
3. {Key takeaway 3}

{Add more as applicable}

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
