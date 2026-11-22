# Devloop Output: setup.test.sh speed regression (~114s) — bisect and fix the cause

**Date**: 2026-11-21
**Task**: scripts/setup.test.sh is back to ~114s (was ~25s after the 2026-09-30 speed-up) and sets Layer 3 wall time over the 90s budget (docs/TODO.md:1863). Bisect what grew it since 2026-10-01, fix the cause, measure before/after on the container.
**Specialist**: infrastructure (paired with test)
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/layer-all-devloop-helper`
**Duration**: ~4.7h (setup 21:08 through Gate 2)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `1dcfd093386b371c5f28013816c4250e0674fdd5` |
| Branch | `feature/layer-all-devloop-helper` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (spawned) |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` (spawned) |
| Test (paired-test) | `paired-test` (spawned) |
| Observability | `observability` (spawned) |
| Code Quality | `code-reviewer` (spawned) |
| DRY | `dry-reviewer` (spawned) |
| Operations | `operations` (spawned) |
| Semantic Guard | not spawned (test/script performance; no checks.md surface) |

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
{What was the goal of this task?}

### Scope
- **Service(s)**: {Which services were affected}
- **Schema**: {Database schema changes? Yes/No}
- **Cross-cutting**: {Does this affect multiple services? Yes/No}

### Debate Decision
{NEEDED/NOT NEEDED} - {Brief justification}

{If debate was needed, link to debate record: `docs/debates/YYYY-MM-DD-{topic}.md`}

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
| `scripts/lang/_common.sh` | Mine | — |
| `scripts/lang/_common.test.sh` | Mine | — |
| `scripts/layer3.sh` | Mine | — |
| `docs/runbooks/devloop-validation.md` | Mine | — |
| `docs/TODO.md` | Mine | — |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |

---

## Planning

### Problem, in mechanism language

`scripts/setup.test.sh` uses **a whole `deploy.sh` converge (`dep` = sourced `main`) as the test vehicle for single-step behaviour**. Every new branch added to a deploy step became one more end-to-end converge, and each converge also got heavier as `main` grew, so the cost grows as (cases on a step) x (cost of the whole converge). This is the same mechanism the 2026-09-30 speed-up fixed for `provision.sh`: group (P) was redesigned to "direct calls, e2e only for main's orchestration", but that rule was applied only to (P) and never stated or enforced for the `deploy.sh` group (D8). Story 4 (MH pool, GC registration rule, removed-MH settle) then added its cases to D8 as converges.

Secondary: the 7 settle **deadline** cases each spin on the wall clock (`date +%s`, poll 0) for 1-2 real seconds until a 1s deadline passes.

### Bisect (scratch worktree, this container, 32 cores; `bash scripts/setup.test.sh`, wall clock)

Every commit since 2026-10-01 that touched `scripts/setup.test.sh`:

| Commit | Date | Wall | Cases |
|---|---|---|---|
| `6cfdfd20` (baseline, PATH sealed) | 10-01 | 29.2s | 660 |
| `ed9f8d6e` .. `b8283d01` (10 commits) | 10-03..11-06 | 27.6-32.6s | 685-837 |
| **`85e28783`** Story 4 I-2 (MH slot pool) | 11-08 | **68.4s** (+36s) | 1006 |
| **`9106aee9`** Story 4 GC-2 (registration rule, settle wait) | 11-10 | **108.6s** (+40s) | 1110 |
| `66ad9db5`, `0e592dfe`, `84bf90c0` | 11-10..11-13 | 107-116s | 1110-1121 |
| `1dcfd093` (start) | 11-21 | 118.6s | 1121 |

Attribution (every `src_run` call timed with a wrapper in a scratch copy; same machine):

| Commit | e2e `dep` converges | avg per converge | `dep` total | everything else |
|---|---|---|---|---|
| `b8283d01` | 16 | 0.83s | 13.3s | ~18s |
| `85e28783` | 32 | 1.27s | 40.7s | ~26s |
| `9106aee9` | 50 | 1.55s | 77.7s | ~29s |
| `1dcfd093` | 54 | 1.57s | 85.0s | ~32s |

So 72s of the 87s growth is the converge vehicle: 3.4x the runs, each 1.9x dearer (one converge, timed per step: `apply_env_root` 0.61s, now a real wrapper render + JSON conversion + apply list + template write since I-2; `cache_renders` 0.25s; `converge_mh_pool` 0.17s; `prune_superseded_images` 0.17s). The other ~14s is new render cases (D2/D2b/D4/D4b/D2d, real kustomize) — coverage, not a vehicle problem. Group (P) held its rule: 19 e2e `provision.sh` runs at baseline, 20 now.

### Same class elsewhere?

- Within setup.test.sh: (P) held its rule (19 e2e `provision.sh` runs at baseline, 20 now); D8 is the only regressed group.
- `layer-all.test.sh` 2.7s (`6cfdfd20`) -> 20-34s now, from the killed-run (K) cases (`d7430433`..`b183b2f6`): real-signal teardowns spending real grace, plus start/exit process-tree snapshots per full run (@paired-test: ~12s snapshots, ~8s teardown polling). A different mechanism (orchestrator e2e by design); recorded in the refreshed `docs/TODO.md` "Layer 3 still slow" entry (1865), not fixed here.
- **Why it was not seen**: this regression is ~13 days old (setup.test.sh was ~30s until `85e28783`, 2026-11-08). It was MASKED, not unseen: Layer 3 was already ~131s from other entries since 2026-09-30, so `WARN BUDGET_TOTAL_BREACH` and the per-layer `BUDGET_BREACH` fired every run before and after (131 -> 134s), and nothing reported per-entry duration. Fixes for that, below: the per-entry duration line (names the culprit) and the e2e-run count (deterministic tripwire for this mechanism). No per-entry WARN threshold (lead ruling: at a ~60s honest ceiling a default would fire every run, repeating the always-on alarm).

### Fix (revised after Gate-1 input)

Prototype of the direct calls and fake clock alone (scratch worktree, single run): 118.6s -> 63.3s, 1121/1121.

No `provision.sh` / `lib/common.sh` / `deploy.sh` edit: no blueprint change, no cluster rebuild. Two files of code: `scripts/setup.test.sh`, `scripts/lang/_common.sh` (+ its test).

1. **ONE deploy env home** `d4 <cmd...>` (DRY 1, security C1): every deploy-test invocation (res, mig, dep, the new step helper, and every raw `bash "$DEPLOY"` run incl. RUN_GUARDED, no-runtime, no-timeout, insufficient-disk, kind-list-fail, no-cluster) runs through it. It runs `<cmd>` in a subshell with `DT_HOST_GATEWAY_IP`/`DT_KIND_CONFIG` unset and the D4 env exported (`PATH=${D4_BIN}:…`, `TMPDIR=$RWORK`, `DT_CLUSTER_NAME=d4cluster`, `DEVLOOP_MIN_DISK_GB=0`, job/settle poll knobs 0, `DT_MH_SH=$D4_MH`). Per-call variation only through harness-named overrides (`D4_PATH` for NORT/NOTO bins, `D4_CLUSTER`, `D4_MIN_DISK_GB`), never ambient names, so an ambient `DEVLOOP_MIN_DISK_GB` cannot leak in. Same for (P): `prov`/`psrc` share one `penv`.
2. **ONE step-under-trap helper** `dstep <fn> [args]` (code-reviewer 1, DRY 2), the deploy twin of (P)'s `ptrap`: `d4 src_run "$DEPLOY" '<seed>; trap __deploy_exit_trap EXIT; step "${ARGS[@]}"'`. Always inside src_run's `bash -c` (security C2: the trap and `set -e` never touch the harness shell). Converted: D8b (2), D8c settle (8), deadline loop (7), env-down / metrics-unreadable / no-replaced-array (3), mh.sh token loop (7), env-down / baseline-zero / headroom (3) -> `dstep converge_mh_pool`; MH prune (4) -> `dstep prune_superseded_images`; debug image cold/warm (2) -> `dstep preload_debug_image`. 36 converges.
3. **Cross-step handoff (paired-test 1)**, state for each converted step:
   - Render cache: main runs `cache_renders` before every step, so `dstep` always seeds it. The seed files are written ONCE by deploy.sh's own `cache_renders` (`src_run "$DEPLOY" 'cache_renders; cached_render root …; cached_render migrate-base …'`), so their content is main's by construction. The seed sets `ROOT_RENDER`, `MIGRATE_BASE_RENDER`, `RENDERS_CACHED` (deploy.sh internals, the coupling code-reviewer named). Control: a `dstep`-seeded `cached_render root` / `migrate-base` must read back byte-identical to the seed files, so renaming any of the three goes red (go-red (h)).
   - `IMAGE_REFS` (prune): hand-seeded with the MH repo's built ref (what `resolve_image_refs` would set). Real handoff asserted by the kept e2e `deploy-rc` (`prune-evicts-older-*`, `prune-keeps-current`) and unchanged-redeploy/unreadable-history cases.
   - MH pool stub state: the mh.sh stub reads only its knobs and `$MARK/mh.count`; nothing upstream writes it, so no handoff. Real order asserted by the kept e2e `deploy-rc` (`deploy-mh-pool-upgrade-then-top-up`, "MH pool after root wait", `deploy-never-waits-on-mh-template`) plus go-red (m).
   - `preload_debug_image`: reads only `MH_DEBUG_IMAGE` and the kind/podman stubs. The kept e2e FATAL knob cases assert main stops before the build.
   **Hand-off table** (paired-test Gate-1 point 1): what each `dstep` case group gets WITHOUT the real upstream step, and the kept e2e case that asserts the real hand-off.

   | `dstep` group | Hand-set instead of an upstream step | Real producer in main | Kept e2e covering the real hand-off |
   |---|---|---|---|
   | all groups | `ROOT_RENDER`, `MIGRATE_BASE_RENDER`, `RENDERS_CACHED=true` (seeded from files written ONCE by deploy.sh's own `cache_renders`) | `step cache_renders` | `deploy-renders-the-root-once`, `deploy-root-carries-built-tags` (deploy-rc); seed read-back control `[dstep-seed-*-reads-back]`; go-red (h) |
   | `converge_mh_pool`: D8b, D8c settle, deadline loop, env-down / metrics-unreadable / no-replaced-array, mh.sh token loop, baseline-zero, headroom (30) | nothing: the mh.sh stub reads only its STUB_MH_* knobs and `$MARK/mh.count` (written only by the stub itself); GC pods / metrics come from the kubectl stub's knobs, the GC namespace from the seeded root render. `mh-template` ConfigMap state is NOT read by converge_mh_pool (mh.sh owns it; the stub models it) | `apply_env_root` (writes `mh-template`), `wait_for_env_root` | `deploy-mh-pool-upgrade-then-top-up`, `deploy-mh-pool-top-up-owner-baseline`, `deploy-mh-pool-reported`, `deploy-never-waits-on-mh-template` (deploy-rc); the template write itself by D4b `contract-mh-template-written` (apply_env_root); NEW in deploy-rc: `deploy-writes-mh-template-before-pool` (the e2e converge writes `applied.mh-template.json`, and its first `mh.sh` call follows it); go-red (m) |
   | `prune_superseded_images`: MH keep set (4) | `IMAGE_REFS[localhost/mh-service]=localhost/mh-service:${BUILT_TAG}` (what `resolve_image_refs` sets from the build); `DEPLOYED_REFS` left empty (only the migrations repo reads it, and it is not in this IMAGE_REFS); `mh-template` ConfigMap supplied by `STUB_MH_TEMPLATE_OLD` exactly as the e2e cases did | `build_first_party_images` + `resolve_image_refs`; `apply_env_root`/`write_mh_template` | `prune-evicts-older-from-node/host`, `prune-keeps-previous-generation`, `prune-keeps-current` (deploy-rc), `prune-unchanged-redeploy-*`, `prune-history-unreadable-*` |
   | `preload_debug_image`: cold / warm (2) | nothing (reads `MH_DEBUG_IMAGE` and the kind/podman/crictl stubs) | — | `deploy-debug-image-STUB_NODE_PULL_FAIL/INVISIBLE-*` (kept e2e: stops before the build, named step) |

   **The fake clock applies**: deploy.sh reads time ONLY via `$(date +%s)`, at deploy.sh:1561 (deadline) and 1584 (loop check), both in `wait_removed_mh_unselectable`; no `$EPOCHSECONDS`, `$SECONDS` or `printf %()T` in deploy.sh (grep), and `lib/common.sh:_ts` (`printf %()T`) only formats log lines. `date` is a PATH lookup, so the per-case PATH prefix reaches it; the exact read-count control proves it at run time.

4. **Fake clock (security C3, code-reviewer 3)**: a `date` stub in its own dir under `$WORK`. It execs the real date by the absolute path captured when the stub is written (never a PATH lookup), keeps its counter under `$MARK`, and applies only to the 7 deadline cases as a per-command PATH prefix (never exported, never in D4_BIN/TOOLBOX). Those 7 also run bounded: `src_run` gains `-t <secs>` (`exec timeout --kill-after=5 <secs> bash -c …`: GNU timeout puts itself in its own process group and signals the WHOLE group, so stubs die with the case; no `setsid`, which would move the child out of that group. Verified by a case that leaves a sleeping stub behind), so a never-ending wait fails as a case in seconds instead of hanging Layer 3 (go-red (e)). Exact read-count control (baseline + 1 poll per pod) proves the wait reads `date +%s` through PATH and that the stub clock ended it (deploy.sh:1562/1585 use `$(date +%s)`; no `$EPOCHSECONDS`/`printf %()T` in the wait).
5. **Host seal (lead ruling, security)**: `export KUBECONFIG="${WORK}/no-such-kubeconfig"` beside the PATH seal; new case: a deploy.sh `${KUBECTL}` call without the stubs fails closed (non-zero, no cluster reached).
6. **e2e run budget (paired-test 4, code-reviewer 2, DRY 3)**: ONE `e2e_record <group>` writing under `$WORK`, ONE end-of-suite assert loop over a `group -> budget` table next to the helpers, asserting `==` (no silent slack). Recorded where main actually runs: `src_run` records when its snippet calls `main` (group = the sourced script's basename), and the one wrapper for executed runs (`run_main <script> [args]`, which every `bash "$DEPLOY"/"$PROV_COPY"/"$SETUP"` site uses) records too. A static check fails if the file runs `bash "$DEPLOY"`, `bash "$PROV_COPY"` or `bash "$SETUP"` anywhere but `run_main`, which closes the hand-rolled bypass (go-red (i), (i')). Failure text: `[e2e-budget-<group>] N main runs, budget M: e2e is for main's orchestration only — call the step with dstep/ptrap; a NEW orchestration assertion bumps the constant here with a comment saying what it orchestrates.` Comment: the count does not see per-converge cost growth (the 1.9x half); the duration line is the signal for that. Constants are set to the post-fix counts (computed, recorded in main.md).
7. **Per-entry duration line (lead ruling, observability 1a)**: `parallel_queue_run` writes `ENTRY_DURATION NAME=<prefix> DURATION=<n>` into each entry's replayed stdout right after its STATUS line (`$SECONDS` in the entry's subshell; no fork; bash-4 safe). A runner-died entry gets `DURATION=-`. `tee_collect_statuses` and `parse_status_line` key on `^STATUS=` (checked), so neither reads it. `_common.test.sh`: the registration-order expectation gains the lines; one line per entry in registration order; a 1s-sleep stub reports `DURATION>=1`; the died entry (the existing `d.sh` kill-9 stub) reports `DURATION=-`.

### Go-red plan (paired-test 2): every mutation run against BOTH the start-commit suite and the new suite

Mutations in a scratch copy of the script-under-test: (a) shrink drains the lowest slot; (b) settle sums across pods; (c) the recompute control is dropped; (d) `not-available-timeout` maps to `mh-pool-failed`; (e) the deadline never breaks (must end red in bounded time); (f) MH prune drops `previous_images`; (g) `preload_debug_image` always pulls; (h) a seeded internal is renamed (`RENDERS_CACHED`, and separately `ROOT_RENDER`); (i) one extra `dep` / (i') a hand-rolled `bash "$DEPLOY"` both fail the budget; (j) `__deploy_exit_trap` emits a second DEPLOY_FAILED line on an mh-pool failure; (k) an unguarded failing command mid-`converge_mh_pool` (set -e live); (l) MH prune drops the CURRENT ref; (m) main runs `converge_mh_pool` before `wait_for_env_root` (the kept e2e must go red); (n) a stray step reaches the real kubectl (KUBECONFIG seal). Table in main.md: case, old suite, new suite; both red = equivalent; old red / new green = regression. (h), (i), (i'), (n) have no old-suite equivalent and are marked new.

### Measurement plan

3 runs each, median, nothing else heavy running: setup.test.sh at `82a50567`, `6cfdfd20`, `018fa673` (parent of I-2; three non-test commits in that interval), `85e28783`, `9106aee9`, `1dcfd093`, and the result. `scripts/layer3.sh` before/after, 3 runs each, and one `layer-all.sh` `GUARD_AUDIT_DURATION` (Layers 3+6). If it is still over 90s, name the new critical path. Re-time `layer7.test.sh`, `validate-frame-vectors.test.sh`, `ts/fmt.test.sh`, `run-story.test.sh`, `layer-all.test.sh` to refresh TODO 1865 and reconcile 2926. Per-group assertion counts before/after (security C5).

### Planned files

| File | Change |
|---|---|
| `scripts/setup.test.sh` | `d4`/`penv` env homes, `dstep`, seeded cache + control, fake clock + `src_run -t`, KUBECONFIG seal + case, `e2e_record`/`run_main` + budget table + static check, 36 cases converted (assertions kept) |
| `scripts/lang/_common.sh` | `ENTRY_DURATION` line in `parallel_queue_run` |
| `scripts/lang/_common.test.sh` | duration-line cases; registration-order expectation updated |
| `docs/TODO.md` | 1863 RESOLVED (numbers, the ~13-day / masked wording); 1865 refreshed (re-timed entries + layer-all.test.sh growth and cause); 2926 folded into 1865 and deleted (DRY F2) |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | setup.test.sh pointer (`d4`, `dstep`, e2e budget); `_common.sh` duration line |
| `scripts/layer3.sh` | (added at Gate 3, operations F2 / Lead) queue registered heaviest first, with the rule in a comment |
| `docs/runbooks/devloop-validation.md` | (added at Gate 3, operations F3 / Lead) how to investigate a Layer 3 budget breach from the per-entry lines |

Why `_common.sh` / `_common.test.sh` are here: the per-entry duration line was requested by @observability at Gate 1 and ruled in by the Lead (it would have made this bisect one grep); its final shape is `emit_step_duration`'s (DRY F1 + observability, Gate 3).

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired-test) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

## Pre-Work

None.

---

## Implementation Summary

### Measurements (devloop container, 32 cores, 3 runs each, median; nothing else heavy running)

| What | Command | Runs | Median |
|---|---|---|---|
| setup.test.sh @ `82a50567` (2026-09-30 speed-up) | `bash scripts/setup.test.sh` (scratch worktree) | 28.1 / 24.7 / 28.0 | **28.0s** (653) |
| setup.test.sh @ `6cfdfd20` (2026-10-01 baseline) | same | 29.1 / 28.3 / 25.7 | **28.3s** (660) |
| setup.test.sh @ `b8283d01` | same | 32.0 / 32.9 / 32.0 | **32.0s** (837) |
| setup.test.sh @ `018fa673` (parent of I-2) | same | 31.7 / 32.1 / 32.1 | **32.1s** (837) |
| setup.test.sh @ `85e28783` (I-2) | same | 67.0 / 67.7 / 68.1 | **67.7s** (1006) |
| setup.test.sh @ `9106aee9` (GC-2) | same | 108.3 / 107.8 / 106.8 | **107.8s** (1110) |
| setup.test.sh @ `1dcfd093` (start) | same | 116.5 / 118.9 / 115.3 | **116.5s** (1121) |
| **setup.test.sh, this change** | `bash scripts/setup.test.sh` (/work) | 64.4 / 63.1 / 62.5 | **63.1s** (1142) |
| `scripts/layer3.sh` @ start | `bash scripts/layer3.sh` (scratch worktree)* | 150.7 / 138.6 / 136.9 | **138.6s** |
| **`scripts/layer3.sh`, this change** | `bash scripts/layer3.sh` (/work) | 82.6 / 85.0 / 83.3 | **83.3s**, all OK |
| GUARD_AUDIT_DURATION (Layers 3+6) | `./scripts/layer-fast.sh` | 1 run | L3 83s + L6 2s = **85s** < 90s; no `BUDGET_TOTAL_BREACH` |
| `scripts/layer3.sh` under concurrent load, before reorder | layer-fast / @operations | 2 runs | 95s / 96s (load ~7-11): over budget |
| **`scripts/layer3.sh`, heaviest-first reorder (Gate 3, ops F2)** | `bash scripts/layer3.sh` (/work), load 7-10 (no quiet window) | 91.4 / 77.9 / 82.2 | **82.2s** under load (serial sum 343s) |

\* The scratch worktree has no installed `node_modules`; its `pnpm-deps-stale` and proto-golden-format entries fail fast there. Both are short entries, so they do not change the start commit's critical path (setup.test.sh, ~116s).

New critical path (this change's per-entry duration lines): setup-disk-guard-selftest 68s, run-story-selftest 38s, frame-vectors-guard-selftest 34s, mh-pool-tool-selftest 26s, layer7-selftest 23s, layer-all-orchestrator-test 23s. Recorded in the refreshed `docs/TODO.md` entry "Layer 3's remaining slow self-tests".

Other suites, individually (for the TODO refresh): layer7.test.sh 18.5s, validate-frame-vectors.test.sh 32.6s, ts/fmt.test.sh 9.1s, run-story.test.sh 35.5s, layer-all.test.sh 21.2s.

### Per-group assertion counts (security C5)

Every group is byte-identical in count except: (D8) 34 -> 42 (+5 seed controls, +1 fake-clock real-date control, `deploy-writes-mh-template-before-pool`, `deploy-mh-pool-after-root-wait`), (D8c) 246 -> 253 (+7 `*-fake-clock-one-poll`), new (H) +6 (KUBECONFIG seal x3, e2e budget x2, no-unrecorded-bash-run). (B) provision-org, (C), every (P) group, (F) unchanged. Total 1121 -> 1142; Gate 3 added 4 more in (H) (kubeconfig positive control x2, `src-run-t-times-out`, `src-run-t-kills-the-whole-group`): **1146**; then +2 for the unbudgeted-script check (one per recorded script): **1148**.

### What changed

| Item | Before | After |
|---|---|---|
| D8b/D8c/MH-pool/MH-prune/debug-image cold+warm | 36 whole converges (`dep`) | `dstep <fn>`: one step under the real `__deploy_exit_trap`, bounded (default 60s) |
| settle deadline cases (8) | 1-2s real wall-clock spin each | fake `date +%s` clock (absolute real-date exec), exact one-poll control, 10s bound |
| deploy-test env | hand-written at ~20 sites | ONE `d4` (harness-named overrides only) |
| (P) env | prov/psrc + 2 hand-run sites | ONE `penv` |
| e2e runs | uncounted | `E2E_BUDGET` deploy.sh=31 provision.sh=25, `==`, recorded in `src_run` (snippet calls `main`) / `run_main`; static check for raw `bash` runs |
| host seal | PATH only | + `KUBECONFIG` -> missing file under `$WORK` (the container exports a real one) |
| Layer 3 queue | no per-entry time | `LAYER=3 STEP=<prefix> DURATION=<s>` (stderr, `emit_step_duration`, the pipeline's per-step shape; renamed from `ENTRY_DURATION` at Gate 3, DRY F1 + observability) per entry, timed inside the entry; none for a dead runner (its runner-died STATUS line is the signal); entries registered heaviest first |
| deploy-rc | (no order check vs root wait) | `deploy-writes-mh-template-before-pool`, `deploy-mh-pool-after-root-wait` (shared stub events log) |

### Go-red table (mutation of the script under test; old = start-commit setup.test.sh, new = this change; same tree otherwise)

| # | Mutation | Old suite | New suite | Reading |
|---|---|---|---|---|
| a | shrink drains the LOWEST baseline slot (`max`->`min`) | red, 1 (`mh-pool-above-baseline-drains-highest`) | red, same 1 | equivalent |
| b | settle sums `sel` across pods | red, 14 (442s: real-clock waits run out) | red, same 14 (388s) | equivalent |
| c | recompute control (b) dropped | red, 15 | red, same 15 | equivalent |
| d | `not-available-timeout` -> `mh-pool-failed` | red, 3 | red, same 3 | equivalent |
| e | settle deadline never breaks | **HANG** (suite killed at 600s) | red, 44, in 141s (every deadline case killed by its 10s bound); no surviving stub/deploy process (/proc scan) | new strictly better |
| f | MH prune drops `previous_images` | red, 1 | red, same 1 | equivalent |
| g | `preload_debug_image` always pulls | red, 1 (`deploy-debug-image-warm-no-pull`) | red, same 1 | equivalent |
| h1 | rename `RENDERS_CACHED` in deploy.sh | n/a (no seam) | red, 66 (`dstep-seed-*-reads-back` + every dstep case) | new control |
| h2 | rename `ROOT_RENDER` in deploy.sh | n/a | red, 64 (`dstep-seed-root-reads-back` + settle cases) | new control |
| i | one extra `dep` | n/a | red, `[e2e-budget-deploy]` | new control |
| i' | a raw `bash "$DEPLOY"` run | n/a | red, `[e2e-budget-no-unrecorded-bash-run]` | new control |
| j | `__deploy_exit_trap` always emits a 2nd line | red, 23 (18 MH-pool/settle) | red, same 23 | equivalent: the dstep cases run under the real trap |
| k | unguarded `false` after `mh_run upgrade` | red, 84 | red, 84: the same failures except the old suite's e2e debug-image and MH-prune cases, which went red only incidentally (their converge died in the pool step); the new suite's 7 fake-clock controls go red instead | equivalent on the target (set -e live in dstep) |
| l | prune drops the CURRENT ref | red, 2 | red, same 2 | equivalent |
| m | main runs `converge_mh_pool` before `wait_for_env_root` | **green** (pre-existing gap) | red, `deploy-mh-pool-after-root-wait` | gap closed |
| n | `export KUBECONFIG` seal removed | n/a | red, `kubeconfig-sealed-to-a-missing-file` | new control |
| o | `src_run -t` gains `--foreground` (Gate 3, security S2) | n/a | red, `src-run-t-kills-the-whole-group` | new control |
| p | a `bash "${DEPLOY}"` run (brace spelling; Gate 3, code-reviewer F1) | n/a | red, `e2e-budget-no-unrecorded-bash-run` | new control |
| q | no kubectl on the sealed PATH (Gate 3, security S1 / code-reviewer F2) | n/a | red, `kubeconfig-probe-positive-control-rc` + `-reads-decoy` (the seal cases would otherwise pass vacuously) | new control |
| r | a stray `src_run "$LIB_COMMON" 'main() { :; }; main'` (a script with no budget key; Gate 3, paired-test F1) | n/a | red, `[e2e-budget-unbudgeted] common.sh` | new control |

Mutations a, e, h1, h2, i, i', m, n ran on the final file. The rest first ran on an earlier revision and were re-run on the final file with identical failure sets. Logs: scratchpad `mut/<m>.<old|new|new2>.log` (not kept in the repo).

---

## Files Modified

```
 docs/TODO.md                                      |   6 +-
 docs/specialist-knowledge/infrastructure/INDEX.md |   4 +-
 scripts/lang/_common.sh                           |  14 +-
 scripts/lang/_common.test.sh                      |  15 +-
 scripts/setup.test.sh                             | 320 ++++++++++++++++------
```

### Key Changes by File
| File | Changes |
|---|---|
| `scripts/setup.test.sh` | `src_run -t` + e2e recording; `E2E_BUDGET`/`e2e_record`/`run_main`; KUBECONFIG seal; `penv`; `d4`; `dstep` + seeded render + read-back control; fake clock; 36 cases converted; events-log order assertions; (H) seal/budget/static checks |
| `scripts/lang/_common.sh` | each `parallel_queue_run` entry ends its stderr with `emit_step_duration` (`LAYER=<n> STEP=<entry> DURATION=<s>`), timed inside the entry's subshell; a dead runner emits none |
| `scripts/lang/_common.test.sh` | order test expects the lines (normalised), died entry `-`; measured-duration case |
| `docs/TODO.md` | 1863 RESOLVED; 1865 refreshed (layer-all.test.sh growth + cause, new critical path); 2926 folded into 1865 and deleted (DRY F2) |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | setup.test.sh helpers; per-entry duration line ATION pointer |

---

## Devloop Verification Steps

`./scripts/layer-fast.sh`: PASS (rc 0, 261s): L1 10s, L2 6s, L3 83s, L4 158s, L5 2s, L6 2s. Layer 7 not run (Lead's Gate 2). After the Gate-3 DRY fixes: PASS (rc 0, 274s), L3 95s, `WARN BUDGET_TOTAL_BREACH GUARD_AUDIT_DURATION=97` — measured while other agents were loading the machine (every Layer 3 entry 15-20% slower than the quiet 3-run medians: setup 77s vs 68s, run-story 45s vs 38s). The quiet median (85s) is inside the budget by a thin margin; recorded in the TODO 1865 entry. After the Gate-3 fixes (incl. the heaviest-first reorder): PASS (rc 0, 289s), L3 88s + L6 2s = 90s, no WARN, at load average ~8-10.

---

## Gate 2 (2026-11-22)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree. Result: `TOTAL_DURATION=2238 TOTAL_RESULT=OK`, exit 0, with no `BUDGET_TOTAL_BREACH` (Layers 3+6 = 81s, under 90s). Verdict: `GATE2=PASS REASON=none LAYER_ALL_EXIT=0 SIGNAL=none`. Layers: L1 10s, L2 6s (no fmt rewrites), L3 79s (top entries: setup 73s, run-story 43s, frame-vectors 39s), L4 151s, L5 2s, L6 2s, L7 1988s.

The first attempt was refused with `PRECONDITION_FAILURE REASON=concurrent-layer-all`, because the implementer's own layer-fast held the lock. That's the lock from `2026-11-21-killed-run-gate2-pass` working as designed. It was retried after that run exited.

Note: the Lead's Phase updates to this file were overwritten during concurrent edits by teammates; the Phase row was restored to `complete` here.

## Lead fast check (2026-11-22)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-fast.sh`: L1–L3 OK (L3 78s), L4 FAIL on `ac-service crypto::tests::test_jwt_iat_one_second_past_boundary` ("Token with iat 1 second past boundary should be rejected"). That's unrelated to this diff: a wall-clock second-boundary race (`now` read before sign/verify; if a second ticks, iat lands exactly on the inclusive skew boundary). A Layer 4 re-run was OK, and layers 5 and 6 OK. Surfaced to the user as a separate fix: `crates/ac-service/src/crypto/**` is a GSA, so it needs auth-controller + security.

## Code Review Results

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | S1 KUBECONFIG seal positive control; S2 standing group-kill case for `src_run -t` |
| Test (paired-test) | RESOLVED-FIXED | 1 | 1 | 0 | F1 `[e2e-budget-unbudgeted]`; independent mutations (e)(j)(m)(n) plus 2 of its own; setup.test.sh median 125.5→67.4s |
| Observability | CLEAR | 0 | 0 | 0 | duration line reuses `emit_step_duration`; no line for a died runner |
| Code Quality | RESOLVED-FIXED | 3 | 3 | 0 | bypass regex; KUBECONFIG positive control; plan file table |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | one duration-line home; TODO 2926 merged into 1865 |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | queue model in TODO 1865; heaviest-first layer3.sh; runbook breach recipe |

### Security Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Test Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Observability Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Code Quality Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### DRY Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED

**True duplication findings** (entered fix-or-defer flow):
{List findings sent to implementer, or "None"}

**Extraction opportunities** (appended to `docs/TODO.md`):
{One bullet per `docs/TODO.md` entry added, citing the section heading the entry was added under, or "None"}

### Operations Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Semantic Guard Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Native verdict**: SAFE / UNSAFE (mapped by Lead per `.claude/agents/semantic-guard.md` §Verdict Mapping)
**Findings**: {count} found, {count} fixed, {count} deferred

{Per-finding `[check-name]: file/path.rs:line - description` block, or "No findings"}

{Note any findings folded with Code Reviewer at Gate 3 §Deduplication, with "(also flagged by code-reviewer)" attribution.}

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

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: two independent units. (1) `scripts/lang/_common.sh` + `_common.test.sh` (the per-entry duration line) revert together, on their own, safely. (2) `scripts/setup.test.sh` reverts as one file: a partial revert would leave cases calling helpers (`d4`, `dstep`, `run_main`) that no longer exist, or an `E2E_BUDGET` that no longer matches the run count. Docs follow (2). No production script, so no cluster state to undo. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

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
