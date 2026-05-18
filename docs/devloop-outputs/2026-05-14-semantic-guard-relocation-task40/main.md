# Devloop Output: Semantic-Guard Relocation to Reviewer Panel

**Date**: 2026-05-14
**Task**: Semantic-guard relocation from layer pipeline to reviewer panel (R-62, ADR-0033 Wave 3 #9)
**Specialist**: operations
**Mode**: Agent Teams (full)
**Branch**: `feature/browser-client-join-task40`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `b4afe6917ae856ac59e74fe52431af9ed8671378` |
| Branch | `feature/browser-client-join-task40` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` (Iteration 2) |
| Implementer | `implementer@devloop-2026-05-14-semantic-guard-relocation-task40-iter2` |
| Implementing Specialist | `operations` |
| Iteration | `2` (continue --light, complete 2026-05-18) |
| Iter 2 — Security | Gate 3: CLEAR (0 findings; concurred on L5/L6 pre-existing) |
| Iter 2 — Code Quality | Gate 3: CLEAR (0 findings; ADR Compliance verified for ADR-0033 §8 amendment + ADR-0024 §1 unchanged; orphan-sweep clean; concurred on L5/L6) |
| Iter 2 — Gate 2 Outcome | L5/L6 FAIL same as Iter 1 (pre-existing); L3 attempt-1 FAIL on scope-drift (top-of-doc table mismatch with --continue diff); attempt-2 PASS after option-(b) fix (active-scope table moved on top, Iter 1 archived; guard `--continue`-parser upgrade filed as P3 tech-debt). |
| Security | Gate 3: CLEAR (0 findings) |
| Test | Gate 3: CLEAR (0 findings) |
| Observability | Gate 3: RESOLVED (1 Gate-1 finding on dedup criterion fixed at planning; 0 at Gate 3) |
| Code Quality | Gate 3: RESOLVED (2 findings, both fixed in-loop: ADR-0024 §Negative comparison point + missing `docs/TODO.md` row in classification table) |
| DRY | Gate 3: CLEAR (0 findings) |
| Operations | Gate 3: RESOLVED (1 finding deferred-accepted: ADR-0033 §8 / §Neutral "6 → 7" historical narrative — non-blocking prose tidy; peer-reviewer; implementer is also operations) |
| Semantic Guard | Gate 3: SAFE → CLEAR (0 findings; first formal Gate 2 reviewer panel addition per ADR-0033 Wave 3 #9) |
| Gate 2 Outcome | FAIL with documented pre-existing-tech-debt acceptance (L5 R-61 proto lint reproduces on base; L6 RUSTSEC-2023-0071 rsa Marvin via sqlx-mysql, no fixed upstream). All 7 reviewers concurred with out-of-scope acceptance per task #39 precedent. |

---

## Task Overview

### Objective

Remove the interim semantic-guard placement from the validation layer pipeline and reposition it as a 7th reviewer in the Gate 2 reviewer panel. This is the final Wave 3 doc/process change that closes ADR-0033's "layer pipeline = pure shell" property: with this devloop, the pipeline carries zero agent invocation and all human/agent judgment lives in the panel.

### Scope

- **Service(s)**: none (docs + skill + agent definition only)
- **Schema**: No
- **Cross-cutting**: Yes — SKILL.md governs every future devloop; ADR-0024 §1 and ADR-0033 §Implementation Status are the cross-specialist trackers; `docs/devloop-outputs/_template/main.md` is the per-devloop scaffold.

### Debate Decision

NOT NEEDED. The decision (option a — coexist with code-reviewer via distinct lenses + dedup step) is already recorded in ADR-0033 §8. This devloop implements that decision; the operational tunings (dedup criterion strictness, Gate 1/Light mode participation, agent verdict-vocabulary, mirror surfaces) are working details, not redesign.

---

## Cross-Boundary Classification

**Active scope: Iteration 2.** The Layer A scope-drift guard parses the first §Cross-Boundary Classification table in this file; for `--continue` mode the active iteration's table must be on top. The Iteration 1 9-row table is archived under §Implementation Summary → §Iteration 1 Cross-Boundary Classification (historical reference). Iter 1's edits are recorded in git at commit `09b9729a04f4d6f0617128cae54ed7fcf01f96b1` and unchanged by Iter 2. A guard upgrade to parse the most-recent §Iteration N table is filed as tech-debt in `docs/TODO.md` §Polyglot Pipeline Follow-ups.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `.claude/agents/semantic-guard.md` | Mine | — |
| `.claude/skills/devloop/SKILL.md` | Mine | — |
| `.claude/skills/devloop/review-protocol.md` | Mine | — |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | Mine | — (Iteration 1 reclassified to Mine per code-reviewer challenge; same logic applies — this is operations' own Wave 3 #9 row + an in-scope §8 amendment) |
| `docs/decisions/adr-0024-agent-teams-workflow.md` | Mine | — (orphan-sweep consequence of §8 amendment; stale SAFE/UNSAFE + dedup framing in §1 Rationale + Verdict-Action cell; same reasoning as Iter 1's count-update reclassification — purely consequential to the already-approved ADR-0033 §8 amendment) |
| `docs/TODO.md` | Mine | — (removing Iteration 1 entries this devloop placed; RUSTSEC-2023-0071 entry preserved) |

**GSA check**: none of the above paths fall under ADR-0024 §6.4 Guarded Shared Areas (no `proto/**`, no `crates/common/src/jwt.rs`, etc.). Skill, ADR, and agent-definition edits are not wire-format / auth-routing / detection-forensics / schema surfaces.

**Layer B classification-sanity guard** is expected to pass: no GSA paths in the table; every row is Mine, so no Owner field is required.

---

## Planning

### Final dedup criterion (locked)

Three reviewers (observability, semantic-guard, security) pushed back on the trigram fallback as unsafe — concrete failure mode: a `credential-leak` finding folded into an unrelated naming/error-context finding on the same line by virtue of shared boilerplate vocabulary, hiding a P1 vuln behind a style suggestion. The asymmetric-cost argument (false fold = silent loss of a credential-leak; missed fold = one extra line of output) means we bias toward "show both."

**Locked criterion (codified in SKILL.md Step 7 §Deduplication)**:

> **Two findings fold iff ALL THREE are true**:
> 1. **Same file path** (literal string match, post-canonicalization).
> 2. **Same line number, OR overlapping line range** (a semantic-guard line-N finding folds with a code-reviewer hunk-range that contains N).
> 3. **Same check/concern category** — the semantic-guard finding's `[check-name]` tag appears as a literal substring (case-insensitive, stem-tolerant per DRY) in the code-reviewer finding's text, by name or by an enumerated synonym from the per-check vocabulary below.
>
> **Per-check synonym vocabulary** (seed list; extend in-place when a new synonym surfaces in practice):
> - `credential-leak`: credential, secret, password, token, key, JWT, leak
> - `actor-blocking`: actor, blocking, await, spawn, message loop, select!, mpsc, oneshot
> - `error-context-preservation`: error context, map_err, lost, discarded, swallowed, anyhow, propagat, preserv
> - `metrics-path-completeness`: metric, counter, histogram, gauge, instrument, exit path, early return, all-paths recording
>
> **When in doubt, present separately.** Under-deduping costs one extra finding in the panel summary (cheap). Over-deduping silently drops a real concern (expensive, possibly unrecoverable).
>
> **Tie-break / unified phrasing** (per operations Q2(b) + code-reviewer concur): when a fold occurs, the **semantic-guard finding's text is the primary** (more specific lens, check-name-tagged for searchability); the code-reviewer duplicate is annotated as "(also flagged by code-reviewer)" and dropped from the verdict count.
>
> **Security-finding preservation rule** (per security Concern 3): when a fold occurs, if either contributor is a `credential-leak`-tagged semantic-guard finding, the merged presentation MUST preserve the `credential-leak` attribution and recommended fix verbatim. A credential-leak tag is never elided.
>
> **Escape hatch** (per DRY): if Lead folds two findings the implementer believes are distinct concerns, the implementer can request un-fold during the discussion turn. Dedup is an optimization, not a gate.

**Manual at Gate 3, no automated aggregator.** Lead applies the criterion when constructing the Gate 3 verdict table. An automated panel-aggregator is **deferred** — recorded as a tech-debt pointer in `docs/TODO.md` under Devloop Process.

**Trigram fallback dropped entirely.** It was an attractive-nuisance: looked rigorous, not eyeballable by the Lead at Gate 3, and incentivized exactly the false folds the dedup step exists to avoid.

### Agent verdict-vocabulary (locked)

**Approach: option (b) translation-layer.** Semantic-guard's agent file keeps its native `SAFE` / `UNSAFE` verdict line, AND adds an explicit mapping sentence: at Gate 3 the Lead maps `SAFE → CLEAR`; `UNSAFE` → `RESOLVED` (if findings fixed/accepted) or `ESCALATED` (if disagreement persists). Rationale (per semantic-guard Concern 3): `SAFE`/`UNSAFE` vocabulary is referenced from `scripts/guards/semantic/checks.md` and from historical devloop-output files; a hard switch would break the artifact-search trail. Translation-layer approach preserves history while still aligning at Gate 3 aggregation.

DRY's preferred option (a — agent emits CLEAR/RESOLVED/ESCALATED directly) is acknowledged as cleaner long-term; the dissent is recorded here. If `SAFE`/`UNSAFE` references in checks.md and history files are migrated in a future devloop, the agent file's verdict line can flip to option (a) at that time. Tracked as a tech-debt pointer.

**Test reviewer (c) is honored**: agent file's existing Output Format block (L19-37) is **functionally preserved** — only adds the mapping paragraph; the Lead applies the mapping, the agent does not change its emit.

**Finding-level fields**: per DRY's clarification — `[check-name]` tags MUST appear in each finding (load-bearing for dedup); finding-level Status uses `Fixed / Deferred (accepted) / Spun-out (accepted) / Escalated` per `review-protocol.md §Verdict Format`.

### Lifecycle change (locked per operations #1)

Semantic-guard moves from "spawned at Gate 2 after `scripts/layer-all.sh` passes" to "spawned at Step 3 alongside other 6 reviewers; sits through Gate 1 plan confirmation; waits for 'Start Review' at Gate 2 pass; reviews; sends verdict." The standard Reviewer prompt template (SKILL.md Step 3 L260-290) is generic enough to work as-is — the diff-focus stays in the agent file, not the prompt.

### Gate 1 plan-review participation

Semantic-guard participates at Gate 1 (sends "Plan confirmed" before Lead issues "Plan approved"). Caveat: for docs-only devloops the confirmation is a no-op pass — agent file gets a one-line note (per DRY Q3) "At Gate 1, semantic-guard's typical confirmation for non-Rust diffs is 'Plan confirmed — no diff to analyze yet; will examine at Gate 2.'" Symmetry > minor noise.

### `--light` mode — explicit pin (per operations micro-add)

`--light` mode is **NOT** changed by this devloop. Semantic-guard is **full-mode-only**. SKILL.md §Lightweight Mode gets an explicit sentence stating this so a future Lead reading only `--light` docs cannot misread §Team Composition as implying semantic-guard joins all devloops. Concretely: under the "Third reviewer selection" list, add "(Semantic-guard is full-mode-only; not eligible as a `--light` context reviewer.)"

### Test-code exemption (per semantic-guard Concern 2 + test reviewer (a))

`.claude/agents/semantic-guard.md` L13 and L43 (the test-code exemption) are **preserved verbatim**. The SKILL.md team-composition Purpose cell adds an explicit short clause referencing the exemption: "applies to non-test production code per `.claude/agents/semantic-guard.md` §Judgment Calibration." Prevents a future contributor from inadvertently widening scope to test files via SKILL.md.

### Flake-rate budget framing (per test reviewer (b))

ADR-0033 §14 line 308 ("Reviewer panel (incl. semantic-guard) | Best effort — agent-based, occasional model variance acceptable") is **untouched** — best-effort variance classification continues.

### Distinct-lens framing — expanded per code-reviewer #4 + semantic-guard

SKILL.md Team Composition row Purpose cell:

> **Diff-level anti-pattern checks per `scripts/guards/semantic/checks.md` (credential leak, actor blocking, error-context preservation, metrics path completeness). Distinct from code-reviewer's general lens (Rust idioms, ADR compliance, naming, error handling); findings can overlap and dedupe at Gate 3 (see Step 7 §Deduplication). Applies to non-test production code per `.claude/agents/semantic-guard.md` §Judgment Calibration.**

The "metrics path completeness" check is included per semantic-guard's catch — earlier drafts missed the fourth check.

### Mirror surfaces — consolidated inventory

`.claude/skills/devloop/SKILL.md`:
- L37 count statement (`7 teammates / 6 reviewers` → `8 teammates / 7 reviewers`)
- §Team Composition Full Mode table (L39-48): add Semantic Guard row + `<!-- Mirror of ADR-0024 §1 Team Composition / SKILL.md §Team Composition. Update both locations together. -->` comment block (DRY F + DRY mirror-comment shape)
- Step 3 spawn loop reviewer-name iteration
- Step 3 Reviewer prompt template (L260-290) — generic, works for semantic-guard as-is
- Step 4 team-list (L296-313): `- Semantic Guard Reviewer: @semantic-guard (full mode only)`
- Step 5 Gate 1 confirmation table (L326-332): add Semantic Guard row
- Step 6 — **REMOVE** "Semantic Guard Agent (interim placement)" subsection (L418-432) entirely
- Step 6 unicast "Start Review" loop (L436): semantic-guard in unicast list
- Step 7 Gate 3 verdict table (L454-463): add Semantic Guard row
- Step 7 — **INSERT** Deduplication sub-step between "Wait for all reviewer verdicts" and "If any ESCALATED" (criterion text per Final dedup criterion above)
- Step 8 commit-message Verdicts enumeration (L487): spell out semantic-guard for symmetry
- Step 9 user-report enumeration (L532-538): add Semantic Guard line
- Lightweight Mode (L56-86): explicit "semantic-guard is full-mode-only" sentence

`.claude/skills/devloop/review-protocol.md`:
- 1-2 sentence distinct-lens note + cross-reference to SKILL.md Step 7 §Deduplication (criterion lives in SKILL.md only, not duplicated here per DRY J)
- Note: no literal "6 reviewer" enumeration in this file per operations micro-point (i); skip the spot-check edit

`.claude/agents/semantic-guard.md`:
- L7 reframe: "spawned by the devloop Lead as a Gate 2 reviewer (alongside security/test/observability/code-reviewer/dry-reviewer/operations)"
- Workflow section reframe to standard reviewer lifecycle (Plan confirmation → wait for Start Review → review → verdict)
- Output Format **functionally preserved** (per test (c) + semantic-guard Concern 3): keep SAFE/UNSAFE as native verdict; ADD a mapping paragraph (SAFE→CLEAR; UNSAFE→RESOLVED-or-ESCALATED per Fix-or-Defer Model)
- Add Gate 1 short note for docs-only devloops ("typical confirmation is 'Plan confirmed — no diff to analyze yet'")
- Preserve test-code exemption L13 + L43 verbatim
- Preserve `[check-name]` tag requirement on findings (load-bearing for dedup)

`docs/decisions/adr-0033-polyglot-validation-pipeline.md`:
- L364 status-table row: `❌ Pending` → `✅ Done`, devloop column → `2026-05-14-semantic-guard-relocation-task40`
- Light prose tidy on "interim" / "will move" wording about semantic-guard
- L308 §14 flake budget — **untouched** per test (b)

`docs/decisions/adr-0024-agent-teams-workflow.md`:
- L33-47 §1 Team Composition: header "7 Teammates" → "8 Teammates"; body "7 teammates (Lead + Implementer + 6 reviewers)" → "8 teammates (Lead + Implementer + 7 reviewers)"; reviewer table adds Semantic Guard row
- L70 ("Spawn 7 teammates with composed prompts") → "Spawn 8 teammates"
- L484 ("Larger review team — 7 teammates instead of 6") → "Larger review team — 8 teammates instead of 7" (consequential drawback update; trade-off framing stays)
- Add `<!-- Mirror of ADR-0024 §1 Team Composition / SKILL.md §Team Composition. Update both locations together. -->` comment block matching SKILL.md (DRY mirror-comment shape)

`docs/devloop-outputs/_template/main.md`:
- L32-37 Loop State table: add `| Semantic Guard | \`{agent_id or pending}\` |` row
- L186 "Layer 7: Semantic Guards" subsection: rename to "Layer 7: Env-tests" + remove the SAFE/UNSAFE block (task #38 residual)
- L198-237 §Code Review Results: add `### Semantic Guard Reviewer` block matching the shape of the other six

`docs/user-stories/2026-05-02-browser-client-join.md`:
- L737 task #40 row: Status `Pending` → `Completed`; Devloop Output column → `docs/devloop-outputs/2026-05-14-semantic-guard-relocation-task40/main.md`

`docs/specialist-knowledge/semantic-guard/INDEX.md`:
- L7 relink: `"Validation Layer 7 (env-tests integration) → SKILL.md ('Layer 7' section)"` → `"Reviewer-panel framing (Gate 2 reviewer slot) → SKILL.md (§Team Composition, Step 6 Gate 2). Check definitions → scripts/guards/semantic/checks.md."`

### Orphan-reference sweep (per code-reviewer #6)

After removing the SKILL.md L418-432 interim block, grep across `.claude/`, `docs/decisions/`, `docs/runbooks/`, `docs/specialist-knowledge/`, and `docs/devloop-outputs/_template/` for orphan references to:
- "interim placement"
- "Layer 7: Semantic Guards"
- "7 teammates" / "6 reviewers" / "6 → 7"
- "SAFE/UNSAFE" outside the semantic-guard agent file and `scripts/guards/semantic/checks.md`

Note: "Wave 3 #9" and "ADR-0033 §8" are legitimate ADR locators and are NOT orphan markers — they're expected to keep appearing (ADR-0033 row #364 itself, the user-story task #40 row, etc.). The sweep targets stale framing language, not citations.

`docs/devloop-outputs/202*` historical record files are **excluded** from the sweep (record of state-at-time-of-write).

### `scripts/guards/run-guards.sh` — verified untouched

L5-6 ("during devloops") and L159-160 ("during devloop validation") are generic enough to survive the relocation. No edit needed; this file is **not in the Cross-Boundary Classification table** (no diff entry expected).

### `scripts/layer7.sh` — verified untouched

File is already a pure-shell `STATUS=N/A REASON=wave2-pending` stub with no agent invocation. The interim semantic-guard spawn currently lives in **SKILL.md Step 6** (L418-432), which we remove. After this devloop, `grep -r semantic-guard scripts/` returns only `scripts/guards/semantic/` (the check-definition directory) — no layer-script invocations remain.

### Runbook impact — verified none

`docs/runbooks/devloop-validation.md` (task #39) contains:
- L213 "semantically" — unrelated prose
- L310 "scripts/guards/semantic/credential-leak.sh" — generic example path

Neither needs updating; relocation is transparent to the runbook (per operations Q4).

### Validation plan

- Layer pipeline (`scripts/layer-all.sh`) — doc-only changes; Layer 3 (Guards) will run `validate-cross-boundary-classification.sh` against this main.md (all rows are Mine, no GSA paths — guard expected pass).
- Layer 7 (env-tests) — N/A wave2-pending, unchanged.
- Doc-citation guards — verify ADR-citation guards still pass after touching ADR-0033 / ADR-0024.
- Manual: re-read SKILL.md and ADR-0024 §1 end-to-end after edits to confirm count consistency (8 teammates / 7 reviewers in both); run the orphan-reference sweep grep set; verify the SAFE/UNSAFE references in the agent file remain identical post-edit.

---

## Pre-Work

None. Branch is on `b4afe6917ae856ac59e74fe52431af9ed8671378` (clean working tree).

---

## Implementation Summary

Doc/process-only changes implementing ADR-0033 Wave 3 #9. Nine files modified:

1. `.claude/agents/semantic-guard.md` — workflow rewrite to Gate 1/2/3 lifecycle (was: post-layer-pipeline spawn); §Output Format gained Verdict Mapping section (SAFE→CLEAR, UNSAFE→RESOLVED-or-ESCALATED); test-code exemption + `[check-name]` tag requirement preserved/emphasized; Gate 1 docs-only confirmation template added.
2. `.claude/skills/devloop/SKILL.md` — count: 7→8 teammates / 6→7 reviewers; new Semantic Guard row in Team Composition table + mirror-comment HTML block; `--light` mode explicitly pinned as "not eligible for semantic-guard"; Step 4 team-list adds semantic-guard; Step 5 Gate 1 confirmation table expanded to 7 reviewers; **Step 6 interim semantic-guard subsection REMOVED** (was lines 418-432); Step 6 unicast loop includes `@semantic-guard`; Step 7 verdict table adds Semantic Guard row + verdict-mapping prose + **new §Deduplication sub-step** (strict-only criterion, per-check synonym vocabulary, tie-break rule, security-finding preservation rule, escape hatch); Step 8 commit-message Verdicts line spelled out; Step 9 user-report includes Semantic Guard.
3. `.claude/skills/devloop/review-protocol.md` — added §Semantic-Guard ↔ Code-Reviewer Distinct Lenses cross-reference (dedup criterion lives in SKILL.md Step 7, not duplicated here).
4. `docs/decisions/adr-0033-polyglot-validation-pipeline.md` — Wave 3 #9 status row `❌ Pending` → `✅ Done` with this devloop slug.
5. `docs/decisions/adr-0024-agent-teams-workflow.md` — §1 Team Composition: 7→8 teammates, 6→7 reviewers; added Semantic Guard row; updated rationale; mirror-comment HTML block; L74 spawn count; L488 drawback count.
6. `docs/devloop-outputs/_template/main.md` — Loop State table row added; Layer 7 subsection renamed (Semantic Guards → Env-tests) with semantic-guard relocation note; §Code Review Results §Semantic Guard Reviewer subsection added.
7. `docs/user-stories/2026-05-02-browser-client-join.md` — task #40 row: Status `Pending` → `Completed`; Devloop Output column filled.
8. `docs/specialist-knowledge/semantic-guard/INDEX.md` — L7 relinked from "Layer 7 (env-tests integration)" to "Reviewer-panel framing (Gate 2 reviewer slot)".
9. `docs/TODO.md` — added two tech-debt pointers under §Polyglot Pipeline Follow-ups: automated panel-aggregator deferred + SAFE/UNSAFE → CLEAR/RESOLVED/ESCALATED migration deferred. The L6 RUSTSEC-2023-0071 finding from this devloop's Gate 2 (pre-existing supply-chain debt) added as a third pointer (see §Gate 2 Outcome below).

`scripts/layer7.sh` verified untouched (already a pure-shell N/A stub).

### Iteration 1 Cross-Boundary Classification (historical reference)

Moved here from top-of-document at Iteration 2's Gate 2 attempt-2 fix-up so the Layer A scope-drift guard sees Iter 2's active scope on top. Iter 1's edits are recorded in git at commit `09b9729a04f4d6f0617128cae54ed7fcf01f96b1` and unchanged by Iter 2.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `.claude/skills/devloop/SKILL.md` | Mine | — |
| `.claude/skills/devloop/review-protocol.md` | Mine | — |
| `.claude/agents/semantic-guard.md` | Mine | — (reclassified per code-reviewer challenge: panel-composition follow-through belongs to operations; code-reviewer declined Owner role and ADR-0024 §6.2 routes downward-classification disputes to ESCALATE — accepted the upgrade-equivalent move to Mine) |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | Mine | — (reclassified per code-reviewer: ADR-0033 Wave 3 #9 row is operations' own row; status-table flip on operations' own Wave-3 row is in-domain, not cross-boundary) |
| `docs/decisions/adr-0024-agent-teams-workflow.md` | Mine | — (count-update is purely consequential to the already-approved ADR-0033 §8 decision; reclassified per code-reviewer's self-co-sign-anomaly note) |
| `docs/devloop-outputs/_template/main.md` | Mine | — (added per DRY item E + operations blocking-gap #8; template is operations-owned by SKILL.md precedent) |
| `docs/user-stories/2026-05-02-browser-client-join.md` | Mine | — (story tracking-table row #40 update at story-close convention) |
| `docs/specialist-knowledge/semantic-guard/INDEX.md` | Mine | — (reclassified per code-reviewer: closest-specialist routing not appropriate for an agent without a dedicated owner; line-7 relink is panel-composition follow-through) |
| `docs/TODO.md` | Mine | — (three tech-debt entry adds under §Polyglot Pipeline Follow-ups: RUSTSEC-2023-0071 supply-chain debt surfaced at this devloop's L6 Gate 2; automated Gate 3 panel-aggregator deferred; semantic-guard SAFE/UNSAFE → CLEAR/RESOLVED/ESCALATED verdict-vocabulary migration deferred. Adds-only; no edits to existing entries. Added per code-reviewer Finding 2 — every touched file needs a row even if Mine, per review-protocol.md Gate 1 §6.) |

**Iter-1 GSA check (historical)**: none of the above paths fall under ADR-0024 §6.4 Guarded Shared Areas. **`scripts/layer7.sh`** was NOT in this table (verification-only read per operations item #4); already a pure-shell `STATUS=N/A REASON=wave2-pending` stub with no agent invocation.

---

## Files Modified

```
.claude/agents/semantic-guard.md
.claude/skills/devloop/SKILL.md
.claude/skills/devloop/review-protocol.md
docs/decisions/adr-0024-agent-teams-workflow.md
docs/decisions/adr-0033-polyglot-validation-pipeline.md
docs/devloop-outputs/_template/main.md
docs/devloop-outputs/2026-05-14-semantic-guard-relocation-task40/main.md
docs/specialist-knowledge/semantic-guard/INDEX.md
docs/TODO.md
docs/user-stories/2026-05-02-browser-client-join.md
```

All nine in-scope files match the Cross-Boundary Classification table. Plus this devloop's own `main.md`.

---

## Devloop Verification Steps

### Gate 2 Outcome (attempt 1 of 3)

`scripts/layer-all.sh` summary:

| Layer | Verb | Result | Duration |
|-------|------|--------|----------|
| L1 | Compile | OK | 24s |
| L2 | Format | OK | 2s |
| L3 | Guards | OK | 8s |
| L4 | Test | OK | 213s |
| L5 | Lint | **FAIL** | 16s |
| L6 | Audit | **FAIL** | 4s |
| L7 | Env-tests | N/A (wave2-pending) | — |

**L5 Lint failure** — `nx run proto-gen:lint` returned 21 R-61 STANDARD findings on `proto/internal.proto` and `proto/signaling.proto`.

- **Pre-existing**: reproduces on a clean checkout of the base branch (`docs/user-story-tracking-cleanup`) with this devloop's diff stashed. Verified by team-lead.
- **Story-design accepted**: the R-61 cleanup is sequenced as Track 2 #29 / #30 / #31 in this user story, explicitly scheduled *after* Track 3 wave (this task is Track 3 Wave 3 #9). Task #29 is the one that lands the temporary `proto/buf.yaml` `lint.ignore` scaffolding masking these findings during the cleanup window; #29 has not yet run. See `docs/user-stories/2026-05-02-browser-client-join.md` §Parallelization Opportunities and task #31's wire-breaking note ("Removes the temporary `lint.ignore` block from `proto/buf.yaml` entirely as the final step").
- **Not introduced by this devloop**: this devloop's diff is doc-only across 9 `.md` files; touches no `proto/**`.

**L6 Audit failure** — `cargo audit` reports `RUSTSEC-2023-0071` (rsa 0.9.10, Marvin timing side-channel) transitively via `sqlx-mysql 0.8.6`.

- **Pre-existing**: reproduces on base-branch clean checkout. Verified by team-lead. This devloop's diff touches no `Cargo.toml` / `Cargo.lock`.
- **No fixed upgrade available**: upstream `rsa` crate has not released a patched version as of this devloop. Mitigations require an SDK swap or transitive-dep override, neither in scope here.
- **Surfaced since task #38**: task #38 (`2026-05-12-skill-step6-rewrite-task38`) had PASS Gate 2 on 2026-05-12; the advisory has surfaced in the audit database between then and 2026-05-15. Not previously triaged.
- **Recorded as tech debt** in `docs/TODO.md` §Polyglot Pipeline Follow-ups → "RUSTSEC-2023-0071 (rsa 0.9.10 Marvin timing sidechannel) supply-chain debt".

**Precedent**: task #39 (`docs/devloop-outputs/2026-05-14-devloop-validation-runbook-task39/main.md`) handled a similar pre-existing-env-gap Gate 2 outcome the same way — reviewers concurred on env-gap interpretation, no escalation needed. Awaiting equivalent concurrence here.

---

## Code Review Results

TBD.

---

## Tech Debt Pointers

- `docs/TODO.md` §Polyglot Pipeline Follow-ups — automated panel-aggregator (Gate 3 dedup) deferred
- `docs/TODO.md` §Polyglot Pipeline Follow-ups — semantic-guard verdict-vocabulary migration (option-a: agent emits CLEAR/RESOLVED/ESCALATED directly once SAFE/UNSAFE references in `scripts/guards/semantic/checks.md` and history are migrated)
- `docs/TODO.md` §Polyglot Pipeline Follow-ups — RUSTSEC-2023-0071 (rsa 0.9.10 Marvin timing sidechannel; transitive via sqlx-mysql) — surfaced at this devloop's L6 Gate 2; no fixed upgrade available upstream

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `b4afe6917ae856ac59e74fe52431af9ed8671378`
2. Review all changes: `git diff b4afe6917ae856ac59e74fe52431af9ed8671378..HEAD`
3. Soft reset (preserves changes): `git reset --soft b4afe6917ae856ac59e74fe52431af9ed8671378`
4. Hard reset (clean revert): `git reset --hard b4afe6917ae856ac59e74fe52431af9ed8671378`
5. No schema or infra changes — `git reset` alone is sufficient.

---

## Issues Encountered & Resolutions

TBD.

---

## Lessons Learned

TBD.

---

## Appendix: Verification Commands

```bash
./scripts/layer-all.sh
./scripts/guards/simple/validate-cross-boundary-classification.sh docs/devloop-outputs/2026-05-14-semantic-guard-relocation-task40/main.md

# Orphan-reference sweep after edits:
grep -rn "interim placement\|Layer 7: Semantic Guards\|6 reviewers\|7 teammates\|6 → 7" \
  .claude/ docs/decisions/ docs/runbooks/ docs/specialist-knowledge/ docs/devloop-outputs/_template/ \
  | grep -v 'docs/devloop-outputs/202'
```

---

## Human Review (Iteration 2) — `/devloop --continue --light`

**Date**: 2026-05-17
**Mode**: `--light` (3 teammates: implementer, security, code-reviewer)
**Start Commit (Iter 2)**: `09b9729a04f4d6f0617128cae54ed7fcf01f96b1` (i.e., the Iteration 1 commit)

**Feedback** (six items, all simplifications — drop overengineering shipped in Iteration 1):

1. **`.claude/agents/semantic-guard.md`** — drop the "non-Rust diffs or docs-only changes you typically have nothing to flag" caveat. TS client code is incoming and will have the same semantic concerns (credential-leak, error-context, metric-path-completeness all generalize to TS). Replace with a language-neutral framing along the lines of "diffs with no executable surface" or just drop the docs-only template entirely.
2. **`.claude/skills/devloop/SKILL.md` Step 6 unicast loop** — drop the "including @semantic-guard alongside @security, @test, @observability, @code-reviewer, @dry-reviewer, @operations" call-out. The pre-existing wording "Message each reviewer individually (unicast, not broadcast)" plus the existing 7-reviewer roster is sufficient; the special call-out reads like semantic-guard is a second-class addition.
3. **SKILL.md Step 7 — native verdict vocabulary** — semantic-guard should emit `CLEAR`/`RESOLVED`/`ESCALATED` natively. The Iteration-1 "translation layer" rationale (grep-continuity through historical devloop-output files) is weak: those outputs are frozen-at-conclusion records, no new SAFE/UNSAFE artifacts get generated. Drop the §Verdict Mapping section in the agent file and the verdict-mapping paragraph in SKILL.md Step 7.
4. **SKILL.md Step 7 — §Deduplication section** — remove. The dedup machinery (per-check synonym vocabulary, tie-break, security-finding preservation, escape hatch) exists to solve a problem we don't have. Reviewers already produce overlapping findings (security ↔ code-reviewer on auth, observability ↔ code-reviewer on metrics) and the Lead handles overlap ad-hoc in the verdict report without any formal step. The Lead can continue to do that for semantic-guard ↔ code-reviewer overlap. **Amend ADR-0033 §8** to drop the dedup-step decision (one-sentence amendment with rationale).
5. **`.claude/skills/devloop/review-protocol.md` §Semantic-Guard ↔ Code-Reviewer Distinct Lenses** — remove. The distinct-lens framing is already in the agent prompts (semantic-guard.md and code-reviewer.md); doesn't need codification in the shared review-protocol.md.
6. **`docs/TODO.md`** — clear both Iteration-1 entries:
   - "Automated Gate 3 panel-aggregator for semantic-guard ↔ code-reviewer deduplication" — dies along with the manual dedup step (item 4).
   - "Migrate semantic-guard agent from SAFE/UNSAFE to native CLEAR/RESOLVED/ESCALATED verdict vocabulary" — promoted into this iteration's scope (item 3).

### Iteration 2 Scope

| File | Iteration 2 edits |
|------|---|
| `.claude/agents/semantic-guard.md` | drop Rust/docs-only caveat in Workflow §Gate 1; rewrite §Output Format to emit native `CLEAR` / `RESOLVED` / `ESCALATED` directly; drop §Verdict Mapping section; preserve `[check-name]` tag mandate and test-code exemption |
| `.claude/skills/devloop/SKILL.md` | Step 6: remove unicast-loop semantic-guard call-out (back to pre-Iter1 wording, but list now naturally enumerates 7 reviewers); Step 7: drop §Deduplication subsection entirely; drop verdict-mapping paragraph; verdict table reads naturally with 7 rows |
| `.claude/skills/devloop/review-protocol.md` | drop §Semantic-Guard ↔ Code-Reviewer Distinct Lenses section |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | §8 amendment: drop "Deduplication step in the panel summary" sentence (and the §Deduplication-step bullet from §Implementation Status row), replace with one-sentence rationale noting that in practice reviewer-finding overlap is handled ad-hoc by the Lead, no formal step needed |
| `docs/TODO.md` | remove the two Iteration-1 entries (Gate 3 panel-aggregator + SAFE/UNSAFE migration); RUSTSEC-2023-0071 entry stays |

### Iteration 2 Cross-Boundary Classification

Moved to the top-of-document §Cross-Boundary Classification table (active scope) at Gate 2 attempt-2 fix-up. The Layer A scope-drift guard parses the first §Cross-Boundary Classification table in this file and must see the active iteration's rows. The Iteration 1 9-row table is archived under §Implementation Summary → §Iteration 1 Cross-Boundary Classification (historical reference). See `docs/TODO.md` §Polyglot Pipeline Follow-ups for the tracked guard upgrade to parse the most-recent §Iteration N table (proper long-term fix for `--continue` mode).

### Iteration 2 Gate 2 Outcome

**Attempt 1 of 3** — FAIL on L3 scope-drift parity (active-iteration table mismatch).

Layer summary:

| Layer | Verb | Result | Notes |
|-------|------|--------|-------|
| L1 | Compile | OK | — |
| L2 | Format | OK | — |
| L3 | Guards | **FAIL** | `validate-cross-boundary-scope.sh` flagged 2 of the Iter-1 paths (`_template/main.md`, `user-stories/2026-05-02-browser-client-join.md`) as planned-but-untouched. Root cause: the guard parses the first §Cross-Boundary Classification table and saw the Iter-1 9-row table; Iter 2's 6-row table at the bottom was invisible. **Fix**: top-of-document table replaced with Iter 2's active scope; Iter 1 table moved to §Implementation Summary as historical reference. Re-run pending. |
| L4 | Test | OK | — |
| L5 | Lint | **FAIL** | Pre-existing R-61 proto findings, same as Iter 1 (reproduces on base; story-design accepted via Track 2 #29-#31 sequencing). Documented in detail under §Iteration 1 Gate 2 Outcome — applies identically here. |
| L6 | Audit | **FAIL** | Pre-existing RUSTSEC-2023-0071 (rsa 0.9.10 Marvin via sqlx-mysql), same as Iter 1 (reproduces on base; no fixed upstream). Tracked in `docs/TODO.md` §Polyglot Pipeline Follow-ups. |
| L7 | Env-tests | N/A | wave2-pending, unchanged |

L5 + L6 reviewer concurrence (security + code-reviewer) follows the Iter 1 / task #39 precedent — both are pre-existing, neither introduced by Iter 2's doc-only diff. Awaiting reviewer sign-off at Gate 3 as in Iter 1.

**Attempt 2 of 3** — pending re-run by team-lead after L3 fix-up.

### Iteration 2 Tech Debt (new)

- `docs/TODO.md` §Polyglot Pipeline Follow-ups — **Layer A scope-drift guard `--continue`-mode parser upgrade**. Guard currently parses the first `## Cross-Boundary Classification` H2 heading in a devloop main.md, which mismatches active scope when `--continue` mode appends additional iterations. Workaround applied at Iter 2 Gate 2: keep the active iteration's table on top, archive earlier iterations' tables under §Implementation Summary. Proper fix: parse the table nearest the most-recent `### Iteration N Scope` subsection (or alternatively the table inside the most-recent `## Human Review (Iteration N)` block). Owner: operations (`/devloop`'s scope-drift guard). Priority: P3 (workaround clean; matters at the next `--continue` devloop).
