# Devloop Output: ADR-0024 §6 Skill Rollout (Items #17 + #18)

**Date**: 2026-04-18
**Task**: Update `.claude/skills/devloop/SKILL.md` and `.claude/skills/devloop/review-protocol.md` per ADR-0024 §6
**Specialist**: code-reviewer
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `12c18a50156f3144082ab14853d16967670a4d9b` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Iteration | `2` |
| Implementer | `implementer@devloop-adr-0024-skill-rollout` |
| Implementing Specialist | `code-reviewer` |
| Security | `security@devloop-adr-0024-skill-rollout` (RESOLVED) |
| Test | `test@devloop-adr-0024-skill-rollout` (RESOLVED) |
| Observability | `observability@devloop-adr-0024-skill-rollout` (CLEAR) |
| Code Quality | `N/A (implementer is code-reviewer)` |
| DRY | `dry-reviewer@devloop-adr-0024-skill-rollout` (RESOLVED) |
| Operations | `operations@devloop-adr-0024-skill-rollout` (RESOLVED) |

Note: the Code Quality reviewer slot is collapsed into the implementer role since code-reviewer IS the implementer. Five reviewers (security, test, observability, dry-reviewer, operations) instead of the standard six.

---

## Task Overview

### Objective

Operationalize ADR-0024 §6 (Cross-Boundary Ownership Model) by updating the two skill surfaces that govern future devloops:
- `.claude/skills/devloop/SKILL.md` — add §Cross-Boundary Edits, three-tier classification table, Paired flag argument, Guarded Shared Areas enumeration, default-posture flip
- `.claude/skills/devloop/review-protocol.md` — add Step 0 Guarded Shared Areas scoping, spin-out as third fix-or-defer path, Ownership-lens verdict field, sed-test worked example

**Stretch goal**: net SKILL.md smaller after the additions via pruning redundancy and tightening prose. The skill is already dense; larger skills are harder to implement consistently.

### Scope

- **Service(s)**: None (skill files only)
- **Schema**: No
- **Cross-cutting**: Yes — skill governs all future devloops across all services

### Debate Decision

NEEDED — completed. See `docs/debates/2026-04-18-devloop-cross-ownership-friction/debate.md`, resulting in ADR-0024 §6 amendment at commit `12c18a5`.

---

## Planning

Gate 1 passed. All 5 reviewers confirmed (observability, security, test, operations, dry-reviewer). Plan summary:

**SKILL.md additions:**
- New §Cross-Boundary Edits (after §Team Composition) covering three-tier classification (Mechanical / Minor-judgment / Domain-judgment), owner-involvement table (review-only / hunk-ACK via trailer / owner-implements), Guarded Shared Areas (criterion + enumerated list inline + stricter-inside property), default-posture flip for Mechanical outside GSA, `Approved-Cross-Boundary:` trailer reference, classification workflow with monotonic-upgrade rule
- `--paired-with=<specialist>` flag added to Arguments at top (§6.5: flag not mode, N=1 exemplar, N≥4 sweep pattern, does not exempt GSA)

**review-protocol.md additions:**
- Step 0 amendment: GSA scoping bullet (paths/criterion + annotations from §6.4 inline)
- Fix-or-Defer Model: Spin-out as third path alongside Fix and Defer (operations' "next scheduled devloop for owning specialist" trigger, "returns to current devloop on abandonment" hedge)
- Ownership Lens verdict field with per-classification reviewer question
- Three-anchor sed-test worked example: FU#3a-status clean pass / FU#3c co-sign required / jwt.rs GSA override

**Size-reduction plan (net target: −29 lines):**
- Merge two team-composition roster tables (L43-51 + L192-200) into one four-column table
- Extract Continue Mode implementer prompt duplication (~16 lines verbatim)
- Compress Workflow Overview ASCII diagram (keep step headers authoritative)
- Compress Layer 8 validation prose by ~30%
- Compress Idle ≠ Done from 7 lines to 3 while preserving semantic content
- Collapse When-to-Use section (preserve `/debate` pointer + "never manually spawn via Task tool" guardrail)

**Mitigations for 3-way GSA inline:**
- Anchor-of-truth HTML comment at each of the 3 sites (SKILL.md + review-protocol.md + ADR-0024 §6.4): "Mirror of ADR-0024 §6.4 enumerated list. Update all three locations together when extending via micro-debate."
- Follow-up tech-debt item: ~15 LOC guard to diff the three locations

**Policy flag** (open): if a fourth location wants GSA inlined during implementation (e.g., code-reviewer ADR-compliance checklist), escalate before adding — don't accumulate sync-drift surface silently.

---

## Implementation Summary

TBD — populated during implementation phase.

---

## Files Modified

TBD.

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Escalated |
|----------|---------|----------|-------|----------|-----------|
| Security | RESOLVED | 3 | 3 | 0 | 0 |
| Test | RESOLVED | 1 | 1 | 0 | 0 |
| Observability | CLEAR | 0 | — | — | — |
| DRY | RESOLVED | 1 | 1 | 0 | 0 |
| Operations | RESOLVED | 1 | 1 | 0 | 0 |
| Code Quality | N/A (implementer is code-reviewer) | — | — | — | — |

All findings fixed. No escalations.

### Security Specialist — RESOLVED
1. Spin-out-for-GSA-trailer-missing ambiguous (review-protocol.md) → fixed: clarified current devloop not required to carry missing owner's trailer when spun out
2. Trailer casing inconsistent (TODO.md) → fixed: canonical mixed-case `Approved-Cross-Boundary:` with note disambiguating ADR §6.8 all-caps emphasis
3. Cross-reference anchor missing (SKILL.md:30) → fixed: "see §Cross-Boundary Edits below and ADR-0024 §6.5"

### Test Specialist — RESOLVED
1. Monotonic-upgrade rule missing from review-protocol.md (reviewers read this file on spawn) → fixed: added `### Classification Monotonicity (Ownership Lens)` subsection

### Operations Specialist — RESOLVED
1. Minor-judgment `for:` example abstract, missing runbook/convention coupling → fixed: expanded wording naming runbook prose + alert-conventions doc coupling, hunk-ACK by operations required

### DRY Reviewer — RESOLVED
1. Anchor-of-truth HTML comment missing at ADR-0024 §6.4 (source of truth) → fixed: added matching comment at ADR line 389 with source-of-truth wording

### Observability Specialist — CLEAR
All concerns landed without findings.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit: `12c18a50156f3144082ab14853d16967670a4d9b`
2. Review all changes: `git diff 12c18a5..HEAD`
3. Soft reset (preserves changes): `git reset --soft 12c18a5`
4. Hard reset (clean revert): `git reset --hard 12c18a5`

---

## Reflection

All 6 reviewing teammates (including implementer as code-reviewer) updated `docs/specialist-knowledge/{name}/INDEX.md` with a pointer to ADR-0024 §6 + the relevant skill-file subsection. Two specialists (security, test) consolidated existing pointers to stay at the 75-line soft cap. `validate-knowledge-index.sh` clean.

Pending INDEX pointers (not in this devloop's scope; to be added when those specialists participate in a §6-adjacent devloop): auth-controller, global-controller, meeting-controller, media-handler, protocol, database, client, infrastructure.

## Human Review (Iteration 2)

**Feedback**:
- H1: Teammate Roster missing row for `--paired-with=<specialist>` (no `name`/`subagent_type` documented)
- H2: Classification authority stated three times with different voices (impersonal L89, actor-named L97, reminder L232)
- H3/H4: questions about workflow-overview + Continue-mode "your workflow" removals (resolved without change — user accepted analysis that removals were redundant with downstream Instructions Steps)

**Fixes**: H1 added Paired Specialist roster row with overlap-with-standard-reviewer note; H2 consolidated classification-authority voice into L89 up-front, shortened L97 to just Pattern A/B/C + named-convention-author sentence; Step 3 implementer prompt (L232) kept as operational spawn-time reminder.

**Result**: 18/18 guards still pass, INDEX guard clean, SKILL.md +1 net line from original target (650 → 617, -33 vs -29 target).

## Sequencing Decision (User)

New skill behavior (§Cross-Boundary Edits) will **not** be operationally used until §6.8 guards land. Next session picks up the new skill via fresh load.

## Named §6.8 follow-up devloops (captured in TODO.md)

1. `validate-cross-boundary-approval.sh` guard — operations + test + security
2. Classification-failure fixture suite — test (AC + security consult)
3. Scope/claim/session-field rename guard — security + operations + test
4. GSA 3-way sync guard — dry-reviewer

Three Lead-side mitigations (main.md template classification slot, Gate 2 manual trailer-scan note, §Cross-Boundary Edits opening sharpening) fold into these follow-ups rather than being pre-emptive.
