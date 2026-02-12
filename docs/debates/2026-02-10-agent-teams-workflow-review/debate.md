# Debate: Agent Teams Workflow Review

**Date**: 2026-02-10
**Status**: Complete
**Participants**: Security, Test, Observability, Operations, Code-Reviewer, DRY-Reviewer, Auth-Controller, Global-Controller, Meeting-Controller, Media-Handler, Database, Protocol, Infrastructure

> **Note**: Cross-cutting specialist dissent (Security, Test, Observability, Operations scoring < 70) requires explicit user risk acceptance — this is informed risk acceptance, not implicit majority override. See ADR-0024 §5.7.

## Question

Review and evaluate the new Agent Teams-based `/dev-loop` and `/debate` workflows. Are these processes sound? What improvements should be made? The output will be an ADR (ADR-0024) documenting the current workflow design and any agreed improvements.

## Context

Dark Tower's development process evolved through 4 iterations (see AI_DEVELOPMENT.md):
1. Autonomous orchestrator (Claude drives everything) - failed due to skipped steps
2. Step-runner architecture - failed due to context accumulation
3. Skill-based multi-step (user invokes `/dev-loop-init`, `/dev-loop-implement`, etc.) - worked but coordinator context still rotted
4. **Agent Teams** (current) - single `/dev-loop` command, autonomous teammates, Lead only at gates

The old multi-step skills have just been retired. `/dev-loop` and `/debate` are now the sole workflows. This debate reviews whether the current design is sound and what the ADR should capture.

### Key Process Documents Under Review

1. **Dev-loop workflow**: `.claude/skills/dev-loop/SKILL.md`
2. **Debate workflow**: `.claude/skills/debate/SKILL.md`
3. **Review protocol**: `.claude/agent-teams/protocols/review.md`
4. **Debate protocol**: `.claude/agent-teams/protocols/debate.md`
5. **Specialist definitions**: `.claude/agent-teams/specialists/*.md`
6. **Restore workflow**: `.claude/skills/dev-loop-restore/SKILL.md`
7. **Status utility**: `.claude/skills/dev-loop-status/SKILL.md`

## Positions

### Initial Positions (Round 1)

| Specialist | Position | Satisfaction |
|------------|----------|--------------|
| Security | Missing cargo audit in 7-layer; debate shouldn't override security dissent; guard:ignore needs justification | 78 |
| Test | verify-all.sh doesn't exist; no coverage regression detection; test reviewer authority unclear | 72 |
| Observability | **CRITICAL: Excluded from dev-loop team despite being mandatory cross-cutting** | 45 |
| Operations | Mostly sound; needs rollback procedure; checkpoint granularity insufficient | 78 |
| Code-Reviewer | ADR compliance implicit not explicit; review protocol needs scoping guidance | 68 |
| DRY-Reviewer | Well-positioned; review protocol should mention modified severity model (BLOCKER/TECH_DEBT) | 82 |
| Auth-Controller | Practical for substantial work; auto-detection missing key/rotation/jwks patterns; needs lightweight mode | 82 |
| Global-Controller | Missing Protocol reviewer; auto-detection ambiguity; debate should produce implementation guidance | 72 |
| Meeting-Controller | Single-implementer insufficient for cross-service changes; needs lightweight variant for small changes | 72 |
| Media-Handler | No performance benchmarks in verification; dev-loop overhead too heavy for micro-optimizations | 72 |
| Database | Pending | - |
| Protocol | Pending | - |
| Infrastructure | Pending | - |

**Round 1 Average (10 of 13)**: 72.0 — Significant improvements needed before consensus

## Discussion

### Round 1 Summary

**Critical Finding**: Observability specialist (satisfaction: 45) identified that Observability is missing from the dev-loop team composition despite being a mandatory cross-cutting specialist. Current team is 6 (Lead + Implementer + Security + Test + Code Quality + DRY + Operations) but Observability is excluded.

**Key Themes Emerging**:
1. **Observability gap** — Add Observability as reviewer in dev-loop (making 7 teammates total)
2. **verify-all.sh doesn't exist** — Referenced in skill but never created
3. **Lightweight variant needed** — Full dev-loop is too heavy for small changes
4. **Auto-detection patterns incomplete** — Multiple specialists report gaps in their domain keywords
5. **Security dissent protection** — Debate shouldn't majority-override security concerns
6. **ADR compliance should be explicit** — Not just implicit in code review
7. **Rollback/git-state tracking** — Dev-loop should track git state for recovery
8. **Debate → implementation gap** — ADRs should include implementation guidance
9. **Multi-implementer support** — Cross-service changes need sequential dev-loops or multi-implementer

### Round 2

Lead proposed 10 concrete improvements addressing all Round 1 themes. Specialists evaluated, formed coalitions, and refined proposals through bilateral discussions.

### Round 3

ADR-0024 drafted incorporating all agreed improvements. Specialists verified their concerns were captured. Minor refinements made (migration safety mandatory, conditional DB reviewer, proto freshness check, plan confirmation checklist, light mode exclusions expanded). All 13 specialists reached 92%+ satisfaction.

## Consensus

**Consensus reached at Round 3** with 93.7% average satisfaction (all participants ≥ 92%).

### Final Satisfaction Scores

| Specialist | Score | Accept |
|-----------|-------|--------|
| Security | 93 | Yes |
| Test | 92 | Yes |
| Observability | 95 | Yes |
| Operations | 95 | Yes |
| Code-Reviewer | 93 | Yes |
| DRY-Reviewer | 93 | Yes |
| Auth-Controller | 95 | Yes |
| Global-Controller | 93 | Yes |
| Meeting-Controller | 95 | Yes |
| Media-Handler | 94 | Yes |
| Database | 92 | Yes |
| Protocol | 95 | Yes |
| Infrastructure | 93 | Yes |

## Decision

ADR-0024 created: `docs/decisions/adr-0024-agent-teams-workflow.md`

Key decisions:
1. Add Observability as 7th dev-loop reviewer (fixes CLAUDE.md policy inconsistency)
2. Define concrete validation pipeline (6 enforced + 1 reported + artifact-specific layers)
3. Cross-cutting specialist veto protection in debate escalation (< 70 → user risk acceptance)
4. Git state tracking + rollback procedure in every dev-loop
5. Lightweight `--light` variant for small, safe changes
6. Cross-service implementation model (Tier A debate + sequential, Tier B coordination brief)
7. Expanded auto-detection patterns with disambiguation rule
8. ADR template enhancements (Implementation Guidance, Protocol Constraints, Migration Plan)
9. Review protocol improvements (Step 0 scoping, ADR compliance, plan confirmation, guard:ignore justification)
10. Restore pre-flight verification
