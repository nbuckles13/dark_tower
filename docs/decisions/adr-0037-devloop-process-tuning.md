# ADR-0037: Tuning the /devloop Process

## Status

**Proposed — validating on ADR-0036 story 2.**

This ADR is a trial. Each decision below is applied to the machinery now and
exercised on the next user story (ADR-0036 story 2) before it is ratified. The
ADR is promoted to **Accepted** — with per-decision keep/revert/revise notes —
at that story's close. Agent-scope consolidation is deliberately **out of
scope** (see §Deferred); it is the one change large enough to want its own
evidence, and this ADR gathers that evidence first.

**Date**: 2026-09-16

## Context

The ADR-0036 story-1 run (`hear-yourself-through-handler`, 26 tasks) cost
**~$5,682 / 2.8M output tokens / ~125 API-hours**. The team's current bottleneck
is **output tokens under a rate limit**, not wall-clock time — so the goal is to
lower the **token cost of each /devloop without sacrificing quality**. The
premise this ADR tests: there is ceremony in the current process that produces
tokens and almost no shipped-byte changes, and cutting it is free. If that
premise is false for a given cut, the cut should cost nothing to abandon — hence
the trial framing.

Cost was the entry point, but the trial grew to carry the **work-sizing and
consistency discipline** the cost work exposed — how devloop work is cut and
kept coherent (D3, D9), not only how much ceremony it carries. Hence the rename
from the original "Lower Token Cost" title.

### What the story-1 evidence shows (the basis for every decision here)

A structured read of all 26 devloop `main.md` Gate tables established:

- **Gate-3 (code review) is where ~90% of shipped-byte-changing catches happen**
  — ~500+ actionable Gate-3 findings vs **~50–70** at Gate-1 (planning). Gate-1's
  value is real but **concentrated in contract/plan-shaping tasks** (e.g.
  `internal-contract-reshape` caught 6 plan defects *before code*); on tasks with
  no contract surface, Gate-1 was a unanimous rubber-stamp ~4 times in 5.
- **The most-substantive catch came from a reviewer who *executed* something in
  ~18–19 of 24 non-placeholder loops** — ran the binary, mutation/perturbation,
  deletion-to-prove-non-vacuity, enum-exhaustive walk, byte-diff vs upstream,
  negative-control compile. Read-only wins clustered in doc/contract loops.
- **A defect class recurs that no automated gate can catch: "a control or claim
  that reports clean over something it never checked"** (≥8 loops). No layer
  catches it because the layer *is* the thing reporting clean. This is the
  strongest argument for keeping *executing* human/agent review, and the reason
  the cuts below trim ceremony rather than review depth.
- **The load-bearing reviewers are security, observability, operations, dry**
  (~19–22 of 24 loops each). Service specialists mostly performed
  not-touched verification — *except when they owned the changed code*, where
  their value spiked (e.g. `deny-policy`: infrastructure's Gate-1 *absence* let a
  deferral be mispriced). "Owner-on-owned-change" is load-bearing; broad
  cross-boundary ceremony is not.
- **The cross-boundary ownership machinery is ~82% ceremony**: of classified
  cross-boundary rows, only **~12% are Domain-judgment** (where owner review can
  catch something); ~30% Mechanical, ~22% Minor-judgment, the rest own-files.
- **Semantic-guard is disciplined but low-yield**: ~4 real catches in 24 loops,
  never drifted past `checks.md` in a way that shipped a bad check. (The parked
  brief's claim that it "authored an inverted check" was checked against the
  records and is wrong — on `mc-kek` it *caught* an inverted check `@security`
  had drafted into `checks.md`.) So it is a **cost** question, not a misbehavior
  question.
- **Evidence gap**: the per-task cost ledgers did **not survive** the run —
  `run-story.sh` wrote them to the container's `/tmp`, which was lost with the
  container. Only the headline totals remain. This makes the fixed/variable cost
  split un-measurable for story 1 and is itself a decision below.

## Decision

Nine changes, each with what it keeps, what it drops, and the fallout it accepts.
All are trial changes under the Status above.

### D1 — Trim the cross-boundary ownership ceremony to the load-bearing slice

The ADR-0024 §6 ownership lens is applied to *every* file touched outside the
implementer's domain, at three loci (Gate-1 classification table, Gate-3
per-reviewer ownership-lens verdict, close-story ownership-lens retrospective).
~82% of that rides on Mechanical/Minor edits that produce ~no owner catches.

- **Keep**: one lightweight rule — *the implementer may edit **any file** in a
  single devloop; a Domain-judgment cross-boundary edit pulls the owning
  specialist into **planning and review** (e.g. `--paired-with`), and a Guarded
  Shared Area edit additionally pulls **security**.* This is the deliberate change
  from ADR-0024's **owner-implements**: there is no spin-out to a separate owner
  devloop and no re-assigning the implementation — the owner's domain knowledge is
  applied via plan + review, which keeps a cross-domain increment as one task
  (serving D3) while preserving the ~12% Domain-judgment slice that buys anything
  (owner-on-owned-change). The upgrade path (Mechanical/Minor → Domain-judgment)
  is the backstop.
- **Drop**: the classification rows, co-sign trailers, and Gate-3 ownership-lens
  verdict-audit for Mine / Mechanical / Minor-judgment edits; and the
  **close-story ownership-lens retrospective** entirely (it is a meta-audit *of*
  the classifications — with nothing to classify, nothing to audit).
- **Fallout accepted**: replacing a forced 4-way table with a judgment rule means
  an implementer could under-call a Domain-judgment edit and skip the owner. The
  always-on quartet (security/obs/ops/dry) remains on every panel as a backstop,
  and Gate-3 still runs. The rule stays a single guard/prompt line, never a
  reinstated table.

### D2 — Risk-tier the Gate-1 planning round (Lever A)

Full Gate-1 is a panel plan-round on every task; it rubber-stamps the ~80% with
no plan surface.

- **Decision**: run the full Gate-1 panel round only when a task, by its
  **manifest fields**, has a plan surface — *any* of: delivers `proto`/touches a
  wire contract or public API; introduces or moves an architectural seam; spans
  >1 service/specialist; or is author-flagged ambiguous. Otherwise the
  implementer plans inline and proceeds to build; Gate-3 backstops.
- **How/when the call is made**: mechanically, from the `dt-story` manifest's
  existing deliverable-type + deps fields, **at `/user-story` decomposition
  time** (when the author already reasons about contract surfaces) — not a
  per-task judgment by the devloop Lead at run time. The author may only
  **downgrade** a task's tier with a recorded reason; the default errs toward
  full wherever the contract signal exists.
- **Fallout accepted**: a task mis-tiered "simple" that hid a contract surface
  loses the *early* (cheaper) catch, not the catch — Gate-3 still finds it. Rare
  by selection.

### D3 — Size tasks by demonstrable increment, not seam (Lever B)

Story 1's 26 tasks were **seam-cut**. The per-task **fixed overhead** (team
spawn + plan round + a Gate-3 panel of 7–10 reviewers each re-reading context) is
paid once per task regardless of size, so seam-cutting multiplies it.

- **Decision**: size a task as *the largest independently-verifiable behavioral
  increment that completes in one devloop session* — the user-story
  "demonstrable slice" principle applied one altitude lower — not the smallest
  seam. An audit of story 1 under this lens bundles **26 → ~14–16 tasks**.
- **Ceiling (where the balance lives)**: a task must fit one session's working
  context *including its Gate-3 resolution*. ~8 of story-1's tasks were already
  one-session-sized (e.g. the SDK audio pipeline) and must **not** grow;
  the recoverable waste is the ~10 small seam/guard/infra micro-tasks split for
  architectural cleanliness (the transport *seam* split by ADR ordering; infra
  micro-guards; telemetry micro-guards; proto reshapes).
- **Fallout accepted**: larger tasks raise per-task context and failure
  blast-radius. Bundling stops at the one-session ceiling. Note also that part of
  story 1's 26 was *discovered scope* (the late-added routing-correctness
  cluster), which better upfront planning — not larger tasks — addresses.

### D4 — Make semantic-guard conditional, not a standing seat

Semantic-guard returns ~4 real catches in 24 loops at the cost of a full reviewer
seat per task.

- **Decision**: spawn semantic-guard **only when the diff plausibly touches a
  check surface** in `scripts/guards/semantic/checks.md` (payload `Debug`/
  `Display`, credential/secret types, comment-vs-code drift, etc.); otherwise
  skip it for that task. Do **not** rebuild it as scripted per-check model
  infrastructure — that is machinery this ADR is explicitly not investing in.
- **Fallout accepted**: a check-surface heuristic that under-fires could miss a
  catch semantic-guard would have made (~1 per 6 tasks historically); the quartet
  backstops, and the heuristic keys on the same surfaces `checks.md` names.

### D5 — Persist the story-runner cost ledger outside the container

The story-1 ledgers died with the container because `run-story.sh` wrote
`RUN_DIR` under `${DEVLOOP_TMP:-/tmp/devloop}`.

- **Decision**: default the ledger/run-dir to a **host-persistent, git-external**
  path (`~/.cache/devloop/story-runs/<story>/`), not the container's `/tmp`.
  Without this, every future cost analysis dies with the container and the
  validation of *this ADR* (D2/D3 depend on before/after token measurement) is
  impossible.
- **Fallout accepted**: none material; it is a `RUN_DIR` default change plus a
  loud failure if the path is unwritable (never a silent fall-back to `/tmp`).

### D6 — Fold in the already-designed recovery lanes

Three parked, already-designed mechanics that reduce re-spawn cost (each recovery
today re-hydrates a ~100-turn transcript):

- **(e)** the runner/guard fixes already landed mid-story (`--revalidate`/
  `--restart`, scope-guard exemptions, per-instance env-config discovery) —
  documented, not rebuilt.
- **(f)** terminal-phase recovery: a Gate-3-close **commit-intent** checkpoint +
  a model-free `run-story.sh --finish` lane that re-runs the gate and commits the
  recorded intent with no model turn.
- **(g)** interactive-rescue: a `run-story.sh --interactive` retry mode that
  spawns `claude` attached to the operator's TTY, then falls through to the
  unchanged gate/commit/complete machinery.

These are mechanical and low-risk; they are included so the trial exercises them,
but they do not change review shape.

### D7 — fmt auto-fixes locally, checks in CI

The rust fmt layer runs `cargo fmt --all -- --check` (`scripts/lang/rust/fmt.sh`),
so a purely-mechanical formatting miss fails the gate and costs the implementer a
manual fix or a re-run — for a deterministic transform that carries no signal.

- **Decision**: the fmt layer **applies** the fix and reports what it changed when
  run locally (devloop / run-story), and stays **`--check`** in CI (gated on the
  `CI` env var). Same split for the TypeScript fmt layer.
- **Why safe (not a masking violation)**: formatting is deterministic and
  semantically empty, so auto-applying it hides no real failure — unlike
  auto-fixing lint, which could. CI stays read-only and fails loud, so a
  formatting miss is still caught where auto-fix runs (locally).
- **Note**: this reverses an earlier deliberate "check-only per code-reviewer #2"
  choice; the reversal is scoped to local runs and carries this rationale.

### D8 — Point-of-failure log directive in the pipeline aggregator

On a layer failure the failing sub-check's detail is **already captured**
(`run-guards.sh` names the failed guard and prints its first error lines;
`layer-all.sh` writes it to `${DEVLOOP_TMP}/layer-N.log`), yet a model seeing only
the aggregate "Layer N FAILED" re-runs the whole layer to get detail it already
has — pure token/time waste.

- **Decision**: on a layer FAIL, `layer-all.sh` prints an explicit, self-locating
  directive in its own output — naming the `layer-N.log` file **and** the
  anti-pattern ("read the log to triage; do NOT re-run the layer to capture
  detail, it is already there"), optionally with a 2–3 line grepped teaser of the
  `FAILED`/`VIOLATION` lines. The instruction lands at the point of failure, in the
  output the model is already reading — harder to miss than a SKILL line, which is
  kept only as a demoted backup.
- **Optional enhancement**: `run-guards.sh` emits a structured `FAILED_GUARDS=<names>`
  line `layer-all.sh` can surface, so the summary names the exact guard, not just
  the log — not required for the core fix.

### D9 — Cut work by invariant; never ship a partial invariant

The recurring failure this repo has fought for a long time: devloops "stay in
their own lane," and the accumulation is a mis-mash of inconsistencies — a rule
established in some places but not others (the polyglot fmt lane exposed at the
D7 devloop is the canonical case: rust fmt applied locally, the TS "fmt layer"
formatted no TypeScript, proto was double-formatted). Each devloop was locally
correct — bounded, in-scope — but a half-applied rule is **worse than an
unstarted one**: it is a landmine a later reader steps on, filed in a TODO that
rots. That debt grows exactly as unbounded as task size does; it is just moved to
a ledger nobody watches. This is the negative twin of D3 (D3: don't cut too
small; D9: don't leave gotchas), and D1 is what makes it affordable (the
cross-owner "extra work" is now done in-loop, not deferred).

- **Decision — cut by invariant, not by lane or file-locality.** The unit of
  work is "establish this consistency rule *everywhere it applies*," which is
  naturally bounded (one concept) and complete (no partial). Cutting an invariant
  by lane/language/service is the seam that manufactures the mis-mash.
- **Never ship a partial invariant.** If the whole rule genuinely cannot fit one
  devloop, do **not** dribble it — **hold the behavior change entirely** and keep
  the *consistent* status quo until every instance can flip at once. A
  consistent-but-less-convenient state beats an inconsistent-but-more-convenient
  one, because the inconsistency is the expensive part.
- **A necessary consistency deferral gets a forcing function**, not a passive
  TODO — a guard that fails, or a gate that blocks — because passive TODOs are
  *how* the mis-mash accumulated (CLAUDE.md "add a guard that fails on drift").
- **Three implementation homes**: (1) `/user-story` decomposition — cut tasks by
  invariant; a task establishing a rule owns every instance of it, and an
  invariant too big for one session is *one* deliberately-sized task, never
  lane-splits. (2) `/devloop` lead — on finding an instance of the invariant
  outside the stated scope mid-loop, default to completing it in-loop (D1 pulls
  the owner in), fallback to hold-complete, never ship-partial. (3)
  `review-protocol` — **DRY** carries the explicit Gate-1 charge ("what invariant
  does this establish, and does the plan cover every instance?"), and a shipped
  partial invariant is a Gate-3 finding for **any** reviewer → ESCALATE (the
  backstop, including for light-tier tasks that skipped Gate-1).
- **Fallout accepted**: "cover every instance" can push a devloop larger, up
  against D3's one-session ceiling. The reconciliation is that the bound is the
  *invariant's own scope*, not a file count — cut by invariant and the task is
  both bounded and complete; when an invariant is genuinely too big, schedule it
  as one complete devloop rather than several partial ones. Story 2 tests whether
  "always complete the invariant" is affordable or needs a carve-out.

## Deferred (not in this ADR)

- **Agent-scope consolidation ("fewer agents, larger scopes").** The crux, and
  the highest-leverage change — but it wants real data. The evidence already
  frames the tension: the always-on quartet earns its seat, but service-specialist
  value is *conditional on owning the changed code*, so consolidation must
  preserve owner-on-owned-change, not merely cut headcount. Revisited after story
  2, with this ADR's measurements in hand.
- **Change-scoped Layer 7.** L7 dominated *gate time* (~8 min) but is a
  **time** lever, not a token lever; with tokens as the bottleneck it is not
  worth the change now.

## Consequences

- **Lower per-task fixed overhead** (D2/D3) and fewer standing seats (D1/D4) cut
  output tokens on the tasks that were paying for ceremony, not review.
- **Review depth is unchanged where it earns its keep**: Gate-3, the executing
  quartet, and owner-on-owned-change all remain. The cuts target ceremony, not
  the catches.
- **The trial is measurable** (D5): story 2's per-task token/turn ledger is
  compared against the story-1 headline baseline; a decision that does not move
  tokens, or costs a caught defect, is reverted at ratification.
- **Risk is bounded by construction**: every cut leaves Gate-3 and the quartet as
  backstops, and every "skip" (D2 tiering, D4 conditional) errs toward running
  when the signal is present.

## Validation plan (story 2)

1. `run-story.sh` writes a persisted ledger (D5) from task 1.
2. Story 2 is decomposed under D3 (demonstrable-increment sizing) with D2 tier
   tags in the manifest.
3. At close, compare: output tokens/task, Gate-1 vs Gate-3 catch counts, and any
   defect that reached demo/manual-test that a cut would have caught earlier.
4. Promote to Accepted with per-decision keep/revert/revise notes; open the
   consolidation question with the numbers.

## When to Revisit

- At ADR-0036 story-2 close (ratification + consolidation decision).
- If any single decision demonstrably lets a real defect through, that decision
  is reverted immediately rather than at close (fail-loud, per CLAUDE.md).

## References

- ADR-0024 (agent boundaries / ownership lens) — D1 narrows its application.
- ADR-0033 (polyglot validation pipeline) — L7 deferral references §4.
- ADR-0035 (deterministic story runner) — D5/D6 edit its runner.
- `docs/TODO.md` §"Devloop & Team Structure Review" — the cold-start brief and
  evidence pointers this ADR acts on.
