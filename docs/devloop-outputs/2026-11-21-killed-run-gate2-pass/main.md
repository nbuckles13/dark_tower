# Devloop Output: A killed layer-all.sh must never yield GATE2=PASS

**Date**: 2026-11-21
**Task**: Root-cause and fix the Gate-2 verdict path so a killed/incomplete `layer-all.sh` run can never emit `GATE2=PASS` (docs/TODO.md:2226); prove with a test that kills a run mid-layer.
**Specialist**: infrastructure (paired with test)
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/layer-all-devloop-helper`
**Duration**: ~6h (planning through Gate 2; Gate 2 itself 41m)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `d74304338aaad0a82a43e179a5027a4d4e1e1744` |
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
| Semantic Guard | not spawned (no checks.md surface in a shell verdict-path diff) |

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
| `scripts/lang/_layers.sh` (new) | Mine | — |
| `scripts/lang/_common.sh` | Mine | — |
| `scripts/lang/_child_group.sh` (new) | Mine | — |
| `scripts/layer7.test.sh` | Mine | — |
| `scripts/layer-all.sh` | Mine | — |
| `scripts/lang/_gate2_binding.sh` | Mine | — |
| `scripts/verify-completion.sh` | Mine | — |
| `scripts/workflow/run-story.sh` | Mine | — |
| `scripts/workflow/run-story.test.sh` | Mine | — |
| `scripts/layer-all.test.sh` | Mine | — |
| `scripts/layer1.sh` | Mine | — |
| `scripts/layer2.sh` | Mine | — |
| `scripts/layer3.sh` | Mine | — |
| `scripts/layer4.sh` | Mine | — |
| `scripts/layer5.sh` | Mine | — |
| `scripts/layer6.sh` | Mine | — |
| `scripts/layer7.sh` | Mine | — |
| `scripts/lang/_audit_gate.test.sh` (fixture: `stage_common_lib`) | Mine | — |
| `scripts/lang/_test_helpers.sh` (`stage_common_lib`: the one fixture list of what `_common.sh` sources) | Mine | — |
| `crates/dt-guard/tests/credential_leak_key_custody_fixtures.rs` (fixture: also copy `_layers.sh`) | Mine | — |
| `scripts/lang/_common.test.sh` | Mine | — |
| `scripts/lang/_layer_skeleton.test.sh` | Mine | — |
| `scripts/lang/_get_base_ref.behavior-equivalence.test.sh` (fixture: also link `_layers.sh`) | Mine | — |
| `scripts/lang/_wrapper_trap.test.sh` | Mine | — |
| `scripts/lang/_dispatch.test.sh` | Mine | — |
| `scripts/guards/simple/selftest-gate2-verdict.sh` | Mine | — |
| `docs/runbooks/devloop-validation.md` | Not mine, Minor-judgment | operations |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | Mine | — |
| `docs/TODO.md` | Mine | — |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |

---

## Planning

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (rev 4; C1-C4) |
| Test (paired-test) | confirmed (rev 4; C1-C9, G1-G6) |
| Observability | confirmed (rev 4) |
| Code Quality | confirmed (rev 4) |
| DRY | confirmed (rev 4) |
| Operations | confirmed (rev 4) |

### Lead rulings during planning (2026-11-21)

- **Scope widened (complete the invariant):** every hardcoded layer list 1..7 on the gate path moves to `LAYER_MAX`, reachable through a dependency-free home sourced by `_common.sh`. A second `LAYER_MAX=7` literal is blocked.
- **Single source of truth for PASS:** one exit-derivation function in `layer-all.sh` (a missing terminal status counts as UNKNOWN, exit 2), called by both the EXIT trap and the normal end. `emit_gate2_verdict` maps rc 0 to PASS. Consumer-side per-layer re-checks (security S4) are declined as the parallel check the operator forbade.
- **Same-class inclusion:** the tree signature is taken at exit, not at run start. In scope by default.
- **Signal traps:** must not hold TERM until the running layer finishes (the reaper would then SIGKILL with no verdict written).

### Implementer plan (infrastructure, 2026-11-21)

**Root cause — reproduced, hypothesis CONFIRMED.** Hermetic repro: real `layer-all.sh`, stub layers via the `DEVLOOP_TEST`/`LAYER_SCRIPT_DIR` seam, stub layer 7 sleeps; signal layer-all mid-layer-7.

| Signal (target) | process exit | verdict |
|---|---|---|
| TERM (pid) | 143 | `GATE2=PASS LAYER_ALL_EXIT=0`, LAYER 1–6 only |
| HUP (pid) | 129 | same |
| INT (process group, = Ctrl-C) | 130 | same |
| TERM (process group, = reaper killing the tree) | 143 | same |
| KILL (pid), prior PASS run on disk | 137 | prior run's `GATE2=PASS` left in place (RUN_AT of the earlier run) |

Mechanism-language: **the verdict is derived from an input that is not the run's outcome.** Bash runs the EXIT trap from its fatal-signal handler; there `$?` is the last *completed* command's status (0 here), not 128+N, and bash then re-raises the signal so the process exits 143 while the file says 0. Leg 2: the verdict file is only ever written at exit, so an uncatchable death leaves whatever an earlier run wrote. Wider class (same root, raised for ruling below): an EXIT trap or end-of-script that renders a verdict from `$?` or from whatever partial state accumulated.

**Design (rev 3 — supersedes rev 1/2; folds every Gate-1 input and Lead ruling).**

*Ordering in an authority run:* traps install → stale-snapshot sweep → start snapshot → run-start verdict → sentinel / merge-base preconditions → layer loop → normal end. Fast-lane and shard runs get the same traps and the same in-flight-layer teardown, but never touch the verdict or the snapshot.

1. **`LAYER_MAX` home: `scripts/lang/_layers.sh` (new).** Plain assignments only: `LAYER_MAX`, `LAYER7_SUITES`, `layer7_suite_known`. No shell options. `_common.sh` and `_gate2_binding.sh` source it, and run-story.sh reaches it through the binding.
   - Sites switched: emitter loop; `verify-completion.sh:92`, with the usage text at `:10`/`:49` interpolated; run-story.sh `:1063` message, `:1528` comment and `:1548-1549`; run-story.test.sh `:389` and `:649`, plus prose at `:1824`/`:1937`; layer-all.test.sh `mk_all_ok` and the expected text at `:504`.
   - Guard (layer-all.test.sh): exactly one assignment matching `^[[:space:]]*(readonly[[:space:]]+)?LAYER_MAX=` across non-`*.test.sh` files under `scripts/`, and it must be in `_layers.sh`. A negative fixture string in the test shows that an inline env use like `LAYER_MAX=3 cmd` inside a test is not counted, because test files are out of scope.
2. **Process-group lib: `scripts/lang/_child_group.sh` (new, dependency-free).** run-story.sh's `kill_child_group` (TERM the session, poll `kill -0` 10×0.5 s, then KILL, then reap) moves here verbatim. The spawn and interruptible-wait half of `run_isolated` moves too, as `child_group_spawn` and `child_group_wait`. `run_isolated` stays in run-story as a thin wrapper that adds the lane and log handling. There is one copy.
   - layer-all runs each layer as `child_group_spawn bash -c '"$1" 2>>"$2" | tee "$3"; exit "${PIPESTATUS[0]}"' _ <layer> <err> <log> </dev/null`, then `child_group_wait`. That puts the layer in its own setsid session, and its rc and logs are unchanged.
   - Only the recorded pid/pgid is ever signalled; there is no pattern kill.
   - stdin is `/dev/null` because the layer no longer owns the tty. No layer reads stdin.
   - SIGKILL orphan: closed by R4-1 below.
3. **One signal trap for TERM/INT/HUP** records `__signal=<NAME>` and runs `exit $((128+N))`. It fires at once because layer-all is in an interruptible `wait`.
4. **Two derivation functions in layer-all.sh.**
   - **`derive_layer_all_exit`** is the exit rule, used by the normal end, the EXIT trap and `aggregate_shard_summaries` (DRY #1; aggregate's `agg_fail` diagnostics stay ahead of the call).
     - It walks `layers_to_run` once and fills the global `layer_missing` csv.
     - A layer counts as recorded only if it has BOTH `layer_rc[n]` and `layer_status[n]` (security C1). The loop now assigns rc before status. An unrecorded layer is UNKNOWN, i.e. 2.
     - A `NOT-RUN` layer is skipped only when `stopped_early>0`; otherwise it yields 2 (code-reviewer #3). NOT-RUN never reaches `status_to_exit_code`, so the guardrail holds.
     - Result: max(`status_to_exit_code(worst)`, every rc, `$1`=floor).
     - It is total under `set -u`: every read has a default. The trap runs it under `set +e`, and if it fails the result is exit 2.
   - **`derive_gate2`** is the ONE home of PASS. `gate2_verdict=PASS` iff `layer_all_exit==0` AND both snapshots exist with non-empty signatures AND they are equal. Anything else is FAIL, failing closed (security C2). `gate2_reason` comes from a closed set: `none` (PASS) | `layer-failed` | `pipeline-incomplete` (layers missing: a signal, an untrapped death, an abort or a pre-loop exit) | `tree-changed-during-run` | `tree-snapshot-failed` | `in-progress` (the run-start verdict).
5. **EXIT trap (`__layer_all_exit_trap`, all modes).** It starts with `local rc=$?; set +e`, then:
   - (a) Floor = 128+N if a signal was trapped, else `rc`.
   - (b) `derive_layer_all_exit floor`.
   - (c) `kill_child_group` for the in-flight layer. This is bounded at about 5 s, and its failure is ignored, so it can never skip the emit.
   - (d) If the loop started but didn't reach its end, print `PIPELINE_INCOMPLETE MISSING=<csv> IN_FLIGHT=<n|none> SIGNAL=<TERM|INT|HUP|none> LAYER_ALL_EXIT=<n>` on stderr (observability (d) plus ops' signal field). It uses the `layer_missing` list from (b), never a second loop, and fail-fast NOT-RUN layers are excluded.
   - (e) Authority mode: exit snapshot, then `derive_gate2`, then emit from the START snapshot, then remove the snapshot dir.
   - Process exit = floor. Pre-loop exits keep their own codes: the sentinel leak stays exit 1, which layer-all.test.sh 7a/7b pin. A caught signal exits 128+N, and an untrapped signal is re-raised by bash. The verdict's `LAYER_ALL_EXIT` is the derived value, so for a pre-loop exit 1 or an untrapped death it can exceed the process exit. That is documented. No caller keys on the verdict field being 1. run-story uses the process rc, so 128+N lands in the operator lane.
6. **Snapshot (Lead ruling 4 + security C2 + ops A1/A3).**
   - `gate2_snapshot_worktree <dir>` captures HEAD, BASE_REF, slug, the NUL records and the signature. It is the ONLY tree reader: the emitter's inline HEAD/slug/signature code is deleted.
   - `<snapshot>` is MANDATORY on `emit_gate2_verdict`; there is no fallback. Test callers snapshot explicitly.
   - The snapshot dir is a fixed `${DEVLOOP_TMP}/gate2-snapshot/`, `rm -rf`'d at the start of every authority run. That is the sweep, so a SIGKILLed run's leftover goes on the next start.
   - If the start snapshot fails → `PRECONDITION_FAILURE REASON=gate2-snapshot-failed`, exit 2, before Layer 1. The verdict is removed, so a missing verdict counts as fail-closed.
   - If the exit snapshot fails or a signature is empty → FAIL `tree-snapshot-failed`.
   - If the signatures differ → FAIL `tree-changed-during-run`, and stderr gets `WARN GATE2_TREE_CHANGED_DURING_RUN PATHS=<first 5, csv> — verdict will not bind; re-run before commit`.
7. **Run-start verdict.** It is written through the same emitter from the start snapshot as `GATE2=FAIL REASON=in-progress`, with no LAYER lines. If the write fails → `rm -f`; if a verdict is still there → `PRECONDITION_FAILURE REASON=gate2-verdict-invalidate-failed`, exit 2.
   - Security's (a) blocks.
   - Security's (b) also blocks, deliberately, because the latest run never finished. This is documented.
   - The contract becomes: a missing file means never invoked or the snapshot was refused; `REASON=in-progress` means running now or killed uncatchably. That wording is updated together in layer-all.sh `:387`, the binding header and runbook §8.5.
8. **Emitter.** Signature: `emit_gate2_verdict <verdict> <reason> <exit> <ls> <ld> <snapshot>`.
   - `GATE2=PASS` only for the literal `PASS`; anything else is FAIL.
   - New keys: `REASON=`, `MISSING=` and `SIGNAL=`.
   - LAYER lines over 1..LAYER_MAX: a missing layer renders as `LAYER n MISSING 0` and fail-fast NOT-RUN as `LAYER n NOT-RUN 0`, display only.
   - `GATE2_SCHEMA` = `gate2-verdict/v2`.
   - `verdict-not-pass:` appends `REASON=` (observability (c)).
9. **Layer lifecycle (Lead ruling, token `layer-incomplete`).** `layer_lifecycle_done` sets the normal-end mark. It is called at:
   - the end of `layer1.sh`…`layer6.sh`;
   - `layer7.sh` `:295` (`precondition_fail`), `:595` (`implementer_fail`), `:1035` (SKIPPED-NO-CLUSTER) and after `__layer7_main`.

   With no mark, `__layer_lifecycle_end` renders `STATUS=UNKNOWN`, with `REASON=layer-incomplete` on the LAYER line. The token is constant with no rc suffix, and it covers both signals and `set -e` aborts. A layer that forgets the call reads UNKNOWN, exit 2, on every run, which is loud. Synthetic layers in `_common.test.sh`, `_wrapper_trap.test.sh` and `_dispatch.test.sh` gain the call.
10. **Docs.**
    - Runbook:
      - §3 `:252`; `:177` (UNKNOWN now has a wrapper emitter) and `:1468`.
      - §8.5: lifecycle, the derivation, the start-bound tree, fmt APPLY (opt-in `DEVLOOP_FMT_APPLY`) always forcing a re-run when it rewrites, and FAIL with no LAYER lines meaning `in-progress`: check whether a layer-all is still running, and if not it was killed uncatchably, so re-run and check memory pressure.
      - Token rows: `PIPELINE_INCOMPLETE`, `GATE2_TREE_CHANGED_DURING_RUN`, REASON values, and `layer-incomplete` under UNKNOWN with observability's wording on telling the two causes apart.
      - Symptom row: exit 129/130/137/143 is the operator lane; re-run, and if it repeats check memory pressure or a concurrent run; no orphan to wait for.
      - Sentence: process 128+N vs the verdict's derived `LAYER_ALL_EXIT`.
    - ADR-0033 §4 dated amendment. TODO `:2226` marked `[x]`. Header comments. Infra INDEX.

**Tests.** The harness follows @paired-test C1–C9.
- Runs are launched as `setsid env --default-signal=INT … bash layer-all.sh &`, with the pgid read from `/proc`.
- The blocking stub is a tree: `sleep 120 & echo "$$ $!" > $DEVLOOP_TMP/l<n>.started; wait`.
- Marker poll: 30 s cap at 0.05 s steps. Promptness: a 20 s exit cap with a named label, then SIGKILL the group.
- Survivors: BEFORE any harness reaping, poll for up to 10 s that both recorded pids are gone (`k<n>-no-surviving-layer`). The harness EXIT trap kills every recorded pid as a backstop.
- Every K case asserts positively: `GATE2=FAIL`, the exact `REASON`, the exact `MISSING=`, a non-zero `LAYER_ALL_EXIT`, process rc == 128+N, and the `PIPELINE_INCOMPLETE` line. Verdict reads go through `gate2_verdict_get`.

Cases:
- **K1–K4:** {TERM, INT, HUP} × {pid, group} with layer 7 blocking; trapped signals also assert `SIGNAL=` and `LAYER_ALL_EXIT`==rc. Plus TERM with layer 3 blocking → `MISSING=3,4,5,6,7`, `ran.layer4` absent.
- **K-USR1:** an untrapped, default-fatal signal → `pipeline-incomplete`, `SIGNAL=none`. This is the revert test for "unrecorded = UNKNOWN".
- **K5 (SIGKILL):** a control full run on a fixed `DEVLOOP_TMP` asserts PASS first. The re-run is SIGKILLed → `GATE2=FAIL REASON=in-progress`, no LAYER lines, and the file content differs from the saved PASS copy (`cmp`, not RUN_AT).
- **K6:** the layer's own process is SIGKILLed → rc 137 → FAIL. This is a regression guard and already green today.
- **K-lifecycle (ops B2):** a real `_common.sh` lifecycle stub layer has one OK child and then blocks. TERM goes to the layer pid only → UNKNOWN `layer-incomplete` in `layer-N.log`, the summary row and the verdict `LAYER` line; layer-all survives and the verdict is FAIL.
- **K-tree (security C3):** layer-all runs inside a throwaway `git clone --shared` of the repo, so the live worktree is never mutated (paired-test C7). A stub layer edits a tracked file → `tree-changed-during-run` plus the WARN line, and `gate2_validate_commit` over the staged edit blocks.
- **Controls:** green baseline → PASS with `REASON=none`. A fail-fast stop → FAIL with no `PIPELINE_INCOMPLETE`.
- **Lifecycle unit tests (`_common.test.sh`, real child bash):**
  - L1: one OK child, then TERM to the pid and to the group → `STATUS=UNKNOWN`, `REASON=layer-incomplete` pinned, rc 143.
  - L2: `false` after an OK child → UNKNOWN, rc 2.
  - L3a: falling off the end after `layer_lifecycle_done` → OK rc 0.
  - L3b: emit-then-exit via the done mark → its own status.
  - Layer 7's three deliberate exit sites are pinned in `layer7.test.sh`. I'll extend its hermetic cases if they don't already reach each site.
- **Selftest (library level):** literal-PASS-only emitter (`FAIL`, empty and `pass` all give FAIL); snapshot → edit → emit → hook blocks; snapshot → no edit → allows; the ~17 fixtures move to the new signature with an explicit snapshot.

**Rev 4 amendments (override rev 3 where they conflict).**

- **R4-1 Orphan closure and concurrency (Lead ruling, ops B3(c) criteria, security C4).** This replaces rev 3 item 2's "known and accepted".
  - **Lock:** every layer-all mode that spawns layers (authority, fast lane, shard) first takes `flock -n` on `${DEVLOOP_TMP}/layer-all.lock`. Child layers are spawned with that fd closed, so the lock is released when layer-all dies, even on SIGKILL, and an orphan cannot hold it.
    - If the lock is busy → `PRECONDITION_FAILURE REASON=concurrent-layer-all`, exit 2. Nothing is killed. The message prints the recorded owner pid and starttime and the live identity, so the operator can find the other run with `kill -0 <pid>`.
    - flock is the atomic form of "owner alive + identity matches". A recycled owner pid can't cause a false refusal, because the lock is gone with the dead owner.
    - This closes security C4 too: two runs can never share the snapshot dir or the verdict.
  - **In-flight record:** `${DEVLOOP_TMP}/layer-all.inflight` holds owner pid + owner starttime (`/proc/<pid>/stat` field 22) + layer n + pgid + leader starttime + layer script path + RECORDED_AT.
    - Written when a layer launches. Removed after the in-trap teardown on normal or trapped exit, so it survives only an uncatchable death.
  - **At start, holding the lock, if a record exists:**
    - Leader identity matches (live pgid leader, same starttime, cmdline has the recorded script path) → `kill_child_group` it and print `REAPED_STALE_LAYER_GROUP PGID=<g> LAYER=<n> OWNER_PID=<p> RECORDED_AT=<t>` (token agreed with @observability).
    - Mismatch or group gone → drop the record and print `STALE_LAYER_RECORD_DROPPED PGID=<g> REASON=<leader-gone|identity-mismatch>`. Nothing is signalled.
  - **Layer start line:** each layer prints `LAYER_START LAYER=<n> PGID=<g>` on stderr.
  - **Precondition:** `flock` and `setsid` (util-linux) are checked up front; if either is missing → `PRECONDITION_FAILURE`, exit 2.
- **R4-2 Kill bound SSoT (ops B3(a)(b)).**
  - `_child_group.sh` owns `CHILD_GROUP_GRACE_POLLS=10` and `CHILD_GROUP_POLL_SECS=0.5`. `kill_child_group [polls]` defaults to the grace.
  - layer-all's trap passes `$((CHILD_GROUP_GRACE_POLLS / 2))`. That is derived from the same constant, so it is always strictly below run-story's grace (2.5 s vs 5 s).
  - The exit-time tree hash is skipped whenever the verdict is already FAIL (code-reviewer C). That leaves only the normal end hashing, so the trap's total time is teardown plus emit.
- **R4-3 Snapshot dir.** It stays at a fixed `${DEVLOOP_TMP}/gate2-snapshot/` because it is now only ever used under the lock.
  - At start: `rm -rf` it, then `mkdir -m 700`. `mkdir` failing means something is already at the path (a symlink or a race) → `PRECONDITION_FAILURE REASON=gate2-snapshot-failed`. A pre-existing symlink is never followed.
  - The dir is removed on every trap path, including the invalidate-failed and snapshot-failed exits.
- **R4-4 REASON (observability; dry: one producer).**
  - `derive_gate2` is the only producer, including `in-progress` (its start-phase call).
  - Precedence, stated in the derive_gate2 header and the runbook: `pipeline-incomplete > layer-failed > tree-snapshot-failed > tree-changed-during-run`. `in-progress` appears only on the start verdict.
  - Pre-loop refusals are folded into `pipeline-incomplete` with `MISSING=<all>` and `SIGNAL=none` (observability accepted this).
  - `verdict-not-pass:` appends `REASON=`, `SIGNAL=` and `MISSING=`.
  - A fail-fast stop counts as reaching the end of the loop: no `PIPELINE_INCOMPLETE`, and `REASON=layer-failed`.
- **R4-5 Run-start verdict check (G4).** The emitter moves the file into place with `mv -fT`, so it can never land inside a directory.
  - After the start emit, layer-all checks that a regular file at the path reads `REASON=in-progress`. If not → `rm -f`. If something still exists → `PRECONDITION_FAILURE REASON=gate2-verdict-invalidate-failed`.
  - The test makes the verdict path a non-empty directory. `rm -f` fails on that even as root, so the test is deterministic and root-proof with no chmod.
- **R4-6 Answers to CR #1–#3.**
  - #1: the trap opens with `local rc=$?; set +e`. Every array read has a default, and a failed `derive_*` gives exit 2 and FAIL.
  - #2: the process exit is the floor, so a pre-loop exit keeps its own code (the sentinel leak stays 1, pinned by 7a/7b). Only the verdict field carries the derived value. I grepped: no caller keys on the verdict field's value.
  - #3: a NOT-RUN layer with `stopped_early==0` gives 2, and NOT-RUN never reaches `status_to_exit_code`.
- **R4-7 Tests added to rev 3's list.**
  - **K-tree** replaces `clone --shared`. Fixture: `git init` in `$WORK`, one commit, `update-ref refs/remotes/origin/main HEAD`, then cwd in the fixture runs the real layer-all with stubs. `DEVLOOP_TMP` is outside the fixture. I'll confirm layer-all resolves the repo from cwd, not `__here`.
    - T1: stub layer 3 edits a tracked file → `tree-changed-during-run` plus the path on stderr.
    - T2: the same fixture with no edit → PASS.
    - T3: a new untracked, non-ignored file → `tree-changed-during-run`.
    - T4: two causes at once (layer 3 FAILs and edits) → `REASON=layer-failed`, which pins the precedence.
  - **Kill bound:** a stub that traps and ignores TERM is reaped by KILL within the bound, with no survivors.
  - **Orphans:**
    - SIGKILL the layer-all group mid-layer, then re-run → `REAPED_STALE_LAYER_GROUP` is printed and both recorded pids are dead before Layer 1.
    - A record whose leader starttime is wrong (pointing at a live unrelated sleeper) → that sleeper is NOT signalled, and `STALE_LAYER_RECORD_DROPPED` is printed.
    - Concurrent: run A blocks in layer 3; run B → `concurrent-layer-all`, exit 2; A is untouched and later completes.
  - **G2:** a shard (`--layers 3`) and a fast lane (`--max-layer 6`), each killed mid-layer → a pre-seeded sentinel verdict is byte-identical afterwards, no verdict is written, and nothing survives.
  - **G3:** a v1-schema PASS over the matching tree is rejected by the hook.
  - **G5:** the stub records layer pid and grandchild pid, and both are polled dead before any reaping.
  - **G6:** each K case asserts the exact `PIPELINE_INCOMPLETE MISSING=… IN_FLIGHT=… SIGNAL=… LAYER_ALL_EXIT=…` line (USR1 gives `SIGNAL=none`).
  - **G1:** L2 (`set -e`) and L1's group-TERM variant are in. Layer 7's done-calls get named `layer7.test.sh` cases: `l7-done-precondition-fail`, `l7-done-implementer-fail`, `l7-done-skipped-no-cluster` and `l7-done-normal-end`. Each turns red if its call is deleted.
  - **Fail-fast control:** no `PIPELINE_INCOMPLETE`, and `REASON=layer-failed`.
- **R4-8 Runbook additions.** Rows for:
  - `REAPED_STALE_LAYER_GROUP` / exit 137 (an orphan was auto-reaped at the next start).
  - `concurrent-layer-all`: find the other run, then wait for it or stop it.
  - fmt-apply as the usual `tree-changed-during-run` cause, with the fix: run layer-fast (or `scripts/fmt.sh`), then layer-all.

  The "no orphan to wait for" text is corrected.
- **R4-9 Gate-2 evidence (Lead Step 7.5).** The design needs a real authority run on a clean tree to come out PASS. Before "Ready for review" I'll check with `git status --porcelain` before and after `layer-fast.sh` that layers 1–6 leave the tree unchanged: no codegen output, no lockfile drift. I'll record the result here.

- **R4-10 Late additions (Lead, ops, security, paired-test, observability).**
  - (a) **Per-layer reap.** After every layer's `child_group_wait` returns, layer-all runs `kill_child_group` on that layer's session, bounded by the layer-all grace. Normally this is a no-op. A layer killed on its own while layer-all carries on can therefore not leave a cargo or sleeper child running. Test: K-lifecycle also asserts that the recorded grandchild is gone once layer-all has moved past that layer.
  - (b) **Kill before emit, deliberately.** The trap kills the in-flight layer first, so the exit snapshot and the verdict follow the layer's death. The emit is delayed by at most the 2.5 s layer-all grace. If a reaper's KILL lands inside that window, the run-start `REASON=in-progress` verdict is what remains, which is correct.
  - (c) **Lock fd never reaches a layer.** The lock fd is closed on every `child_group_spawn` (`{lockfd}>&-`). Otherwise an orphan would keep holding the lock and the reap path would be dead code.
    - Test: an authority run with a blocking stub; SIGKILL layer-all only, so the stub survives. The next run must acquire the lock, print `REAPED_STALE_LAYER_GROUP`, leave no stub process, and print no `concurrent-layer-all`.
    - The lock holder writes its pid and `RUN_AT`-format start time into the lock file, for display only. The `concurrent-layer-all` refusal prints the lock path and that holder line.
    - `flock` is prechecked the same way run-story prechecks `setsid`, with a remedy in the message.
  - (d) **Nested bound race test (ops B3(b)).** A layer that traps and ignores TERM. A run-story-style outer `kill_child_group` (full grace) TERMs layer-all's session, waits, then KILLs. Layer-all's inner teardown (half the grace) must KILL the layer first. Assert no layer process survives.
  - (e) **`gate2-snapshot-failed`, root-safe.** Run layer-all from a cwd that is not a git work tree (a plain `mktemp -d`, with `DEVLOOP_TMP` outside it). The start snapshot's git calls fail → `PRECONDITION_FAILURE REASON=gate2-snapshot-failed`, exit 2, and no verdict file is left. The snapshot is taken before the merge-base precondition, so it is the first check to trip.
  - (f) **Observability nits.** `STALE_LAYER_RECORD_DROPPED` also carries `LAYER=<n>`. `RECORDED_AT` uses the `RUN_AT` format (`date -u +%Y-%m-%dT%H:%M:%SZ`). The runbook gains benign/informational rows for `STALE_LAYER_RECORD_DROPPED` and `LAYER_START`. `GATE2_TREE_CHANGED_DURING_RUN` and `PIPELINE_INCOMPLETE` stay in the token list.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

Implemented per plan rev 4 + R4-10. Where the code differs from the plan text, the code governs and the differences are listed here.

### Verdict path (root cause)
| Item | Before | After |
|------|--------|-------|
| PASS decision | EXIT trap: `GATE2=PASS` iff `$?==0` (0 after a fatal signal) | `layer-all.sh::derive_gate2` only (over `derive_layer_all_exit`: a layer counts only with status AND rc; unrecorded = UNKNOWN; NOT-RUN without a fail-fast stop = 2). Shared with `--aggregate` |
| Emitter | derived GATE2 from its exit arg; computed the tree itself | `emit_gate2_verdict <snapshot> <verdict> <reason> <exit> <signal> <missing> <ls> <ld>`: renders, PASS only for literal `PASS`; snapshot mandatory; `mv -fT`; schema v2 (REASON/SIGNAL/MISSING, a LAYER line per layer, `MISSING` for unrecorded) |
| SIGKILL leg | prior PASS survived | authority run writes `REASON=in-progress` from the start snapshot before Layer 1; refuses (`gate2-snapshot-failed` / `gate2-verdict-invalidate-failed`) if it cannot |
| Signals | no traps; layer a foreground pipeline | each layer in its own session (`_child_group.sh`), interruptible wait, one TERM/INT/HUP trap → 128+N; EXIT trap tears the in-flight layer down (TERM→KILL within `CHILD_GROUP_INNER_POLLS` = grace/2), prints `PIPELINE_INCOMPLETE`, then derives and emits |
| Tree | signed at exit | signed at start; a clean end re-snapshots and FAILs `tree-changed-during-run` (+ `WARN GATE2_TREE_CHANGED_DURING_RUN`) on a difference; exit snapshot skipped when already FAIL |
| Concurrency / orphans | none | `flock` on `${DEVLOOP_TMP}/layer-all.lock` (fd closed in layers); in-flight record; next run reaps an identity-matched orphan (`REAPED_STALE_LAYER_GROUP`) or drops a mismatch (`STALE_LAYER_RECORD_DROPPED`) |
| Layer lifecycle | killed layer printed worst finished child (OK) | `layer_lifecycle_done` mark (end of layers 1–6; layer7's 3 deliberate exits + end); missing → `STATUS=UNKNOWN REASON=layer-incomplete`; layers 2/4/5/6 gained `|| true` on the dispatcher pipe so `set -e` cannot skip the mark (outcome unchanged: the collected STATUS lines carry the result, as before) |
| LAYER_MAX | `_common.sh` literal + `1..7` lists in emitter, verify-completion, run-story(.test), layer-all.test | `scripts/lang/_layers.sh` (dependency-free; sourced by `_common.sh` and `_gate2_binding.sh`); lists derive from it; K20 pins one assignment |
| kill_child_group | run-story.sh private | `scripts/lang/_child_group.sh` (moved; run_isolated is a thin wrapper); zombie-aware liveness |

### Additional changes found during implementation
- **bash 5.2.15 lost-signal bug.** Under load, a group TERM that lands while layer-all is running a command substitution makes the trap fail to parse (`trap: unexpected EOF while looking for matching ')'`), so the signal is lost and the run carries on. Reproduced: 5 of 25 runs under CPU load. Fix: the spawn→`wait` window is now fork-free (`proc_starttime_into`, `printf -v %()T`, `$EPOCHSECONDS`, builtin `read` in `child_group_alive`). Re-measured: 0 of 30 runs under the same load. The residual is documented in runbook §8.5: a lost signal cannot forge a PASS, because the run either completes or the reaper's KILL leaves `in-progress`.
- **The hook did not check SCHEMA.** `gate2_validate_commit` now refuses any verdict that is not v2, so the claim that a v1 PASS stops binding is actually true (selftest k5).
- **Fixtures that copy `_common.sh`** (`_dispatch.test.sh`, `_audit_gate.test.sh`, `_get_base_ref.behavior-equivalence.test.sh`, dt-guard's `credential_leak_key_custody_fixtures.rs`) now also copy `_layers.sh`.
- **Residual, not a regression:** when a layer process is killed on its own and one of its children still holds the layer's stdout, `tee` (and therefore layer-all's wait) stays open until that child exits. That was already the behaviour before this change. The per-layer reap handles stragglers that do not hold the pipe (K14).

### Gate-3 review fixes (iteration 1)
- **observability O1–O4.** One `__run_incomplete` predicate drives both the REASON and the `PIPELINE_INCOMPLETE` line. A NOT-RUN layer without a fail-fast stop is counted in MISSING. `__derive_exit_failclosed` prints `WARN GATE2_DERIVE_FAILED` and sets `MISSING=?`. tee ignores the teardown TERM, so a killed lifecycle layer's log ends `UNKNOWN layer-incomplete` (K21).
- **security F1.** Dispatcher exits are recorded with `layer_lifecycle_child_rc`, not dropped with `|| true`. A non-zero dispatcher exit over an exit-0 aggregate is now UNKNOWN layer-incomplete (`_common.test` L4/L4b).
- **security F2.** An authority run removes the old verdict immediately after taking the lock.
- **security F3 / code-reviewer F4.** The teardown falls back to `__CHILD_PID`.
- **security F4.** The runbook now states that an edit made and reverted inside one run is not caught.
- **code-reviewer F1.** `kill_child_group … group-only` is used for the post-wait reap and the stale reap, and never signals a bare pid (K24).
- **code-reviewer F2.** Timestamps use `printf '%(%s)T'` (bash 4.2+) instead of `$EPOCHSECONDS`.
- **code-reviewer F3.** `__run_completed` is set only at the final `exit`, so a post-loop abort reports `pipeline-incomplete` (K22).
- **code-reviewer F6.** `__inflight_reaped` is set when `wait` reaps the leader and reset before each spawn; the trap tears down group-only in the post-wait window (K25).
- **code-reviewer F5.** If the stale leader is gone but its group is alive, the run reaps it group-only and prints `REASON=leader-gone` (K23).
- **code-reviewer residual note.** Added at the spawn site and in the runbook.
- **dry-reviewer F1/F2.** `stage_common_lib` in `_test_helpers.sh` is now the one fixture list of what `_common.sh` sources. The `N+1..LAYER_MAX` comment is fixed.
- **Found while fixing.** `gate2_snapshot_worktree` failed when layer-all was run from a subdirectory: the paths are top-relative. It now runs at the work-tree top whatever the caller's cwd.

### Gate-3 review fixes (iteration 2, paired-test; Lead ruling)
- **F1.** The teardown never actually granted its grace. `kill_child_group` polled the session leader (the `bash -c` wrapper), which dies on TERM at once. Following the Lead ruling, the one shared function now polls the GROUP: `child_group_has_live_member` is a fork-free, zombie-aware `/proc` pgrp scan, run within the same bound before the KILL. run-story's outer teardown therefore gets the same fix. K26 uses a real lifecycle layer whose registered cleanup takes about 1 s; its cleanup marker and its `UNKNOWN layer-incomplete` line both land. K26 is red under a leader-poll mutation (K25 also reddens). K13's dynamic assert reddens on the inner>outer mutation. An interim wrapper-side `trap : TERM` was dropped so there is a single mechanism.
- **F2.** `_layer_skeleton.test.sh` `__check_child_rc` requires every dispatcher pipe into `tee_collect_statuses` to end `|| layer_lifecycle_child_rc "$?"`. `emit_status … |` lines are exempt. The check has negative and positive fixtures, and reverting layer2 to `|| true` fails it by name. layer3's brace group was aligned with the rule.
- **F3.** `__lc_kill` waits with a bound and a named `-exit-prompt` label.
- **F4.** Polls are 0.05 s × 100 (same 5 s grace; inner = grace/2 = 2.5 s). layer-all.test.sh re-measured at 24 s for 429 assertions.
- **F5.** Bash's job-death notices in the harness are swallowed: k_wait_exit runs with stderr to /dev/null.

### R4-9 Gate-2 evidence (pre-review)
`./scripts/layer-fast.sh` (layers 1–6) left the work tree byte-identical (`git status --porcelain`, `git diff`, and untracked-file hashes compared before and after: TREE-UNCHANGED). No real layer routinely dirties the tree, so a real authority run on a clean tree can PASS the start/exit snapshot check.

---

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

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final (post-iteration-2) tree, 2026-11-21, `TOTAL_DURATION=2491 TOTAL_RESULT=OK`, exit 0. Verdict: `SCHEMA=gate2-verdict/v2 GATE2=PASS REASON=none LAYER_ALL_EXIT=0 SIGNAL=none MISSING=` (RUN_AT 2026-11-21T07:59:02Z). The start/exit tree snapshots matched, so this is the R4-9 evidence that a real authority run on a clean tree passes the tree-changed check.

| Layer | Result | Duration | Notes |
|-------|--------|----------|-------|
| 1 cargo check | OK | 8s | |
| 2 fmt | OK | 6s | APPLY mode; no rewrites (tree unchanged) |
| 3 guards + script self-tests | OK | 138s | `WARN BUDGET_TOTAL_BREACH GUARD_AUDIT_DURATION=140 BUDGET=90`, pre-existing (`setup.test.sh` ~114s on main; filed in docs/TODO.md) |
| 4 unit tests | OK | 153s | passed=5 failed=0 ignored=36 |
| 5 integration | OK | 2s | |
| 6 clippy | OK | 2s | |
| 7 env-tests | OK | 2182s | dev-cluster rebuild + full env-test run |

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 4 | 4 | 0 | F1 truncated dispatcher → OK fixed (medium) |
| Test (paired-test) | RESOLVED-FIXED | 5 | 5 | 0 | F1 group-poll grace (K26), F2 skeleton guard, F3 bounded wait, F4 poll step, F5 job notices; 22-revert mutation pass + final-tree re-check, every fix leg has a red-on-revert test |
| Observability | RESOLVED-FIXED | 4 | 4 | 0 | O2/O3 untestable without seam (coverage note) |
| Code Quality | RESOLVED-FIXED | 6 | 6 | 0 | residual stdout-holder wait: accepted, documented |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | stage_common_lib helper; no extraction opportunities |
| Operations | RESOLVED-FIXED | 11 | 11 | 0 | P1–P5, B1–B3, A1–A3 + fmt conditions; CLEAR re-gated on paired-test F1 (group-poll), verified; L3 budget breach pre-existing (TODO setup.test.sh entry) |
| Semantic Guard | not spawned | | | | |


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
