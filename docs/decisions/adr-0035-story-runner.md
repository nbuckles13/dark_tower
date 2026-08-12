# ADR-0035: Deterministic Story Runner (`run-story`)

## Status

Accepted (2026-08-10)

## Context

The devloop workflow (ADR-0024) ships product but requires a human at **every task boundary**: spawning each task's devloop, monitoring for derailment, coordinating mid-story fixes, merging branches back. The goal is to lift human interaction from **per-task** to **per-story** — plan a story, then come back to it done or to a small queue of genuine decisions.

This continues the ADR-0016 → 0021 → 0022 → 0024 trajectory, each rebuild moving trust out of model-followed instructions and into deterministic enforcement. `run-story` completes that arc: **the orchestration layer stops being instructions and becomes a program.**

Validated by **Run #1** — the pending tasks of the `browser-client-join` story, chosen as real work on a mature codebase with a known baseline. Measured cost, from the run's own ledger: **10 tasks, $415.97 total, mean $41.60, median $37.26, range $14.01–$76.36**. Per-task cost varies ~5× and is not predictable from task size; treat any per-task figure as a mean, never a planning band.

### Constraints that shape the design

- **Quota, not compute, binds throughput.** Measured: 17.1h of work against 106.4h of inter-task gap — the runner was **14% working, 86% waiting**.
- **The devloop session dominates per-task cost.** Gate time is 14.2% of an `env_tests` task and 4.4% otherwise; ~1–2% of story wall-clock.
- **The execution substrate is experimental** — agent teams inside print mode, documentation-silent, with no supported alternative (see §8).

## Decision

### 1. Deterministic runner, no LLM in the control flow

`scripts/workflow/run-story.sh` walks the task manifest; `crates/dt-story` owns all manifest reads/writes. Every decision keys on an exit code or a manifest field, never on model judgment. Because no model sits in the control flow, a structured event log is a faithful record rather than a model's narration of itself.

### 2. Serial, single-branch, single-container — permanently

One container (ADR-0025), one story branch, tasks in dependency order, committing in place. This is a **correctness property, not a v1 simplification**. Three independent reasons, no shared failure mode:

- **Quota binds first.** Parallelism attacks the 14%; even unbounded concurrency saves at most 17.1h of ~123h elapsed.
- **Layer 7 is structurally hostile.** Each branch needs a live Kind cluster; concurrent rebuilds make layer 7 flaky by construction, and a flaky gate under unattended operation is worse than a slow one.
- **Divergent bases manufacture defects.** Parallel branches mean reviewers review against divergent bases — the shape of the #63 defect, at scale.

Serial also enables cluster reuse across consecutive tasks, which parallel branches forfeit.

**Revisit only when both hold**: quota is no longer the binding constraint (API-billed execution, or a materially larger window) **AND** a per-task layer-7 cluster-isolation story exists. The second is a capacity question — do not redesign a helper that is not what is in the way.

**Head-of-line blocking is solved without parallelism.** Add opt-in `--keep-going` (default off): on escalation, record it, skip that task and its dependents, continue with dep-satisfied tasks, exit 1 at the end with the queue. Requires `dt-story next --skip-escalated` (filter to `Pending` only — `next` deliberately re-selects `Escalated` tasks and would otherwise loop within one invocation) plus one runner branch; the dependent cascade falls out of the existing deps filter.

### 3. Pass/fail authority is the runner's own pipeline run — never the devloop's verdict

Fast floor (layers 1–6) per task; layer 7 when the task is `env_tests`-tagged; full `layer-all.sh` at story close.

**The rule exists because self-reported success has failed before, in two distinct ways** — both cases where a gate said PASS while the work it was gating had not happened:

- **A layer reported success without running its tests.** A stubbed or unimplemented layer emitted a green result indistinguishable from a real one, so the pipeline was green over tests that never executed.
- **A skip at a pipeline edge was rationalized as deliberate.** An unexpected `SKIPPED-NO-VERB`/`UNKNOWN` from a wrapper was accepted as an intentional no-op rather than a failure, so a layer that did not run reported no problem. Closed by a wrapper `EXIT` trap plus dispatcher exit-code tightening (`docs/devloop-outputs/2026-06-08-silent-skip-class-fix-task50/`).

Both are *absence* presenting as *success*, which is why the runner re-executes rather than reading a result.

**The Gate-2 verdict artifact is not consumed as authority, at any granularity.** Consuming it converts the runner's independent verdict back into the devloop's self-report in structured form — format does not confer independence, since the artifact is produced inside the session being audited.

**Governing rule for any future proposal to read the verdict:**

> **Read the artifact only to add suspicion, never to subtract verification.**

A proposal is admissible if and only if it can only ever *increase* the set of things that fail. No carve-outs — a rule with a carve-out is one someone later widens.

Three admissible uses, all adopted:

- **VERDICT-MISMATCH cross-check.** Read `GATE2` + `LAYER` lines before re-running; if the verdict claims PASS and the runner's re-run goes red, emit `STORY_RUN: VERDICT-MISMATCH` as both a `slog` line and a journal event. Payload: `{task, verdict_head, runner_head, claimed, observed_rc, first_red_layer, verdict_layers, runner_layers}`. Trigger requires all of: verdict exists and parses; `verdict.HEAD == head_before`; verdict claims PASS and the re-run went red. **Never gates.** It is the only mechanism that would *detect* a bad verdict rather than merely be immune to one.
- **Freshness check.** `verdict.HEAD == head_before` (`run-story.sh:254`). `GATE2_VERDICT_FILE` is a single mutable path every task overwrites, so this is a precondition for reading the artifact for any purpose.
- **Commit-completeness assertion.** Assert the task commit contains a devloop main.md at `Phase=complete`; today the runner checks only that HEAD moved. **This is an honest-mistake control, not anti-forgery** — it is `--no-verify`-bypassable and slug-conjunct-gated. Do not over-trust it.

**Record the runner's own per-layer results**, not the artifact's. The runner already loops layers with each `rc` in hand and discards all but the first failure.

**Standalone detector, independent of the above**: layer-4 target counts of `passed=0` inside a `PASS` verdict are the signature of the first failure class above — green over tests that never ran. **The re-run does not cover this class** — a re-run that also runs zero tests is equally green. This is the one thing the artifact provides that independent execution does not.

**Implementation constraint**: `_gate2_binding.sh` requires the verdict to stay ephemeral and uncommittable, refusing to write if `DEVLOOP_TMP` resolves inside the worktree. The runner may parse values out; it must never archive the artifact into the tree.

### 4. Manifest as state SSoT

A `dt-story` task-metadata block in the story markdown: id, status, specialist, `env_tests`, deps, self-contained prompt, tag. Verbs: `next` / `complete` / `escalate` / `validate` / `add-task`.

`complete` is idempotent — the headless devloop marks its own task before the runner does (double-writer, not error). Escalated tasks are retryable: **rerunning the runner is the resume gesture.**

**A story file carries exactly two task artifacts, with no overlapping fields.** Today it carries three, and the same task data appears in all of them — `specialist` and `deps` three times, `status` and the devloop prompt twice. That is the drift this decision exists to end:

| Artifact | Holds | Status? |
|---|---|---|
| **§Ordered Task List** | human-readable intent: what the task is for, and which requirements it covers | **No** |
| **§Task Metadata (`dt-story` manifest)** | everything machine-read: id, status, specialist, `env_tests`, deps, prompt, tag, devloop-output slug | **Yes — the only place** |
| ~~§Devloop Tracking table~~ | *(deleted)* | — |

**Delete §Devloop Tracking.** Every column it carries is either already in the manifest (invocation, specialist, deps, status) or belongs there (devloop-output link — the runner already records the slug). It is hand-edited, and it demonstrably drifted during Run #1: duplicate rows for tasks 18/19/20 and a conflicting status for 20, found during backfill.

**The Ordered Task List keeps no status column and duplicates no manifest field.** Prose descriptions and machine state have different edit rhythms — descriptions are written once at planning, status changes on every task — and a table that mixes them will drift on the fast-moving column. Where a reader wants both, `dt-story` can render the join; the file should not store it twice.

Consequence for `/close-story`: completeness is `dt-story next` exit 3, never a table read.

### 5. Container boundary is enforced, not documented

`preflight-story.sh` hard-fails outside a container (`/run/.containerenv` ∪ `/.dockerenv`), with a named `STORY_RUNNER_ALLOW_HOST=1` escape hatch. Runtime-written sentinels, deliberately: a baked marker exists only after a rebake, so landing the guard before the rebake would fail every run closed — the image-bake coupling that caused the task #64 idle-death. The hatch is deliberate; a gate with no override gets commented out.

The runner's entire safety argument rests on this boundary, because the devloop runs with `--dangerously-skip-permissions`. `CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS` is written to `settings.json` only in-container and exported per-invocation otherwise.

### 6. Failure classification

On nonzero devloop exit, a minimal haiku canary probe classifies into `healthy | session-limit | auth-expired | infra`. Healthy → task escalation; 429 → sleep to the advertised reset and resume; auth/infra → loud stop, manifest untouched, resume pointer persisted.

**Classification is field-based only for healthy-vs-failed.** The three-way failure sub-classification falls back to text matching on `.result`; in both Run #1 captures `api_error_status` was null and the prose grep decided the outcome. The `429` field check has never fired in production, and there are **zero** real session-limit captures.

Consequence to fix in §10: a reworded quota message classifies as `infra` and the story **stops instead of sleeping**, defeating the come-back-to-it-done promise. The likely core of the fix is an inversion — **default an unclassifiable `is_error:true` toward the retryable lane**, since sleeping unnecessarily costs a retry slot while dying unnecessarily costs the story.

Also: run the canary with `--allowedTools ""` (keep skip-permissions — a permission prompt would hang it into a timeout misclassified as `infra`).

### 7. Advisory auto-remediation, with a suppression hard-gate

At story close the runner force-runs every `scripts/lang/*/audit.sh` and per red language auto-appends a scoped remediation devloop. Language-agnostic; fails loud on an undeclared-owner red language.

**Any commit that changes `audit-suppressions.toml` stops the run** (`exit 1`, not a log line). Rationale: an audit suppression is an exception to a mechanical gate, and **an unattended runner adding a suppression is a machine adjudicating its own exception** — definitionally the category that cannot be automated.

- Scope is **any** change to the manifest, not only remediation tasks: a suppression in a non-remediation task is *more* alarming, and the manifest has ~3 entries in its lifetime, so the cost is ~zero extra stops.
- Detect **any addition**, not just added `id =` lines — an extended `expires` date is equally a policy decision and is the cheaper path for a model under a fix-over-suppress mandate.
- Watch the SSoT only, not generated derivatives; `audit-suppressions-check.sh` already hard-fails on drift inside layer 3.
- The task is already complete and committed when this fires, so **rerunning the runner is the review gesture** — no ack file, no flag.

### 8. Substrate: float the CLI, harden the probe

Float with a version-cached substrate probe; do not pin. Pinning does not reduce trust in the vendor, only changes when we ingest; ADR-0025 isolation is the blast-radius control; a CLI CVE cuts *for* float. Pinning is also a three-site SSoT task (`Dockerfile`, `entrypoint.sh`, `devloop.sh`), not a flag.

**The Agent SDK is off the table permanently, at any panel size.** Measured: of 1,184 `SendMessage` calls in Run #1, **47% are sibling-to-sibling**, with 278–283 reviewer→implementer — which *is* the finding-delivery mechanism. Parent→child subagents cannot express it. A three-seat panel is as locked in as an eight-seat one, so **substrate risk (§8) and review value (§9) are independent variables and must not be coupled.**

Required probe hardening:

- **Assert restrictions, not only capabilities.** The probe asserts what *can* happen; every control we depend on is a restriction, and restrictions fail open and quiet. Assert that a Stop hook fires and can block, and that a tool absent from `--allowedTools` is refused.
- **Assert roster completeness.** The probe spawns one teammate; a partial roster is a silent review-coverage loss where a review that did not happen is indistinguishable from one that found nothing.
- **Assert delivery, not delivery-with-retry.** The probe currently retries a dropped ping by design, so a lossy channel passes. Send N pings, assert N replies, making loss a rate.
- **The probe cache marker stays per-container-lifetime.** It encodes only the CLI version while the probe validates version + settings state + agent registration; persisting it would assert validation on strictly less evidence than produced it.
- **The CLI update must not race the probe.** `entrypoint.sh` runs it foreground and unmasked. `devloop.sh` skips the update (not the attach) while a run is in flight — the attach path is also the credential-recovery path the AUTH-EXPIRED lane sends operators to. Backstop: assert `claude --version` against the probed version each task, halting to the infra lane on drift.

**Flip to pin when**: two substrate-probe failures within one quarter, **or** any advisory affecting the CLI, **or** any observed change in `--dangerously-skip-permissions` scope.

**Note the real hedge against silent substrate degradation is the mechanical floor, not the probe** — a reviewer that never receives its assignment produces no findings, and absent review output looks identical to "no findings."

### 9. Review panel: route by surface, do not shrink

The panel earns its cost on available evidence. Every seat produced findings on 31–63% of Run #1 reviews against a proposed marginal-yield-zero bar of 90% CLEAR. *(Those percentages are n=13 on a single composition-biased story and derived from verdict labels; **only the floor generalizes** — do not cite an individual rate as a per-seat yield.)*

**Both known escapes were diff-locality failures, orthogonal to headcount.** The credential-lifetime guard's author states it directly: *"A diff-scoped guard goes quiet the moment the offending declaration stops being touched — which is exactly how the motivating defect survived review."* #63 lived in the gap between two diffs. Eight diff-local reviewers cannot catch a defect that is not in the diff; neither can sixteen, nor four. The instruments that work are **full-tree standing invariants** and **system-global acceptance tests**.

**Reviewer yield transforms rather than decays.** Every mechanized gate manufactures exceptions, and exception-adjudication cannot itself be mechanized — 422 suppression annotations in `crates/` alone, plus a whole `dt-guard` module built to detect bad justifications for turning guards off. Guard count does not predict yield: the most-mechanized beat measured second-highest.

Two mechanisms, replacing the headcount question:

1. **Per-role mechanization boundaries.** Each role publishes what its gates cover and what residue only judgment covers, following `scripts/guards/semantic/checks.md`. **Every claimed residue item must cite a specific corpus finding, and the citation must be checkable** — a role asked what it uniquely catches will answer with what it believes, and belief failed twice as a predictor here. Boundary rule: *guards check artifacts in isolation; residue is correspondence between artifacts* — claim↔signal, test↔behavior, catalog↔code. Re-evaluate a role when a guard lands on its beat, not on a schedule.
2. **`review_mode: light|full`** in the manifest, set at `/user-story` planning time, defaulting to full — selecting between the **existing** modes rather than at Lead discretion mid-flight. **Light-mode composition is out of scope**: the measured rates come from full-mode tasks and say nothing about small contained changes. **Precondition**: a reviewer's checklist must become machine-checkable assertions *before* that seat is trimmed, never after. **Invariant**: `review_mode` shrinks the review panel only — the pipeline run, acceptance-test presence check and guards are invariant to it. That the deterministic floor does not move is the only reason a lighter panel is contemplatable.

**Do not run an escaped-defect-rate A/B**: ~1 escape in ~200 devloops needs hundreds per arm, and neither escape was headcount-attributable, so it would measure a relationship the evidence says does not exist. Any future measurement must account for **Gate-1 value** (a large share of reviewer output shapes the plan before code exists and is invisible to Gate-3 tallies) and must count findings, not verdict labels.

### 10. Model tiering: only downgrade reviewers, only on evidence

**The question**: every task today runs the Lead, the implementer and all reviewers on one model (`STORY_MODEL`). Should some of them run on a cheaper one?

**The answer is a qualified yes, and nothing changes yet.** Three rules:

1. **Only downgrade, never upgrade.** Fable bills separately from the Max subscription, so moving *up* costs real money and multiplies by the number of reviewers. Moving *down* saves subscription quota — and quota, not compute, is what binds story wall-clock (§2), so quota saved is wall-clock returned.
2. **Only downgrade reviewers, never the implementer.** The implementer's model determines the quality of the work; a reviewer's model determines the chance of catching a problem. Weakening the author to save money is the wrong trade at any price. `STORY_MODEL` currently sets one model for everyone, so **separating implementer from reviewer is a prerequisite**, not a detail.
3. **Choose which reviewers by what their gates already cover.** Where a deterministic guard already enforces a reviewer's beat, that seat is mostly confirming what the pipeline proved, and a cheaper model suffices. Where the seat's value is judgment a guard cannot encode, keep the strong model. This is the same residue question as §9, so **the residue maps produced there are the input to this decision.**

**Rejected alternative**: downgrading task *classes* that historically pass first try. The causality runs backwards — those tasks may have been clean *because* they were well reviewed, and nothing in the data separates the two.

**Blocked until §11 lands.** No artifact currently records which model ran, which seat produced which finding, or whether a task succeeded or escalated. Without those, any tiering decision is a guess. **Instrument first, tier later.**

### 11. Run record: durable, structured, schema-asserted

The runner already writes a structured per-task cost ledger. Extend it with **`model`** (without it, subscription-covered and incremental spend are indistinguishable), **`specialist`**, **`outcome`**, **`slug`**, **`wall_clock_s`**, **`gate_rc`/`gate_elapsed_s`**. Emit one event per canary classification **including the currently-silent `healthy` branch**, and one per session-limit sleep carrying its **computed absolute wake time** and `reset_source`. Move the story-level rollup into a `trap … EXIT` so escalated runs stop producing no cost data.

`RUN_DIR` needs a durable, config-driven home (`STORY_RUN_DIR`) — required especially for the escalation and infra lanes, which exit without committing and therefore leave nothing in `docs/devloop-outputs/`.

**Governance boundary**: the run record is an **evidence artifact, not a monitored signal**. It has no dashboard, alert, SLO or on-call, and ADR-0032's metric-governance regime does **not** apply. The single hook is that the test suite (§12) asserts the journal's field schema, so emitter and schema cannot drift.

**Retention**: derived scalars are promotable to `docs/devloop-outputs/<slug>/`, git-tracked. **Raw stream-json transcripts and canary responses stay ephemeral and gitignored, permanently** — not because they are known to contain credentials (a scan of Run #1's 70 files found none) but because **their contents are uncontrolled**: a devloop that debugs an auth failure or echoes an env var captures it verbatim into 6–11 MB nobody re-reads before attaching it somewhere.

**Per-story budget ceiling**: a loud stop, not a throttle. One counter against the ledger, set at ~2× the planned estimate (a 14-task story at the measured median is ≈$520, so order $1,000), exiting like the INFRA lane so rerunning is the resume gesture. Depends on the ledger being **durable and parseable**, and on `outcome`, or it cannot distinguish a costly success from a costly failure.

### 12. The runner requires a test suite before further change

`run-story.sh` holds canary classification, a session-limit sleep lane, resume-pointer persistence, auto-remediation with an in-loop commit, and `git reset --hard` + `git clean -fdq` reachable from a string-grep classification decision — with zero tests, against `dt-story`'s 26.

**Seams first, then suite** — a naive suite written today would be destructive, since the runner computes `REPO_ROOT` from `BASH_SOURCE` and always operates on the real repo, and calls `preflight-story.sh` unconditionally.

- **Seams**: repo-root override (the destructive-path containment, non-negotiable), `DT_STORY`, layer-script dir, preflight bypass, audit-glob root — `DEVLOOP_TEST`-gated and rejected under `GITHUB_ACTIONS`, with a seam-inertness case per the `layer-all.test.sh` precedent.
- **Suite**: `claude`, `sleep` and `date` are external commands, so PATH-injection under `env -i` covers the canary lane, sleep math and clock control with zero source changes. Wire into `layer3.sh`. Hermeticity is not novel work — `layer7.test.sh` already pins a live-cluster layer with a fake `dev-cluster` on PATH.
- **Extraction of the four control-flow functions is a follow-up under green tests**, not a prerequisite: extracting untested code is a refactor without a net.
- Also in scope: de-prosing the canary lanes; the substrate probe; the §7 gate; journal schema assertions; and a review pass asserting the recovery paths' comments match the messages they print (a test suite cannot catch a wrong comment, and two were found misdirecting operators mid-incident).
- **Canary fixtures must carry all three `api_error_status` shapes** — present-with-429, present-with-null, absent. Production emits present-with-null; `jq`'s `//` collapses null and missing today, so a fixture omitting the key looks like it covers the real case and would stop covering it the moment anyone hardens that check.

### 13. Acceptance criteria must be machine-assertable or explicitly annotated

A prose ban is an instruction followed by a model — exactly what this ADR's lineage replaces with enforcement. Homed in `dt-story validate`, already invoked by preflight on every run.

Each task carries an `acceptance` list whose entries take **one of three forms, never free prose**: `passes_after: <task-id>`, `assert: <test-id>`, or `manual: <reason>`.

**No test names are invented at planning time.** `passes_after` names a **task**, not a test, and that is the normal planning-time form. It is knowable when planning: *"this task's behavior is proven by the assertion that lands in task N."* Concretely, the story's acceptance tests are written by an early task and committed **red** with expected-fail annotations, so the suite stays green-runnable all story; each red assertion carries the id of the task that will turn it green. `assert: <test-id>` is for a test that **already exists** — an existing E2E that must keep passing — and is the exception at planning time, not the rule.

**Validation is lifecycle-aware, which is what makes this work**:

| Checked at | Assertion |
|---|---|
| **Planning** (`/user-story` writes the manifest) | 1. Every task has ≥1 entry.<br>2. No entry is free prose — *this is the "ban manual verification note" made mechanical*; choosing `manual:` becomes a visible, deliberate act rather than the path of least resistance.<br>3. Every `passes_after:` names a task that exists in the manifest and is not already complete.<br>4. `manual:` has a non-empty reason; validate reports the count so they stay reviewable. |
| **Task completion** | 5. Every `assert:` on this task resolves to a test that exists in the tree. *(Enforced here, not at planning — at planning the test may legitimately not exist yet.)* |
| **Story close** | 6. **No `passes_after` is unsatisfied.** `dt-story next` does not reach exit 3 with a dangling promise. |

(3)+(6) are the crux: #63's criterion was *"manual verification note in main.md; automated assertion lands in task 60"* — a promise, tracked as an unchecked TODO box, in a task marked complete. Under this guard that is a `passes_after: 60`, task 63 may still complete, and **the story cannot close until 60's assertion is green.**

(4)+(5) are the crux: #63's criterion was *"manual verification note in main.md; automated assertion lands in task 60"* — a promise, tracked as an unchecked TODO box, in a task marked complete. Under this guard that is a `passes_after: 60` and the story cannot close until it is green.

**The flow, and who flips the annotation.** An early task writes the acceptance test and commits it **red** under an expected-fail annotation, so the suite stays green-runnable while the assertion is unsatisfied. The implementing task later makes it pass **and removes the annotation in the same commit**.

**No runner machinery un-marks it, because the annotation must be self-clearing.** An implementer who forgets to remove it reds their own gate immediately, so the flip cannot be silently skipped. That is a stronger guarantee than a runner-side un-mark, and it needs no new code.

**This is a property contract, not one language's annotation** — acceptance tests here are both TypeScript (Playwright) and Rust (`crates/env-tests` at layer 7), so the mechanism must be stated per language. Any expected-fail marker used for `passes_after` must have all three properties:

1. **Green-runnable while red** — the suite passes with the assertion unsatisfied, so the story stays runnable.
2. **Self-clearing** — the marker *fails* once the behavior lands, forcing the flip.
3. **Proves the test can fail** — a vacuous assertion is caught the moment it is committed.

| Language | Marker | Notes |
|---|---|---|
| TypeScript / Playwright | `test.fail()` | Fails the suite if the test passes. |
| Rust (unit + `env-tests`) | `#[should_panic(expected = "<distinctive assertion text>")]` | `cargo test` fails with *"test did not panic as expected"* once the behavior lands, and a vacuous test that never panics fails immediately. Matches the existing house convention — every `should_panic` in the tree already carries `expected =`. |
| Rust — **not** `#[ignore]` | — | **Fails properties 2 and 3**: an ignored test never runs, so it neither clears itself when the feature lands nor demonstrates it can fail. `#[ignore]` is reserved here for `unimplemented!()` stubs, which `dt-guard test-rigidity` already keys on. Do not overload it. |

**Rust-specific caveat**: `should_panic` is satisfied by *any* panic matching the substring, so a setup `unwrap()` on a `None` can masquerade as a correctly-red acceptance test. The `expected =` string must be distinctive to the assertion itself — not a generic message another failure could produce.

**The hazard this creates, and its partial guard.** The implementing task must edit a test file it did not author, which is the classic route to "make it pass by weakening the assertion." `ts-no-test-removal` (TypeScript only — Rust has no equivalent) catches deletion of a test *file* but is **file-deletion-only in v1** — the block-count heuristic is deferred — so weakening an assertion in place is not mechanically caught. Therefore: **a `passes_after` task's licence to touch the test file is limited to removing the named annotation.** Any other change to that file is a review flag, and this is the one place where the panel is load-bearing rather than confirmatory.

**Committing red is itself a test of the test.** A test that does not fail against unimplemented behavior is proven vacuous the moment it is committed — which test-after never reveals, because a vacuous test passes and looks fine. This is the main reason the inversion is worth its cost, and it is a check the previous ordering could not perform at all.

**What must be knowable when the test is authored — and the altitude rule that follows.** Acceptance tests assert **user-observable behavior through stable seams**, not internal shapes: *"a signed-in user joining a meeting sees their registered display name in the roster"* is authorable at planning without knowing how any service plumbs it. If an assertion cannot be written without first knowing an internal API shape, **the assertion is at the wrong altitude** — that is unit/integration territory owned by the implementing task, not story acceptance. And if the user-observable behavior itself is genuinely undecided, acceptance-tests-first is premature: the story needs a design task, and `manual:` with a stated reason is the honest interim.

**Stated limit**: this closes the *bookkeeping* hole, not the *adequacy* hole. It cannot judge whether a test asserts the right thing, and a test that is too **weak** still escapes — the same class as having no test. That remains reviewer judgment.

### 14. Exploratory tooling needs no ADR; promotion to load-bearing does

> **Exploratory tooling needs no ADR. Promotion to load-bearing requires one, and the ADR is not complete until the test debt is paid.** "Load-bearing" means something else now depends on it — a gate other people's commits pass through, production or deploy surface, or anything that cannot be cheaply abandoned. Build first **with a written kill criterion**, which is what makes it an experiment rather than a hunch.

Build-then-document was correct here: no up-front debate produces the incident taxonomy below, and an ADR written first would have specified handlers for *anticipated* failures — exactly what the twice-then-mechanize rule rejects.

**Condition on the rule: the promotion review must inspect the artifact, not the prose.** Seven factual claims in this document's own draft and rewrite were contradicted by the implementation, and every one was found by reading code or run artifacts. A review that reads only the ADR ratifies its errors.

**Two things here crossed the line before this ADR existed**, and are recorded so the rule is seen to have teeth: the `/devloop` SKILL.md edits (a gate every devloop live-reads), and preflight's rewrite of the operator's global `~/.claude/settings.json` — "cannot be cheaply abandoned" in the most literal sense, since it persisted on the machine.

## Consequences

### Positive
- Per-story autonomy: Run #1 drove ~15 tasks with human involvement only at genuine decision points, including cross-invocation resume.
- **The runner's own orchestration overhead is unmeasurable** — a task's gate end and the next task's prompt write repeatedly share the same minute. A serial bash loop is fast enough.
- Every Run #1 failure became a mechanism rather than a recurrence.
- Serial execution enables cluster reuse across consecutive tasks.

### Negative
- **Experimental substrate dependency** with no supported alternative (§8), mitigated by a probe whose limits are now named.
- **Quota-bound**: large stories span quota windows; the runner sleeps through them unattended. Do not co-develop on the same account during a run.
- **The runner is untested** until §12 lands, which is why §12 gates everything else.
- **Evidence durability is silently conditional**: the host mount that preserved Run #1's artifacts sits behind two checks that ERROR and continue anyway, so a run can proceed in a mode where nothing survives with nothing saying which mode it is in.

### Neutral
- Requires the follow-on work below before the workflow is coherent end-to-end.

## Implementation Status

✅ **Done** (Run #1, validated end-to-end): `dt-story`; `run-story.sh` (serial loop, independent gate, canary classification, session-limit lane, escalated-retry, auto-remediation, structured cost ledger); Stop hook + preflight substrate config; headless `/devloop` mode; commit-time todo-tracking guard.

✅ **Landed with this ADR** — nine fixes across five files: container-boundary enforcement and the settings-footprint split (`preflight-story.sh`); bg-wait export, in-flight marker, per-task CLI version assertion, JSONL `-c`, per-attempt escalation records, infra-incident records, `opus` alias default (`run-story.sh`); in-flight CLI-update skip (`devloop.sh`); npm update race and masking (`entrypoint.sh`).

**Only the `entrypoint.sh` fix requires an image rebake**; the other eight are live at merge, since `devloop.sh` runs host-side and the workflow scripts are read from the mounted clone.

❌ **Pending**: everything in §Follow-on work.

## Follow-on work

`/user-story` decomposes this list into a `dt-story` manifest; the runner then executes it with a `--stop-after` seam that halts for review before the runner self-modifies.

**Where the detail lives — the two halves have different sources, deliberately, to avoid duplicating either:**

- **Skill-batch items** (`/user-story`, `/close-story`, `/devloop`, `devloop.sh`) — detail bodies in `docs/TODO.md` §Story Workflow Follow-ups, which predates this ADR. Those entries are annotated with the section and group that decided them; this ADR owns the decision, grouping and ordering, and the TODO entries stay the single source for the bodies.
- **Runner and gate work** (§2 `--keep-going`, §3 verdict reads, §7 suppression gate, §8 probe hardening, §9 routing and residue maps, §10 tiering, §11 run record, §12 seams and suite, §13 acceptance criteria) — **specified in the numbered sections above; this ADR is the source.** Do not re-file them into `docs/TODO.md`; a second copy would drift from the decision that created it.

**Bootstrap caveat for the first planning pass**: two things that would improve it are themselves in this list — `/user-story` emitting the manifest at planning time, and §13's machine-assertable acceptance criteria. So Group 1 gets planned with hand-written acceptance criteria and should be re-validated once §13 lands. This is the same inert-until-reinvoke logic as the `--stop-after` seam, not an oversight.

**Group 1 — runner core, strict order.** §12 is a hard predecessor of everything: landing a gate change, tiering, or the suppression gate before it means using the live run as the test.

1. **§12 · seams + suite + journal schema assertions** — opening cases are the four known runner defects (`--stop-after` silent no-op; canary stderr merged into the JSON it parses; the pipeline's operator/implementer exit-code taxonomy collapsed into an implementer-blaming escalation; Stop hook failing open on a `git rev-parse` error), each fixed alongside its pin.
2. **§11 · durable `STORY_RUN_DIR` seam** — one change serving evidence durability, §12's test seam, and the probe-log home.
3. **§3 · VERDICT-MISMATCH cross-check, freshness check, commit-completeness assertion.**
4. **§7 · suppression hard-gate** with the widened detector and its own test case.
5. **§3 · layer-4 `passed=0` detector** (standalone; the re-run cannot cover this class).

**Group 2 — safe anywhere (inert until reinvoked, or affecting only future runs).**
- **§13 · acceptance-tests-first — immediately after Group 1.** The only follow-on item with direct defect evidence behind it.
- `/user-story` emits the manifest at planning time; `/close-story` reads the manifest, not the drifted table.
- `devloop.sh --run-story` mode; exports `DEVLOOP_SLUG`.
- **§9 · `review_mode` field**; **§9 · per-role mechanization boundaries** (including rewriting `.claude/agents/code-reviewer.md`, whose first-listed focus is deny-level clippy and therefore cannot produce a review finding).
- **§2 · `--keep-going`**; **§11 · budget ceiling**; stale `Co-Authored-By` trailers.

**Group 3 — live-read `/devloop` skill edits (run last; a fresh `claude -p` reads these per task, so a broken edit breaks the rest of the in-flight run).**
- `/devloop` headless-only.
- **§10 · per-seat model tiering** — requires per-teammate model overrides in `SKILL.md`, which is why it belongs here rather than Group 2.
- **Re-run the preflight substrate probe after any Group 3 edit**, since it changes what every subsequent session reads.

**Cross-cutting**: every item phrased as a *skill-file instruction* should be re-examined as a `dt-story validate` check instead — at least the acceptance-criteria ban, the `env_tests`/deps completeness check, and any DAG-degeneration detector qualify. A document claiming a trajectory of moving trust out of instructions should not close by adding three new instructions.

## Alternatives Considered

- **Workflow-tool-as-runner** (JS orchestration engine). Rejected: a nested workflow agent has no Agent tool, so it cannot spawn teammates.
- **Skill-as-runner** (main-session loop). Rejected: the Lead's context blows up over a story — the context-rot failure that killed ADR-0021/0022.
- **Nested-subagent devloop** (the abandoned March 2026 `/story-run`). Rejected: a devloop Lead spawned as a subagent cannot create a team.
- **Agent SDK.** Rejected permanently — see §8. Not a fallback for a smaller panel either.
- **Consuming the Gate-2 verdict as authority.** Rejected — see §3. **Recorded trap for whoever revisits this**: the natural implementation (recompute `gate2_records_worktree | gate2_signature`, compare to `SIGNATURE`) is silently worthless post-commit. On a clean tree the record stream is empty, so the digest is the SHA-256 of the empty string — a universal constant that passes for any clean tree, including a stale verdict left by a different task. `gate2_validate_commit` is also staged-index-only and returns 0 vacuously post-commit. And `SIGNATURE` is an **unkeyed digest, not a signature**: widening what it covers buys zero authenticity, and no key can fix it, since any key the producer holds is readable by the session hosting it. The field should be renamed `TREE_DIGEST`.

## When to Revisit

- **§2 serial** — quota no longer binding **AND** per-task layer-7 cluster isolation exists.
- **§8 float** — two probe failures in a quarter, any CLI advisory, or any change in skip-permissions scope.
- **§9 panel** — when a guard lands on a role's beat (per-role, not on a schedule).
- **§10 tiering** — once §11's journal supplies `model`/`specialist`/`outcome`.

## Run #1 incident taxonomy

Each became a mechanism:

| Incident | Became |
|---|---|
| Specialist `subagent_type` unregistered | agent-file frontmatter |
| Print-mode Lead idle-death at gates | Stop-hook completion enforcement |
| Stop hook unbaked (entrypoint image edit) | preflight-owned substrate registration |
| Session-limit 429 mid-devloop | canary session-limit lane |
| Credential rotation orphaning container creds | AUTH-EXPIRED lane → `--refresh-creds` |
| Failure-classification misreads (2×) | canary probe replaces log-grepping |
| Manifest revert friction after escalation | escalated-retry (rerun = resume) |
| Devloop double-marking its own task | idempotent `complete` |
| Step-9 docs written after Gate 2 | commit-time todo-tracking guard |
| First-ever story-close audit-red | advisory auto-remediation |
| #63 display-name fix half-done | caught by #60's tests → §13 |

**On #63**: the panel *saw* it and correctly scoped it out per ADR-0024 §6.3 owner-implements; the escape was in story bookkeeping accepting a prose promise as acceptance for machine-assertable behavior. That is why §13 exists.

## Debate Reference

`docs/debates/2026-08-10-adr-0035-story-runner/debate.md` — participants, positions, the defect register, measurements, rejected options and the debate's method findings.

## References

ADR-0024 (Agent Teams devloop) · ADR-0025 (containerized devloop — the boundary §5 enforces) · ADR-0028 (zero-retry flaky policy) · ADR-0030 (host-side cluster helper) · ADR-0032 (metric governance — deliberately *not* applied, §11) · ADR-0033 (polyglot validation pipeline) · ADR-0034 (guard pipeline as Rust binary)
