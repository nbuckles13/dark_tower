# Devloop Output: Remove changelog tables from runbooks

**Date**: 2026-10-02
**Task**: Delete runbook changelog/revision-history tables (git log is the SSoT), relocate any non-history content, and remove every instruction/guard that requires changelog rows
**Specialist**: operations
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/update-deps-fix-dev-flow`
**Duration**: ~1h50m (setup 2026-10-02 23:38 → commit 2026-10-03 ~01:50, incl. a stalled layer-fast notification)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `3ee98d55dd407ef6d780acd2330bc9f1bec59af6` |
| Branch | `feature/update-deps-fix-dev-flow` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@session-10a21854` |
| Implementing Specialist | `operations` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security@session-10a21854` |
| Test | `test@session-10a21854` |
| Observability | `observability@session-10a21854` |
| Code Quality | `code-reviewer@session-10a21854` |
| DRY | `dry-reviewer@session-10a21854` |
| Operations | `operations@session-10a21854` |
| Semantic Guard | `not spawned — doc/process-only diff, no checks.md surface` |

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
| `docs/runbooks/TEMPLATE.md` | Mine | — |
| `docs/runbooks/devloop-validation.md` | Mine | — |
| `docs/runbooks/client-dev-local.md` | Mine | — |
| `docs/runbooks/ac-service-incident-response.md` | Mine | — |
| `docs/runbooks/mc-incident-response.md` | Mine | — |
| `docs/runbooks/mh-incident-response.md` | Mine | — |
| `docs/runbooks/ac-service-deployment.md` | Mine | — |
| `docs/runbooks/gc-deployment.md` | Mine | — |
| `docs/runbooks/mc-deployment.md` | Mine | — |
| `docs/runbooks/mh-deployment.md` | Mine | — |
| `docs/runbooks/gc-incident-response.md` | Not mine, Mechanical | observability |
| `docs/observability/runbooks.md` | Not mine, Minor-judgment | observability |
| `docs/observability/alerts.md` | Not mine, Minor-judgment | observability |
| `docs/observability/dashboards.md` | Not mine, Minor-judgment | observability |
| `docs/PROJECT_STATUS.md` | Not mine, Mechanical | team-lead |
| `.claude/workflows/process-review-record.md` | Not mine, Mechanical | team-lead |
| `crates/dt-guard/tests/no_insecure_browser_flags_e2e.rs` (comment-only cite fix) | Not mine, Mechanical | infrastructure |
| `docs/TODO.md` (cite fix only) | Not mine, Mechanical | team-lead |
| `docs/observability/label-taxonomy.md` (cite fix) | Not mine, Minor-judgment | observability |

---

## Planning

### Mechanism restatement
Instance: "runbooks end with a changelog table every devloop appends to."
Mechanism: **hand-maintained per-file edit history inside a doc, at a fixed location every edit touches** — it duplicates `git log` (SSoT) and makes every parallel branch conflict at the same hunk. Three shapes of it live in `docs/runbooks/`: `## Changelog` tables, `**Version History**:` lists, and `**Last Updated**:` header fields (mh-deployment's was bumped 2026-09-30; most others are stale, e.g. mc-incident-response says 2026-05-01 while it has Scenarios 15-21 — drift, the other failure mode of the same duplicate). All three are in scope.

### Approach
1. **Every runbook header**: replace `**Last Updated**: <date>` with `**History**: \`git log -- docs/runbooks/<file>.md\`` (same line, so header shape is unchanged). This is the single pointer per runbook; it lives in the header because that is where a reader looks for freshness. TEMPLATE.md gets the same line with its placeholder path.
2. **Delete tables/lists**: `## Changelog` in TEMPLATE.md, client-dev-local.md (+ ToC link), devloop-validation.md (`## 11. Changelog`; it is the last section, so no renumbering; no referrer to §11 anywhere — swept). Delete the `**Version History**:` list in ac/gc/mc-incident-response `## Maintenance and Updates`; the rest of that section (ownership, review schedule, change process) is not history and stays, so the `#maintenance-and-updates` ToC anchors in gc/mc stay valid.
3. **Dependents** (swept `.claude/**`, `scripts/`, `crates/`, `.github`, `.githooks`, `infra/`, `packages/`, specialist INDEX files, ADRs): no skill, agent definition, guard, or test asserts or instructs changelog rows / Last Updated / Version History. Two instruction-shaped dependents found:
   - `docs/observability/runbooks.md` (the runbook index): "Include "Last Updated" date in each runbook" (§Version Control), "last updated" in the incident-runbook header standard, "version history" in the Maintenance-and-Updates standard, and five per-runbook `**Last Updated**:` restatements (already drifted). Rewrite the rule to "history is `git log`; no in-file dates/changelogs", drop the restatements.
   - `.claude/workflows/process-review-record.md`: "Add PRR numbers to workflow changelogs" — no workflow changelog exists; drop the bullet (commit messages already carry PRR refs per the bullet above it).
4. No new guard proposed. The rows were appended because TEMPLATE.md and the existing tables modelled it; removing both removes the source. Open to @test/@code-reviewer if you think a `docs/runbooks/` deny-pattern guard earns its keep — my lean is no (prefer existing tooling; it would be new guard machinery for a doc convention).

### Non-history content found inside the tables/lists, and where it goes
| Source row | Rule/decision | Destination |
|---|---|---|
| mc-incident-response Version History 2026-05-01 | Per-scenario `**Severity**:` lines deliberately mix ADR-0031 lowercase (`page`/`warning`/`info`, from Sc 11) and inherited Title Case; **do not normalize without an ADR follow-up** | **Moves**: new note under `## Severity Classification` (only place it lived) |
| client-dev-local 2026-09-09 | §5 heading carries no F-count so its anchor does not rot ("name a set's members, never restate its count"); `### F7 — …` slug is linked from mh-incident-response.md (3x) — do not rename F-entry headings without fixing referrers | **Moves**: short note at the top of §5 |
| devloop-validation 2026-08-15 (Gate-3 row) | Lane correctness = token **plus** cause line, not remediation alone | **Moves**: one sentence appended to §6.7 "Lane discipline" (the instance is already there; the generalisation is not) |
| devloop-validation 2026-08-15 | only a 404 evidences provisioning fault; bcrypt-dominated probe latency; `DEVLOOP_ORG_PROBE_TIMEOUT` | Already in body (§6.7 :702/:739, §8) — no move |
| devloop-validation 2026-08-29 | Counter-delta failure deliberately not auto-classified; never grep suite output | Already in §8 / §6.7 — no move |
| devloop-validation 2026-08-21 | Guard timeout retry-discriminator; fail-fast vs run-all | Already in §3, §6.3, §8 — no move |
| devloop-validation 2026-09-17 | `Code Coverage` documented ADVISORY pending confirmation | Already in §8.6 — no move |
| devloop-validation 2026-09-22 | self-heal safety posture (positive container-not-running, in-process check-and-set not file marker, RW mount), `ready-late` rename deferral | Already in §6.7/§8; deferral in `docs/TODO.md` (kubeconfig-stale entry) — no move |
| devloop-validation 2026-09-28 | layer-fast → review → Gate 3 → layer-all order | Already in §2 table — no move |
| devloop-validation 2026-08-15 | "a branch that cannot fire is a defect" | Rationale for a code change, not runbook content; retained by git log + that devloop's output — dropped |
| client-dev-local 2026-09-30 F18 | fix by agreement, never by clamping | Already in F18 body — no move |
| client-dev-local 2026-09-09 | `dev-web.test.sh` pins Secure-Context slug + prose | Already in body (:273) — no move |
| client-dev-local 2026-07-29 | deliberately diverges from TEMPLATE.md | Already in the banner — no move |
| gc-incident-response 2026-08-14 | do-not-run guard on Scenario 8 psql writes (`DATABASE_URL` mismatch) | Already in Scenario 8 body (:1210-1222) — no move |
| ac/mc/gc remaining rows | pure history | dropped |

### Wider class outside `docs/runbooks/` — surfaced for a scope call
Same mechanism, other owners: `docs/observability/alerts.md` and `dashboards.md` (footer `**Last Updated**` + a `Last Updated` column in their ownership tables — already drifted: dashboards.md footer says 2026-03-27, a row says 2026-09-27), `docs/PROJECT_STATUS.md` (`**Last Updated**: 2026-01-12`). **Recommendation: include them** (complete the invariant; ~6 small edits, observability is already a reviewer). **Exclude** `docs/decisions/adr-0020…` `## Revision History`: its Reason column records a decision rationale (an ADR's revision record is content, not edit history), it is not an append hotspot, and ADR amendment convention is owned elsewhere. `TEMPLATE.md`'s `**Template Version**` footer — superseded by Revision 1 below (removed).

### Revision 1 (after Lead scope ruling + reviewer pre-scans)
- **Also removed** (Lead ruling: git-derivable dated/version metadata): `**Document Version**` footers in ac/gc/mc/mh-deployment.md (all four carry one, not only mh) and `**Template Version**` in TEMPLATE.md. **Kept**: `Last Reviewed` / `Next Review` footers and the Ownership / Review Schedule / Change Process parts of Maintenance sections (a no-change review leaves no commit — not git-derivable).
- **Pointer form, identical everywhere**: header line `**History**: \`git log --follow -- docs/runbooks/<file>.md\`` replacing `**Last Updated**` in place (1:1 line swap, so no line shift).
- `docs/observability/runbooks.md`: remove per-runbook `**Last Updated**` copies (:47/:89/:110/:129) and own footer (:213); drop "last updated" from the header-structure item (:170) and "version history" from item 8 (:182); rewrite the :200 instruction to "history is `git log`; no in-file dates or changelogs".
- `alerts.md` / `dashboards.md`: **included** — owner (@observability) asked for it: drop footer `**Last Updated**` and the `Last Updated` column of each ownership table; Owner/Reviewer stay. `PROJECT_STATUS.md` header line: included under the same ruling. ADR-0020 Revision History: excluded (decision rationale, not edit history).
- client-dev-local.md: also drop the ToC `(F1–F19)` range — it restates the set and is the rot the §5 rule names. The `Secure Context and Media Setup` section (pinned by `scripts/dev-web.test.sh`) is not touched.
- devloop-validation.md: one 2026-08-15 principle relocates to §6.7 "Lane discipline" (below the :624 contract line cited by `crates/dt-guard/tests/no_insecure_browser_flags_e2e.rs` — that cite was already stale; fixed in Revision 2): (a) a lane is its token **plus** its cause line, not its remediation alone. [(b) "a classifier arm that cannot fire is deleted" — superseded by Revision 2: **dropped, not relocated**.] Token-set counts from the 2026-09-22 row are deliberately not relocated (restated counts rot).
- `docs/TODO.md` kubeconfig-stale → `ready-late` entry checked: self-contained (cites the devloop output, not the changelog row) — no edit.
- No other heading is renamed or renumbered (incident-runbook `#scenario-N-…` anchors are alert-linked).

### Revision 2 (Gate-1 reviewer adjustments)
- **Version labels** (operations, Lead): all five removed — `**Document Version:**` in ac-service-deployment.md, gc-deployment.md, mc-deployment.md (bold-colon form) and `**Document Version**:` in mh-deployment.md, plus TEMPLATE.md `**Template Version**:`. `Last Reviewed` / `Next Review` and TEMPLATE.md `**Template Source**:` kept.
- **TEMPLATE.md pointer** uses the template's bracket placeholder style: `**History**: \`git log --follow -- docs/runbooks/[runbook-file].md\`` so a copied runbook never points at TEMPLATE.md's own history. TEMPLATE.md models the line only; the prose rule ("history is git log; no in-file dates, versions or changelogs") is stated **once**, in `docs/observability/runbooks.md` §Version Control (DRY).
- **client-dev-local §5 note** carries local facts only (heading deliberately carries no F-count; F-entry headings have referrers — `mh-incident-response.md` links `### F7 — …`, so do not rename an F heading without fixing them) and cites the count rule's home (`docs/observability/metrics/mc-service.md` §`mc_media_sender_binding_responses_total`) rather than restating it.
- **devloop-validation §6.7**: appends only the one-sentence generalisation (lane correctness = token + cause line, not remediation alone); the ac-unreachable instance already at that paragraph is not re-explained. "A branch that cannot fire is a defect" is **dropped** (code rationale, accepted by Lead + operations) — superseding Revision 1's relocation of it.
- **All relocated notes are phrased as standing rules** ("do not X, because Y"), never as history.
- **Ownership tables** in alerts.md / dashboards.md: drop the `Last Updated` column with header/separator/row cell counts kept consistent.
- **Stale cite** (test): `crates/dt-guard/tests/no_insecure_browser_flags_e2e.rs` comment cites `devloop-validation.md:624` (already wrong — :624 is a `base-ref-unresolved` audit row; the contract is the §8 catalogue row `NOTE dt-guard allowlisted mention`, ~:810) and `run-guards.sh:183`. Replace with a § + row-name reference and "`run-guards.sh`'s exit-0 arm `^WARN ` grep". Comment-only, two lines.
- **Line cite into client-dev-local** (code-reviewer): `docs/observability/label-taxonomy.md` cites `docs/runbooks/client-dev-local.md:1283` for `dt_client_media_mute_transitions_total{action}` — already stale (the metric is at :1302, inside `### F12`), and the new §5 note would shift it further. Re-point to the anchor form `client-dev-local.md` §5 F12 (devloop-validation §10 cite convention). Post-edit check: `grep -rnE 'runbooks/[a-z-]+\.md:[0-9]+'` — remaining hits are `docs/TODO.md` historical entries (regions above/untouched) and a test fixture string, verified unaffected.
- `docs/TODO.md:873`'s "Version History" mention is a resolved historical entry, not an instruction — intentionally untouched.

### Gate 1 — Plan Approval

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed (after Revision 1) |
| Semantic Guard | not spawned |

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`. Plan approved by Lead.

Lead scope rulings: include `Last Updated` / `Document Version` / `Template Version` everywhere in runbooks plus observability/{runbooks,alerts,dashboards}.md and PROJECT_STATUS.md; keep `Last Reviewed`/`Next Review` and non-history Maintenance subsections (not git-derivable); exclude ADR-0020 Revision History (decision content); no new deny-guard.

### Gate 3 — Final Approval

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | All security-relevant history-row rules confirmed already in bodies |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | mc severity note range; §5 note F7-only |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | mc severity note range (F2 withdrawn — (b) dropped by Gate-1 agreement) |
| Code Quality | RESOLVED-FIXED | 3 | 3 | 0 | severity note; §5 note example + grep; main.md (b) consistency |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | TODO.md:949 bare-filename cite shifted by Version History removal |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | severity note restated set; accepted Lead ruling for number-free wording |
| Semantic Guard | — | — | — | — | not spawned |

Lead ruling (review): mc severity note uses number-free wording (no restated scenario set), over operations' preference to name Sc 11-13.

### Gate 2 — Full Validation (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`)

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 5s |
| 2 Format | OK | 5s |
| 3 Guards | OK | 136s |
| 4 Test | N/A (proto has no test verb; rust+ts passed) | 259s |
| 5 Lint | OK | 2s |
| 6 Audit | N/A (no-dep-changes) | 2s |
| 7 Env-tests | OK | 865s |

`TOTAL_RESULT=N/A`, exit 0. Attempts: layers 1-6 = 1, layer 7 = 1.

---

## Pre-Work

Lead note: INDEX.md and review-protocol.md were passed to teammates by path (each told to read them first) rather than inlined into spawn prompts.

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Hand-maintained history removed (git log is the SSoT)
| Item | Before | After |
|------|--------|-------|
| Runbook headers (all 11 incl. TEMPLATE) | `**Last Updated**: <date>` | `**History**: \`git log --follow -- docs/runbooks/<file>.md\`` (TEMPLATE: `[runbook-file].md` placeholder), same line |
| `## Changelog` / `## 11. Changelog` | TEMPLATE.md, client-dev-local.md (+ToC link), devloop-validation.md | deleted |
| `**Version History**:` lists | ac/gc/mc-incident-response.md | deleted; rest of Maintenance and Updates kept |
| `**Document Version**` / `**Template Version**` | 4 deployment runbooks, TEMPLATE.md | deleted; `Last Reviewed` / `Next Review` / `Template Source` kept |
| observability docs | runbooks.md per-runbook + footer dates, "Include Last Updated" rule; alerts.md / dashboards.md footer + `Last Updated` ownership column | removed; rule rewritten once in runbooks.md §Version Control |
| PROJECT_STATUS.md | `**Last Updated**` | removed |
| process-review-record.md | "Add PRR numbers to workflow changelogs" | removed |

### Non-history rules relocated
- mc-incident-response.md §Severity Classification: severity casing is mixed on purpose; don't normalize without an ADR-0031 follow-up.
- client-dev-local.md §5: maintainer note — heading carries no F-count (cites mc-service.md for the rule), F-headings have cross-runbook referrers, don't retitle without fixing them. ToC `(F1–F19)` dropped.
- devloop-validation.md §6.7 Lane discipline: one sentence — lane correctness = token + cause line.

### Line cites re-pointed to anchors
- `crates/dt-guard/tests/no_insecure_browser_flags_e2e.rs` (was `devloop-validation.md:624`, already stale; `run-guards.sh:183`).
- `docs/observability/label-taxonomy.md` (was `client-dev-local.md:1283`, already stale → § F12).
- `docs/TODO.md` Guard Coverage Gap entry (was `client-dev-local.md:740`; shifted by the ToC line removal → §4.5 rung 1).

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

### Layer 1: cargo check
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 2: cargo fmt
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 3: Simple Guards
**Status**: ALL PASS / X FAILED
**Duration**: ~Xs

| Guard | Status |
|-------|--------|
| api-version-check | PASS/FAIL |
| no-hardcoded-secrets | PASS/FAIL |
| no-pii-in-logs | PASS/FAIL |
| no-secrets-in-logs | PASS/FAIL |
| test-coverage | PASS/FAIL |

{Details on any failures}

### Layer 4: Unit Tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Test counts, any failures}

### Layer 5: All Tests (Integration)
**Status**: PASS/FAIL
**Duration**: ~Xs
**Tests**: {X passed, Y failed}

{Details on any failures}

### Layer 6: Clippy
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any warnings}

### Layer 7: Env-tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Wall-clock time for dev-cluster rebuild + env-test run; pass/fail summary; log path}

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

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

**This devloop:**

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `3ee98d55dd407ef6d780acd2330bc9f1bec59af6`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit (docs/comment-only; any partial revert is also safe, but relocated rules and their table deletions should move together) (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

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
