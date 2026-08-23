# Devloop Output: Pipeline fail-fast + guard-timeout operator lane

**Date**: 2026-08-21
**Task**: Two changes to the validation pipeline — (1) reclassify guard timeout/kill from FAIL to the PRECONDITION_FAILURE operator lane at the emission site; (2) make `layer-all.sh` stop at the first failing layer for interactive runs while keeping run-all for unattended callers (story runner, CI).
**Specialist**: operations
**Mode**: Agent Teams (full, `--paired-with=test`)
**Branch**: `feature/fast-fail-guards`
**Duration**: ~1h20m (setup → commit)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `045b8620328099dafa3d85025391d9c1e8f1a3f9` |
| Branch | `feature/fast-fail-guards` |
| Lead Model | `claude-opus-4-8[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Gate 1 | PASSED 2026-08-21 — all 6 reviewers confirmed (security, test, observability, code-reviewer, dry-reviewer, semantic-guard); `validate-cross-boundary-classification.sh` = STATUS=OK; doc grep-verified consistent to ladder-mirror + R-A2 |
| Gate 2 | PASSED 2026-08-21 — `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh` exit 0, TOTAL_RESULT=N/A (proto-placeholder aggregate; success-class). Zero FAIL/PRECONDITION/UNKNOWN. L1 OK, L2 OK, L3 OK (37 guards + all self-tests incl. NEW run-guards.test.sh 20/0, layer-all-orchestrator, run-story), L4 N/A (cargo-test+nx-test OK; proto placeholder), L5 OK, L6 N/A (audit SKIPPED-NO-DIFF; proto placeholder), L7 OK (env-tests-passed + browser-e2e-passed, live cluster, 479s). Change-2 live: PIPELINE_MODE=run-all SOURCE=run-all-env. |
| Implementer | `implementer` (spawned) |
| Implementing Specialist | `operations` |
| Iteration | `1` |
| Security | `security` (spawned) |
| Test | `test` (spawned; paired co-implementer) |
| Observability | `observability` (spawned) |
| Code Quality | `code-reviewer` (spawned) |
| DRY | `dry-reviewer` (spawned) |
| Operations | implementer (this loop's implementer) |
| Semantic Guard | `semantic-guard` (spawned) |

---

## Task Overview

### Objective
Stop the pipeline from spending a full run — including Layer 7's cluster bring-up — and a real Gate 2 attempt to learn about a Layer 3 problem that may be a machine fact rather than a code defect.

**Change 1 — guard timeout → operator lane.** A guard that times out currently reports `STATUS=FAIL` (exit 124 → `guard-timeout-<name>`, exit 137 → `guard-timeout-kill-<name>`), indistinguishable from a guard that found a violation. On 2026-08-14 a pipeline launched into a concurrent cargo workload saw Layer 3 go 13s→713s and return `FAIL guard-timeout-validate-kustomize` on a clean diff, consuming a real Gate 2 attempt. A timeout is a fact about the machine, so it belongs on the operator lane — reclassify to `PRECONDITION_FAILURE`. The taxonomy already supports this (rank ladder, `status_to_exit_code`, aggregation all handle `PRECONDITION_FAILURE`); Layer 7 is simply the only current emitter. **The change is at the emission site, not in the taxonomy.**

**Change 2 — fail-fast for interactive runs.** `layer-all.sh` runs all seven layers even after one fails, accumulating the worst result. For an interactive devloop that is pure waste (you fix Layer 3 and re-run everything). Stop at the first failing layer for interactive runs; keep run-all for unattended callers (story runner sets `DEVLOOP_HEADLESS`; CI sets `GITHUB_ACTIONS`), where one pass reporting everything broken beats a fast first answer.

### Scope
- **Service(s)**: none (validation tooling only — `scripts/`, `docs/runbooks/`)
- **Schema**: No
- **Cross-cutting**: Pipeline tooling; affects every devloop + the story runner + CI

### Out of scope (task-stated, hard boundaries)
- Do NOT change any guard's timeout value; do NOT remove any timeout (an unbounded guard hangs the pipeline).
- Do NOT reclassify any failure other than timeout and kill.

### Debate Decision
NOT NEEDED — narrow, mechanism is settled by the task; the taxonomy already carries `PRECONDITION_FAILURE`.

---

## Lead Scouting Notes (pre-planning context for the team)

Read these before planning — they are the results of the Lead's read-through of the pipeline. Verify, don't trust blindly.

### Change 1 — where the timeout is emitted and how it aggregates
- Emission site: `scripts/guards/run-guards.sh`, `classify_guard_exit()` — the `124)` and `137)` arms each `echo "STATUS=FAIL REASON=guard-timeout[-kill]-${GUARD_NAME}"`. These are the two lines to reclassify to `PRECONDITION_FAILURE`.
- Aggregation path: `layer3.sh` runs `run-guards.sh` inside a `{ … } | tee_collect_statuses` block. `tee_collect_statuses` (`_common.sh`) collects **every** `STATUS=` line on stdout — so the per-guard timeout STATUS line printed by `run-guards.sh` **is** collected and fed into `aggregate_worst_status`, independent of `run_and_emit`'s own `STATUS=OK/FAIL guards-…` line. `PRECONDITION_FAILURE` (rank 6) outranks `FAIL` (rank 5), so Layer 3 aggregates to `PRECONDITION_FAILURE` → `status_to_exit_code` = 2. **No taxonomy edit needed** — confirmed the ladder + exit map already handle it (`_common.sh:257-322`).
- Consider whether `run-guards.sh`'s own exit code / counters should distinguish a timeout from a violation (a standalone caller of `run-guards.sh` sees exit 1 today). A separate precondition counter → exit 2 would make standalone `run-guards.sh` honest too, and keep `FAILED_GUARD_NAMES` meaning "found a violation." Weigh against scope — the layer-level reclassification works via the STATUS line alone. Implementer/team to decide and record the rationale.
- **Mixed case to reason about explicitly**: if one guard times out (now PRECONDITION_FAILURE) AND another finds a real violation (FAIL via `run_and_emit`), the aggregate is PRECONDITION_FAILURE (operator lane wins per the rank ladder). State whether that masking of a real defect is acceptable or needs handling. (Per the ladder's documented intent it is deliberate; but say so.)

### Change 1 — SIGKILL / exit 137 decision (task asks for a deliberate ruling)
- In `run-guards.sh`, exit 137 arrives specifically from `timeout --kill-after`: the guard ignored SIGTERM and was SIGKILL'd after the grace window. That is the timeout mechanism firing harder — a machine fact. An OOM kill also lands as 137 and is likewise an environment fact. Both dominant interpretations are environment, not diff-defect → classify 137 as PRECONDITION_FAILURE alongside 124. Record the reasoning (don't just assume). This is in-scope ("timeout and kill" are the two the task authorizes).

### Change 2 — how to select the mode, and the honesty constraint
- Detection: unattended = `DEVLOOP_HEADLESS` set (story runner, `run-story.sh:1041`) OR `GITHUB_ACTIONS` set (CI). Interactive = neither. Prefer a config-driven override env (e.g. `DEVLOOP_FAIL_FAST`) that defaults from that detection, per CLAUDE.md "config over hardcoding." Interactive default = fail-fast; unattended default = run-all (current behavior).
- **Honesty constraint (CLAUDE.md "fail loudly; never mask")**: when fail-fast stops early, the layers that did not run must NOT be reported as OK. The `LAYER_SUMMARY` block + `TOTAL_RESULT` must remain truthful and the exit code must still be that of the failing layer. Decide how to render un-run layers (a distinct not-run marker, or omit them with a loud "stopped early at layer N" line) without adding a new aggregation enum if avoidable, and without letting an un-run layer demote or inflate `TOTAL_RESULT`.
- **Test seam interaction**: `scripts/layer-all.test.sh` is itself run every devloop (wired as a guard self-test in `layer3.sh:59`) and drives `layer-all.sh` with stub layers under `LAYER_SCRIPT_DIR` (DEVLOOP_TEST-gated). It runs with neither `GITHUB_ACTIONS` nor `DEVLOOP_HEADLESS` set, so a naive "interactive ⇒ fail-fast" default would flip existing worst-wins-across-layers cases (e.g. case (g): FAIL@L4 + PRECONDITION@L7). The tests must stay green — either exercise run-all explicitly there and add dedicated fail-fast cases, or otherwise reconcile. Test (paired) owns getting this right.

### Change 2 — runner routing verification (task flagged this to check, not assume)
- **Verified (Change 1 routing — no teaching needed):** `run-story.sh` classifies the gate purely on the layer exit code `gate_rc` — `rc == 1` → implementer lane (`escalate`, consumes an attempt); `rc != 0` (i.e. ≥ 2) → operator lane (`record_infra_incident` "pipeline-precondition", manifest untouched, does NOT consume the attempt budget). See `run-story.sh:1204-1222` and its comment citing `_common.sh status_to_exit_code`. Because Change 1 makes `layer3.sh` exit 2 on a guard timeout (PRECONDITION_FAILURE → 2), **Layer 3 inherits the operator lane automatically — the runner needs no teaching for Change 1.**
- **Verified (Change 2 — the runner DOES need a one-line change; finding S3, raised by @security, confirmed by Lead):** `DEVLOOP_HEADLESS=1` is only a per-COMMAND prefix on the `claude` devloop invocation (`run-story.sh:1041`); it is never `export`ed into the runner's own shell. The runner calls `layer-all.sh` in exactly ONE place — the story-close full-pipeline authority gate at `run-story.sh:1388` (`./scripts/layer-all.sh >"$closelog"`), run from the runner's shell where `DEVLOOP_HEADLESS` is unset. So a naive `DEVLOOP_HEADLESS || GITHUB_ACTIONS` detector would classify that unattended authority gate as INTERACTIVE and fail-fast it — losing the one-pass full report ADR-0035 relies on. **Fix: prefix line 1388 with the run-all signal** (e.g. `DEVLOOP_HEADLESS=1 ./scripts/layer-all.sh …`, consistent with line 1041, or `DEVLOOP_FAIL_FAST=0`). This adds `scripts/workflow/run-story.sh` to the diff + Cross-Boundary Classification table (operations-owned, so "Mine").
  - Note: the PER-TASK gate (`run-story.sh:1168`) invokes individual `scripts/layer${n}.sh` in its own loop and **already `break`s at the first failing layer** — it never calls `layer-all.sh`, so it is UNAFFECTED by Change 2. Only the story-close gate matters here.

### Security pre-plan constraints (from @security — design these in)
- **S1**: the fail-fast STOP predicate MUST be exit-code-based (`rc != 0`), NOT status-based. A status-based stop would halt on an exit-0 `N/A`/`SKIPPED-*` layer at `final_exit==0` → `emit_gate2_verdict` derives `GATE2=PASS` over a tree whose later layers never ran (green-forging). Add a defensive fail-loud if the loop ever breaks early with `final_exit==0`.
- **S2**: un-run layers must NOT aggregate as `UNKNOWN` (empty log → empty status → UNKNOWN → top of ladder → misleads a real FAIL into operator-lane exit 2, and misuses the "dispatcher bug" enum). Aggregate `TOTAL_RESULT` over the layers that RAN; give the rest a distinct non-aggregating marker + greppable `STOPPED_EARLY_AT_LAYER=<n>`; exit stays the failing layer's code.
- **S4 (ruling, no code change)**: timeout + real violation mixed case is fail-closed — a non-timeout guard failure rides `run_and_emit`'s `STATUS=FAIL guards-failed` regardless of aggregation; Layer 3 → PRECONDITION_FAILURE → exit 2 → runner operator lane HALTS, manifest untouched, task never completes; recovery needs a green hand re-run which a live violation prevents. Operator-lane precedence masking the FAIL in the aggregate LABEL is deliberate/acceptable.
**MIXED-CASE EXIT — FINAL LEAD RULING (2026-08-21). THIS IS THE SINGLE OPERATIVE RULE; it supersedes ALL earlier phrasing (both the interim ladder-mirror re-ruling and the interim VIOLATION-wins ruling) anywhere in this document. See R-A below for the same rule in the Reconciled Decisions.** For the STANDALONE `run-guards.sh` exit code: **LADDER-MIRROR / PRECONDITION-wins — `PRECONDITION_GUARDS>0 → exit 2` DOMINANT, `elif FAILED_GUARDS>0 → exit 1`, `else 0`** — mirroring `_common.sh::__status_rank` + `status_to_exit_code`, consistent with the LAYER aggregation, **COUPLED with R-A2 (the `STATUS=FAIL REASON=guard-violations` machine trace + the `MIXED_LANE:` line), which is mandatory precisely because it is what makes ladder-mirror safe here.**

**Why this is final, and why it changed (record the real sequence so it never re-oscillates):** This question oscillated three times — interim ladder-mirror → VIOLATION-wins → final ladder-mirror — because the answer is COUPLED to R-A2, and each earlier ruling was made before that coupling was on the table. The VIOLATION-wins ruling rested on ONE premise: *standalone, the exit code is the violation's ONLY machine-readable trace* (the `*)` violation arm emits no `STATUS=` token). **R-A2 (@security's F2, which IS landing) falsifies that premise** by giving the violation its own machine trace (`STATUS=FAIL REASON=guard-violations`), AND its `MIXED_LANE:` line explicitly names the real violation — which **neutralizes the exact masking risk** ("a human dismisses the defect as an operator hiccup") that was VIOLATION-wins' whole justification. With the violation independently legible three ways (STATUS trace, `FAILED:` line, `MIXED_LANE:`), the exit code no longer needs to carry it, and ladder-mirror becomes STRICTLY BETTER: exit code == layer verdict == pipeline lane (ONE ladder, no divergence), the self-test derives EVERY arm from `status_to_exit_code` (drift-reds survives for ALL cases, including mixed), nothing to document at two sites, and no reader-reconciliation hazard (which is THIS devloop's own subject). @observability (who argued both sides and verified the `*)` arm) surfaced the coupling and recommends this; @dry-reviewer's original position was anti-divergence; @security is indifferent on ordering (green-safe either way — `run_and_emit` reads zero/non-zero only) and R-A2 is its actual requirement; @code-reviewer accepts PRECONDITION-wins. All six converge here. Conditions:
1. **R-A2 is NON-OPTIONAL** and is what licenses ladder-mirror: the violation MUST stay loudly legible (STATUS=FAIL guard-violations trace + the `MIXED_LANE: precondition=<n> violations=<m> — exit 2 (operator lane); the <m> violation(s) above are REAL and must be fixed; re-run on a quiet machine for a clean implementer-lane verdict.` line). Exit-2 is only safe because these name the violation explicitly — it is NOT dismissible as a mere machine hiccup.
2. **ONE ladder, documented ONCE**: the `ANCHOR (DRY):` comment on run-guards.sh's exit block names `_common.sh::__status_rank` + `status_to_exit_code` as the SSoT it mirrors (and why it can't call them directly — run-guards.sh sources `guards/common.sh`, not `_common.sh`; and why the hand-rolled `STATUS=` line bypasses `emit_status` — deliberate). NO "divergence" doc is needed (there is no divergence — that machinery, from the superseded VIOLATION-wins ruling, is DROPPED).
3. **Self-test derives EVERY arm from `status_to_exit_code`** (@test authors, NON-OPTIONAL): pure-timeout standalone → 2, mixed standalone → 2 (PRECONDITION dominant), violation-only standalone → 1 — all SSoT-derived, so any ladder change reds the test. This restores the full drift-reds property the interim ruling had sacrificed.

Fix the stale run-guards.sh header comments (`:8-12` exit codes, `:116-117` both-mappings-as-STATUS=FAIL).
- Detector must route BOTH arms (`DEVLOOP_HEADLESS`, `GITHUB_ACTIONS`) through ONE predicate; do NOT weaken `assert_no_ci_sentinel_leak` to make the `GITHUB_ACTIONS` arm testable.

### DRY reviewer findings + LEAD RULINGS (the task's framing was narrower than the problem)
Confirmed by Lead read-through. These are the highest-value planning findings — the task's "check the runner" framing missed that the motivating 2026-08-14 incident was **interactive**, and the interactive attempt budget lives in prose (SKILL.md), not in `run-story.sh`.

- **D1 — SKILL.md attempt-budget de-scope. RULING: IN SCOPE (task-critical).** The interactive Gate 2 attempt-budget carve-out ("PRECONDITION_FAILURE / exit 2 does NOT consume an attempt — retry once, then escalate to operations") is scoped to **Layer 7 only** in SKILL.md (`:428`, `:444`, Limits table `:650-652`); layers 1-6 get only "Max 3 attempts" (`:438`, `:650`) with no operator-lane carve-out. So a Layer-3 guard timeout emitting PRECONDITION_FAILURE would STILL burn an interactive Gate 2 attempt — the exact waste the task names ("consuming a real Gate 2 attempt"). Change 1 does NOT achieve its stated goal for the interactive lane without this. **Fix**: generalize the L7-only carve-out so a PRECONDITION_FAILURE (exit 2) from ANY layer is the operator lane — retry once, don't consume one of the 3 L1-6 attempts, then escalate to operations. FAIL (exit 1) still consumes an attempt, as today. This makes the interactive Lead's Gate 2 mirror what `run-story.sh` already does in code (the exit-1-vs-exit-2 lane split). Edit `.claude/skills/devloop/SKILL.md` Gate 2 "If fail (layers 1-6)" prose + the Limits table. Add SKILL.md to the classification table (operations-owned → Mine; not a GSA).
- **D3 — SKILL.md:387 run-all restatement → citation. RULING: IN SCOPE (SSoT hygiene).** `:387` restates the run-all invariant verbatim and quotes the `set +e` code comment ("does NOT stop at the first red layer … reports the state of all seven"). Change 2 makes that conditional. Restating (vs citing) is the exact mechanism that caused the 2026-08-17 misread (TODO.md:468). **Fix**: put the conditional statement in ADR-0033 §4 (amendment) and have SKILL.md:387 CITE it rather than restate — reduce the verbatim restatement to a pointer. (Same SKILL.md edit as D1.)
- **D2 — TODO.md:468 closed entry reversal. RULING: amend, don't silently contradict.** That entry records "**The pipeline was NOT changed and must not be**" and "**No ADR amendment; the pipeline is correct as built**", with the "Gate 2 attempts capped at 3" counter-argument (`:476`). The task authorizes the reversal; it MUST be written as a dated amendment to that entry (not silently contradicted), and the plan must ANSWER the counter-argument. **The answer to record**: the 2026-08-17 "one pass reports everything, conserving the 3-attempt budget" value accrues to callers who do NOT re-run interactively (story runner, CI) — Change 2 PRESERVES run-all for exactly those callers, so the recorded value is kept where it applies. For interactive runs a human is present and re-runs after fixing anyway; and the cost basis changed after 2026-08-20 (Layer 7 unconditional, ~10-15min cluster bring-up every run; every layer every language), so "running on" now costs far more than the 873s measured in the L2-red example, on every interactive iteration. Change 1 further removes the machine-fact-timeout case from the budget entirely (D1). Net: the interactive/unattended split reconciles the 2026-08-17 concern with the new cost reality. Add `docs/TODO.md` to the classification table.
- **Doc-mirror lockstep (SSoT — CLAUDE.md). RULING: all must move with the code, else we re-create the stale-doc landmine that caused 2026-08-17.** Confirmed sites: `docs/decisions/adr-0034-…md:209` (§9, states the old `Exit 124 → STATUS=FAIL` contract verbatim); `crates/dt-guard/src/ts_retained_credentials.rs:577` (a `///` comment saying a hang "presents as STATUS=FAIL REASON=guard-timeout-…" — update the ENUM to PRECONDITION_FAILURE; REASON token stays byte-identical; it is a comment, not logic); runbook `§3` ladder + `§6.3` + `§8` Caveat (632-636, now actively misleading). REASON tokens `guard-timeout-<name>` / `guard-timeout-kill-<name>` stay byte-identical everywhere.
- **SSoT pointer fix (trivial, operations-owned).** `docs/specialist-knowledge/operations/INDEX.md:30` and `docs/TODO.md:1011` point the STATUS-rank SSoT at `scripts/guards/common.sh:__status_rank`; it actually lives at `scripts/lang/_common.sh:272`. Fix in-tree (fix-don't-defer).
- **TODO drift-guard defer-triggers**: two TODO entries' defer-triggers fire on this diff — implementer to record an explicit re-defer ruling (or address if trivial), don't leave silent.

Scope note: this grew from "two script edits" to include SKILL.md prose + several lockstep doc mirrors. That is appropriate, not scope-creep — D1 is how the task's goal is met interactively, and the mirrors are single-source-of-truth discipline (landing the code while leaving 4 docs stating the old contract is the precise 2026-08-17 failure mode). Real code surface stays small: `run-guards.sh`, `layer-all.sh`, one `run-story.sh` line, SKILL.md prose, test files.

### Documentation to update
- `docs/runbooks/devloop-validation.md` §3 STATUS ladder: the `PRECONDITION_FAILURE` row (line ~88) currently reads "Layer 7 only" — it must record that **Layer 3 can now emit it** (guard timeout/kill), with the two REASON tokens `guard-timeout-<name>` / `guard-timeout-kill-<name>`. Also review §6.3 (Layer 3) for the timeout failure-mode description. The task explicitly calls for this ladder update.
- Document Change 2's fail-fast behavior + the `DEVLOOP_FAIL_FAST` (or chosen name) knob wherever the pipeline's run-all contract is described (`layer-all.sh` header comment references ADR-0033 §4 "does NOT stop at the first red layer" — that invariant is now conditional; reconcile the header/ADR reference honestly).

### ADR-0033 §4 reconciliation (important)
- `layer-all.sh`'s header and ADR-0033 §4 currently state the pipeline runs all layers "sequentially" and "does NOT stop at the first red layer." Change 2 makes that conditional (interactive fail-fast). The implementer must reconcile the ADR reference — either the ADR text or a clear note that the run-all invariant now holds only for unattended callers. Do not silently contradict the ADR. Operations to decide whether this needs an ADR amendment note.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) | Notes |
|------|----------------|---------------------|-------|
| `scripts/guards/run-guards.sh` | Mine | — | Change 1: 124/137 → PRECONDITION_FAILURE; counter split; ladder-mirror exit (2 dominant) + R-A2 |
| `scripts/lang/_common.sh` | Mine | — | adds pure `fail_fast_mode()` predicate (R-C) |
| `scripts/layer-all.sh` | Mine | — | Change 2: fail-fast loop + NOT-RUN render |
| `scripts/layer3.sh` | Mine | — | wire the new `run-guards.test.sh` |
| `scripts/workflow/run-story.sh` | Mine | — | S3: run-all signal on the L1388 close gate |
| `scripts/workflow/run-story.test.sh` | Mine (test writes) | — | F1 pin: close-gate forces run-all + ambient-`=1`-override |
| `scripts/layer-all.test.sh` | Mine (test co-owns) | — | fail-fast end-to-end cases |
| `scripts/lang/_common.test.sh` | Mine (test writes) | — | `fail_fast_mode` truth-table |
| `scripts/guards/run-guards.test.sh` | Mine (test writes) | — | NEW — timeout emission + exit precedence |
| `docs/runbooks/devloop-validation.md` | Mine | — | §3/§4/§6.3/§8 + §11 changelog |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | Mine | — | §4 fail-fast amendment |
| `docs/decisions/adr-0034-guard-pipeline-as-rust-binary.md` | Mine | — | §9/:209 enum amendment |
| `.claude/skills/devloop/SKILL.md` | Mine | — | D1 :444/:652 de-scope; D3 :387 cite §4; S5 retry note |
| `crates/dt-guard/src/ts_retained_credentials.rs` | Mechanical | dt-guard (operations+test, ADR-0034) | :577 enum in `///`; value-neutral, REASON token byte-identical, no logic |
| `docs/TODO.md` | Mine | — | D2 amend :468; check off :1011; SSoT pointer fix; R-I re-defer + N4 scope note |
| `docs/specialist-knowledge/operations/INDEX.md` | Mine | — | :30 SSoT pointer fix |

None are Guarded Shared Areas. `ts_retained_credentials.rs` is a Mechanical doc-comment-only touch (the guard's logic is untouched); flagged to @security/@observability/@test (dt-guard maintainers) who raised it.

---

## Reconciled Decisions (post Gate-1 input)

Gate-1 surfaced two genuine conflicts; both round-tripped through the Lead and are now
settled. Each decision names its final authority. **R-A oscillated three times during planning
(interim ladder-mirror → VIOLATION-wins → FINAL ladder-mirror); the single operative rule is the
"MIXED-CASE EXIT — FINAL LEAD RULING" block above (LADDER-MIRROR / PRECONDITION-wins, coupled with
R-A2). R-A below matches it. Any "VIOLATION-wins / exit-1-first / deliberate-divergence" phrasing
anywhere in this document is SUPERSEDED and must not be implemented.**

**R-A — run-guards.sh standalone exit precedence: LADDER-MIRROR / PRECONDITION-wins, coupled with
R-A2. FINAL — Lead ruling (the "SINGLE OPERATIVE RULE" block above).** Precedence:
`PRECONDITION_GUARDS>0 → exit 2` **DOMINANT** (checked FIRST), `elif FAILED_GUARDS>0 → exit 1`,
`else 0` — mirroring `_common.sh::__status_rank` (PRECONDITION_FAILURE 6 > FAIL 5 > OK) +
`status_to_exit_code`. **This is ONE ladder, not a divergence**: standalone run-guards.sh exit ==
the LAYER verdict (layer3 aggregates PRECONDITION-wins too) == the pipeline lane. Why ladder-mirror
(not violation-wins): the violation-wins case rested on "standalone, the exit code is the violation's
ONLY machine-readable trace" — **R-A2 falsifies that premise** by giving the violation its own
`STATUS=FAIL REASON=guard-violations` trace + the `MIXED_LANE:` line naming it, so the violation is
legible three ways (STATUS trace, `FAILED:` line, MIXED_LANE) and the exit code no longer needs to
carry it. With the masking risk neutralized, ladder-mirror is strictly better (one ladder, self-test
derives every arm from SSoT, no two-site divergence doc, no reader-reconciliation hazard — this
devloop's own subject). Guards:
  - **ONE ladder, documented ONCE** (@dry Condition 1): the run-guards.sh exit-block `ANCHOR (DRY):`
    comment names `_common.sh::__status_rank` + `status_to_exit_code` as the SSoT it mirrors, and why
    it can't call them directly (run-guards.sh sources `guards/common.sh`, not `_common.sh`) and why
    the hand-rolled `STATUS=` line bypasses `emit_status` (deliberate). NO "divergence" doc — there is
    no divergence.
  - **Self-test derives EVERY arm from `status_to_exit_code`** (@test, NON-optional): pure-timeout
    standalone → 2, **mixed standalone → 2** (PRECONDITION dominant, SSoT-derived — NOT hardcoded),
    violation-only standalone → 1. Any ladder change reds the test → full drift-reds property (the
    interim violation-wins ruling had sacrificed the mixed case; ladder-mirror restores it).
  - Blast radius (re-verified by @observability + @security): `layer3.sh:27` via `run_and_emit` is
    run-guards.sh's ONLY programmatic caller (grepped scripts/.github/.claude/crates/infra — no
    CI/YAML caller); `run_and_emit` branches zero/non-zero only, so run-guards.sh's own exit never
    moves the layer verdict. Standalone exit is read only by a human at a terminal.

**R-A2 — machine-readable violation trace + human MIXED_LANE line. NON-OPTIONAL — it is what makes
ladder-mirror safe (@security F2 + @observability, Lead condition 1).** Because a violating guard's
`*)` arm prints only human `FAILED: <name>` (no `STATUS=`), standalone run-guards.sh emits, when
`FAILED_GUARDS>0`, a summary `STATUS=FAIL REASON=guard-violations` line (machine trace; in layer3 it's
redundant with `run_and_emit`'s FAIL, aggregate unchanged; `parse_status_line` unaffected — layer3's
own summary line is emitted LAST). AND, in the mixed case (both>0), a loud greppable `MIXED_LANE:
precondition=<n> violations=<m> — exit 2 (operator lane); the <m> violation(s) above are REAL and must
be fixed; re-run on a quiet machine for a clean implementer-lane verdict.` Exit 2 (operator lane) is
only SAFE because these name the coexisting violation explicitly — it is NOT dismissible as a mere
machine hiccup. Without R-A2, ladder-mirror would mask the violation behind an operator-lane exit;
with it, the violation stays loudly legible while the exit code stays consistent with the layer/pipeline.
**R-A2 side effect on the stderr LAYER=3 anchor (@dry-reviewer finding — fold the runbook rows):** in a
violations-ONLY Layer-3 run the stream now orders `STATUS=FAIL guard-violations` (run-guards.sh) BEFORE
`STATUS=FAIL guards-failed` (`run_and_emit`), so `worst_reason_for_status` (`_common.sh:341`, first FAIL
reason) flips the stderr `LAYER=3 … REASON=` anchor from `guards-failed` to `guard-violations`. That's the
observability 3am anchor (P2) — an IMPROVEMENT (specific cause, not generic), but it must be documented, not
discovered: the runbook rows keyed on `guards-failed` (`:334` + the task-#58 `:335` I'm editing anyway) get a
`guard-violations` row / are extended to name both tokens and which surface shows which (stdout carries both;
the stderr LAYER= line now shows `guard-violations`). **ANCHOR (DRY) scope (@dry):** `guard-violations` makes
THREE hand-rolled `STATUS=` emissions in run-guards.sh (2 timeout arms + this summary line), all bypassing
`emit_status`; the `ANCHOR (DRY):` comment covers ALL THREE sites, and a FOURTH is the recorded trigger to
revisit routing through `emit_status` (which would need the rejected _common.sh-sourcing).

**R-B — fail-fast precedence: validity-first, then BOTH unattended lanes force run-all (refusing
an explicit `=1`), then the explicit knob, then interactive default. FINAL — Lead R-B ruling +
@test's aligned truth-table (supersedes @security F3's "GHA-only refusal": headless is ALSO an
unattended authority lane, so BOTH refuse; F3's "local operator can see what they set" mis-read
headless as interactive — headless = an unattended story-runner session).** Truth table
(top row wins):
| `DEVLOOP_FAIL_FAST` | env | result | SOURCE |
|---|---|---|---|
| bad value | any (GHA/headless incl.) | **INVALID** → caller exits 2 loud | `invalid` |
| `1\|true\|yes` | GHA or headless | **RUNALL** (refused + announced) | `unattended-override-refused` |
| `0\|false\|no` | GHA | RUNALL | `github-actions` |
| `0\|false\|no` | headless | RUNALL | `headless` |
| unset | GHA | RUNALL | `github-actions` |
| unset | headless | RUNALL | `headless` |
| `1\|true\|yes` | neither | FAILFAST | `fail-fast-env` |
| `0\|false\|no` | neither | RUNALL | `run-all-env` |
| unset | neither | FAILFAST | `interactive-default` |
Validity is a property of the VALUE alone, checked BEFORE env (@security F3 — a typo is loud even
in CI/headless, "fail loudly"). The unattended refusal is defense-in-depth on gate-coverage
authority (ADR-0035): an *ambient* `=1` (inherited runner env, leaked export, copied workflow
block) must not shrink an authority run's coverage; it cannot forge green (red stays red), and the
refusal is LOUD, not silent. Keep the two `=1`-in-unattended vs bad-value cases DISTINCT: bad value
→ exit 2 (fail-closed); refused `=1` → run-all + WARN (a stray env var must NOT break CI).

**R-C — mode predicate lives ONCE in `_common.sh::fail_fast_mode()`** (pure, sourceable,
tri-state, **NO self-exit**; @test + @security + @dry-reviewer E, Lead-adopted): reads env only,
echoes `FAILFAST <source>` / `RUNALL <source>` / `INVALID` to stdout, returns 0 always. `SOURCE`
labels: `interactive-default` | `headless` | `github-actions` | `fail-fast-env` | `run-all-env` |
`unattended-override-refused` | `invalid`. The **caller** in layer-all.sh does the loud `exit 2` on
INVALID and the `WARN FAIL_FAST_OVERRIDE_IGNORED` emission on `unattended-override-refused` (helper
stays pure — it decides, the caller acts). @test unit-tests the full matrix — incl. the
`GITHUB_ACTIONS`→RUNALL arm the `LAYER_SCRIPT_DIR` seam structurally can't reach — in `_common.test.sh`.
Single source of the precedence truth. (Name `fail_fast_mode` not `_enabled` — @code-reviewer: the
`_mode`/`resolve_` shape signals the tri-state resolver, not a 0/1 boolean.)

**R-D — un-run layers render `NOT-RUN`** (display-only marker; @observability O6, @dry-reviewer,
@test, @code-reviewer): a single `readonly NOT_RUN="NOT-RUN"` const near the top of layer-all.sh
(no bare-string typo drift between the write-loop and the aggregation-skip). Explicitly SET into
`layer_status[n]`/`layer_dur[n]=0` in a post-break loop, so (a) the `${…:-UNKNOWN}` sites never
fire for un-run layers (@security S2 / @observability O5 — UNKNOWN would wrongly inflate
TOTAL_RESULT→exit 2), (b) aggregation `continue`s past NOT-RUN (never votes — not a new enum,
@dry D2), (c) `emit_gate2_verdict` RECORDS `LAYER n NOT-RUN 0` (@observability O8's explicit
disambiguation over @dry D1's leave-unset; safe — LAYER lines are informational-only, nothing
parses them back, and NOT-RUN only ever attaches to a FAIL verdict). **Guardrail comment (@security)
at the NOT_RUN decl**: it must NEVER be added to `__status_rank` or given a `status_to_exit_code`
arm — the `*)` fail-closed backstop makes a leak loud (exit 2), but a well-meaning `NOT-RUN → 0`
arm would silently reintroduce the S1 `GATE2=PASS`-forgery vector. Stop predicate = **`rc != 0`**
(@security S1 / @test / Lead — never status-based; catches even a lying `STATUS=OK; exit 1` layer),
placed at the END of the loop body AFTER the final_exit update + layer_status/dur assignment
(@code-reviewer ordering). Defensive assert (N3 — a FIXED greppable token, not just "loud", so it's
identifiable when it lands in someone's log and rides the §4 grep): `stopped_early>0 && final_exit==0`
→ `PRECONDITION_FAILURE: stopped early at layer <n> but final_exit==0 — orchestrator invariant
violated REASON=fail-fast-exit-invariant` on stderr + `exit 2`. @test adds the fail-closed case: a
SKIP/N-A/SKIPPED-NO-CLUSTER layer (exit 0) must NOT stop the run and L7 MUST still run (guards
`rc != 0` against a future status-based regression).

**R-E — greppable mode/stop lines** (@observability O7/F7 + N1): stderr `PIPELINE_MODE=<fail-fast|run-all>
SOURCE=<label>` ALWAYS emitted at start (not just on stop — the mode-in-force is a fact in the log,
incl. someone else's pasted log); on an unattended-override refusal (R-B), emit the CANONICAL
`PIPELINE_MODE=run-all SOURCE=unattended-override-refused` line PLUS a `WARN FAIL_FAST_OVERRIDE_IGNORED
REQUESTED=1 MODE=run-all SOURCE=<github-actions|headless>` token (N1 — matches the existing
`WARN BUDGET_*` convention so `grep 'PIPELINE_MODE='` AND `grep 'WARN '` both find it; a prose-only
`:`-delimited refusal would be missed). Stderr `STOPPED_EARLY LAYER=<n> RESULT=<enum> NOT_RUN=<list>`
at the break; NOT-RUN cells in the stdout summary. Neither new line starts with `STATUS=`. (When the
failing layer is the LAST one — nothing skipped — `NOT_RUN=` is empty; @test noted this is harmless.)

**R-F — line-anchored stderr banner on each timeout arm** (@observability O3): `PRECONDITION_FAILURE:
guard <name> timed out after <N>s (operator lane; suspect concurrent machine load) REASON=guard-timeout[-kill]-<name>`
relayed into `layer-3.stderr.log` (run-guards.sh stderr → layer-all's `2>>layer-3.stderr.log`), so
the documented §4 grep `^(ERROR|PRECONDITION_FAILURE):` finds a guard timeout. Placement confirmed:
run-guards.sh stderr under layer3 lands in that log (per @team-lead's ask to confirm).

**R-G — fast-tier budget check gated on L3+L6 both running** (@observability O9/N2 / @dry C1 / Lead #3):
skip the 90s check with a canonical greppable `WARN BUDGET_TOTAL_SKIPPED REASON=layers-not-run LAST_RAN=<n>`
token (N2 — joins the `WARN BUDGET_*` family so `grep 'WARN BUDGET_'` returns all three states:
breach / total-breach / skipped) when layer 3 OR 6 is `NOT-RUN` — keyed off the **NOT-RUN status**
(not `layer_dur==0`; a genuinely fast layer 6 can measure 0s), per @dry. Never sum absent data.

**R-H — no new run-guards discovery seam** (@test): PATH-stub `timeout` approach (hermetic,
deterministic, no production seam → no matching CI-sentinel-rejection needed).

**R-I — drift-guard triggers (@dry G7): RE-DEFER, recorded.** TODO.md:184 (runbook↔`__status_rank`
rank-ladder drift guard) + :458 (backticked-identifier-in-prose guard) both fire; both task-sized
(a new dt-guard subcommand each). Re-defer per their own triggers; noted so it's not silent. Manual
mitigation: I land runbook §3 ladder + `_common.sh::__status_rank` consistent in one change.
**N4 scope note (add to TODO.md:184):** even once built, that guard as scoped (runbook ↔ `__status_rank`)
would NOT cover the new run-guards.sh exit-block ladder-MIRROR — run-guards.sh (sourcing
`guards/common.sh`, not `_common.sh`) now carries a THIRD encoding of the PRECONDITION>FAIL>OK
precedence outside `__status_rank`'s file, so the entry must record that the guard's scope needs to
include the run-guards.sh exit-block mirror or it gets built and still misses exactly the drift
surface this devloop introduced.

**R-L — guard-timeout retry discriminator (@security S5, Lead-ruled — prose only, no code/classification
change).** A guard timeout has TWO root causes, and retry-once is the discriminator: (i) does NOT
reproduce (passes on retry, or on a quiet machine) → transient machine contention → OPERATOR lane, no
attempt consumed (the 2026-08-14 motivating case); (ii) REPRODUCES (times out again on retry and/or a
quiet machine) → DIFF-CAUSED (the changeset pushed the guard past its 30s budget — a large file set, a
pathological scanning input) → the timed-out guard rendered NO verdict, so whatever it would have caught
went unreported → route to the IMPLEMENTER, CONSUME an attempt, do NOT escalate to operations. Exit stays
`PRECONDITION_FAILURE`/2 (unchanged); this is the TRIAGE bias, stated in the two places D1/R-J touch:
SKILL.md's generalized carve-out AND runbook §6.3's guard-timeout row — "a timeout that reproduces on
retry or on a quiet machine is diff-caused: route to the implementer and consume an attempt; only a
non-reproducing timeout is the operator lane." Stops "timeout ⇒ operations" from becoming the lazy default
that lets a diff-caused guard-non-verdict escape (@security's sharpest form of the violation-as-operator-lane
risk — here the timing-out guard is the SOLE signal and it's silent).
**Boundary-case second discriminator (@security refinement, Lead-accepted):** "reproduces" is a coin-flip
for a guard sitting right at the 30s budget — the likeliest shape of a diff-caused timeout is a diff that
pushes a guard JUST over, so it alternates lanes run-to-run. So triage a SECOND signal alongside
reproduce-on-retry: **did this guard's runtime JUMP vs its normal cost, AND did the diff GROW its input
set?** A guard that normally takes 2s and now takes 30 is diff-correlated whether or not the retry squeaks
under budget. Both prose sites (SKILL.md carve-out + runbook §6.3) name both discriminators.

**R-J — SKILL.md is task-critical, two edits (@dry-reviewer G1 / @observability F1 / Lead D1+D3).**
(a) **:444 + :652 attempt-budget de-scope** — the operator-lane carve-out ("PRECONDITION_FAILURE /
exit 2 does NOT consume an attempt; retry once, then escalate to operations") is scoped to **Layer 7
only**; generalize to **any layer emitting PRECONDITION_FAILURE (exit 2), incl. a Layer-3 guard
timeout**. FAIL (exit 1) still consumes an attempt. WITHOUT this, the interactive Lead (who reads
SKILL.md, not run-story.sh) still burns a Gate-2 attempt on a guard timeout — the task's headline
objective, and the 2026-08-14 incident was interactive. (b) **:387 + :407 + :423 run-all reconcile**
— :387 restates the run-all invariant verbatim (and invented the "does NOT stop at first red layer"
attribution — @observability verified ADR-0033 §4 says only "sequentially"); reduce :387 to a
CITATION of ADR-0033 §4 (interactive fail-fast / unattended run-all); :423 "Layer 7 always runs"
gets a fail-fast caveat; :407 self-justifying-status paragraph notes NOT-RUN is NOT self-justifiable.

**R-K — reversed-decision + mirror reconciliations (Lead D2, doc-mirror lockstep).** TODO.md:468
CLOSED entry ("pipeline NOT changed and must not be" / "No ADR amendment", with the "attempts capped
at 3" counter) gets a DATED amendment recording the reversal + the answer (run-all preserved for the
callers who don't re-run interactively — story runner, CI; interactive humans re-run anyway; post-
2026-08-20 the cost basis changed — L7 unconditional ~10-15min every run — so running-on costs far
more than the measured 873s). ADR-0033 §4 = light clarifying amendment ("sequentially" stays true;
the false claim was SKILL.md's). ADR-0034 §9/:209 = enum amendment (124/137 → PRECONDITION_FAILURE,
tokens byte-identical). ts_retained_credentials.rs:577 `///` = enum only. runbook §3(:88,:94)/§4(:168
emitter list)/§6.3/§8(:613 name guard-timeout as the concrete two-STATUS-line instance; fix the
now-misleading :632-636 caveat)/§11 changelog. INDEX.md:30 + TODO.md:1011 SSoT-pointer fix
(`guards/common.sh:__status_rank` → `_common.sh:272`). TODO.md:1011 checked off (this entry closes it).

---

## Planning

**Restated in mechanism-language.** Two wastes share one root: a Layer-3 machine-fact
(a guard that never finished measuring) is currently indistinguishable from a Layer-3
code-defect, and the pipeline then keeps spending on Layers 4-7 (incl. cluster bring-up)
after a red it already knows about.

### Change 1 — guard timeout/kill → operator lane (`scripts/guards/run-guards.sh`)

**Emission-site reclassification (the load-bearing edit).** In `classify_guard_exit()`,
the `124)` and `137)` arms change their emitted line from
`STATUS=FAIL REASON=guard-timeout[-kill]-<name>` to
`STATUS=PRECONDITION_FAILURE REASON=guard-timeout[-kill]-<name>`. **REASON tokens are
preserved verbatim** (`guard-timeout-<name>` / `guard-timeout-kill-<name>`) — the runbook,
ADR-0034 §9, and the task-#58 triage note all reference them. No taxonomy edit: verified
`aggregate_worst_status` (rank 6) and `status_to_exit_code` (→2) already carry
`PRECONDITION_FAILURE` (`_common.sh:230,280,313`).

**Why the layer inherits it with no `layer3.sh` change (verified).** `layer3.sh` runs
`run_and_emit "guards" run-guards.sh` inside `{ … } | tee_collect_statuses`.
`tee_collect_statuses` collects EVERY `^STATUS=` line on stdout — so the per-guard
`PRECONDITION_FAILURE` line printed mid-run is collected alongside `run_and_emit`'s own
`STATUS=FAIL guards-failed`. `aggregate_worst_status(FAIL, PRECONDITION_FAILURE)` =
`PRECONDITION_FAILURE` → Layer 3 exits 2. Confirmed.

**exit-137 / SIGKILL ruling (deliberate, in-scope).** 137 here arrives specifically from
`timeout --kill-after`: the guard ignored SIGTERM and was SIGKILLed after the grace window
— the timeout mechanism firing *harder*. The only other dominant reading of a bare 137 is
an OOM kill, which is *also* an environment fact, not a diff defect. Every dominant
interpretation is environment → classify 137 as `PRECONDITION_FAILURE` alongside 124. The
task authorizes exactly "timeout and kill"; this is in-scope.

**run-guards.sh own exit code / counters — make standalone honest (LADDER-MIRROR / PRECONDITION-wins
per R-A / the FINAL LEAD RULING above).** Change:
- Add `PRECONDITION_GUARDS` counter + `PRECONDITION_GUARD_NAMES` array; the 124/137 arms
  increment THESE, not `FAILED_GUARDS`/`FAILED_GUARD_NAMES` (so `FAILED_GUARD_NAMES` keeps
  meaning "found a violation").
- Summary reports timed-out guards under a distinct "operator lane" heading; violations keep the
  existing "Failed guards" heading. R-A2: emit `STATUS=FAIL REASON=guard-violations` when
  `FAILED_GUARDS>0`, and the `MIXED_LANE:` line when both>0.
- **Exit precedence = LADDER-MIRROR / PRECONDITION-wins (R-A / FINAL LEAD RULING):**
  `PRECONDITION_GUARDS>0 → exit 2` **DOMINANT (checked FIRST)**, `elif FAILED_GUARDS>0 → exit 1`,
  `else 0` — mirroring `_common.sh::__status_rank` + `status_to_exit_code`. ONE ladder: standalone
  exit == layer verdict == pipeline lane. Carries an `ANCHOR (DRY):` comment naming the SSoT it
  mirrors (and why it can't call it — sources `guards/common.sh`, not `_common.sh`; and why the
  hand-rolled `STATUS=` line bypasses `emit_status`). Coupled with R-A2 (mandatory), which keeps the
  coexisting violation legible so exit 2 is safe.
- **Blast radius (verified)**: only `layer3.sh` (via `run_and_emit`, zero/non-zero only —
  the per-guard STATUS line still drives aggregation, so exit 1-vs-2 here does NOT change
  the layer verdict) and standalone callers (who benefit). No CI/YAML caller.

**Mixed case (one guard times out → PRECONDITION_FAILURE, another finds a real violation → FAIL) —
both boundaries AGREE (one ladder):**
- **LAYER boundary (layer3.sh)** AND **STANDALONE boundary (`./run-guards.sh`)** both → operator lane
  → **exit 2** (PRECONDITION rank 6 > FAIL rank 5). Deliberate per the ladder's intent: under
  contention severe enough to time a guard out, the run is untrustworthy — re-run on a quiet machine;
  the FAIL is deterministic and resurfaces there, so no defect is lost. Same masking already
  accepted+tested for Layer 7 PRECONDITION vs Layer 4 FAIL (`layer-all.test.sh` case (g)).
- **R-A2 keeps the violation legible** so exit 2 is not a mask: `STATUS=FAIL REASON=guard-violations`
  trace + the `MIXED_LANE: … exit 2 (operator lane); the <m> violation(s) above are REAL and must be
  fixed …` line name the coexisting violation loudly (this is what LICENSES ladder-mirror).
- **R-L retry discriminator applies**: a timeout that REPRODUCES on retry/quiet-machine is
  diff-caused → implementer lane + consume attempt (the timed-out guard rendered no verdict); only a
  non-reproducing timeout is the operator lane.

### Change 2 — fail-fast for interactive runs (`scripts/layer-all.sh`)

**Knob + default detection (config over hardcoding) — full precedence in R-B.** New
`DEVLOOP_FAIL_FAST`, resolved by the pure `_common.sh::fail_fast_mode()` helper (R-C):
- **validity FIRST**: any value other than `1/true/yes/0/false/no` → INVALID → caller exits 2 loud
  (fail-closed on a typo, loud even in CI/headless).
- **both unattended lanes force run-all**: `GITHUB_ACTIONS` OR `DEVLOOP_HEADLESS` set → run-all; a
  well-formed explicit `=1` there is REFUSED + announced (`WARN FAIL_FAST_OVERRIDE_IGNORED`, R-B/N1),
  not honored — an ambient var must not shrink an authority run's coverage. (Refused `=1` → run-all,
  NOT exit 2; only a malformed value exits 2.)
- **interactive (neither env)**: explicit `=1` → fail-fast, `=0` → run-all, unset → fail-fast.

**Stop condition = `rc != 0`** (the layer's observed process exit — same signal `final_exit`
accumulates). Covers FAIL(1), PRECONDITION_FAILURE(2), FAIL-MISSING-VERB(2), UNKNOWN(2), and
the "lying STATUS=OK but exit 1" case. OK / SKIPPED-* / N/A all exit 0 → not a stop. So a
Layer-3 guard timeout (now exit 2) STOPS an interactive run before Layer-7 cluster bring-up
— the two changes compose to kill exactly the 2026-08-14 waste.

**Honesty constraint — un-run layers rendered `NOT-RUN`, never OK.** On stop at layer N:
- `layer_status[N+1..7]="NOT-RUN"`, `layer_dur=0`, set in a post-break loop (so the arrays
  are fully populated — no `${…:-UNKNOWN}` ambiguity; UNKNOWN would wrongly map to exit 2).
- `NOT-RUN` is a **DISPLAY-ONLY marker, NOT a new aggregation enum**: the aggregation loop
  `continue`s past it, and `status_to_exit_code` is never called on it. So an un-run layer
  can neither demote nor inflate `TOTAL_RESULT`/`LAYER_ALL_EXIT`. `TOTAL_RESULT` = aggregate
  of layers that ACTUALLY ran (through the failing one) = the failing layer's status;
  `final_exit` = the failing layer's rc. Both truthful. (Task constraint "avoid a new
  aggregation enum" satisfied — NOT-RUN never aggregates.)
- A loud `FAIL_FAST: stopped at layer N (RESULT=…, exit …); layers N+1-7 NOT RUN. Set
  DEVLOOP_FAIL_FAST=0 to run all.` on stderr.
- `emit_gate2_verdict` safe (verified `_gate2_binding.sh:487,526-535`): GATE2 derives from
  `$layer_all_exit` alone; the per-layer loop just RECORDS `LAYER n NOT-RUN 0` (human-
  readable, not enforced). Truthful.

**Unattended run-all BEHAVIOR is byte-identical to today** (fail_fast=0 ⇒ no break, no NOT-RUN,
no skip in aggregation). CI is unchanged (`GITHUB_ACTIONS` ⇒ run-all by detection). The story
runner needs the **one-line S3 fix**: the close gate at `run-story.sh:1388` runs `layer-all.sh`
from a shell where `DEVLOOP_HEADLESS` is UNSET (it's a per-command prefix on the `claude`
invocation only), so it must be prefixed **`DEVLOOP_FAIL_FAST=0`** or it would fail-fast an unattended
authority gate. Why `DEVLOOP_FAIL_FAST=0` (not `DEVLOOP_HEADLESS=1`): R-B/Model B already refuses an
ambient `=1` under headless, so headless is functionally safe — but `=0` is defense-in-depth (the
close gate then does NOT depend on the helper's precedence table staying correct through future edits),
AND an inline `=0` prefix OVERRIDES any ambient `DEVLOOP_FAIL_FAST=1` inherited into the runner's shell
(a `DEVLOOP_HEADLESS=1` prefix adds a var but leaves the ambient `=1` present — the raw value @test's F1
truncation pin reads). So: CI unchanged; story runner one-line change (S3, `DEVLOOP_FAIL_FAST=0`);
per-task gate untouched (loops layerN itself, never calls layer-all).

### Test seam (`scripts/layer-all.test.sh`) — @test paired, co-owned

`layer-all.test.sh` runs every devloop under Layer 3 with NEITHER env var set ⇒ new default
= **fail-fast**. Cases that assert worst-wins-ACROSS-layers break: **case (g)** (FAIL@L4 +
PRECONDITION@L7 → asserts exit 2) would now stop at L4 → exit 1. Plan (co-authored with
@test):
- Pin the run-all cross-layer cases (g, and any multi-failing-layer case) to explicit
  run-all via `DEVLOOP_FAIL_FAST=0` (keeps the worst-wins property tested). `DEVLOOP_HEADLESS=1`
  is the alternative; `GITHUB_ACTIONS=1` CANNOT be used because it's rejected when
  `LAYER_SCRIPT_DIR` is set (sentinel).
- ADD fail-fast cases (neither env var; default fail-fast): (i) FAIL@L3 + stub@L7 → stop at
  L3, exit 1, L4-7 `RESULT=NOT-RUN`, and `assert_no_marker ran.layer7` (proves it was SKIPPED,
  not just relabeled). (ii) PRECONDITION@L3 → stop at L3, exit 2, L4-7 NOT-RUN. (iii) explicit
  `DEVLOOP_FAIL_FAST=0` interactive → runs all (marker present for all). (iv) `DEVLOOP_HEADLESS=1`
  → unattended default run-all. (v) bad `DEVLOOP_FAIL_FAST=ture` → exit 2 loud, no stub ran.
- (Optional, discuss) a `run-guards.sh` timeout smoke-test — currently NO run-guards self-test
  exists; a sleepy-fake-guard case (à la 2026-05-19) proves exit 2 + `PRECONDITION_FAILURE`.
  Weigh vs scope with @test; at minimum I smoke-test manually.

### Documentation (in scope)

- `docs/runbooks/devloop-validation.md`: §3 STATUS ladder `PRECONDITION_FAILURE` row (now
  Layer 3 AND 7; add the two guard-timeout REASON tokens); §3 exit-2 table note; §6.3 add a
  guard-timeout failure-mode row (operator lane, does NOT consume an attempt, re-run quiet,
  cite 2026-08-14); a concise fail-fast + `DEVLOOP_FAIL_FAST` + `NOT-RUN`-render note (§1/§2);
  cross-ref the existing task-#58 "If it TIMES OUT" note (REASON token unchanged, add "now
  operator lane").
- `scripts/layer-all.sh` header: reconcile the run-all wording (now conditional on unattended
  mode) + document the knob.
- `docs/decisions/adr-0033-polyglot-validation-pipeline.md` §4: an AMENDMENT note (mirroring
  the existing 2026-08-20 amendment style) — the "runs layer1..7 sequentially / all layers"
  invariant now holds unconditionally only for UNATTENDED callers; interactive runs fail-fast.
  Warranted (changes a documented invariant); small focused amendment, not a new ADR.

### Verification + S3 (runner DOES need a one-line change)
Runner routing (verification, no change): `run-story.sh:1204-1221` routes `gate_rc==1`→implementer
(consumes attempt), `≠0`→operator (`record_infra_incident`, manifest untouched, no attempt). Layer 3
→ exit 2 lands operator automatically. **Confirmed by read**; re-confirm end-to-end after impl.
**S3 (DOES need a change — @security F1, Lead-confirmed):** the story-close authority gate at
`run-story.sh:1388` runs `./scripts/layer-all.sh` from the runner's own shell where `DEVLOOP_HEADLESS`
is UNSET (line 1041 sets it only as a per-command prefix on the `claude` invocation). A naive detector
would fail-fast that unattended authority run. **Fix: `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh
>"$closelog" 2>&1`** at line 1388 (Res-1 — `=0` over `DEVLOOP_HEADLESS=1`: Model B independently
refuses an ambient `=1` there, but `=0` is defense-in-depth AND overrides an ambient `=1` the runner
inherited, which a `DEVLOOP_HEADLESS=1` prefix does not; it's also what @test's F1 truncation pin
asserts). The per-task gate at :1168 loops layerN itself, already breaks first-fail, never calls
layer-all — untouched.

---

## Implementation Summary

Built to the FINAL LEAD RULING (ladder-mirror + R-A2, Model B, S3 = `DEVLOOP_FAIL_FAST=0`).

**Production code (mine):**
- `scripts/guards/run-guards.sh` (Change 1) — `classify_guard_exit` 124/137 arms emit
  `STATUS=PRECONDITION_FAILURE REASON=guard-timeout[-kill]-<name>` (stdout) + a line-anchored
  `PRECONDITION_FAILURE: guard <name> timed out …` banner (stderr, into `layer-3.stderr.log`). New
  `PRECONDITION_GUARDS`/`PRECONDITION_GUARD_NAMES` counters (124/137 increment these only). Summary
  emits `STATUS=FAIL REASON=guard-violations` when `FAILED_GUARDS>0` + the `MIXED_LANE:` line when
  both>0 (R-A2). Exit block (ANCHOR (DRY) commented): `PRECONDITION_GUARDS>0 → exit 2` first,
  `elif FAILED_GUARDS>0 → exit 1`, else 0. Header `:8-12`/`:116-117` comments reconciled.
- `scripts/lang/_common.sh` — new pure `fail_fast_mode()` (Model B, validity-first; tri-state
  `FAILFAST/RUNALL/INVALID` + SOURCE label; no self-exit).
- `scripts/layer-all.sh` (Change 2) — `readonly NOT_RUN` + guardrail comment; mode detection after the
  sentinel (INVALID → exit 2 loud; refusal → `WARN FAIL_FAST_OVERRIDE_IGNORED`); always-on
  `PIPELINE_MODE=… SOURCE=…`; `rc != 0` fail-fast break at loop end; post-break NOT-RUN marking +
  `STOPPED_EARLY` + `fail-fast-exit-invariant` defensive assert; budget check gated on L3+L6 both run
  (`WARN BUDGET_TOTAL_SKIPPED`); aggregation `continue`s past NOT-RUN.
- `scripts/workflow/run-story.sh:1388` (S3) — `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh …`.
- `scripts/layer3.sh` — wired the new `run-guards.test.sh` self-test.

**Docs/ADR/SKILL lockstep (mine):** runbook §3 ladder row + new "Fail-fast vs run-all" subsection +
§4 emitter list + §6.3 guard-timeout/guard-violations rows + retry discriminator + §8 caveat fix
(the "layer3 flattens PRECONDITION_FAILURE" claim was false) + §8 catalogue rows + §11 changelog;
ADR-0033 §4 amendment; ADR-0034 §9 amendment; `ts_retained_credentials.rs:577` `///` enum (Mechanical);
SKILL.md D1 (attempt-budget de-scope, any-layer PRECONDITION_FAILURE) + D3 (:387 → citation) + :407/:423
+ S5 retry note; TODO.md D2 amendment + entry closed + SSoT-pointer fix + R-I re-defer/N4; INDEX.md
pointer fix. **Doc-citation fixes**: `run-story.sh:1388` line-cites → `run-story.sh` (no-line-numbers
guard); `run-guards.sh::classify_guard_exit` `::` cites dropped (the fn is indented inside an `if`,
so `SH_FN_PAREN_RESOLVER`'s `^`-anchored regex can't resolve it).

**Tests (@test authored, verified green against my code):** `_common.test.sh` fail_fast_mode truth
table; `layer-all.test.sh` fail-fast cases (incl. reworked case (g)); `run-story.test.sh` F1 pin;
new `run-guards.test.sh`.

**Local verification (quiet machine):** full guard suite 37/0; `_common.test.sh` 72/0;
`layer-all.test.sh` 64/0; `run-story.test.sh` 187/0; `run-guards.test.sh` 17/0; `cross-boundary-scope`
clean; doc-citation guards clean; `cargo check`/`fmt` on dt-guard clean. Change-1 smoke (PATH-stub
`timeout`): all-124/137→exit 2 + PRECONDITION lines + banners; all-1→exit 1 + guard-violations;
mixed→exit 2 + guard-violations + MIXED_LANE; all-0→exit 0. Change-2 smoke (stub seam): interactive
fail-fast stops@first rc≠0 with NOT-RUN + STOPPED_EARLY + BUDGET_TOTAL_SKIPPED; run-all (headless/=0)
runs all 7 with worst-wins (case-(g) FAIL@L4+PRECONDITION@L7 → exit 2); refusal → run-all + WARN;
INVALID → exit 2 loud.

---

## Validation (Gate 2)

**PASSED** — `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh` (run-all, to get the complete one-pass report), exit 0.

| Layer | Result | Notes |
|-------|--------|-------|
| 1 Compile | OK | cargo-build + dt-guard + dt-story + ts + proto |
| 2 Format | OK | rust + ts + proto |
| 3 Guards | OK | 37 guards / 0 violations; all self-tests incl. **run-guards.test.sh (NEW)**, layer-all-orchestrator, run-story |
| 4 Test | N/A | cargo-test + nx-test PASSED; proto placeholder N/A propagates (benign, pre-existing) |
| 5 Lint | OK | clippy + nx-lint + buf-lint |
| 6 Audit | N/A | audit SKIPPED-NO-DIFF (no dep-manifest change); proto placeholder N/A |
| 7 Env-tests | OK | **env-tests-passed + browser-e2e-passed** against a live Kind cluster (479s) |

`TOTAL_RESULT=N/A` (exit 0) — the benign proto-placeholder aggregate for a clean run of this repo; N/A ranks above OK in the label but is success-class (`status_to_exit_code`=0). **Zero FAIL / PRECONDITION_FAILURE / UNKNOWN / FAIL-MISSING-VERB anywhere** — since N/A was the worst status, nothing higher-ranked occurred. Change-2 validated live: `PIPELINE_MODE=run-all SOURCE=run-all-env` on stderr.

**Post-Gate-2 fixes re-validated:** the F1 (runbook §4 ordinal, docs-only) and F2 (run-guards.sh MIXED_LANE→stderr) fixes + @test's +1 stderr pin landed after the full run; re-ran **Layer 3 on the clean final tree → exit 0** (37 guards/0, run-guards-selftest 21/0, all self-tests green), which re-validates the only changed production file (run-guards.sh); layers 1/2/4/5/6/7 are unaffected by a docs fix + one stderr line. Classification-sanity guard: STATUS=OK.

---

## Review (Gate 3)

**All six verdicts in — no ESCALATE. Gate 3 PASSED.**

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | RESOLVED-DEFERRED | 0 new (all Gate-1 landed) | — | 1 (STATUS-rank drift guard) |
| Test | CLEAR | 0 | — | — |
| Observability | RESOLVED-DEFERRED | 2 | 2 | 1 (N5 cosmetic) |
| Code Quality | CLEAR | 0 | — | — |
| DRY | RESOLVED-DEFERRED | 1 (F1) | 1 | 1 (ADR-0019 extraction) |
| Semantic Guard | CLEAR | 0 | — | — |
| Operations | (implementer) | — | — | — |

- **@security — RESOLVED-DEFERRED.** All seven Gate-3 criteria verified in code (R-A2 mandatory + load-bearing comment; sentinel ahead of loop+mode-detection; S1 rc-based stop + defensive assert; S2 NOT-RUN guardrail; R-B Model B; S3 =0 at :1388; R-L reproduce-discriminator). Security test coverage incl. the counter-separation negatives, the exit-0-skip-doesn't-stop cases, the GITHUB_ACTIONS-arm pin, all three F1 pins. One accepted deferral (below). ts_retained_credentials.rs Mechanical, correct.
- **@test — CLEAR.** Four test files cover both changes + F1, hermetic (PATH-stubbed timeout, no cluster/sleep, DEVLOOP_TEST seams intact), non-vacuous. Verified: run-guards self-test derives every mixed-case arm from `status_to_exit_code` (IS the mechanical drift guard); GITHUB_ACTIONS arm covered in _common.test.sh; F1 pins non-vacuous. Final totals run-guards 21 / _common 72 / layer-all 64 / run-story 187 = 344, all green.

Running log of reviewer findings + responses (fix by default; defer with justification).

- **@semantic-guard — CLEAR.** All four fail-open/masking watch items verified against the diff
  (NOT-RUN unbypassable + never aggregates; 124/137 → PRECONDITION_* only; PRECONDITION-first exit;
  R-A2 renders both traces). No findings.
- **@observability — 2 findings, BOTH FIXED (no deferrals).**
  - *Finding 1 (stale ordinal):* runbook §4 said "A THIRD emitter" for `setup.sh`, but adding
    run-guards.sh made it the fourth. Fixed by DROPPING the ordinal (`A further emitter (not a layer
    script) is …`) — the ordinal was the stale-prone part (same class as D3/F6 this diff fixes).
  - *Finding 2 (MIXED_LANE stdout-only):* the §4 one-pass triage reads stderr, so a mixed-case
    operator grepping stderr saw only the timeout banner and could dismiss it — the exact dismissal
    R-A2 exists to prevent, and R-A2 is what licenses ladder-mirror. Fixed (not deferred): run-guards.sh
    now echoes `MIXED_LANE:` to stderr AS WELL AS stdout (kept stdout for @test's assert + the standalone
    summary). `guard-violations` stays stdout-only deliberately (a `STATUS=` line tee_collect_statuses
    must vote on). Verified: MIXED_LANE on both streams, run-guards.test.sh stays 20/0.
- **@code-reviewer — CLEAR.** ANCHOR (DRY) + NOT_RUN const + break placement + ladder-mirror exit +
  R-A2 all verified against code; three self-tests green (20/72/64). Optional micro-nit TAKEN: removed
  the dead `ff_rc` capture in layer-all.sh (helper always returns 0; `|| true` is the clean idiom).
- **@dry-reviewer — 1 finding (F1) + a TODO append; both handled.**
  - *F1 (§4 ordinal drift):* SAME as @observability Finding 1 — ALREADY FIXED (dropped the ordinal
    entirely; @dry's message crossed the fix). Classification table complete (16 rows / 16 files).
  - *TODO append (ADR-0019 extraction, not a diff change):* appended to TODO.md §Cross-Service
    Duplication — `_common.test.sh`'s local `assert_exit_code` duplicates `_test_helpers.sh` + the
    `dry-reviewer/INDEX.md:14` "only definitions in-tree" inaccuracy. Owner: test.
- **Environment false-red handled (NOT a code change):** the full guard suite briefly showed
  `validate-subdomain-regex-sync` FAIL — root cause was 3 stray gitignored coverage artifacts
  (`packages/sdk-core/coverage/*.html` + 2 `.nx/cache` copies) created THIS session by @test's TS
  builds (my earlier clean run was 37/0). The guard excludes `node_modules/target/dist` but NOT
  `coverage/`/`.nx/`. Not real regex drift (the pinned count 10 is correct for source). Removed the
  gitignored artifacts → suite back to 37/0. Flagged the guard-exclusion gap to @team-lead as a
  recommended follow-up (out of this task's scope; CI is unaffected since Layer 3 runs before the
  coverage-generating Layer 4). **RESOLUTION — @team-lead ruled DEFER (reversing an interim fold-in),
  on @dry-reviewer's deeper read:** the guard is an EXACT-EQUALITY pin (`total -ne EXPECTED_TOTAL`,
  10 = 7 enumerated + 3 unenumerated test pins), so a `--exclude-dir=coverage/.nx` band-aid is (a)
  whack-a-mole (the next generated dir reopens it) and (b) leaves a FALSE-NEGATIVE: delete an
  unenumerated pin (10→9) while a stray artifact adds +1 (→10) and the guard passes SILENTLY having
  lost a real pin — the drift-masking it exists to prevent. The proper fix (scan **tracked files only**
  via `git ls-files -z | grep -F`) closes both directions by construction but is task-sized (it
  interacts with the `SUBDOMAIN_GUARD_ROOT` synthetic-tree seam, which isn't a git repo). So: NO guard
  edit in this diff; the interim `--exclude-dir` hunk + its table row + this note's fold-in claim were
  REVERTED; a `docs/TODO.md` entry captures the proper fix (@dry-reviewer to draft, I appended). Stray
  `coverage/`/`.nx/` artifacts cleaned so the Layer-3 re-run is honest.

---

## Accepted Deferrals

Four accepted deferrals — **all pre-existing items surfaced during this work, none introduced by this changeset; nothing from this diff is left broken.** Bodies live in `docs/TODO.md`; these are pointer bullets (§Accepted Deferrals pointer-only discipline, `todo-tracking` guard).

- **STATUS-rank-ladder three-way drift guard** (@security; re-deferred with N4 scope-widening; task-sized `dt-guard` subcommand; owner: operations) → `docs/TODO.md` §Env-Test Resilience & Runbook Validation.
- **`SOURCE=story-close` label refinement** (@observability N5) — deliberately NOT `docs/TODO.md`-tracked (Lead ruling: a log-label nicety, not a coverage gap; correctness half already closed by S3); recorded here so the decision-to-not-track stays visible rather than silent.
- **`_common.test.sh` / `_test_helpers.sh` extraction** (@dry-reviewer, ADR-0019 exception; owner: test) → `docs/TODO.md` §Cross-Service Duplication.
- **`validate-subdomain-regex-sync.sh` tracked-files-only rewrite** (@implementer + @dry-reviewer; Lead reversed an interim fold-in; owner: operations) → `docs/TODO.md` §Env-Test Resilience & Runbook Validation.

---

## Final Summary

**Complete.** Both changes landed as specified, plus the interactive attempt-budget de-scope (D1 — the task's "check the runner" framing missed that the motivating 2026-08-14 incident was interactive) and the diff-caused-timeout discriminator (S5). Full doc lockstep (ADR-0033 §4 amendment, ADR-0034 §9, runbook §3/§4/§6.3/§8, SKILL.md attempt-budget + :387 citation, TODO.md). Gate 1 unanimous, Gate 2 clean (all seven layers incl. live-cluster env-tests + browser E2E), Gate 3 all six verdicts CLEAR/RESOLVED-DEFERRED with no ESCALATE. 344 test assertions green.

Notable — three better-than-asked qualities in the diff (recorded per @observability): the §6.3 reproduce-on-retry discriminator closes a masking hole the operator-lane framing would otherwise have created; the budget gate keyed off NOT-RUN status (not `layer_dur==0`) avoids a false-fire on a genuinely fast Layer 6; and the "do not complete the enum" NOT_RUN guardrail + the ANCHOR-names-the-only-drift-guard comment make the next editor's mistake loud instead of silent — the property this whole change is about.
