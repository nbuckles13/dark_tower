# Devloop Output: run-story.sh seams, hermetic test suite, and the five R-2 defects

**Date**: 2026-08-13
**Task**: Add `DEVLOOP_TEST`-gated seams (`STORY_REPO_ROOT`, `DT_STORY`) to `run-story.sh`, validate `--stop-after` against the manifest, build a hermetic test suite at `scripts/workflow/run-story.test.sh` wired into `layer3.sh`, fix the five R-2 defects with a failing test each, and give the canary `--allowedTools ""`.
**Specialist**: test (paired with infrastructure)
**Mode**: Agent Teams (v2) — full, headless (run-story task #1)
**Branch**: `feature/story-runner-hardening`
**Duration**: ~2h (setup → commit; 3 review→implementation iterations, 4 pipeline runs of which 1 binding)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `9807f0b507a5c533580284b2ae86679e1ce29ee6` |
| Branch | `feature/story-runner-hardening` |
| Story | `docs/user-stories/2026-08-11-story-runner-hardening.md` (task #1, R-1..R-5) |
| Headless | Yes (`DEVLOOP_HEADLESS=1`) — escalation contract per SKILL.md §Headless Mode |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` (Gate 3 passed) |
| Implementer | `implementer` (spawned) |
| Implementing Specialist | `test` |
| Paired Specialist | `paired-infrastructure` (spawned) |
| Iteration | `1` |
| Security | `security` (spawned) |
| Test | `test` (spawned) |
| Observability | `observability` (spawned) |
| Code Quality | `code-reviewer` (spawned) |
| DRY | `dry-reviewer` (spawned) |
| Operations | `operations` (spawned) |
| Semantic Guard | `semantic-guard` (spawned) |

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | **confirmed** |
| Test | **confirmed** |
| Observability | **confirmed** (O-16 carried to Gate 2, not gate-blocking) |
| Code Quality | **confirmed** (CQ-9a carried to Gate 3; confirmation explicitly not an endorsement of the ordering-only probe) |
| DRY | **confirmed** |
| Operations | **confirmed** |
| Semantic Guard | **confirmed** |
| Paired Infrastructure | **confirmed** |
| Protocol (co-implementer) | **confirmed** |

### Gate 3 — Final Verdicts (Lead record)

| Reviewer | Verdict | Notes |
|----------|---------|-------|
| Security | **RESOLVED-FIXED** | 1 finding — `DEVLOOP_TMP_DEFAULT`, a *live* containment bypass |
| Test | **RESOLVED-DEFERRED** | T-1/T-2/T-3 fixed; M3 accepted as **inherited** debt |
| Observability | **RESOLVED-FIXED** | O-1..O-16 + OBS-1..OBS-4 |
| Code Quality | **RESOLVED-FIXED** | CQ-1..CQ-17; 4 classification upgrades honoured at Gate 1 |
| DRY | **RESOLVED-FIXED** | 1 finding; 2 extraction opportunities → `docs/TODO.md` |
| Operations | **RESOLVED-DEFERRED** | OPS-1..OPS-4 fixed; 2 accepted deferrals |
| Semantic Guard | **RESOLVED-FIXED** (native **SAFE**) | 3 findings fixed |
| Paired Infrastructure | **RESOLVED-DEFERRED** | 3 findings fixed; 2 accepted deferrals |
| Protocol | **RESOLVED-FIXED** | 4 findings fixed; Ownership Lens on both `dt-story` rows |

**Zero ESCALATED.** Three reviewers landed RESOLVED-DEFERRED, so accepted deferrals exist — see
§Accepted Deferrals. A single accepted deferral forces RESOLVED-DEFERRED for that reviewer
regardless of how many findings were fixed; that is deliberate, so the cost shift is not averaged
away by the fixes.

**Binding Gate 2 run** (quiescent frozen tree — three identical `git diff HEAD | sha256sum`
samples before start): `LAYER_ALL_EXIT=0`. Layers 1/2/3/4/5/7 `OK`; Layer 6 `N/A` on
self-justifying reasons only (`no-dep-changes` ×2 dep-gated, proto's
`not-applicable-to-this-lang` placeholder, `buf-breaking-passed`) — no `FAIL-MISSING-VERB`.
Layer 3 12s against the per-layer 20s warn. `run-story.test.sh: 153 passed, 0 failed`;
`layer-all.test.sh: 26 passed, 0 failed`; **zero** `CONTAINMENT-*` / `NOT-QUIESCENT` tokens.

Three earlier pipeline runs were discarded as non-binding because the tree moved underneath them.
Only a run on a frozen, quiescent tree binds.

### Lead arbitrations

1. **ADR-0024 §6.2 auto-route to ESCALATE** — ruled the **purposive** reading: an upgrade raised
   at Gate 1 and accepted uncontested does *not* force ESCALATED; that is reserved for an upgrade
   surviving to Gate 3 contested. Grounds: `review-protocol.md` defines ESCALATED as *unresolved
   disagreement*; §Classification Monotonicity states the rule's purpose as protecting owners
   *during fix-or-defer triage*, a Gate-3 activity; and the literal reading would penalise raising
   a concern at the gate the ADR itself calls cheapest. @paired-infrastructure then found the same
   ruling already recorded in **three prior devloops** — so the defect is in the text, not in its
   readers. Filed as an ADR amendment (`docs/TODO.md`), naming both ADR-0024 §6.2 *and*
   `review-protocol.md` §Classification Monotonicity, since changing one moves the seam.
2. **E4b scope** — the escalate-lane resume defect is **out of scope**: pre-existing, and the fix
   is a runner state-machine change straddling `dt-story`'s contract. Filed loudly rather than
   patched unreviewed after a freeze. @operations' framing is the one that matters: the operator
   lane's recovery works and the implementer lane's does not, so the lane distinction this task
   built is currently sound on one side only — and the broken side is the one the runner exits
   through most often.

**Gate 1 CLOSED — "Plan approved" issued.** Classification-sanity guard re-run against the
amended table: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`.

**Conditional domain reviewer added at Gate 1**: `protocol`. The plan classifies
`crates/dt-story/src/main.rs` and `tests/cli.rs` as *Not mine, Minor-judgment* with owner
`protocol`, and ADR-0024 §6.3 requires the named owner to be a reviewer on the devloop and to
confirm the hunk at Gate 1 and Gate 3. Protocol was not on the panel, so the classification was
not satisfiable as written until it was added.

**Classification-sanity guard (Gate 1)**: `validate-cross-boundary-classification.sh` →
`STATUS=OK REASON=cross-boundary-classification-clean-1-files`.

Note the guard could not run on first invocation: `target/release/dt-guard` was not built and the
guard exited **1** — the *implementer* lane. That is a live instance of the misdeclared-enum
problem §Planning's R-4 section says the split does not close (a wiring fault arriving as rc 1).
Lead built `dt-guard` + `dt-story`; guard then passed. Recorded as a measurement rather than a
projection.

**Lead ruling — scope expansion approved in principle** (so it is not relitigated at Gate 3):
`devloop-stop-hook.sh` (ADR-0035 names it as R-2 defect 4's site — the requirement, not creep),
`preflight-story.sh` (one precondition line), `.github/workflows/ci.yml` (3 lines; the review
protocol's anti-pattern list rules out "one-line change outside the PR's intended scope" as a
deferral, and the owner is on the panel), and `crates/dt-story/` (the only route to R-5 without a
second manifest parser).

---

## Task Overview

### Objective

`scripts/workflow/run-story.sh` has zero tests and five known defects. This devloop makes the
runner testable without letting a test run touch the operator's repository, and fixes the
defects — each with a test that fails against today's code and passes after the fix.

Requirements covered (`docs/user-stories/2026-08-11-story-runner-hardening.md`):

- **R-1** — a runner test run cannot touch the operator's repository, invoke a real `claude`,
  sleep in real time, or run the real validation pipeline. Containment is *verified*: the
  redirected root must be its own git top-level **and** differ from the real one.
- **R-2** — five defects, each with a failing-first test:
  1. canary classification misfires on a typo in prose matching;
  2. canary stderr is merged into the stdout file it parses, so one warning line makes the JSON unreadable;
  3. every operator-class gate failure is recorded against the implementer;
  4. any transient git error is misclassified;
  5. the task prompt is interpolated into `/devloop "%s" --specialist=%s`, so a double quote truncates the instruction.
- **R-3** — failure classification pinned against every `api_error_status` shape production emits
  (present-with-429, present-with-null, absent) and against non-JSON output.
- **R-4** — rc 1 stays a task escalation; rc 2 routes to the infra lane with the manifest untouched.
- **R-5** — `--stop-after <id>` validated against the manifest, not as a bare integer.

Plus the ADR §6 one-liner: the canary runs with `--allowedTools ""`, keeping
`--dangerously-skip-permissions`.

### Scope
- **Service(s)**: none — workflow scripts only (`scripts/workflow/`, `scripts/layer3.sh`, `scripts/lang/_test_helpers.sh`)
- **Schema**: No
- **Cross-cutting**: Yes — the runner is the story-workflow substrate; operations + infrastructure + security all hold stakes

### Debate Decision
NOT NEEDED — ADR-0035 is accepted and this is its named follow-on (story task #1). No new
architecture; bounded to existing scripts.

### Team-composition note

The runner emits `/devloop "%s" --specialist=%s` and structurally cannot carry `--paired-with`.
The story's Implementation Plan lists this task as `test` **paired with `infrastructure`**, and
its "run the devloops individually" block spells the flag out. The Lead therefore spawned
`paired-infrastructure` as an active collaborator and Gate-2/Gate-3 reviewer, matching the
planned composition rather than the flag the runner's format string could fit.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2. Every file the plan touches has a row, not only the cross-boundary ones.

| Path | Classification | Owner (if not mine) | Note |
|------|----------------|---------------------|------|
| `scripts/workflow/run-story.sh` | Not mine, Domain-judgment | `infrastructure` (@paired-infrastructure, active co-implementer) | The story runner is workflow/infra surface. I am `test`; the story pairs this task with `infrastructure` precisely because the seam design, containment model and lane taxonomy are infra judgment. Co-designed, not merely reviewed. |
| `scripts/workflow/run-story.test.sh` (new) | Mine | — | Test suite; `test` owns coverage strategy and quality gates. |
| `scripts/workflow/devloop-stop-hook.sh` | **Not mine, Domain-judgment** (U-3) | `infrastructure` (@paired-infrastructure, §6.5 pairing) | The flip blocks stops that are permitted today, and the *remedy shape* — fail-closed, a distinct block reason, and deliberately **not** lowering `DEVLOOP_STOP_BLOCK_MAX` for that branch — is judgment ADR-0035 did not make for me: it names the defect, not the remedy. |
| `scripts/layer3.sh` | **Not mine, Minor-judgment** | `infrastructure` (@paired-infrastructure, trailer given in-thread) | **Upgraded from Mechanical at the owner's request, and they are right.** §Rollback calls this hunk "this change's largest blast radius"; a hunk that is simultaneously the largest blast radius and `Mechanical` is a contradiction inside one document. Against review-protocol.md's sed-test it fails all three legs: structure is *added* not substituted; it is not value-neutral (it changes what the always-run gate does for every contributor on every run, which is why it has its own trip threshold and first-position rollback step); no guard covers the change-pattern. "Identical in form to its six neighbours" is a claim about shape; the sed-test is about effect. |
| `scripts/lang/_test_helpers.sh` | Not mine, Minor-judgment | `infrastructure` (shared pipeline scaffolding; `test` is the domain consumer) | Promoting `assert_marker`/`assert_no_marker`/`assert_absent` in parameterized form + refreshing the stale file header. Sourced by 8 other test files; signature pre-ruled by @dry-reviewer. |
| `scripts/layer-all.test.sh` | Mine | — | `@test-owned` per `layer7.test.sh:28-29`. **7 conversions** (@dry-reviewer's measurement, correcting my "8 marker call sites"): `assert_marker` at `:159`/`:209`, `assert_no_marker` at `:185`/`:198`, `assert_absent` at `:148`/`:184`/`:208`, plus the 3 definitions deleted from `:85-96`. Zero local copies left. |
| `scripts/workflow/preflight-story.sh` | **Not mine, Domain-judgment** (U-2) | `infrastructure` (@paired-infrastructure, §6.5 pairing satisfies the mechanism) | **Row corrected — it is not "one line".** Two assertions: a combined `dt-guard`+`dt-story` guard-binary check (one check naming `cargo build --release -p dt-guard -p dt-story`), and a **verb-currency probe** (`list-tasks --help`, zero-vs-non-zero). ~15 LoC with a **new refusal lane** — the plan's own words are "newly refuses a run that succeeds today", and §6.2 lists behaviour changes under Domain-judgment. Both lines `|| fail`-guarded (that selects the operator lane, not style). Ordering is load-bearing: both sit beside `:69`'s existing `[ -x ]`, **after** the `:58-64` container-boundary gate, which stays first — it is ADR-0035 §5's entire safety argument and no missing-binary check may exit before the host/container question is settled. |
| `.github/workflows/ci.yml` | Not mine, Minor-judgment | `infrastructure` (@paired-infrastructure, hunk-ACK offered in-thread) | 3 lines: unconditional `cargo build --release -p dt-guard -p dt-story` before `layer-all.sh`, mirroring `ci-client.yml:84-89`. Guard binaries are produced by a *skip-gated* compile verb (`lang/rust/changed.sh:6` triggers only on `crates/`) but consumed by *always-run* Layer-3 guards, so a `packages/**`-only PR reds layer 3 today. Wiring my suite in adds a second victim to a live break; declining would spend the story's deferral budget on a 3-line fix whose owner is in the room. **Closes the CI half only — see below.** |
| `crates/dt-story/src/main.rs` | **Not mine, Domain-judgment** (U-4) | `protocol` — **active co-implementer of this hunk** per §6.3/§6.5, not merely reviewer | New read-only `list-tasks` verb (R-5) + its bullet in the `:1-12` CLI contract block, which is *literally an exit-code contract*. @code-reviewer is right and my original text was wrong twice: §6.2 lists **API semantics** under Domain-judgment and Minor-judgment's examples are without exception adjustments to existing things, never new public interface; and my "no gate authority" claim is contradicted by my own §R-5, where preflight probes this verb and refuses the run (CQ-8). @protocol chooses the projection. |
| `crates/dt-story/tests/cli.rs` | **Not mine, Domain-judgment** (U-4) | `protocol` (co-implementer) | CLI test for the new verb, alongside the existing 26. |
| `docs/runbooks/devloop-validation.md` | Not mine, Minor-judgment | `infrastructure` (@paired-infrastructure — **written by the owner**, already in the tree) | Two §8 symptom rows (`dt-guard-binary-missing`, `dt-story not built but manifests exist`) naming the skew as producer/consumer rather than code defect, with @code-reviewer's three-consumer framing (CI covered by the new step, **plain local devloop not**), plus a §8 caveat that `run-story.sh` routes a gate exit 1 to a *task escalation*, so a `pipeline-red` naming either symptom is not the task's fault. Row present so the Gate-2 scope-drift guard doesn't flag it — the guard doing its job. |
| `docs/TODO.md` | Not mine, Mechanical | `operations` | **Filed by reviewers, not by me** — @dry-reviewer (sentinel-idiom + git-toplevel-predicate extraction opportunities, §Cross-Service Duplication), @test (M3, §Spin-out tracking), @paired-infrastructure (guard-binary producer/consumer skew **and** the enum collapse, §Polyglot Pipeline Follow-ups). Row present so the table accounts for every file the change causes to move, per §Spin-out tracking. |
| `docs/devloop-outputs/2026-08-13-run-story-seams-and-hermetic-tests/main.md` | Mine | — | This file. |
| `docs/user-stories/2026-08-11-story-runner-hardening.md` | Not mine, Minor-judgment | `operations` (story owner) | **Prose only, one Deferred bullet.** This change *falsifies* a documented claim: the "sixth defect, unreachable as written" entry cleared the `sort \| head` pipeline on SIGPIPE reasoning that is correct for SIGPIPE and silent about the `find`-exit mechanism, which was reachable and is fixed here. Corrected in this commit rather than at story close, because a Deferred entry exists so re-opening is a decision rather than a rediscovery — left as written it would stop the next reader looking. Whole-file exempt from the scope-drift guard (`cross_boundary_scope.rs`), so the row is for the record, not the gate. No manifest-block edit; `dt-story complete` owns that region. |
**`ci.yml` closes half the break, knowingly** (@test's finding, accepted as their option (a)). The
skip-gate lives in `_dispatch.sh:136-138`, which runs identically in a local devloop, so a fresh
worktree with a `packages/**`-only change has no `target/`, skips rust compile, and reds Layer 3
the same way — and after this commit it reds my suite's precondition too. The `ci.yml` hunk does
not touch that, and it creates a **second build site for the same two binaries** alongside
`lang/rust/compile.sh:17-18`, which is the drift hazard CLAUDE.md §Single source of truth names.
@code-reviewer (CQ-5) puts it as one invariant with three consumers, which is the framing the
hunk comment adopts:
|---|---|
One invariant, two mechanisms, one gap. I am not redesigning it here, and @test explicitly did not
ask me to: `layer1.sh:3-9` calls
dispatcher-as-single-enforcement-point "the load-bearing pipeline invariant", so moving these
builds outside the dispatcher is a real ADR-0033 layering decision, and `ci-client.yml:84-89` is
existing precedent for the shape chosen. So: a one-line note lands **in the `ci.yml` hunk itself**
recording that the local half is knowingly open and why, and @paired-infrastructure's follow-up
entry names the local half explicitly rather than only the CI symptom. What must not happen is the
local half closing silently by assumption because `target/` usually happens to be warm.
**Ownership Lens rulings accepted in full (U-1..U-4, @code-reviewer).** I am not contesting any of
the four, and two of them correct errors of mine rather than matters of taste:
- **U-1 `layer3.sh` Mechanical → Minor-judgment.** Accepted before @code-reviewer ruled, at
  @paired-infrastructure's request. The decisive argument is the ADR's own default, not blast
  radius: §6.2 requires Mechanical to be *covered by the guard pipeline*, and defaults to
  Minor-judgment where no guard covers the file-type × change-pattern. My own **M3** establishes
  that none does — and the dangerous partial version of this edit (test file present, wiring line
  absent) is precisely the silent state no guard catches.
- **U-2 `preflight-story.sh` → Domain-judgment.** My row said "one line"; §R-5 specifies ~15 with a
  new refusal lane. The row was wrong and the tier followed the wrong row.
- **U-3 `devloop-stop-hook.sh` → Domain-judgment.** Consistent with U-2 rather than lenient on one
  of a pair.
- **U-4 `crates/dt-story/**` → Domain-judgment, with @protocol as active co-implementer.** The
  right call and I should have seen it: I justified Minor-judgment partly on "no gate authority",
  and my own §R-5 has preflight probe the verb and refuse the run on it (CQ-8). Requesting
  @protocol co-implement those three hunks — the same arrangement `run-story.sh` already has with
  @paired-infrastructure. No spin-out.
@code-reviewer's coherent reading is the one I'd endorse: the infrastructure-owned surface here is
**one Domain-judgment cluster** — which is exactly why the story paired this task with
`infrastructure` — with `layer3.sh` the single Minor-judgment exception, its content fully
determined by its six neighbours.
Per ADR-0024 `:382`/`:434`/`:464`, owner **confirmation at Gate 1 and Gate 3 is the mechanism**;
commit trailers are an optional durable breadcrumb and the commit-time enforcement pass was
dropped. I am taking the `ci.yml` trailer @paired-infrastructure offered, because its reason clause
is where the "closes CI only" scoping lives, and will take @protocol's if they want one on record.
Not touched, and deliberately so: `docs/decisions/adr-0035-story-runner.md`
(§12 is satisfied by this task but the ADR's Implementation Status is story-close's job, not task 1's),
`.claude/skills/devloop/SKILL.md` (story task 4 explicitly forbids touching it).

| Consumer | Covered |

| CI (`ci.yml` → `layer-all.sh`) | yes — the 3-line build |

| story runner (`run-story.sh` → `layer${n}.sh`) | yes — the preflight assertion |

| plain local devloop (`layer-all.sh` / `layer3.sh` standalone) | **no** |

---

## Planning

**A note on citations in this document** (@semantic-guard, @code-reviewer, and the Lead).
`run-story.sh` grew from 549 lines at `9807f0b` to ~1000 during this devloop, and three reviewers
read three different correct line numbers for the same statement within one thread. Line numbers
into this file have therefore been **stripped rather than refreshed** — refreshing re-arms the trap
one edit later. Stable anchors are used instead: enclosing function names, or a `grep -n` the reader
can run. This is the same correspondence failure as the comment overclaims below (a citation asserts
a fact about a file state that is not established when the reader reads it), so it is fixed the same
way rather than by being more careful. Citations into files this change does **not** edit are kept.

### The principle this gate found (P0)

**An assertion may only claim what is actually established at the point it is made.**

One sentence, five surfaces, five instances found in a single Gate 1 — which is why it is named
here rather than fixed five times:

| Surface | Instance | Found by |
|---|---|---|
| **Messages** | the rc-2 recovery text; `RETRYABLE-UNCLASSIFIED`; the stale-binary refusal; the terminal `--stop-after` check | @operations, @code-reviewer, @observability |
| **Justifications** | my classification row claimed "no gate authority" while §R-5 has preflight refuse the run on that verb (CQ-8) | @code-reviewer |
| **Lane routing** | M4 — the runner detected correctly and routed wrongly, six times | @operations |
| **Class labels** | M2/M4 both under-described the dirty-tree check, in the direction that erodes a planned fix | @semantic-guard |
| **Checks** | `[ -n "$out" ]` may only be *commented* as a contract check if the contract is actually written | @code-reviewer |

The unifying failure is not being wrong — it is a claim whose warrant can disappear **with nothing
failing**. That is why the remedy is never "add a comment": comments are the surface this rule
polices, not the tool it polices with. Where the warrant is load-bearing it becomes a structural
check in code (CQ-9a's three explicit checks, replacing an ordering dependency a comment was
supposed to protect); where it is genuine but supporting, the comment states it and says so.

Recorded again under §Lessons Learned. @code-reviewer has asked @semantic-guard whether it belongs
in `scripts/guards/semantic/checks.md`, which is the right home if it survives contact — their file,
their row, not routed through this task.

### Mechanism restatement (wider class than the task names)

Three restatements. Two widen the work and are folded in; one is wider than this task and is
surfaced rather than taken.

**M1 — "one validated root seam subsumes five."** ADR-0035 §12 names five seams (repo-root,
`DT_STORY`, layer-script dir, preflight bypass, audit-glob root). The mechanism is that
`run-story.sh` derives `REPO_ROOT` from `BASH_SOURCE` and `cd`s into it, and **every**
filesystem path it touches afterwards is CWD-relative: `scripts/workflow/preflight-story.sh:78`,
`scripts/layer${n}.sh:477`, `scripts/layer7.sh:484`, `./scripts/layer-all.sh:531`,
`scripts/lang/*/audit.sh:219`, `scripts/lang/<lang>/audit-remediation.md:242`,
`docs/user-stories/*:45`, `docs/devloop-outputs/*:262,427`, `.devloop-escalation.json:326,460`,
`audit-suppressions.toml:510`. The single exception is `target/release/dt-story:53` — a *built
artifact* that must stay pointed at the real binary while the tree moves. So the seam count is
2 because of M1, not by preference, and this argument goes in the seam block's comment
(@code-reviewer #2, @paired-infrastructure #3) so the next reader doesn't add a third.

*Same-owner siblings M1 exposes:* `scripts/workflow/preflight-story.sh:25-26` uses the identical
idiom and has zero tests. **Not taken** — task-sized, not trivial: preflight's untested surface is
a container-boundary gate and a live 420s substrate probe, which need their own harness and their
own containment argument (it writes `$HOME/.claude/settings.json`). Recorded here rather than
silently skipped.


**CQ-3 — the code must not contradict its own "two seams" comment.** Step 5 below makes the runner
*validate* `DEVLOOP_TMP` under the active sentinel, so a reader finds three sentinel-governed env
vars under a comment asserting two, and resolves the contradiction by deleting whichever they trust
less. The comment says it explicitly: `DEVLOOP_TMP` is a **pre-existing** redirect (the `RUN_DIR` assignment, the `RUN_BASE` assignment),
not a new seam — the sentinel adds a *constraint* on it, not a *redirect* through it.

**M2 — "a conditional comparing a command substitution against a known value fails OPEN."**
This is R-2 defect 4's actual mechanism, and it has **four** instances, not one:

| Site | Today | Fail-open consequence |
|------|-------|----------------------|
| `devloop-stop-hook.sh`'s HEAD comparison | `head="$(… \|\| true)"`, then `[ -z "$head" ] \|\| [ "$head" != "$START" ]` → `exit 0` | git error ⇒ **allow the stop**; headless Lead exits with no commit and no escalation |
| the post-devloop HEAD comparison | `[ "$(git rev-parse HEAD)" = "$head_before" ] && escalate … devloop-no-commit` | git error ⇒ `"" != $head_before` ⇒ **no escalation**; runner gates and marks complete a task that may not have committed |
| `persist_resume_pointer`'s HEAD comparison | `[ "$(git rev-parse HEAD)" != "$head_before" ] && return 0` | git error ⇒ resume pointer silently not persisted |
| the dirty-tree check | `! git diff --quiet \|\| ! git diff --cached --quiet` | `git diff` is tri-state (0 clean / 1 dirty / >1 **error**); an error reports as "dirty tree", exiting 2 with a message naming the wrong cause |

**The `git-error` lane carries a `log` pointer too** (@observability O-16). Without it the record is
`detail` + `canary: null` + `log: null` — a bare classification with nothing to read next, which is
the condition O-2 exists to close on the gate lane, and **worse here**: on the gate lane the
operator at least knows to read `$gatelog`; on this one there is no artifact at all, because the
failing `git` command's stderr currently goes nowhere. The operationally distinct cases all live in
that stderr and are indistinguishable without it — `not a git repository`, an `index.lock` held by
another process, a corrupt object database, a `safe.directory` ownership refusal: four different
operator actions behind one lane value. So each site that routes to `git-error` captures the failing
invocation's stderr to `$RUN_DIR/task-<id>.git-error.<ts>.log` and passes it as the 4th argument.
Referenced by path, never inlined (O-9 holds), and `$RUN_DIR` is under `DEVLOOP_TMP`, outside the
work tree, so `git add` and the the manifest-bump amend amend cannot sweep it. It pays best at the dirty-tree check, where the
current failure "exits 2 with a message naming the wrong cause" and the stderr is precisely what
names the right one. Scoped to the sites that route to the lane — **not** `persist_resume_pointer`'s HEAD read, which writes no
incident record at all (see the exception below).

**the dirty-tree check shares the consequence, not the mechanism** (@semantic-guard, running my own audit question
back at me). M2's label is "a conditional comparing a command substitution against a known value";
three of the four are exactly that, but the dirty-tree check is `if ! git diff --quiet || ! git diff --cached
--quiet` — no substitution, no comparison, no known value, just a direct exit-status test on a
tri-state command. It fails open like its siblings and its **remedy differs in kind**: the three
string-compare sites are fixed by checking the substitution's rc, the dirty-tree check by splitting rc>1 from
rc==1. So it was under-described by **both** frames in the same direction, and the two
under-descriptions compound — M2 implies "add an rc check on the substitution" (there is none) and
M4 implied "fix the message" (there is a real detection bug). Either label taken at face value
ships something that is not the fix, which is what makes its explicit paragraph load-bearing rather
than belt-and-braces.

**Audit rule, generalised** (mine, sharpened by @semantic-guard): a class label whose remedy is
weaker than the one already planned for a member is a reclassification that erodes a fix — and
**the erosion risk concentrates on whichever member the label fits worst**, so the check is cheap:
for each restatement, name the member matching the label least well and confirm its planned remedy
survives the label. the dirty-tree check is that member for M2 and M4 both. M1 and M3 are clean by construction —
M1's roster is a list of paths that either are or aren't CWD-relative, M3's is a binary
file-has-no-invocation-site predicate, so neither label can under-describe a member.

All four are in scope (same owner, same mechanism — protocol's sibling rule). the pre-devloop `head_before` read
`head_before="$(git rev-parse HEAD)"` is a fifth, different shape: it aborts under `set -e` with
exit 1 and no lane, which the runner's exit-code header documents as "task escalated". Also fixed.

**Where the corrected branches exit — named per site** (@operations 2), because a git failure
landing on `escalate` would violate R-4 inside the diff that fixes R-4. A `git rev-parse` /
`git diff` failure is unambiguously environment, so **all five route to the operator lane**:
`record_infra_incident "$id" git-error …` + `exit 2`, manifest untouched. the dirty-tree check is **detection *and*
message**, and the summary must not say otherwise (@semantic-guard). Its lane is already right — it
exits 2 today — but the fix is *not* message-only: `git diff` is tri-state (0 clean / 1 dirty /
**>1 error**) and today an error is read as "dirty tree", so the fail-open conditional must be
split (rc>1 distinguished from rc==1) *and* the message corrected to name the actual cause. Calling
this "message-only" would let a reader working from the summary rather than the M2 table drop the
tri-state distinction — the fail-open surviving inside the diff that fixes the fail-open class,
which is M4 reproduced one level up. `devloop-stop-hook.sh` has no lane of its own: it fails **closed** (blocks the stop) and
lets the runner's own checks classify.

**`persist_resume_pointer`'s HEAD read is the deliberate exception, and the comment says why** (@paired-infrastructure #5 —
accepted; this would have been a real regression). `persist_resume_pointer` is called at the auth-expired arm and
the infra arm *inside* the `auth-expired` and `infra` arms, **before** those arms write their own incident
record. Exiting from in there on a git failure would kill the run with a generic `git-error` record
and destroy the precise classification the canary just paid for — `auth-expired`, with its
`devloop.sh --refresh-creds` recovery — on the path where the record is the only durable trace,
which is the exact failure `record_infra_incident`'s comment at `record_infra_incident`'s header comment exists to prevent. The
other four sites are genuine lane decisions; this one is best-effort bookkeeping on an exit path.
So it `slogerr`s loudly (naming that the pointer could not be persisted, so a rerun starts fresh)
and **returns**, letting the caller's lane finish and write its own record. Same fail-loud, no lane
hijack — with a comment saying why this site is deliberately the odd one out, or someone will "fix
the inconsistency".

**The stop-hook flip buys a 120-turn loop on a *persistent* git failure** (@operations 3) —
`:36` caps blocks at `DEVLOOP_STOP_BLOCK_MAX:-120`, and the block reason at `:41` tells the Lead
to commit, which on an unreadable `.git` / full disk / locked index is precisely what it cannot do.
Fail-closed is still right for the transient case, so the git-error branch emits a **distinct block
reason** naming the achievable recovery — write `.devloop-escalation.json` — instead of reusing the
"you haven't committed" text. That turns 120 headless Opus turns producing nothing into one or two.
Taking the string-and-conditional half; not lowering the cap for that branch, since the valve's
existing behaviour (open, then let `devloop-no-commit` fire) stays correct and a second cap is a
second thing to keep in sync.

**M4 — "correct detection, wrong or unnamed lane."** @operations' framing, and on reflection it is
the unifying mechanism of this whole task rather than a side observation. The runner was already
detecting nearly everything correctly and *routing or naming* it wrongly:

| Site | Detection | Lane |
|---|---|---|
| the gate-rc block | correct | operator and implementer collapsed into one — **R-4, the headline** |
| the timeout check | correct | comment at the TASK_TIMEOUT comment says infra, code escalates |
| session-limit exhaustion | correct — the canary *positively classified* it | falls through to `escalate` |
| the dirty-tree check dirty-tree | correct (exit 2 is the right lane) | names the wrong cause on a git error |
| `preflight-story.sh` unguarded command | correct | aborts rc 1 under `set -e`; `:19-20` calls that "task escalated" |
| `_dt_guard_wrapper.sh:73-77` | correct | declares `FAIL`, so a wiring fault arrives as rc 1 — the one instance **not** mine |

**The roster splits, and the label must not flatten it** (@semantic-guard): the dirty-tree check and the
preflight unguarded command are **detection *and* lane** defects; the gate-rc block, the timeout check and
session-limit exhaustion are **lane-only** — detection there is already correct and only the routing
is wrong. A class label whose remedy is weaker than the one already planned for a member is a
reclassification that erodes a fix, which is the same guard applied to R-4's own summary.

Five of six close in this diff. Stating it as a class is the point: a reader who sees six separate
fixes adds a seventh instance without noticing — the `_dt_guard_wrapper.sh` row is exactly that, a
lane defect nobody recognised as one until this week — whereas a reader who sees a named class asks
"which lane, and does the message say so?" of the next branch they write. It is also the cheapest
available form of the lane-recovery documentation the story records as debt. So the R-4 comment
carries one clause saying **R-4 is the named instance of this class, not the whole of it**, which
sits naturally beside the two residuals already being stated (misdeclared `FAIL`, and
prose-authored escalations) — all three are the same question, *who decides the lane*, asked of a
different actor.

**M3 — "a control that is registered but never invoked."** 10 of 16 `scripts/**/*.test.sh` have
no invocation site, so they pass by never executing. The task's instruction ("do not add an 11th")
treats the instance; the mechanism wants a layer-3 guard asserting every `scripts/**/*.test.sh`
has one. **Surfaced, not taken** — it reds immediately against 10 pre-existing files, each needing
its own triage (some are plausibly superseded rather than forgotten), so it is genuinely task-sized
and would block this task on unrelated work. Story already records it under Deferred → "Unrun and
vacuous self-tests"; this restatement adds the mechanism and the cheap closer.

### Seam block — exact ordering (`run-story.sh`, immediately after `set -euo pipefail`, above the current `:23` `REPO_ROOT=`)

Merged ordering per @security #9, @test, @semantic-guard #1, @operations F15. Nothing below runs
after a `cd` or after any `git` invocation:

**Dialect first (@code-reviewer CQ-1, verified — `grep -c 'REASON=\|STATUS=' ` is `0` for *both*
`run-story.sh` and `preflight-story.sh`).** My draft introduced `REASON=` tokens into two scripts
that don't speak that language. `STATUS=`/`REASON=` is the **layer-script** contract, parsed by
`_common.sh`'s `run_and_emit`/`tee_collect_statuses` and keyed by `docs/runbooks/devloop-validation.md`
§6.3 for triage — so using it here would either imply the runner emits STATUS lines or make a
runbook grep return a row that doesn't exist. Every token below is therefore emitted in the host
script's own shape: `STORY_RUN: <TOKEN> …` in the runner (`:96,299,356,437,504,518` precedent) and
`PREFLIGHT: FAIL …` via `fail()` at `preflight-story.sh:29`. The per-lane distinguishing token
stays; only the envelope changes. Token names below are written bare for readability.

**Shape (@code-reviewer CQ-2).** The prologue lands as named functions, not a ~60-line straight-line
block — the file factors everything else (`slog`, `canary_probe`, `canary_classify`, `audit_scan`,
`audit_owner_for`, `persist_resume_pointer`, `report_task_cost`, `escalate`,
`record_infra_incident`), and this would otherwise be its largest inline block. Names mirror the
site whose idiom is being ported: `__test_sentinel_active`, `__any_override_present`, plus
`__assert_seam_containment`. ADR-0035 §12 rules out extracting the four *existing* control-flow
functions under this task; it says nothing about landing new logic unfactored.

1. **CI presence-rejection.** `GITHUB_ACTIONS` set **and** (`STORY_REPO_ROOT` or `DT_STORY` present)
   → `exit 2`, token `story-seam-set-in-ci`. Presence-based, independent of `DEVLOOP_TEST`
   (the `LAYER_SCRIPT_DIR` shape at `_common.sh:215-219`, not the `DEVLOOP_HELPER_SOCKET` shape).
   Justification for taking this against the story's Deferred entry: that entry's premise —
   "CI invokes neither workflow script" — is **retired by this very commit**, since wiring the suite
   into `layer3.sh` puts `run-story.sh` on `ci.yml` → `layer-all.sh` → `layer3.sh`. And
   `_common.sh:200-214`'s own criterion for withholding a CI clause is "CI-INERT BY CONSTRUCTION …
   `assert_no_ci_sentinel_leak` hard-fails the pipeline on a leak" — which is derived from sourcing
   `_common.sh`, and `run-story.sh` sources no `_common.sh`. Premise absent ⇒ clause required.
   **The clause's own comment must say how it coexists with the suite** (@paired-infrastructure #6).
   In CI the suite exercises the seams with this clause satisfied *by absence*, because `env -i`
   scrubs `GITHUB_ACTIONS` from the child — the `layer-all.test.sh:73-77` pattern. That is correct,
   but unstated a reader concludes the runner can never run with seams in CI when in fact it does on
   every push. The comment says what the clause defends against: **ambient leakage into a process
   that inherited the CI environment, not a deliberate scrub.**
2. **Fail-loud override-without-sentinel.** `__any_override_present && ! __test_sentinel_active`
   → `exit 2`, token `story-seam-override-without-test-sentinel`. `__test_sentinel_active`
   is `[[ "${DEVLOOP_TEST:-}" == "1" ]]` — exact match, never `-n`, and the
   "DO NOT change the gating to a truthy `-n` test" header comment is carried across verbatim.
   **Call site named** (@semantic-guard #2): this line, unconditionally, at step 2 of the block.
3. **Resolve `REPO_ROOT`** — real `BASH_SOURCE` form, or seam.
4. **Containment assertion when the seam is active** — both halves, hard `exit 2`:
   - `git -C "$root" rev-parse --show-toplevel` resolves to `$root` *itself*, not an ancestor
     (token `story-repo-root-not-own-toplevel`; also covers not-a-git-repo);
   - and it **differs from the real `BASH_SOURCE`-derived root** (token `story-repo-root-is-real-repo`) — the load-bearing half, the one a no-op redirect fails.
   Comparison is **`-ef` (device+inode), never string equality**, on both halves —
   @paired-infrastructure measured all four shapes on git 2.39.5: a fixture reached via a
   symlinked path (the `mktemp -d` under a symlinked `/tmp` case) resolves `--show-toplevel` to
   the *real* path, so `-ef` says equal while `=` says not-equal. With `=` the suite reds
   spuriously on such a host and the obvious "fix" is to weaken the check. Ordering constraint
   that makes the second half work at all: the real root must be captured **before** the seam is
   applied, or it compares the fixture against itself and always passes.

   **A second ordering dependency, named in a comment** (@semantic-guard F4, measured):
   `[ /existing -ef /missing ]` is **false**, so the "differs from the real root" half evaluates
   TRUE when either operand does not exist — fail-open. It is safe today only because half 1
   (`--show-toplevel` resolves to `$root`) rejects a nonexistent root first. A control that is
   fail-open-but-for-ordering must say so, or a future edit swapping the halves or short-circuiting
   reopens it silently.
5. **`DEVLOOP_TMP` containment** (@paired-infrastructure #1, @operations F14) — **verified live**:
   `DEVLOOP_TMP` is unset in this container and the run driving this devloop holds
   `/tmp/devloop/story-runner/.run-in-flight` (pid 8520). `run-story.sh` installs
   `trap 'rm -f "$INFLIGHT"' EXIT`, so one un-redirected test invocation would delete the live
   run's marker, an attach would then update the CLI, and `:350-354` would kill the story with
   `SUBSTRATE-CHANGED`. Under an active sentinel the runner therefore **requires** `DEVLOOP_TMP`
   to be set and to resolve outside the default `/tmp/devloop` (token
   `story-run-dir-not-redirected`). R-1's own reasoning — "setting the redirect is not the same as
   being redirected" — applied one directory over. @observability adds that this is a
   **record-destruction** finding as much as a containment one: a test invocation sharing
   `RUN_BASE` with a live run doesn't only delete `.run-in-flight`, it writes `task-N.*` artifacts
   into the live run's evidence directory, interleaving fixture canary/incident/cost records with
   the real story's.

   Three implementation constraints. Reject "equal to **or under**" the default, on
   **canonicalized** paths — a naive string-prefix test false-positives `/tmp/devloop-scratch`,
   which is a legitimate value, and a check that rejects legitimate values gets weakened by the
   next person who trips on it (@security). Also reject a `DEVLOOP_TMP` resolving **inside the real
   repo root**, or the run dir lands in the operator's tree as untracked files — the porcelain
   capture would catch that after the fact, but the post-hoc proof exists to catch what we failed
   to anticipate, not to substitute for what we did (@security). And use step 4's `-ef` discipline
   rather than string comparison, for the identical symlinked-`/tmp` reason, failing **open** being
   the direction that loses records (@observability O-14) — with the absent-default case decided
   explicitly and **fail closed** — refuse rather than inherit `-ef`'s false (@semantic-guard: two
   independent controls resting on the same fail-open primitive deserve one shared comment, not two
   local ones): `-ef` needs both paths to exist, and a non-existent
   `/tmp/devloop` means no collision is possible, so it passes.

   **The denylist-of-one-path form has a hole, so the guard states the property positively**
   (@paired-infrastructure #4). "Not the default `/tmp/devloop`" catches today's hazard but not a
   harness that passes the *ambient* `DEVLOOP_TMP` through — which is precisely what a future case
   does when someone writes `DEVLOOP_TMP="$DEVLOOP_TMP"` to "make it work", and in a container where
   `devloop.sh` set `DEVLOOP_TMP=/tmp/devloop-<slug>` that passes the literal check and clobbers a
   real run dir. So under an active sentinel the runner additionally refuses if
   `"$DEVLOOP_TMP/story-runner/.run-in-flight"` **already exists** — a positive statement of the
   property actually wanted ("no other runner owns this run dir"), covering both the default and any
   ambient value. Deliberately sentinel-gated: refusing unconditionally on a pre-existing marker
   would block recovery after a `SIGKILL`ed run leaves a stale one, since the the in-flight-marker trap trap cannot
   fire on a kill. Same reasoning as the containment check itself — hold for the class, not the
   instance.

   **Fourth splice site, created by the defect-5 fix** (@semantic-guard F1, blocking — accepted).
   Post-fix the only variable content inside the quoted `/devloop "…"` argument is the prompt-file
   path, and that path is *not* as constrained as "runner-generated" implies: the prompt-file write composes it
   from `RUN_DIR`, which the `RUN_DIR` assignment builds from `DEVLOOP_TMP` (an env var with no character constraint
   anywhere) and `basename "${STORY_FILE%.md}"` — which on the `-f "$ARG"` branch at `:40-41` is
   operator-supplied argv, so `run-story.sh '/work/docs/user-stories/my"story.md'` puts a `"` into
   it. It inherits the entire defect: a `"` truncates the instruction, a newline splits it off the
   line. Lower likelihood and operator-supplied rather than untrusted — but a fix whose stated
   guarantee is *"no unconstrained byte reaches the quoted argument"* must not ship with one
   unconstrained byte source reaching it, because the next reader will rely on that guarantee.
   Closed here, in this step, since it already resolves and validates `DEVLOOP_TMP`: the **resolved**
   **resolved** `RUN_DIR` must match `^[A-Za-z0-9._/-]+$`, fail loud with its own token — resolved
   meaning post-`${DEVLOOP_TMP:-/tmp/devloop}` default expansion and post-`basename`, not
   `DEVLOOP_TMP` alone: the `basename "${STORY_FILE%.md}"` half is the argv-reachable one and is why
   this is a finding rather than an env-hygiene note. One check covering both
   sources, before any use, sitting with the other refusals rather than as a lone guard at the prompt-file write.

   The runner's predicate and the harness's ("refuse if the resolved run dir is under the real
   `/tmp/devloop/story-runner`") are deliberately two predicates for one property — loci 1 and 3 of
   the three-loci argument, where the harness must not borrow from the artifact under test. The
   runner's is the **stricter** of the two, so the harness can never be the only thing standing
   between a test run and the live run's evidence dir.
6. **`cd "$REPO_ROOT"`.**

Once the seam is honoured, one `slog` line names the active seams, beside the substrate-bound line's
`substrate bound to claude …` (@observability O-8/O-15) — cheap insurance that a seam-active
artifact tree can never be read as production evidence. It sits **after** step 4, so a run dying in
the containment check exits without announcing the seam; that is deliberate, since those refusals
carry their own distinct tokens and a line announcing a seam the runner just refused would be worse
than none.

`DT_STORY` seam (@code-reviewer #3, @operations F17, @paired-infrastructure #4): the incoming
value is captured into a distinct `__seam_dt_story` **before** the `DT_STORY` assignment, and the `DT_STORY` assignment stays an
unconditional assignment on the non-seam path. Never `${DT_STORY:-default}` — that form would
silently honour an ambient env var with no sentinel and no fail-loud, which is exactly the bypass
`audit-suppressions-check.sh:34-35` warns against, and it would let an inherited variable choose
the binary that executes `complete` and `escalate` against the real manifest. That is a *stronger*
justification for the fail-loud half than the audit script had, and it goes in the comment.

### R-4 — the rc mapping, written out

SSoT is `scripts/lang/_common.sh:305-321` (`status_to_exit_code`), not `layer7.test.sh:8-16`
(@operations A1); the latter is cited as the worked example. Mapping is `FAIL→1`,
`PRECONDITION_FAILURE | FAIL-MISSING-VERB | UNKNOWN → 2`, plus a **fail-closed `*` backstop → 2**.

| `gate_rc` | Lane | Manifest | Exit |
|-----------|------|----------|------|
| `0` | proceed | bumped to `completed` | — |
| `1` | implementer — `escalate "$id" pipeline-red "$gatelog"` | task → `escalated` | 1 |
| **any other non-zero** (2, 124, 126, 127, 137, 130, …) | operator — `record_infra_incident` + `exit 2` | **untouched** | 2 |

Not `rc == 2` specifically (@security #6, @operations A2): 127 (layer script missing), 137
(OOM-killed cargo), 124 (a layer killed by a wrapper timeout) are all environment, and an
*unknown* rc must default to not blaming the implementer — mirroring `_common.sh:318-321`'s own
fail-closed backstop so the runner and the pipeline fail closed in the same direction. Scoped to
**any** layer (1-6 and 7), not special-cased to layer 7 (@code-reviewer #5,
@paired-infrastructure #6a, @observability O-3), and the failing layer number is captured and
recorded.

Driven through a **stub `layerN.sh` that exits 2**, never by calling a classifier directly
(@semantic-guard #6), so nothing downstream can normalise the rc without a test noticing.

**The rc-2 lane's state is unlike the other two manifest-untouched lanes** (@observability O-4,
@code-reviewer #6, @operations B4, @paired-infrastructure #6b). By the time the gate runs, the post-devloop HEAD comparison
has already proven HEAD moved — the work **is committed**. So the exit leaves: commit on the
branch, manifest `pending`, and a rerun that would re-run the task on top of its own commit.
Exact console text (ADR-0035:189 puts "recovery paths' comments match the messages they print"
in this task's scope, so this is written to be checked against the code):

```
STORY_RUN: PIPELINE-PRECONDITION task=<id> layer=<n> rc=<rc> — operator-class gate failure
  (see scripts/lang/_common.sh status_to_exit_code). This task's work IS COMMITTED at <sha>;
  the manifest is UNTOUCHED, so a rerun starts a fresh devloop for a task already on the branch.
  Recover by: (1) fix the environment, then rerun scripts/layer<n>.sh by hand and, if green,
  `dt-story complete <story> <id>` — or (2) fix the environment and rerun the runner, accepting
  that task <id> is re-implemented on top of its own commit. Gate log: <gatelog>
```

On that path: `rm -f "$slug_file"` (@operations B5 — the devloop finished and committed, so a
stale pointer from an earlier attempt would send the rerun into `--continue` against a completed
devloop) and **no** `persist_resume_pointer` (it returns early anyway once HEAD has moved).
I am taking @operations' (1)-or-(2) wording over @paired-infrastructure #6b's "write the slug so
the rerun resumes": resuming into a *completed* devloop is the failure B5 warns about, and forging
`complete` would violate ADR-0035 §3. The ambiguity is recorded, not papered over.

**`record_infra_incident` masks its own write failure, and it is the only evidence these lanes
produce** (@operations 1). `record_infra_incident`'s jq write ends `>"$rec" 2>/dev/null || true` and its announcement line announces the
path regardless — but the redirect creates the file *before* `jq` runs, so a `jq` failure leaves a
**zero-byte record** whose path is printed as though it were evidence. For auth-expired, infra and
the three new lanes the manifest is untouched and nothing lands in `docs/devloop-outputs/`, so this
record is the entire durable trace: a silently-empty one recreates the exact Run #1 failure the
function's own comment (`record_infra_incident`'s header comment) says it exists to fix, and is *worse* than no record, because
the operator's first move at 3am is spent on a file that was never written. This is a masked
failure inside the anti-masking mechanism, so `|| true` goes: after writing, assert non-empty and
`jq -e . <"$rec"` parses; on failure `slogerr` loudly with the reason, lane exit unchanged.

`record_infra_incident` gains a 4th `log` argument (@observability O-2) — and at four positionals a bash call site stops being self-explanatory, so the function's existing header (`:144-151`) gains an `# Args:` line in `_test_helpers.sh`'s convention, covering the new `layer` value too (@code-reviewer CQ-6) — `escalate()` already
carries one via `dt-story escalate --log`, and for this lane the canary is `null` and `$gatelog`
is the only evidence. `detail` is built from fixed strings + the layer number + paths only; no
gate output, model prose or CLI stderr is interpolated into the JSON (@observability O-9,
@semantic-guard #5). The `tail -n 50 "$gatelog"` console dump stays on both lanes.

**The rc-1 lane carries the failing layer too** (@observability O-11). O-3 puts the layer number
in the operator lane's record and console line — but by the overclaiming analysis below, rc 1 is
the *more common* gate failure this repo produces, so instrumenting only rc 2 would leave the
commoner lane recording a bare `reason=pipeline-red` and force a reader of
`task-N.runner-escalation.*.json` to grep `$gatelog` to learn where it died. The escalate reason
carries the layer (`pipeline-red-layer<n>`), keeping the two lanes symmetrically readable —
which matters most precisely when the split routes an rc to the wrong one.

**Lane vocabulary is enumerated in one place** (@observability O-12). This change takes
`record_infra_incident`'s `lane` field from 2 values to 5 — `auth-expired`, `infra`,
`session-limit-exhausted`, `devloop-timeout`, `pipeline-precondition`, plus `git-error` from M2,
so 6. Nothing declares its domain today, so the next caller invents a seventh spelling and nobody
notices. A comment above `record_infra_incident` names the closed set with one clause each for
what the lane means and what the operator does about it — the precedent is `canary_classify:185`,
which already does this for `healthy | session-limit | auth-expired | infra` in the same file. The
comment also notes the `devloop-timeout` collision (it exists as a pre-change `escalate` reason and
as a post-change incident `lane`; different record types and filenames, so recoverable).

**Console text: consecutive `slogerr` calls, not one call with embedded newlines**
(@observability O-13). `slog`/`slogerr` are `printf '[%s] %s\n'`, and the `slog` definitions states the property
outright — "All STORY_RUN lines route through these so elapsed time between events is readable in
a captured run log". One call with newlines timestamps only line 1. The four-line
`PIPELINE-PRECONDITION` text above is emitted as consecutive `slogerr` calls so each carries its
own timestamp, rather than as one long single line like `:394`/`:400`/`:518` — the recovery text
is genuinely multi-clause and a single line that long is unreadable in a captured log.

**The R-4 comment must not overclaim** (@operations, verified). The split routes the pipeline's
*declared* operator class. A layer that **misdeclares** an environment failure as `FAIL` still
reaches the implementer lane — and that is the most likely gate failure this repo produces:
`validate-story-manifest.sh:16` and `_dt_guard_wrapper.sh:73-77` both `exit 1` on a missing guard
binary, `run_and_emit` (`_common.sh:138-147`) collapses any non-zero to `STATUS=FAIL`, and
`_common.sh:309-314` maps `FAIL → 1`. So a missing build artifact — a wiring fault by
`_common.sh:251-253`'s own definition of the operator lane — arrives at `:489` as rc **1** and gets
blamed on the task. Fixing that enum is structural and not mine (the `run_and_emit` collapse at
`layer3.sh:29` flattens a guard-level `PRECONDITION_FAILURE` back to `FAIL` anyway);
@paired-infrastructure is filing it. What *is* mine is the preflight guard-binary assertion above,
which converts the likeliest instance into a refusal at start on the correct lane, and a comment
that says plainly what the split does and does not cover, pointing at the follow-up. A comment
reading as though R-4 closed the class is exactly the wrong-comment failure ADR-0035:189 asks us
to stop shipping.

**Measured during this devloop's own Gate 1, not projected**: the Lead's
`validate-cross-boundary-classification.sh` run first exited **1** with
`REASON=dt-guard-binary-missing` because the binary was not built. That is this exact residual
firing in the wild, on the first invocation, inside the devloop whose plan describes it.

The same comment carries a **second residual** (@operations, offered explicitly as take-it-or-leave-it
— taken, because it is the same shape and one sentence). Work the chain: git breaks → the stop hook
now fails closed and tells the Lead to write `.devloop-escalation.json` → the Lead does →
`:460-462` moves it and calls `escalate "$id" devloop-escalated` → the manifest marks the task
**escalated**, implementer-class, for an environment failure. I am **not** fixing that, and
@operations would reject a fix: distinguishing "escalated because the environment broke" from
"escalated because the task is genuinely blocked" requires reading model-authored prose, and
ADR-0035's "no model in the control flow" is the property that makes this runner trustworthy —
trading it away to close this hole is a bad deal, and the evidence survives either way (the Lead's
reason is in the escalation record, and a human is in the loop by construction once a task is
escalated). So the comment's honest formulation is one clause covering both: **the split routes
what the pipeline *declares* and what the runner itself *observes*; it cannot route what a model
asserts in prose.**

### R-4's two other instances in this file — both taken

Same mechanism, same owner, same changeset (@security #7, @test #6, @operations C6/C7):

- **Session-limit retry exhaustion.** Once `limit_waits` reaches `SESSION_LIMIT_RETRIES` the
  `session-limit)` arm falls out, `break`s at the inner loop's `break`, and the session-error escalation runs
  `escalate "$id" devloop-session-error` — a quota outage the canary *positively classified*,
  recorded in the manifest against the task. → `record_infra_incident "$id" session-limit-exhausted`
  + `exit 2`, recovery line naming the reset window. This is a **precondition for the §6
  inversion below**, not an optional extra: inverting the default toward the retryable lane while
  that lane terminates in `escalate` converts *more* outages into recorded implementer bugs.
- **the timeout check contradicts its own comment.** the TASK_TIMEOUT comment says "timeout is an infra escalation,
  not a task verdict"; the code escalates. Resolving toward the comment: rc 124 →
  `record_infra_incident "$id" devloop-timeout` + `exit 2`. A wedged 4h headless session is a
  substrate condition, and `TASK_TIMEOUT` is operator-configured.

### R-2 defect 1 + R-3 — the canary truth table

The "typo in prose matching" is concrete and gets a literal fixture (@test #5): `canary_classify`'s prose grep
greps `.result` case-sensitively for `session limit`, while the session-limit lane's `resets` grep — same file, same field —
already encodes the real message shape by grepping `resets [0-9]\+:[0-9]\+[ap]m`. A message like
`5-hour limit reached ∙ resets 3:00pm` matches `:411` and **not** `canary_classify`'s prose grep, falls past the auth grep,
and returns `infra` — so the story **stops instead of sleeping**, exactly ADR-0035 §6's predicted
consequence. The fixture is that literal string, so the test pins the message shape rather than the
patch.

New table (@code-reviewer #7):

| Canary output | Class |
|---|---|
| not parseable as a JSON object (non-JSON, empty, truncated) | `infra` |
| `is_error:false` + non-empty `.result` | `healthy` |
| `api_error_status` ∈ {401, 403} | `auth-expired` |
| text matches `authenticat\|oauth\|credential\|api key\|unauthorized\|log in`, case-**in**sensitive | `auth-expired` |
| `api_error_status == 429` | `session-limit` |
| **anything else that parsed as an error object** | `session-limit` (ADR-0035 §6 inversion) |

`infra` narrows to "the canary produced nothing parseable" — which is the honest meaning: if the
API returned a well-formed error object, the API is *reachable*, and the retryable lane is right.
Sleeping unnecessarily costs a retry slot; dying unnecessarily costs the story.

**The narrowing is sound only *because* defect 2 is fixed, and that ordering goes in a comment at
the classifier** (@test). Pre-fix, `canary_probe`'s `2>&1` merged stderr into the parsed file, so
one warning line made well-formed output unparseable — meaning the exact input the narrowed `infra`
lane now reads as "API unreachable" was, until this commit, routinely produced by a *healthy* API.
Left as tribal knowledge, someone later "simplifies" the redirect back and silently re-widens the
lane. A case pins that stderr contamination no longer reaches the classifier, since that is the
regression which would resurrect it.

Two truth-table cases added on @test's prompt: a **well-formed error object with a non-quota,
non-auth message** driven end to end (→ `session-limit` → retries exhausted → infra lane, exit 2,
manifest untouched), which is where the inversion's cost actually lands; and an **empty canary
file** (probe killed by `timeout 120`, `jq` fails on zero bytes → `infra`) — the case most likely
to be missed precisely because it doesn't look like a fixture.

**The inversion must not print a message that isn't true** (@operations 4). The inverted-default
path reaches `:437`'s `STORY_RUN: SESSION-LIMIT task=… wait=…`, asserting a quota condition that
was never observed — and since the `resets` grep finds nothing there, the operator sees an
unexplained 1-hour sleep, twice, in a run they came back to expecting "done". So the console token
on that path is distinct: `RETRYABLE-UNCLASSIFIED`, with the line saying this is ADR-0035 §6's
default-toward-retryable rather than an observed 429, and naming the sleep as a default rather than
an advertised reset. Same discipline this diff applies to the R-4 comment and to the TASK_TIMEOUT comment vs its code — the inversion should not introduce a fresh instance of the wrong-message failure while
fixing two.

**present-with-null vs absent** (@code-reviewer #7, ADR-0035:190): they classify **identically**,
deliberately — both mean "no status field to classify on" — and the code says so in a comment
rather than leaving `jq`'s `//` to collapse them by accident. All three shapes get fixtures
(429 / null / absent) plus a fourth with a non-429 numeric status, so the field is provably *read*
and the null≡absent equivalence is **pinned** rather than assumed: if anyone later hardens that
check, the divergence reds a test.

### R-2 defect 5 — out of band, not escaping

**Decisive fact** (@paired-infrastructure, verified): `/devloop`'s arguments are **not mechanically
parsed**. `.claude/skills/devloop/SKILL.md` frontmatter (`:1-4`) has no `$ARGUMENTS` and no
`argument-hint`; the `## Arguments` block at `:18-28` is prose a model reads. So R-2.5 is a
*model-facing ambiguity*, not a shell or CLI parse error.

Three candidate fixes, and why the third wins:

- **Backslash-escaping `"`** — rejected. It closes one of three hazards (the others: an embedded
  newline reading as a second instruction line; a trailing `--flag`-shaped suffix reading as
  additional flags), and its premise is uncheckable — "does the reading model un-escape `\"`"
  has no in-tree answer, and proving it costs a real session. If it doesn't un-escape, the escape
  characters land in the task text: silent mangling instead of silent truncation, a lateral move
  (@semantic-guard #4).
- **Lossy normalisation** (`"`→`'`, newlines→spaces; @paired-infrastructure's proposal) — rejected,
  though it is verifiable and strictly better than escaping. It *changes the task text*, and story
  task 4 makes `/user-story` emit these prompts machine-generated, where quoted identifiers and
  code fragments are routine. A defect class defined as "the task is silently told something
  different" is not closed by a fix that silently tells the task something different.
- **Out of band** (@security #4, @test point-11-amended) — taken. The prompt is already on disk at
  `$RUN_DIR/task-<id>.prompt` (the prompt-file write). The emitted instruction references that **path**; the
  quoted slash-command argument is runner-authored constant text plus a runner-generated path, so
  **no manifest byte is ever on the command line**. Lossless, needs no substrate assumption, and
  needs no skill change — which matters, because story task 4 puts `SKILL.md` off-limits, so a
  true `--prompt-file` flag (structurally the cleanest option) is unavailable. Recording that here
  so the next reader doesn't re-propose it.

The invariant the tests pin is **structural**, per @paired-infrastructure, because it survives
whichever transformation is chosen: given a manifest prompt containing `"` and a newline, the
generated `task_prompt` must contain exactly one `/devloop "` opening, exactly one closing `"`
before `--specialist=`, the `--specialist=<name>` token intact at the end, and the whole `/devloop`
invocation on **one line**. That assertion reds against today's code and greens after the fix.
The chosen transformation is named in a comment at the interpolation site, so task 4's dependency
on it is legible rather than folkloric.

The prompt is already on disk at `$RUN_DIR/task-<id>.prompt` (the prompt-file write). The emitted instruction
references that **path**; no manifest byte is spliced into the command line. All three splice
sites close together:

- `prompt` → out of band by path (path is runner-generated).
- `specialist` → character-class floor `^[a-z][a-z0-9-]*$`, fail loud otherwise. `dt-story`
  constrains it only to non-empty-after-trim (`engine.rs:273-279`), so `test --paired-with=x` is
  an accepted specialist today. Deliberately **not** an `.claude/agents/<name>.md` existence check
  (@test): under an active `STORY_REPO_ROOT` that resolves inside the fixture, so it would either
  break hermeticity or force the fixture to carry an agent tree.
- `continue_slug` (the `--continue` interpolation, the third splice site) → `^[0-9A-Za-z._-]+$`, same treatment.
  Filesystem-derived so lower severity, but the same mechanism, and it should not survive the fix
  to the other two.

Two wording conditions from @security so the splice is removed rather than relocated one level
down: the instruction must direct the Lead to treat the file's contents **as** the task
description, and must **not** read as "read the file, then run `/devloop` with its contents" —
that would ask a model to reconstruct the quoted form, reintroducing the splice with a model
performing it.

**The argv assertion runs in both directions, and absence-only would be vacuous**
(@semantic-guard F2, blocking — accepted; @security asked for the same). Absence-of-sentinel plus
presence-in-file both pass if the runner drops the prompt reference **entirely** — a refactor
building an empty path, or failing to interpolate at all: argv has no sentinel ✓, disk has the
sentinel ✓, green, and the task is dispatched with no instruction whatsoever. So the case also
asserts the **positive** half against the same recorded argv: the resolved prompt-file path is
present and the referencing text is intact. Then absence-of-sentinel means "not spliced" rather
than "not there".

**The claim in the comment is scoped, not "lossless"** (@semantic-guard F3). Out-of-band replaces
one substrate assumption with another of a different shape: *"does the reading model un-escape
`\"`"* becomes *"does the reading model expand the referenced file verbatim rather than paraphrasing
it"*, and neither has an in-tree answer. The mechanism is still right, and the honest argument is
the **asymmetry**, not the absence of an assumption: truncation is silent and unrecoverable — the
lost text exists nowhere — whereas an expansion-fidelity failure leaves the complete prompt durably
on disk at a path the instruction names, so it is auditable after the fact and the ground truth
survives. That is what the comment says. Same standard this diff applies three times over (the R-4
comment, the `RETRYABLE-UNCLASSIFIED` token, the TASK_TIMEOUT comment vs its code), applied to this fix's own claim.

Test asserts structurally against the recorded `claude` argv: the fixture prompt carries a
sentinel containing `"`, a newline, and a trailing ` --paired-with=evil`; the case asserts the
sentinel is **absent** from argv and present **verbatim** in the prompt file on disk.

The code comment at the interpolation site carries the sentence that stops someone reaching for
`tr` later: *"the task is silently told something different" is not closed by silently telling the
task something different.*

Flagged for story task 4 (@operations, @protocol): `/user-story` will now emit prompts that are
never shell- or command-line-interpolated, so it has no escaping obligation — but the Lead must
expand the referenced file, and that instruction wording is a behaviour change worth a look.

### R-5 — `--stop-after`, both halves

Mechanism chosen: **(a) + (b)**. A bash-side YAML parse is off the table (blocking finding from
@dry-reviewer #6, @test #4, @paired-infrastructure #7) — it would be a second manifest parser and
would drift the moment task 4 removes `branch`.

- **(a) Up-front, at second zero.** New read-only **`dt-story list-tasks <story>`** verb
  (@protocol P-6: a noun among five imperative verbs invites a rename after two scripts consume it,
  which is then a breaking change; `list-tasks` matches `add-task`'s shape and is cheapest to settle
  now) printing **`[{"id":…, "status":…, "deps":[…]}, …]`**. @protocol P-3 requires every emitted
  field to have a named consumer in this commit, and leaves the `deps` question to me because it is
  decided by the refusal's wording. **Decision: keep `deps` and write the deps-aware diagnostic** —
  the refusal says *"task 3 exists but is unreachable: dep 2 is pending"* rather than printing a
  flat list of reachable ids. That converts a late `exit 2` at the story-close gate into an accurate
  refusal at second zero, which is the whole point of validating up front, and it is the operator
  case that most needs distinguishing from a typo. `specialist` has no consumer under either
  option — dropped. R-8's own rule ("the manifest declares nothing nobody reads") applied to CLI
  output, with the asymmetry that decides ties: adding a field to a projection later is free,
  removing one that two scripts read is a breaking change. Consumed with `jq`. Validation
  sits where the flag is parsed (the argument-parsing block), **before** `preflight-story.sh:78` and before the loop
  (@operations E10), and stands on its own rather than assuming preflight validated the manifest
  (@paired-infrastructure, @operations F16). Refuses an id **absent** from the manifest, and an id
  present but **not reachable** — already `completed` is the common case, e.g. rerunning with
  `--stop-after=1` after task 1 landed (@operations E11, @observability O-7a). The refusal prints
  the reachable ids.
- **(b) Terminal never-fired check.** Deps-blocked and never-selected ids are only knowable at the
  end, so if the loop reaches the story-close gate with `STOP_AFTER` set and never matched, that
  is a loud `exit 2` **before** the expensive close gate runs — not a silent success
  (@observability O-7b).
- Plus: the stop-after comparison compares with `=` (string), so `--stop-after=01` passes a numeric lookup and never
  matches — normalize at parse time (@operations E12); and reject extra argv, since
  `run-story.sh story --stop-after=1 --foo` silently drops `--foo` today, which is the same
  failure class R-5 names (@operations E13).

I am carrying the `crates/dt-story/` classification row for (a). It is a read-only projection of
an already-parsed structure — no schema change, no state transition, no new field — which is why
it is Minor-judgment rather than Domain-judgment, and it makes `dt-story` the sole manifest-parse
locus for this flag instead of introducing a second one.

**@protocol's Gate-1 conditions on the verb (P-1..P-7), accepted in full.**

- **P-1 · doc block.** `main.rs:1-12` is the one place the CLI contract is written down, so
  `list-tasks` gets its bullet there or the SSoT drifts on day one: *0 with a JSON array on stdout,
  2 on read/parse error, **no other exit code***, and explicitly **read-only, never writes the story
  file** — stating the contrast with `next`, which does (`main.rs:169-175` rewrites when it reopens
  an escalated task). That contrast is also why a separate verb is the only correct shape:
  validating `--stop-after` through `next` would mutate the manifest as a side effect of parsing a
  flag. Uses the existing `load()`, not a bespoke read, so `Manifest::from_yaml` stays the sole
  parse locus (ADR-0035 §4).
- **P-2 · explicit projection struct in `main.rs`** (@protocol's own amendment: `RunnableTask` lives in `engine.rs` because `engine::next` constructs it to express its outcome, whereas `list-tasks` needs no engine logic at all — `load()` plus a projection, entirely CLI-layer — and `main.rs` already houses this exact shape in `cmd_escalate`'s local `EscalationOut` at `:228-235`. Consequence, not reason: the diff stays inside the two files already carrying rows, so there is nothing for the Layer-A scope-drift guard to flag). **Not**
  `serde_json::to_string(&doc.manifest.tasks)`. `manifest::Task` is the *storage* schema
  (`deny_unknown_fields`, documented stable rewrite order), so serializing it would make every
  future manifest edit an unannounced change to a consumed CLI output; `skip_serializing_if` means
  **absent keys**, so a `jq` consumer written against a fixture where a field is present breaks
  where it is absent — the same present-with-null-versus-absent collapse R-3 is pinning fixtures
  for, one artifact over. And two of its nine fields are worse than contract surface: `prompt` is
  arbitrary prose that R-2 defect 5 exists to keep off command lines, and `escalation` is a **path
  into `$RUN_DIR`**, so a widened projection would quietly acquire an operator-path disclosure
  nobody reviewed. Reuses `manifest::Status` rather than stringifying by hand, so the token set
  cannot diverge.
- **P-5 · what `tests/cli.rs` owes**, in the existing `assert_cmd` + fixture style: (1) exit 0,
  stdout parses as a JSON array, and the **complete key set asserted closed-world** — deserialize to
  `serde_json::Value`, take each object's keys, compare against an expected set, *never*
  field-by-field, which is open-world and passes unchanged when a key is added; (2) **read-only
  proof** by whole-file byte equality (not `assert_non_manifest_bytes_preserved`, `cli.rs:137`) on a
  fixture where the reopen is **guaranteed to fire** — escalated task is the lowest-id candidate,
  deps satisfied, no pending task ahead of it (`engine.rs:53` sorts by id, `:55-58` takes the first
  dep-satisfied one), *plus* an in-test precondition running `next` against a copy of the same
  fixture and asserting it **does** mutate, so the case cannot silently stop covering its target if
  selection order changes — prove the trap can spring, or the trap is decoration; (3) malformed
  story → exit 2 with **empty stdout**, matching `next` at `cli.rs:70-77`, so the runner's `jq`
  never sees partial output; (4) one fixture carrying all three statuses, pinning the lowercase
  token set; (5) pinned ordering (manifest order) and single-line compact output — the runner's
  error message prints these ids.
- **P-7 · exit space {0, `EXIT_MALFORMED`}** — see the rc discussion below.
- **P-6 · naming** taken (`list-tasks`, not `tasks`).
- **No versioning discipline**, per @protocol's ruling: producer and sole consumer ship in the same
  commit of the same repo, so there is no skew window — the only skew is a stale build artifact,
  which is a *build-currency* problem solved by the probe, not a *compatibility* problem solved by a
  version field. Adding one would imply a regime we don't have and drag in a deprecation policy for
  it. The two disciplines that survive colocation are P-1 and P-5.1. The doc block notes that
  versioning arrives only if this grows a consumer outside the repo, so the trigger is visible
  rather than folkloric.
- **Forward-compat with story task 4**, verified by @protocol: task 4 removes `Manifest::branch`, a
  *manifest*-level field — `Task` has none — so a task-level projection is untouched by it. The one
  constraint this creates is that task 4 must not rename or drop `Task.status` or `Task.deps`; R-8
  asks for neither, but task 4's implementer should see the dependency. Note the mirror image: had
  the projection serialized `Manifest` directly, task 4's `branch` removal *would* have silently
  changed this output — P-2 stated from the other end.

**The verb's contract, frozen by @protocol (its owner and co-implementer) so the bash side can be
written in parallel.** `dt-story list-tasks <story>` → stdout is **one line**, compact JSON array,
**manifest order as authored, not sorted**; each element exactly
`{"id": 3, "status": "pending", "deps": [1, 2]}` — `id` `u32`; `status` one of
`pending|completed|escalated` via `manifest::Status`'s existing `rename_all`; `deps` **always
present, `[]` when empty** (deliberately not `skip_serializing_if`, so the shape is total and my
`jq` never needs `// []`). No other keys. Exit **0**, or **2** on any read/parse failure with empty
stdout and the diagnosis on stderr. No other exit code, ever.

Manifest order is deliberate: re-ordering would be `dt-story` forming a second opinion about a file
whose order `to_block_body` otherwise preserves exactly. @protocol's fixture carries deliberately
out-of-order ids (3, 1, 2 — legal; nothing in `validate_manifest` requires ordering) so an
accidental sort reds. Where the refusal message wants ids sorted, it sorts in `jq`, which is a
presentation choice rather than a contract.

**One invocation serves all three refusal cases**: (1) id absent → not in the manifest, list the
reachable ids; (2) id present with `status == "completed"` → already completed (the
rerun-with-`--stop-after=1`-after-task-1-landed case); (3) id present and pending/escalated with
some entry in its `deps` not `completed` → unreachable, **naming the specific blocker and its
status** — the wording that made P-3 resolve toward keeping `deps`. Case 3 needs each *dep's*
status, not just the target's, which is why the projection is the whole array rather than a
single-task lookup — that goes in a comment so nobody later "optimises" it into a `--id` query.

**Three exclusions, three checkable reasons** (@protocol, with @paired-infrastructure's addition),
kept distinct rather than collapsed into "don't emit extra stuff", because collapsing loses the part
a future reader can test against: `prompt` — uncontrolled value that R-2 defect 5 exists to keep off
command lines; `specialist` — no consumer; `escalation` and `commit` — **referent invalidatable
without the artifact being rewritten**. `escalation` is a `$RUN_DIR` path that vanishes with the
container; `commit` (`manifest.rs:52`) is a sha that `run-story.sh`'s no-commit-sha comment explicitly declines to
populate because the manifest-bump amend amends and "amending changes the sha, so it can't be recorded inside the
commit it refers to", so any sha recordable there is stale by construction. Different causes, same
hazard, and neither argument would have caught the other.

**Option (a)'s real cost, priced rather than discovered** (@operations, @paired-infrastructure):
`preflight-story.sh:66` proves the binary *exists*, not that it is *current*. A
`target/release/dt-story` predating this commit passes `[ -x ]` and then makes `--stop-after`
refuse every valid id, sending an operator hunting a typo in a flag that was correct. `--version`
cannot catch it (it comes from `Cargo.toml`, which doesn't move per commit). So preflight
**probes the verb the runner is about to depend on** — generalising the rule the runner already
follows at the substrate-binding block, where it binds a CLI version and re-asserts it per task: **assert the
capabilities you consume, not the artifacts you hope were built.**

**The probe cannot key on the exit code** (@protocol P-4; @paired-infrastructure has withdrawn
their earlier contrary wording). Measured in-tree, and I re-ran all three myself:

```
target/release/dt-story list-tasks foo.md    -> rc 2   (clap: unrecognized subcommand)
target/release/dt-story next /nonexistent.md -> rc 2   (main.rs:23 EXIT_MALFORMED)
target/release/dt-story next --help          -> rc 0
```

clap's usage code collides with `EXIT_MALFORMED`, so "stale binary" and "unparseable manifest" are
literally the same signal — the plan's original wording would have told an operator with a bad
manifest to rebuild, which is the wrong-message failure this diff is otherwise policing. So:

- **preflight probes in two steps, with three explicit checks** (@protocol's form, selected over my
  earlier ordering-only version after @code-reviewer argued **against their own CQ-9 proposal** —
  CQ-9a). I had mischaracterised the alternative as "`--help` proves the verb name and nothing
  more"; @protocol proposed **two** invocations, and the second is a shape assertion. Their version
  does everything the ordering version does **and** proves output shape, with no ordering argument
  at all. The objection that decides it: an ordering-derived discriminator makes the *message's*
  correctness rest on two lines staying in order in a file people edit, with **nothing failing if
  they don't** — someone moves or conditionalises `:69`'s `validate` for a good reason, the probe
  returns the same rc, and its message silently becomes wrong. The mitigation I offered was a
  comment, which is the weakest control available and is M3's "registered but never invoked" class
  one level down.

  Measured (I re-ran all of it rather than taking it):
  ```
  dt-story validate   --help → 0      dt-story bogus-verb --help → 2
  dt-story next       --help → 0      dt-story list-tasks --help → 2   (verb absent today)
  ```
  Step 1 depends on nothing but the binary, so reordering cannot break it — and it is already red
  for the right reason before the verb exists, making it a failing-first case at zero cost.

  **`jq -e` cannot distinguish "no output" from "valid output" — verified in-tree:**
  ```
  printf '' | jq -e 'type=="array"'  → 0    # EMPTY input PASSES
  echo '[]' | jq -e 'type=="array"'  → 0
  echo '{}' | jq -e 'type=="array"'  → 1
  ```
  So as a shape assertion it is **vacuous against a producer that emits nothing**. In the pipelined
  form its apparent safety comes entirely from `pipefail` catching `dt-story`'s rc, not from the
  assertion — and the two goals pull opposite ways: the pipeline gets producer-rc safety while
  conflating which stage failed, and the moment anyone rewrites it as a capture to make failures
  attributable (the entire point of step 2) the producer's rc is gone and the surviving `jq -e`
  passes on empty output. Fail-open, in the probe built to prove a binary can answer, inside the
  diff fixing four fail-open conditionals — M2's mechanism reappearing in CQ-9's own fix.

  So: three explicit checks, never a pipeline, each `|| fail`-guarded (which selects the operator
  lane, per @operations):
  ```
  out="$("$DT_STORY" list-tasks "$STORY_FILE")" || fail "<cannot answer: rebuild>"
  [ -n "$out" ]                                 || fail "<answered with no output>"
  jq -e 'type == "array"' >/dev/null <<<"$out"  || fail "<output is not a JSON array>"
  ```
  The probe still sits after `:69` and the note at `:69` stays — but the ordering is demoted from
  load-bearing to explanatory, which is where it belongs. The note at the probe now reads: *step 1
  settles verb existence structurally; `:69` having already parsed this file with this binary is why
  step 2's failure is attributable to the manifest rather than the binary.* Load-bearing →
  structural check in code; genuine-but-supporting → comment.

- **the `--stop-after` site does NOT inherit that certainty and must not claim it**
  (@code-reviewer CQ-9). §R-5 deliberately validates at the argument-parsing block, **before** `preflight-story.sh:78`
  (@operations E10) so a bad flag dies at second zero — but at that point nothing has established
  that the manifest parses, so rc 2 there is genuinely ambiguous. The message names all causes and
  hands the operator the **one command that separates them**: `target/release/dt-story validate
  <story>` — green means the binary is stale, red means the manifest is. Two sites, two different
  certainties, two different messages. Never an id-not-found message; exit 2. Only rc 0 **and** parseable JSON
  reaches the id lookup. That delivers "distinguishes 'this id is not in the manifest' from 'this
  dt-story cannot answer'" without needing an exit code that cannot carry the distinction.
- **Never grep clap's stderr** for "unrecognized subcommand": that string is clap-version-owned, not
  ours, and this repo floats its toolchain — borrowed-signal coupling that goes quiet on a bump.
- **Three causes, not two** (@operations): at the `--stop-after` site preflight has not run, so
  `[ -x ]` has not been asserted either. Measured rc space there is `0` ok / `2` stale-or-malformed
  / `127` not built / `126` not executable, so the message names **not built, stale, or malformed
  manifest** and carries both recovery commands.
- **Both new preflight lines are `|| fail "…"`-guarded, and that selects the lane rather than
  being style** (@operations). `preflight-story.sh:23` is `set -euo pipefail` and `fail()` at `:29`
  is what produces the operator contract (message + **exit 2**); an unguarded failure aborts under
  `set -e` with **exit 1** and no message, the runner's `preflight-story.sh` call propagates it unchanged, and its exit-code header
  documents exit 1 as "task escalated or blocked" — R-4's own mechanism, inside the file we are
  adding the R-4-supporting check to. Applies to the guard-binary assertion and the capability
  probe, and the `| jq -e 'type=="array"'` form needs the guard on the whole pipeline under
  `pipefail`. The "why" goes in a comment, because `|| fail` reads as style until you know it picks
  the lane.
- **`list-tasks` is pinned to exit {0, `EXIT_MALFORMED`}, documented** (@operations). `dt-story`
  already overloads this space — `EXIT_ALL_DONE=3`, `EXIT_BLOCKED=4` (`main.rs:24-25`),
  `EXIT_TAG_EXISTS=4` (`:27`) — so **rc 4 already means two different things by verb** (`BLOCKED`
  for `next` and `TAG_EXISTS` for `add-task`, both in the runner's manifest-outcome case block). A read-only
  projection has no third state, and adding the verb is the cheapest moment to hold that line: if
  `list-tasks` later grows a 3 or 4, every "non-zero means the binary cannot answer" call site
  silently gains a wrong branch.
- The capability-probe comment describes the mechanism as **zero-vs-non-zero on the verb**, not as
  decoding an exit code — otherwise the comment documents a mechanism that is not there.
- Gate-2 case for the stale-binary branch, since a stale binary is hard to fixture (@operations'
  accepted form): drive the `DT_STORY` seam at a stub exiting non-zero for `list-tasks --help`, and
  assert the **three-cause message**, not the exit code alone.

The preflight guard-binary check is **one check naming one command** (@paired-infrastructure), not
a second `[ -x ]` beside `:66` — the current line names only `cargo build --release -p dt-story`,
which is what steers operators into the built-the-wrong-subset state; a second half-command beside
it would move that trap one line over instead of closing it. Message names
`cargo build --release -p dt-guard -p dt-story`. Priced honestly in the comment: this **newly
refuses a run that succeeds today** — fresh container, first task touches `crates/`, the gate's own
Layer 1 builds both binaries before Layer 3 consumes them. It is still right (every task's gate
depends on `dt-guard`, and asserting a dependency at start beats discovering it at Layer 3), and
saying so stops the next person "fixing" the strictness.

### R-2 defect 2 + canary flags

`canary_probe`'s `>"$out" 2>&1` (`canary_probe`'s redirect) becomes `>"$out" 2>"$RUN_DIR/task-<id>.canary.err"` —
**redirected, not discarded** (@observability O-6, @operations D8, @semantic-guard #5): when the
canary is unclassifiable that stderr is the only evidence of why, and `2>/dev/null` would make the
`infra` lane *less* debuggable than today. `record_infra_incident` copies it with the same `${ts}`
alongside the existing canary copy (`:157-161` exists precisely because the fixed per-task path is
overwritten by the next attempt — the identical argument applies) and references **both by path**,
never inlined. `$RUN_DIR` is under `DEVLOOP_TMP`, outside the work tree, so `git add`/the amend at
the manifest-bump amend cannot sweep it.

`--allowedTools ""` goes **last** in `canary_probe`'s argv (@operations D9 — the option is
variadic, `--allowedTools, --allowed-tools <tools...>`, so a following non-option argument would be
consumed), as two argv entries, with `--dangerously-skip-permissions` retained. Pinned together in
one assertion against the recorded argv. A comment at the call site says `--allowedTools` **must
remain last** and why (@security): a flag appended after it in the natural place gets swallowed as
a value, which silently drops skip-permissions — and the symptom is a permission-prompt hang
misclassified as `infra`, the exact thing ADR-0035:108 keeps the flag to prevent.

**Parse-safety measured, not assumed** (@paired-infrastructure, claude **2.1.229**, one haiku call,
$0.016): the exact post-change invocation returns rc 0, **empty stderr**, and a single-object JSON
with `"is_error":false`, `"result":"ok"`, `"permission_denials":[]`. That settles @security #5b's
worry — the empty value is **not** swallowed such that `--dangerously-skip-permissions` is consumed
as `--allowedTools`' argument, which would have silently dropped skip-permissions and reintroduced
the permission-prompt hang. It also rules out the nasty failure mode: had the CLI rejected an empty
value, `canary_probe` would exit non-zero → unparseable → *every* task failure becomes `infra` and
the story stalls at `exit 2` forever. The version is cited in the comment, since `run-story.sh`
already treats the CLI version as the thing that moves underneath it (`:87-96`).

**Bonus from the same capture, and it goes straight into R-3**: the healthy response carries
`"api_error_status":null` — literally present-with-null, on the path production emits constantly.
So `jq -r '.api_error_status // empty'` (`canary_classify`'s `api_error_status` read) collapses null and absent exactly as R-3 and
ADR-0035:190 describe. The null fixture is shaped like that real object rather than hand-minimised.

**Residual stated honestly** (@operations D9, @security #5): this is a *restriction*, and
ADR-0035:129 says restrictions fail open and quiet. The measurement above proves parse-safety and
output shape; it does **not** prove the real CLI treats `[""]` as "no tools" rather than
"unset → allow all", and a stub `claude` can only ever pin argv. Asserting the real restriction is
substrate-probe work that ADR-0035 §8 already owns, and I am not taking it here — the argv pin must
not be read as implying more coverage than it has.

### The suite — `scripts/workflow/run-story.test.sh`

**Hermeticity envelope.** Every case runs the **real** `run-story.sh` (invoked at its real path, so
the `BASH_SOURCE`-derived real root is genuine) under `env -i` with: stub-dir-prepended `PATH`,
`HOME="$WORK/home"` (**not** the operator's — @security #3, @paired-infrastructure #5; the real
preflight writes `$HOME/.claude/settings.json`), `GIT_CONFIG_GLOBAL=/dev/null`,
`GIT_CONFIG_SYSTEM=/dev/null` and a fixture-local empty `core.hooksPath` (so the suite is
reproducible off this machine and no operator hook runs inside a test — **measured**
by @paired-infrastructure: global and system gitconfig are both *empty* in this container, which is
precisely why omitting these would pass here and in CI and then behave differently on an operator
host carrying `commit.gpgsign` / `core.hooksPath` / `commit.template` / `init.defaultBranch=main`;
git 2.39.5 supports both variables, so there is no compatibility caveat), fixture-local
`user.name`/`user.email` (**load-bearing, not a nicety**: with empty global config every commit the
runner makes — the amend at the manifest-bump amend, its fallback at `:501`, the audit-remediation append at
`:297-298` — dies with "Please tell me who you are", so any case reaching task completion fails
without it), an explicitly pinned fixture branch name (a bare `git init` yields `master` here but
`main` on a configured host),
a **mandatory** redirected `DEVLOOP_TMP`, `DEVLOOP_TEST=1`, and the seam vars. `DEVLOOP_TEST` is
set **only** in the `env -i` child, never at file scope — `assert_no_ci_sentinel_leak`
(`_common.sh:186-192`) hard-fails any CI pipeline where it is ambient. **No case sets
`STORY_RUNNER_ALLOW_HOST=1`** (@security #3): that hatch means "I accept skip-permissions against a
host tree", and a suite whose greenness depends on it teaches the pattern.

`claude`, `sleep` and `date` are PATH-injected; **`git` stays real**; `dt-story` is the real binary
via the `DT_STORY` seam. Missing binary is a loud precondition failure in the
`layer-all.test.sh:43-47` style, never a skip (@test #8) — layer 1 builds it, and `layer3.sh` is
routinely run standalone.

The `date` stub delegates to the real `date` for every form and honours a single controlled-now
override, so the sleep-math cases pin the **real** arithmetic — the `+300` margin, the
`reset_epoch <= now → +86400` rollover, and the `93600` cap at `:424` — rather than a
reimplementation (@test #7).

**Three containment loci, and why they are not duplication** (@dry-reviewer's advance ruling):

1. In `run-story.sh` — mandatory, protects every future case, including ones nobody remembers to
   guard.
2. In the tests — asserted **black-box** against the runner's refusal. Two cases: a root that is
   not its own git top-level, and a root that **is** the real repo's top-level. The predicate is
   **not** reimplemented in the test; asserting on a copy tests the copy.
3. The suite's own pre-flight check, before any case runs — deliberately **not** sourced from the
   runner, because a suite that borrows its safety from the seam it exists to verify takes the
   operator's clone down on the exact run meant to catch it. Hard `exit 2`, never an accumulating
   `assert_*` (@test #2) — `_test_helpers.sh:28-36` returns 0 unconditionally, so an assertion-only
   failure would let the harness proceed into `git reset --hard -q`/`git clean -fdq` at
   the session-limit lane's `git reset --hard` / `git clean -fdq`. Note the concrete hazard this catches: a `TMPDIR` pointing inside `/work` makes
   `mktemp -d` land inside the real clone.

**Empirical after-the-fact proof** — configuration verified is not the same as outcome verified.
Consolidated shape per @security's final instruction plus @dry-reviewer items 9-10:

- **Three captures, not two**, via a local `capture_real_tree_state <real-root>` function called
  from both sites (factoring is local; promoting it is forbidden by @dry-reviewer's item-8 ruling,
  and inlining it twice risks the before/after drifting until the comparison silently means
  nothing): `HEAD`, `git status --porcelain`, **and** `git diff HEAD | sha256sum`. The third is
  load-bearing: porcelain prints ` M path` whether or not the content changed, and this suite runs
  inside a devloop whose tree is dirty by construction, so "already modified" is the repo's normal
  state. Untracked *content* is deliberately uncovered — the only two mutators in play are
  `git reset --hard -q` and `git clean -fdq` (the session-limit lane's `git reset --hard` / `git clean -fdq`), and neither rewrites untracked content;
  `clean` removes entries, which porcelain's presence/absence lines already register. That
  reasoning goes in a comment so it reads as scoped rather than as a gap.
- **Explicit per-command rc checks, mandatory not defensive.** The suite needs `set +e` (it drives
  a runner expected to exit 1 and 2 on most cases — `layer-all.test.sh:31` precedent), under which
  neither a bare assignment nor `local x=$(…)` propagates a failure. Empty-before compared against
  empty-after compares **equal**, so a git failure at both ends reports containment verified having
  verified nothing — which is R-2 defect 4 reproduced inside the harness built to catch it. Worth
  a comment saying exactly that, so nobody simplifies the rc checks away. The `| sha256sum` line
  needs its own rc check rather than the pipeline's: a failing `git diff` yields `sha256sum` of
  empty input, `e3b0c442…` — a stable, plausible-looking hash that compares equal every time,
  which is worse than an empty string, and which this story's own Deferred §Gates already flags as
  the `gate2_signature` collapse.
- **Asymmetric failure handling**: a failure at the *before* site is fatal — refuse to run any case,
  because a containment proof that cannot establish its baseline would otherwise run every case
  unverified against the operator's clone. A failure at the *after* site is `exit 2` with the
  diagnostics.
- **One composed `EXIT` trap, never two lines.** Bash traps replace rather than accumulate, so a
  second registration silently disables one of cleanup or proof depending purely on line order.
  Precedent: `audit-suppressions-check.test.sh:212` `trap 'restore_dbot; rm -rf "$WORK"' EXIT`.
  Comparison and diagnostics **first**, `rm -rf "$WORK"` last — the fixture's state is part of the
  diagnosis.
- **The handler sets its own exit code.** `report_results` (`_test_helpers.sh:85-94`) prints the
  summary and then `exit 0`/`exit 1`, so the trap runs after the code is chosen; appending to
  `FAILURES` would never be printed and the suite would still exit green. The handler prints the
  divergence to stderr itself and `exit 2`, under its own distinct token so a reader can tell "the
  suite refused to run" from "the suite ran and mutated the operator's clone". Tree-was-mutated
  outranks all-green — those are exactly the conditions under which the summary is least
  trustworthy.
- Captures use an explicit `git -C <real root>` resolved once from `BASH_SOURCE` at suite start,
  before any seam is set; never the harness's CWD, which is the thing under test.

Stays local to `run-story.test.sh` per @dry-reviewer's item-8 ruling.

**Helper promotion** (@dry-reviewer #2/#3, calibrated by item 8): promote exactly three, all with
two real consumers, all parameterized to drop the `LA_DT` closure and the `ran.layer*` hardcode —
`assert_marker <label> <dir> <glob>`, `assert_no_marker <label> <dir> <glob>` (existence via
`compgen -G`, the shellcheck-clean idiom), `assert_absent <label> <needle> <haystack>`.
**7 call-site conversions, not 8** (@dry-reviewer's count, corrected from mine): `assert_marker`
at `layer-all.test.sh:159`/`:209`, `assert_no_marker` at `:185`/`:198`, `assert_absent` at
`:148`/`:184`/`:208`, plus the 3 definitions deleted from `:85-96`. Zero copies left behind. The stale
`_test_helpers.sh:2` header ("shared scaffolding for per-language `changed.test.sh`") is refreshed,
since 8 files now consume it. **Not promoted**, per the same ruling: the suite's pre-flight
containment check, the before/after tree comparison, and the `claude`/`sleep`/`date` stubs — one
consumer each. No new `scripts/workflow/_helpers.sh`.

**Anti-vacuity.** Every stub records its invocation (`claude` full argv per call, `sleep` its
argument, each stub `layerN.sh` / `layer-all.sh` / `preflight-story.sh` a ran-marker), and every
case asserts markers for the stubs it depends on **and** `assert_no_marker` for the stubs it claims
were not reached. "No real `claude` ran" is proven by a recorded stub invocation, not inferred from
`env -i` (@security #3). The CI-rejection and inertness cases are built as **byte-for-byte copies
of a case already shown green**, with the single env delta, asserting the **distinct rejection
token** — not the exit code alone, since any broken fixture also exits non-zero (@test).

**Case list.** Seam/containment: sentinel absent, `=0`, `=false`, `=yes`, `=1`+override honoured,
override-without-sentinel, `GITHUB_ACTIONS`×(`STORY_REPO_ROOT`, `DT_STORY`), root-not-own-toplevel,
root-is-real-repo, `DEVLOOP_TMP` un-redirected. R-5: non-numeric, absent id, unreachable id,
`01` normalization, extra argv, valid stop (close gate **not** run), never-fired terminal check.
R-2: (a) `resets 3:00pm` prose, (b) stderr byte, (c) rc 1 vs rc 2 vs rc 127 from layers 1-6 **and**
7, (d) all four M2 sites, (e) three splice payloads. R-3: 429 / null / absent / non-429-numeric /
non-JSON / healthy. Plus timeout, session-limit exhaustion, the argv pin, and an all-green baseline
(a fixture where every case expects non-zero proves nothing).

**Budget — measured against the right ceiling** (@paired-infrastructure #3, @code-reviewer CQ-7).
My draft cited ADR-0033 §4's 90s p95 for layers 3+6, but the number that actually fires on every run
is `layer-all.sh:89`'s `DEVLOOP_LAYER_BUDGET_SECS:-20`, and it is **per layer**. Layer 3 already runs
`run-guards.sh` plus seven self-tests, so a 15s suite on top would breach the 20s warn on most runs —
and a warn that fires every time is a warn nobody reads, which is M3's failure arrived at from the
other direction. So: **measure layer 3's wall time before and after** (the gate runs at Gate 2
anyway) and record **both numbers, plus the suite's own measured runtime**, under Devloop
Verification Steps — a target is not a baseline, and rollback item 1's trip threshold is only
actionable against a recorded one. Budget the suite against the layer's *remaining headroom*. If the
total lands over 20s, that is a deliberate decision to raise `DEVLOOP_LAYER_BUDGET_SECS` with a
reason, not something to discover from a warn line later; `cp -a` from one template instead of N
inits is the first lever, and if it is not enough I will raise the case count with
@paired-infrastructure rather than silently absorbing the breach. Achieved by building **one** `git init` template
fixture and `cp -a`-ing it per case rather than N inits; no case waits on a real `timeout 120`,
a real `sleep`, a cluster, a network, or `origin/main` (a fresh `git init` fixture needs no
merge-base, unlike `layer-all.test.sh:41-47`). The `-newer "$start_marker"` scoping is not touched
(@code-reviewer).

**Two hazards live on that `find | sort -rn | head -n 1` pipeline and they have OPPOSITE
thresholds** (@semantic-guard, measured; @code-reviewer confirmed and withdrew their own count as
miscalibrated). My draft said "≤2 dirs keeps the SIGPIPE hazard unreachable", which is true and
reads as though it bounded the class. It does not:

| Hazard | Needs | `≤2` |
|---|---|---|
| SIGPIPE (`sort` still writing when `head` exits) | *many* entries | closes it |
| **Wrong selection** | *exactly 2* | **opens it** |

With two dirs at equal `%T@`, GNU `sort -rn` falls back to a whole-line compare under `-r` and
selects the **lexically largest name** — deterministically, independent of `find` traversal order
(`alpha`/`beta` at the same second → `beta`, every time). Since that value decides
**resume-vs-fresh**, a two-dir case exercises whichever branch the alphabet picked and passes
**stably forever**: a flaky case gets investigated, a stably-green wrong case never does, and
rerunning — the first thing anyone tries — cannot tell them apart. Devloop slugs are date-prefixed,
so same-day fixtures tie on the prefix and are decided by the suffix.

A count cannot express the requirement, so the suite states the **invariant**: at every point the
runner reads that pipeline, **at most one** dir is newer than `$start_marker` — **or** the case sets
mtimes explicitly (`touch -d`) and asserts **which slug was selected**. For the session-limit lane
the second form is strictly better: two dirs *with* the assertion is real coverage of the selection;
two dirs without it is stable vacuity.

**Stated "not covered"** in the test file header (@paired-infrastructure #3, @security #2): because
`preflight-story.sh` is reached relative to the redirected root, the fixture stubs it, so the suite
does **not** exercise the real preflight — its container-boundary gate (ADR-0035 §5), substrate
probe, or settings registration. That is the right call for a hermetic suite and it is a stated,
contained decision rather than a side effect: the seam can only redirect preflight when
`DEVLOOP_TEST` is exactly `1`, `GITHUB_ACTIONS` is unset, and the root passes both containment
halves. Also not covered: whether the real CLI honours `--allowedTools ""` (ADR-0035 §8's probe
owns it). The stop-hook cases live in this same file, in a delimited section, because their defect
is the same M2 mechanism as the two runner siblings and the class should be visible in one place.

### Failing-first evidence (@team-lead ask #1, @test #14, @semantic-guard)

Not reconstructed afterwards, and not committed red (the story's §13 amendment rejects committing
red tests, and a red `layer3.sh` would red every subsequent task's gates). Sequence:

1. Land **seams + containment only** in the working tree, uncommitted. Nothing else.
2. Write the full suite. Run it. Every defect case is red at this point; capture the transcript.
3. Apply the defect fixes **one at a time**, re-running after each, recording which named case
   flipped red→green with which message.
4. Single commit at the end, green.

Each defect maps to a distinctly-named case so the mapping is mechanical, and the per-defect
red→green transcript goes under **Devloop Verification Steps**. A case that would pass either way
is a finding — I will be looking for that too, particularly on the CI-rejection and inertness
cases, which are the vacuity traps here.

Housekeeping: `run-story.sh` is edited with the Edit tool only, never a `>` redirect — it is the
one corruption trigger for the live run driving this devloop (story Notes; @operations F19).

---

## Pre-Work

None — tree clean at `9807f0b`.

---

## Implementation Summary

All five R-2 defects fixed, R-1/R-3/R-4/R-5 satisfied, suite wired into `layer3.sh`.
**148 assertions, 0 failures, ~5s.** Layer 3 total 11s (per-layer warn is 20s).

| Requirement | Fix | Named cases |
|---|---|---|
| R-1 | Two `DEVLOOP_TEST`-gated seams + containment predicate + CI presence-rejection + `DEVLOOP_TMP` isolation | `a-sentinel-*`, `b1`–`b6`, `c1`–`c2` |
| R-2.1 | `canary_classify`: structured-first, case-insensitive multi-phrase, retryable default (ADR-0035 §6 inversion) | `g1` |
| R-2.2 | `canary_probe` writes stderr to its own file | `g2` |
| R-2.3 | Gate rc 1 → escalate; **any** other non-zero → operator lane, manifest untouched | `f1`–`f4` |
| R-2.4 | `git_head`/`tree_dirty`/`git_error_lane` at 5 sites + stop-hook fail-closed | `h1`–`h3` |
| R-2.5 | Prompt passed **out of band** by path; `specialist` and `continue_slug` character floors | `i1`–`i7` |
| R-3 | 429 / null / absent / non-429-numeric / non-JSON / empty / healthy | `g3`–`g9` |
| R-4 | rc split + timeout lane + session-limit exhaustion | `f1`–`f4`, `g10`, `j1` |
| R-5 | `dt-story list-tasks` validation (absent / unreachable / dep-blocked) + terminal never-fired + `01` + extra argv | `e1`–`e7` |
| ADR §6 | `--allowedTools ""` last, keeping `--dangerously-skip-permissions` | `g11` |

### Two defects found BY the suite, not planned

Both are the failing-first process working rather than a formality.

**1. A silent `exit 1` through no lane — M4's seventh instance.** `persist_resume_pointer`'s
`find docs/devloop-outputs … | sort -rn | head -n 1` runs inside a `d="$(…)"` assignment under
`set -euo pipefail`. When `docs/devloop-outputs` is **absent**, `find` exits non-zero, `pipefail`
propagates, and the assignment kills the runner with **exit 1, no message, no lane** — while the
exit-code header documents 1 as "task escalated". It is reachable, not theoretical: the
session-limit lane's own `git clean -fdq` removes that directory, because git does not track empty
directories. Extracted to `newest_devloop_output`, which answers absence explicitly. This is
exactly the class the whole task is about, found in a code path nobody had flagged.

**2. `jq -e` returns 0 on empty input — inside my own classifier.** @code-reviewer measured this
against `preflight-story.sh`'s probe; I then wrote `jq -e 'type == "object"'` as `canary_classify`'s
non-JSON guard and case `g8` (empty canary, probe killed by its `timeout 120`) failed. A `[ -s ]`
check now precedes it. The finding transferred to a second site before the first had landed.

---

## Files Modified

| File | Change |
|---|---|
| `scripts/workflow/run-story.sh` | Seam block + containment; `--stop-after` manifest validation + terminal check; canary probe/classify; gate lane split; M2 git helpers at 5 sites; out-of-band prompt; `record_infra_incident` hardening + `log` arg; `newest_devloop_output` |
| `scripts/workflow/run-story.test.sh` | **New.** 148 assertions across 12 sections |
| `scripts/workflow/devloop-stop-hook.sh` | Fail-closed on `git rev-parse` error, distinct block reason |
| `scripts/workflow/preflight-story.sh` | Guard-binary assertion; `list-tasks` capability probe (3 explicit checks) |
| `scripts/layer3.sh` | One `run_and_emit` wiring line |
| `scripts/lang/_test_helpers.sh` | Promoted `assert_absent`/`assert_marker`/`assert_no_marker`, parameterized; header refreshed |
| `scripts/layer-all.test.sh` | 7 call-site conversions, 3 local definitions deleted |
| `.github/workflows/ci.yml` | Guard-binary build step (CI half only, scoped in-hunk) |
| `crates/dt-story/src/main.rs`, `tests/cli.rs` | `list-tasks` verb (@protocol, co-implementer) |
| `docs/runbooks/devloop-validation.md`, `docs/TODO.md` | Owner/reviewer-written entries |

---

## Devloop Verification Steps

### Failing-first evidence (per-defect red→green)

Seams + containment + `list-tasks` landed first (step 1), then the suite was written against
unfixed defect code. **Baseline: 108 passed / 40 failed** (measured against the DELIVERED 148-assertion suite). Every failure was a real defect, verified
by its message rather than by exit code alone:

| Case | Red (message before the fix) | Green after |
|---|---|---|
| `h1-git-error-not-completed` | *"a git error marked task 1 COMPLETED (false green)"*, rc **0** | operator lane, rc 2, manifest `pending` |
| `f2-rc2-manifest-untouched` | task 1 status `escalated`, rc **1**, escalation record present | `pending`, rc 2, infra incident, no escalation record |
| `e2-absent-id-exit2` | rc **0** — flag silently ignored, `ran.claude.devloop` present | rc 2, `STOP-AFTER-UNKNOWN-TASK`, no devloop |
| `g1-prose-typo-no-infra-lane` | `STORY_RUN: INFRA` present for `…resets 3:00pm` | `SESSION-LIMIT`, sleep recorded |
| `g2-stderr-byte-no-infra` | `STORY_RUN: INFRA` present from one stderr line | `SESSION-LIMIT`, `task-1.canary.err` preserved |
| `i1-sentinel-absent-from-argv` | `SPLICE-CANARY-7f3a` **present** in argv | absent from argv, verbatim on disk |
| `i2-paired-with-not-spliced` | `--paired-with=evil` **present** in argv | absent |
| `j1-timeout-manifest-untouched` | task 1 `escalated` on a timeout | `pending`, infra lane |
| `g10-exhausted-token` | rc 1, escalated on a positively-classified quota outage | rc 2, `SESSION-LIMIT-EXHAUSTED` |
| `h2-stop-hook-fails-closed` | no `"decision":"block"` — git error **allowed** the stop | blocks, distinct git reason |

**Per-defect attribution, re-measured against the delivered suite** (the earlier 112-assertion
figures described a pre-`h3`-repair artifact and have been discarded — an artifact for a different
artifact does not discharge R-2's contract):

| Fix reverted | Red | Collateral |
|---|---|---|
| D1 canary prose classification | 6 — `g1`×3, `g4`, `g5`, `g6` | none |
| D2 canary stderr merge | 4 — `g2`×3, `g11` | none |
| D4 git fail-open | 7 — all `h1`, incl. `h1-git-error-not-completed` (the false green) | none |

Independently reproduced by @semantic-guard on an isolated tree: D2 → 3 red, D5 → 3 red (including
the *positive* `i3-prompt-file-referenced`), R-4 → 11 red including `f2-rc2-manifest-untouched`
reporting *"task 1 status is 'escalated', expected pending"*. @test independently mutated
`SEAM-SET-IN-CI` → exactly 2 red, nothing else moving.

Transcripts: `/tmp/rs-evidence/00-baseline.txt`, `01-attribution.txt`, `99-green.txt`.

### Containment proof — three states, verified against a scratch repo (CQ-16)

The proof distinguishes **held**, **violated**, and **could not be verified**. A suite that mutated
the tree leaves a *stable* post-state; a concurrent writer leaves a *moving* one — so a mismatch is
re-sampled, and if it moved again no single actor explains both deltas. Plus a **quiescence
pre-flight**: two back-to-back samples before any case runs, refusing at second zero if the tree is
already moving. The comparison is **not** narrowed — all three captures stay.

Verified against a scratch git repo rather than by writing to `/work`, driving each state
deliberately (the first attempt mis-reported state 3 because the simulated writer finished between
samples — a real property of the discriminator, now documented as a known limit):

| Driven state | Verdict |
|---|---|
| nothing changes | `HELD` |
| one stable mutation | `CONTAINMENT-FAILURE` |
| writer active across both samples | `CONTAINMENT-UNVERIFIED` |
| quiescent tree, pre-flight | proceeds (no false positive) |
| writer active, pre-flight | `NOT-QUIESCENT`, refuses up front |

Two limits stated in-code rather than papered over: a writer that stops before the re-sample looks
stable and is reported as a containment failure (over-reporting, the safe direction); and a genuine
breach *while* a writer is active is reported as unattributable (under-reporting — which is why the
quiescence pre-flight exists to make that combination rare).

### Vacuity sweep (Gate-2 condition — @paired-infrastructure, Lead-directed)

**The class**: a case whose expected outcome is *also* the outcome of the mechanism never running.
K1 proved it non-hypothetical — a filter rejecting every line would have passed the entire suite, because
"no resumable output" was the ambient expectation. Swept **by mutation**, not by argument, since
assuming the property is what produced the K1 gap.

**Method.** Rather than deleting mechanisms — which for the seam clauses would let a case escape
into the real repository — each refusal's *emitted token* was renamed. That leaves every mechanism
running and removes only the positive signal a case claims to assert, so any case still passing is
relying on a null outcome.

**Result: 21 of 21 rejection cases red.** Every case in (A), (B), (C), plus `e2`/`e3`/`e6`,
`f2`/`f3`/`f4`, `g10`, `h1`, `i7` — each anchored by a positive assertion the null outcome cannot
satisfy.

| Case group | Negative assertion | Positive anchor | Mutation result |
|---|---|---|---|
| `a-sentinel-*` (×4) | `assert_no_marker 'ran.*'` | `SEAM-OVERRIDE-WITHOUT-TEST-SENTINEL` | red |
| `b1`–`b6` | `assert_no_marker 'ran.*'` | 4 distinct `SEAM-ROOT-*` / `SEAM-RUN-DIR-*` tokens | red |
| `c1`/`c2` | `assert_no_marker 'ran.*'` | `SEAM-SET-IN-CI` | red |
| `e2`/`e3` | `assert_no_marker 'ran.claude.devloop'` | `STOP-AFTER-UNKNOWN/UNREACHABLE-TASK` | red |
| `e5`/`e7` | `assert_no_marker 'ran.layer-all'` | `STOPPED after task N` | red |
| `f2` | `assert_no_marker` escalation record | `PIPELINE-PRECONDITION` + incident-record marker | red |
| `g1`/`g2` | `assert_absent 'STORY_RUN: INFRA'` | `sleep.args` / `task-1.canary.err` markers | red |
| `i1`/`i2` | `assert_absent` sentinel in argv | `i3`/`i4` prompt-path + `--specialist=` intact | red (3 assertions) |

**One case was genuinely vacuous, and it was not in the predicted subset.** `h3`
("the stop hook stays silent when it should not fire") asserted only
`assert_absent '"decision":"block"'`. Measured: with `STOP_HOOK` repointed at a nonexistent path,
**h3 passed** while `h2` caught the deletion — its expected outcome is exactly the outcome of the
hook not existing. Now anchored on both halves of the hook's own contract
(`devloop-stop-hook.sh:11`, "Exit 0 with no output = allow the stop"): `assert_exit 0` plus an
explicit empty-output check. Proof-of-trap: with the hook deleted, h3 now fails 2 assertions
(rc 127, non-empty output).

**A 1-in-8 flake, found by running the sweep repeatedly.** K1 passed 7 of 8 runs. Cause: the runner
selects with `find -newer "$start_marker"`, and the stub creates the output dir milliseconds later,
so on a coarse-granularity filesystem the two mtimes compare **equal** and `-newer` (strictly
greater) excludes it. Fixed by setting the mtime explicitly (`touch -d 'now + 1 minute'`) rather
than inheriting it from execution order — the general rule being that **a timestamp a case's
outcome depends on is set, never inherited**, which is the same principle as @semantic-guard's
two-dir `touch -d` requirement arriving one case earlier. Verified **20/20 clean** after the fix.
Worth noting it would have surfaced as ~12% of CI runs, i.e. as an unexplained intermittent red on
someone else's unrelated PR.

### Proof-of-trap: the `report_results` STATUS-collectability assertion (case `h`)

Added to close a seam @operations found, then discovered by @paired-infrastructure to be **blind** —
it certified a property it could not observe. The original fixture injected a `FAILURES` entry
beginning `[case]`, copying the shape every `FAILURES+=` producer uses, so stripping the `'  - '`
prefix still left the line mid-`STATUS=`. Measured across all four combinations:

| Fixture | prefix present | prefix removed |
|---|---|---|
| `"[case] expected STATUS=FAIL …"` (original) | passes | **passes** — blind |
| `"STATUS=FAIL REASON=… injected at line start"` (landed) | passes | **fails** — trap springs |

Both halves then driven independently against the landed fixture:

```
baseline                                        26 passed, 0 failed
mutation 1: delete the '  - ' prefix            25 passed, 1 failed
            -> [h-report-results-no-collectable-status]
mutation 2: stop printing FAILURES entirely     25 passed, 1 failed
            -> the positive half
restored                                        26 passed, 0 failed
```

Mutation 2 is why both halves are required: the negative alone also passes if `report_results`
stops printing failure entries at all, which would destroy the diagnostics the prefix exists to
carry while reporting the property intact.

The same change corrected two false claims this suite had shipped about itself: `_test_helpers.sh`
called the `'  - '` prefix *"the only thing"* keeping that text mid-line (the `[${label}]`
convention is an independent control, and today the one actually doing the work — but it is a
convention no code checks, and callers append to `FAILURES` directly), and case `h`'s own comment
cited the **narrow** formulation while listing OPS-1 among its instances, which is the example that
formulation wrongly clears.

### Gate evidence

```
scripts/workflow/run-story.test.sh: 148 passed, 0 failed
scripts/layer-all.test.sh:           24 passed, 0 failed
scripts/layer7.test.sh:              70 passed, 0 failed
LAYER=3 DURATION=11 RESULT=OK REASON=guards-passed
All guards passed
```

Layer 3 measured **before** this change for comparison at Gate 2; the suite's own 4.1s is the
number rollback item 1's trip threshold is written against.

### Containment, verified rather than assumed

The suite's `EXIT` trap re-compares the real repo's `HEAD`, `status --porcelain` and
`sha256(diff HEAD)` against the capture taken before any case ran, and forces a non-zero exit on
divergence even when every assertion passed. It has reported no divergence across every run.
Two cases prove the predicate black-box: a root inside the real clone (`b1`) and a root set to the
real repository (`b2`) — the load-bearing half.

---

## Code Review Results

Nine reviewers, **zero ESCALATED**. Full verdict table in §Loop State → Gate 3. Summary:

### Security Specialist
**Verdict**: RESOLVED-FIXED — 1 finding, 1 fixed, 0 deferred.
`DEVLOOP_TMP_DEFAULT` was an undeclared third override whose only capability was softening a
containment control — absent from `__any_seam_override_present`, so it escaped both the fail-loud
guard and the CI rejection. A *live* bypass, not latent: `-ef` against a missing operand is false,
so pointing it at a nonexistent path made `SEAM-RUN-DIR-NOT-REDIRECTED` fail open. Hardcoded.
Also endorsed the CQ-16 three-state trade with a standing invariant: `CONTAINMENT-UNVERIFIED` must
never be softened to a pass, warning or skip — the moment it stops being fatal the blind spot
converts from *loses attribution* to *loses detection*.

### Test Specialist
**Verdict**: RESOLVED-DEFERRED — 3 findings, 3 resolved, 1 deferral accepted (M3).
T-1 caught that the failing-first evidence described a **112**-assertion suite while the delivered
artifact had 117 — the green claim was true, but the artifact offered as evidence was not evidence
for the artifact delivered. T-3 then found the phantom `E4`: a comment with no assertions, which is
why OPS-4 survived a suite built to catch untested lanes.

### Observability Specialist
**Verdict**: RESOLVED-FIXED — O-1..O-16 plus OBS-1..OBS-4, 0 deferred.
Applied the declared-set-vs-emittable-set diff twice; the second application found CQ-16's token
set was four, not three, with nothing declaring it — evidenced by the Lead's own summary naming
three and omitting `UNVERIFIABLE`.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED — CQ-1..CQ-17, 0 deferred. ADR Compliance and Ownership Lens in the
reviewer's verdict; four classification upgrades raised at Gate 1 and honoured before
implementation. CQ-16 was found by *running* the suite after the fixes landed, not by reading it.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED — 1 finding, 1 fixed.
Verified the promotion by measurement: repo-wide grep returns exactly three definitions, all in
`_test_helpers.sh`. Reconciled the "7 vs 8" conversion count — 4 marker sites needed edits, 3
`assert_absent` sites were inherited unchanged, 3 definitions deleted.
**Extraction opportunities** (ADR-0019 lane, not fix-or-defer): two, both in `docs/TODO.md`
§Cross-Service Duplication.

### Operations Reviewer
**Verdict**: RESOLVED-DEFERRED — OPS-1..OPS-4 fixed, 2 accepted deferrals.
OPS-1 is the finding of this gate: driving `INCIDENT-RECORD-UNWRITABLE` proved the lane
**unreachable**. OPS-4 found `--stop-after` falsely refusing tasks 3 and 4 of this story's own
manifest — R-5 inverted.

### Semantic Guard Reviewer
**Verdict**: RESOLVED-FIXED · **Native verdict**: SAFE — 3 findings, 3 fixed.
Four of five checks give no signal on bash workflow tooling and were reported as such rather than
padded. Verified by reverting three defect fixes individually against an isolated tree — an
instrument distinct from token mutation.

### Paired Infrastructure (co-implementer)
**Verdict**: RESOLVED-DEFERRED — 3 findings fixed, 2 accepted deferrals.
Escalated the K1 gap into a class and insisted on a mechanical sweep rather than an argument,
which found `h3` vacuous *outside* the predicted subset.

### Protocol (co-implementer)
**Verdict**: RESOLVED-FIXED — 4 findings, 4 fixed, 0 deferred. Ownership Lens on both
`crates/dt-story/` rows at Domain-judgment, satisfied by the §6.5 paired route.

---

## Accepted Deferrals

- `docs/TODO.md` §Test Debt — M3: guard asserting every `scripts/**/*.test.sh` has an invocation site
- `docs/TODO.md` §Polyglot Pipeline Follow-ups — guard-binary skew + `run_and_emit` rc-collapse
- `docs/TODO.md` §From ADR-0024 §6 Amendment — the §6.2 accepted-vs-contested-upgrade clause
- `docs/TODO.md` §Story Workflow Follow-ups — escalate-lane resume is non-functional (E4b)
- `docs/TODO.md` §Cross-Service Duplication (DRY) — seam-sentinel + git-containment predicate
- `docs/TODO.md` §Cross-Service Duplication (DRY) — four-member `env -i` runner family (do not unify)
- `docs/TODO.md` §Inter-Service Protocol Inconsistency — referent-durability convention (task-4 gated)
- `run-story.test.sh` NOT-COVERED block — `STOP-AFTER-NEVER-FIRED`; `persist_resume_pointer`'s WARN

---

## Rollback Procedure

**The risky hunk is not the valuable one** (@operations 5). This change's largest blast radius is
one line: `layer3.sh` gains an always-run self-test that every devloop and every CI run in this
repo executes. The defect fixes and the R-4 lane split are the valuable part and must not be
collateral if the suite turns out slow or flaky. So the mitigations are ordered by granularity:

1. **Targeted (try this first).** Delete the single
   `run_and_emit "run-story-selftest" "${__here}/workflow/run-story.test.sh" || true` line from
   `scripts/layer3.sh`. The suite still runs standalone
   (`bash scripts/workflow/run-story.test.sh`); every runner fix stays in place. **Trip threshold:
   the suite is budgeted at < 15s** (ADR-0033 §4 holds layers 3+6 to 90s p95, per-layer warn at
   `DEVLOOP_LAYER_BUDGET_SECS:-20`). If it exceeds that, or reds non-deterministically, pull this
   line and open a follow-up rather than reverting the diff.
2. **CI build step.** `.github/workflows/ci.yml`'s `cargo build --release -p dt-guard -p dt-story`
   is independently revertible, but note it fixes a *pre-existing* break (guard binaries come from
   a skip-gated compile verb, so a `packages/**`-only PR reds Layer 3 today) — reverting it
   restores that break rather than removing a new one.
3. **Preflight strictness.** The `dt-guard` + verb-currency assertions in `preflight-story.sh`
   newly refuse a run that succeeds today (fresh container, first task touches `crates/`).
   Independently revertible; see the in-code comment before "fixing" the strictness.
4. **Full revert.** Verify start commit `9807f0b507a5c533580284b2ae86679e1ce29ee6`, review with
   `git diff 9807f0b..HEAD`, then `git reset --soft 9807f0b` (preserves changes) or
   `git reset --hard 9807f0b` (clean).
5. No schema or migration changes; no cluster or manifest teardown. `crates/dt-story`'s new
   `tasks` verb is additive and read-only — nothing persists state through it, so reverting it
   needs no data cleanup, only a rebuild.

---

## Issues Encountered & Resolutions

### Issue 1: The classification guard could not run, on the implementer lane
**Problem**: At Gate 1 the Lead ran `validate-cross-boundary-classification.sh` and it exited **1**
with `REASON=dt-guard-binary-missing` — the *implementer* lane, for a missing build artifact that
`_common.sh:251-253` defines as the operator lane.
**Resolution**: Built the binaries; guard passed. Kept as a **measurement** in the R-4 section
rather than a projection — it is the cheapest possible evidence that the enum gap is real, and it
came from the Lead rather than from the author of the claim. The structural half is deferred.

### Issue 2: Three pipeline runs did not bind
**Problem**: The tree moved underneath the first Gate-2 run (fixes landing), and again later.
**Resolution**: Instituted an explicit freeze protocol — implementer signals "tree final", Lead
samples `git diff HEAD | sha256sum` three times, runs the pipeline, and only that verdict binds.
The implementer subsequently refused to edit through a freeze for ~1h while six findings
accumulated, staging every patch in `/tmp` instead. Three reviewers independently recorded that as
the correct call: *"the fix is small" is the reasoning that erodes a control.*

### Issue 3: The containment proof fired on concurrent teammate edits
**Problem**: `run-story.test.sh` reported `TREE-MUTATED` twice — not an escape, but reviewers
writing `docs/TODO.md` while the suite ran. `run_and_emit` collapses the suite's exit 2 to
`STATUS=FAIL` → rc 1 → **a task escalation recorded against a clean diff**: R-4's own class,
produced by the artifact that enforces R-4.
**Resolution**: Three-state handler (held / violated / could-not-verify) plus a quiescence
pre-flight, and `STATUS=PRECONDITION_FAILURE` emitted so `tee_collect_statuses` worst-wins carries
the operator lane without a `_common.sh` change. The comparison was **not** narrowed.

### Issue 4: Fixes reintroducing their own defect class
**Problem**: Six times. `INCIDENT-RECORD-UNWRITABLE`'s repair made its own report unreachable; the
CQ-16 handler asserted stability on a sample never taken; the assertion added to close the class
was itself blind; and the Lead twice — mandating a control in the vacuous form an hour after ruling
that shape a finding, and rejecting a proposal a reviewer never made.
**Resolution**: Each caught by a second reader or by execution; none by its author. Recorded in
§Lessons Learned unsoftened, with @semantic-guard's bound attached: one gate, one surface, all
caught — the pattern exists and second-reader review catches it, *not* how often it occurs.

---

## Lessons Learned

**The instruments were wrong more often than the code.** The story is about controls that detect
correctly and route or describe wrongly, and every instance found *during* this devloop was in an
instrument rather than in product code:

1. `run-story.sh`'s own recovery comment said timeout was an infra escalation while the code
   escalated it as a task verdict.
2. `record_infra_incident` announced a record path after a write it had masked with `|| true`.
3. The story's Deferred section cleared a pipeline on SIGPIPE reasoning that was correct for
   SIGPIPE and silent about the mechanism that was actually reachable.
4. The runbook's §6.3.1 recovery for `dt-guard-binary-missing` offered "re-run `layer1.sh`", which
   builds nothing in exactly the skip-gated case that produces the symptom — the documented
   recovery worked only where the problem never occurred (@paired-infrastructure).
5. My own plan's classification row said "one line" for a ~15-line change with a new refusal lane,
   and claimed "no gate authority" for a verb my own plan had preflight refuse the run on.

Same shape each time: **a control or claim that reads as covering something it does not.** Worth
naming because the defect class this task exists to fix kept reappearing in the things we use to
find it, including the plan describing the fix and the review of the fix.

**What closed them was a second reader, every time.** The mis-described mitigation I offered during
this review — pointing at a comment asking future readers not to remove a guard — belongs in this
list as a *sixth instance of the class*, but not as a sixth failure of the method: it was named,
fixed, measured, and the coverage gap it exposed was closed with a proof-of-trap, all inside one
exchange. Five instances found by review is a finding about review working; recording the sixth as
"and review failed too" would understate the mechanism that caught the other five. The honest
reading is that **no instance in this list was caught by its own author**, and every one was caught
by someone else reading it — which is the argument for the panel, not against it.

**And they share a second property (@paired-infrastructure), which is the one that makes the class
worth a name**: every instance was written by someone who had *just thought carefully about the
thing they then mis-described*. The timeout comment, the Deferred entry, the §6.3.1 recovery, the
classification row — none are careless. Carelessness would have a cheaper remedy. What these need
is a reader asking "what does this claim *not* cover?", which is why the remedy that worked here
was review pressure and a test suite, not more care.

**"What else produces this same observation?"** — the durable sentence, and the one to carry out
of this devloop: **the observation is consistent with more than one story, and only one of them was
checked.** @operations' and @paired-infrastructure's joint formulation, arrived at after
@paired-infrastructure showed that the narrower version (*"does this case pass if the thing it
tests never runs?"*) clears OPS-1, the instance that started the thread: there the mechanism *did*
run and the file *was* created, and `assert_marker` passed on a zero-byte record. Non-execution is
one alternative producer; execution-that-failed-partway is another. Remedy is uniform: name the
alternative producers, then assert something only the real one yields. Live instances here: K1,
`h3`, OPS-1, and @operations' own `report_results` suggestion.

**A comment is not a case.** `E4` was a `# E4:` heading followed by a fixture restore and nothing
else — no `e4-` label existed; the set ran `e1 e2 e3 e5 e6 e7`. It also encoded a wrong model
("the dep never completes because it is escalated", when `engine.rs` selects `Pending | Escalated`
and *reopens* on selection). So **the one case that would have caught OPS-4 didn't exist, while
reading as though it did, in a suite whose stated purpose is catching untested lanes.** The durable
artifact is the header rule with its grep. @test's generalisation bounds their own technique
honestly and belongs beside it: mutation testing proves the tests you *have* are load-bearing and
says nothing about lanes you never wrote — catching that needs enumerating lanes against the code,
which is how @operations found OPS-4.

**Two classes, one root, and neither was caught by care.** The second is *an instrument that reads
as covering more than it does* — the R-4 comment before scoping, the `--allowedTools` argv pin
versus the real restriction, the Deferred entry that reasoned only about SIGPIPE, and @operations'
proposed check, which would have shipped an instance of the second class inside the fix for the
first. Five of six sightings were made by people who had *just* reasoned carefully about the thing
they then mis-specified — including the Lead, who mandated a control and specified it in the
vacuous form one hour after ruling that shape a finding. **That is the argument for a mechanical
check: a class you can catch by being careful does not need one.**

**Reading found five findings; running found the sixth — twice, by different routes.** The
containment proof's concurrent-writer false positive (CQ-16) existed only in the *running* artifact:
it is invisible in any diff, because the defect is a property of what the check can and cannot
attribute at runtime, not of the text. Six rounds of review — nine reviewers, all reading carefully
— did not surface it; one execution did, and then @code-reviewer reproduced it independently minutes
later on the same tree. **OPS-1 is the same lesson arriving by the other route**: three reviewers
read and approved `record_infra_incident`'s repaired write, and only *driving* the branch revealed
that `set -e` aborted the script before the loud message could print — the fix for a masked failure
had made its own report unreachable. Reading establishes what the code says; only execution
establishes what it does. A review gate that ends without running the artifact has checked one of
those two.

**A verification that cannot attribute its result must say so.** CQ-16's real defect was a
two-state proof — held or violated — for a world with three states, the third being *could not be
verified*. Collapsing "unattributable" into "violated" is not a cosmetic mislabel: under layer 3 it
reaches `run_and_emit`, collapses to `STATUS=FAIL`, arrives at the runner's gate as rc 1, and
escalates a task against a clean diff — R-4's exact class, manufactured by the suite built to
prevent it. Generalises past this file: **a control that cannot distinguish "I checked and it is
bad" from "I could not check" will eventually report the second as the first**, and the direction
it fails in is decided by whichever branch someone wrote first.

**A case whose expected outcome is the null outcome tests nothing** (@paired-infrastructure's
generalisation of K1). The mechanical test is a mutation: break the mechanism the case names — does
the case still pass? Swept across the whole negative-assertion subset here; 21 of 21 rejection
cases were properly anchored, and the one genuine vacuity (`h3`) was **outside** the predicted
subset, which is the argument for sweeping mechanically rather than reasoning about which cases
look risky. `assert_no_marker` is where two distinct vacuity modes meet: a negative assertion is
satisfied by the runner dying early for an unrelated reason *and* by the mechanism being absent
entirely. Hardening against the first (byte-for-byte copies asserting a distinct token) does not
touch the second.

**A filter added to an untested path is a control that never runs.** Closing the `2>&1` finding
meant adding a shape floor to `newest_devloop_output`'s selection pipeline — whose *selecting*
branch had no test, because every case drove it with `docs/devloop-outputs` absent or empty. A
filter rejecting every line would have passed every assertion, since "no resumable output" was
the default expectation everywhere else. Case **K1** now covers it, verified by proof-of-trap
(pattern broken to `^NEVER-MATCHES ` → K1 fails; restored → 115/115). Fixing a control is not done
until the path it guards is exercised.

**Failing-first earned its cost twice over.** Both unplanned defects — the silent `exit 1` through
no lane, and `jq -e` returning 0 on empty input inside my own classifier — were found by running
the suite against unfixed code, not by review. Neither was in any reviewer's list, including mine.
The second is the sharper lesson: @code-reviewer measured the `jq -e` behaviour against
`preflight-story.sh`, and I then wrote the identical pattern into `canary_classify` before that
finding had landed. A finding is not absorbed until it has been checked against every site it
could apply to.

**A count is not an invariant.** The fixture-budget exchange (@semantic-guard) turned on two
hazards sharing one pipeline with opposite thresholds, where the reassuring number closed one and
opened the other — and the opened one fails *stably*, so rerunning cannot detect it. Where a
property is what matters, state the property; a count that happens to imply it today will stop
implying it silently.
