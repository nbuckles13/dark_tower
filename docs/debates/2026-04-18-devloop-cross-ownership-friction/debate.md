# Debate: `/devloop` Cross-Ownership Friction on Small Changes

**Date**: 2026-04-18
**Status**: Complete — consensus reached Round 2
**Outcome**: ADR-0024 §6 (Cross-Boundary Ownership Model) amendment
**Participants**: security, test, observability, operations, auth-controller, global-controller, meeting-controller, media-handler, code-reviewer, dry-reviewer, protocol

> **Note**: When cross-cutting specialists (Security, Test, Observability, Operations) score < 70 satisfaction at consensus, this requires explicit user risk acceptance — not implicit majority override. See ADR-0024 §5.7.

## Question

Should `/devloop`'s ownership-boundary rules be formalized, and if so, what should change in `.claude/skills/devloop/SKILL.md` and adjacent skill/protocol/ADR files?

## Context

2026-04 sessions (primarily the ADR-0031 service-owned-dashboards-and-alerts rollout + follow-ups) surfaced persistent friction where the implicit `/devloop` rule "the file's owner specialist implements changes to it" produced disproportionate ceremony for mechanical cross-boundary edits.

### Motivating cases

1. **ADR-0031 alert-rules devloop (2026-04-17)**: operations implementer needed to touch `gc-alerts.yaml` and `mc-alerts.yaml` (~60 lines of mechanical YAML: severity renames + URL rewrites) to make a newly-authored guard pass against existing files. The "that's GC/MC territory" framing produced a grandfather-allowlist mechanism (~80 LOC of guard complexity + TODO entries + follow-up devloop slots) to avoid the edit. In retrospect, the edits were mostly mechanical with 2-3 genuine judgment calls; the allowlist was disproportionate.
2. **MCActorPanic `for: 0m` fix**: one-line change to `mc-alerts.yaml` by operations. Required explicit Lead ruling to authorize. Should have been trivial.
3. **FU#3a AC path → endpoint**: auth-controller rename of `path` → `endpoint` in AC's metrics.rs. Also touched `ac-overview.json` dashboard and `ac-service.md` catalog. Clean because AC owned all three surfaces. Compare to FU#3c below where the rename crossed services.
4. **FU#3c MC + MH `event` → `event_type`**: cross-service rename. meeting-controller specialist implemented BOTH MC's metrics.rs AND MH's metrics.rs changes. MH specialist cross-checked the MH hunks as reviewer. This worked well — the "mechanical is mechanical" posture was explicit in the devloop brief, and the MH reviewer caught semantic concerns that the cross-boundary-editing specialist couldn't have known.
5. **MC heartbeat two-tier consolidation (FU#2)**: meeting-controller specialist touched only their own file but made a severity-classification judgment that deserved observability's taxonomy-review. Pair-with-reviewer pattern worked cleanly.

### The tension

The implicit "owner implements" rule is correct for changes requiring domain judgment (threshold tuning, behavior changes, API semantics) but produces disproportionate ceremony when:

- The change is mechanical (renames, format conformance, path updates, comment fixes)
- The change is a minor defensive adjustment (bumping a `for:` duration up to match convention)
- The file-touching is incidental to the primary work (convention-driven cleanup that naturally spans services)

In those cases the ownership-fetish produces elaborate workarounds, multiple devloops where one would do, Lead-level adjudication thrash, and infrastructure we delete in months.

### Known design axes

1. **Define "acceptable cross-boundary edit."** Options:
   - By size (`≤ N lines`) — crude, easily gamed, wrong axis. A 5-line threshold tune is high-judgment; a 100-line sed is not.
   - By change category — mechanical vs. minor-judgment vs. domain-judgment. Tracks what matters but requires implementer self-classification.
   - By file path × change pattern — e.g., "any specialist may rename across the tree; only the owner may change semantics." Probably cleanest rule but needs careful category definitions.

2. **Owner involvement model**. Three levels, probably all needed:
   - **Review-only** — owner sees it in the standard reviewer gate.
   - **Approval-required** — owner must explicitly ACK the specific cross-boundary hunk (not just the overall PR).
   - **Owner-implements** — route to a separate devloop with owner as implementer.

3. **Ownership detection**. The skill already has a keyword → specialist map for auto-detection; extending to a file-path → specialist map is straightforward. But needs care for shared areas (`crates/common/**`, `proto/**`, `docs/observability/**`, `scripts/guards/**`).

4. **Default posture**. When an implementer surfaces a cross-boundary edit, should the default be "proceed with review" or "defer to owner"? The just-finished sessions showed that "defer" as a default produces large ceremony costs. But flipping to "proceed" risks specialists stepping on each other's core domains.

### Constraints the debate should respect

- **Don't overcomplicate the skill.** Every rule added to `/devloop`'s SKILL.md increases the Lead's coordination surface. The skill is already dense; the Lead has finite attention.
- **Preserve genuine cross-cutting safety.** `crates/common/**`, `proto/**`, and auth-critical paths genuinely need multi-specialist involvement. Any rule must not weaken those.
- **The simplest rule beats the cleverest mechanism.** The allowlist's complexity came from trying to satisfy contradictory scope adjudications; a clear default would have prevented the whole thing.

### Desired output

1. An ADR codifying the ownership rules — either a new ADR or an amendment to `adr-0024-agent-teams-workflow.md`.
2. A **list of surfaces to update** with the new rules, each surface tagged with its owning specialist for the follow-up devloop(s). Expected surfaces likely include:
   - `.claude/skills/devloop/SKILL.md`
   - `.claude/skills/devloop/review-protocol.md`
   - `.claude/agents/*.md` specialist definitions (if per-specialist guidance is needed)
   - `adr-0024-agent-teams-workflow.md` (amend if needed)
   - Possibly `.claude/skills/user-story/SKILL.md` (decomposition rules)
   - Possibly `.claude/skills/debate/SKILL.md` (participant selection)

Each participant should name their own relevant surfaces during the debate so the ADR's follow-up action list is complete.

## Positions

### Final Positions (Round 2)

| Specialist | Position (summary) | Satisfaction |
|------------|--------------------|--------------|
| protocol | Wire-visibility carve-out; amend ADR-0024 over new ADR | 94 |
| global-controller | Paired-as-flag reframe; authz-enforcement surface sensitivity | 94 |
| meeting-controller | Value-neutrality clause; classification monotonicity; webtransport GSA | 93 |
| security | Conditional asks: audit path in GSA + commit-trailer relocation | 92 |
| test | Guard-coverage conditional; hunk-ACK; classification-failure fixture | 92 |
| observability | "Mechanical iff guards catch every partial version"; Paired as flag | 92 |
| operations | Co-owns §6; commit-trailer guard; db/migrations GSA; concept-substitution exclusion | 92 |
| auth-controller | Catch-all for future auth/crypto files in common/ | 92 |
| media-handler | Extending GSA = micro-debate, not new ADR; rule stricter inside GSA | 92 |
| code-reviewer | Co-owns §6; review-protocol.md ownership; sed-test backbone | 92 |
| dry-reviewer | Pattern A/B/C; expanded common/ GSA list; common/ non-ownership clarification | 90 |

## Discussion Summary

**Round 1** (initial positions): 11 specialists posted positions ranging from 45 (DRY, initially hesitant on cross-ownership relaxation) to 85 (protocol, aligned from the start). Key contributions surfaced:
- code-reviewer: sed-test backbone for Mechanical category definition
- dry-reviewer: Pattern A/B/C categorization
- protocol: wire-visibility carve-out identification
- security: two-layer enforcement insistence (classification + Guarded Shared Areas)
- observability: guards-as-safety-net framing
- meeting-controller: monotonic-upgrade rule for classification
- media-handler: rule-is-stricter-inside-GSA counter-intuitive insight
- operations: commit-trailer vs thread-marker durability argument

**Round 2** (consolidated proposal): Lead broadcast a unified framework incorporating Round 1 contributions. Convergence was remarkable — most specialists moved to 90+ within one exchange. Final refinements:
- Security (86→92 conditional): audit path in GSA; commit-trailer relocation
- Observability correction: Paired is a flag, not a mode
- Test amendments: guard-coverage conditional; hunk-ACK for GSA; classification-failure fixture
- DRY clarifications: common/ non-ownership outside GSA; Pattern B named-author requirement; Paired N-ceiling guidance
- MC/Protocol joint: surface precedence overrides category on GSA; classification challenges auto-ESCALATE

## Consensus

All 11 participants ≥ 90 satisfaction at Round 2 close. Average satisfaction: 92.3%.

No cross-cutting dissent triggered — all four mandatory cross-cutting specialists (security, test, observability, operations) at 92. No explicit user risk acceptance required.

## Decision

**Amend ADR-0024 with new §6 "Cross-Boundary Ownership Model"** (chosen over new ADR for the reason protocol gave: rule is workflow-shaped, not architectural, and belongs alongside the existing workflow ADR rather than standing alone).

**Core framework**:
- Three-category classification: Mechanical / Minor-judgment / Domain-judgment
- Three owner-involvement tiers: review-only / hunk-ACK / owner-implements
- Category × tier mapping with reviewer-monotonic upgrade rule
- **Guarded Shared Areas** surface-precedence carve-out (wire / auth / detection / schema)
- **Paired flag** orthogonal overlay, not a third mode
- **APPROVED-CROSS-BOUNDARY** as git commit trailer (durable record)

See: `docs/decisions/adr-0024-agent-teams-workflow.md` §6.

## Debate Record References

- Original ADR-0024 debate: `docs/debates/2026-02-10-agent-teams-workflow-review/debate.md`
- Motivating case: ADR-0031 alert-rules devloop (2026-04-17)
