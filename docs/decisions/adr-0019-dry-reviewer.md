# ADR-0019: DRY Reviewer for Cross-Service Duplication Detection

**Status**: Accepted

**Date**: 2026-01-14

**Deciders**: Nathan, Claude Code Orchestrator

---

## Context

Dark Tower uses a specialist-led development model where each specialist is scoped to a single service:
- Auth Controller specialist owns `crates/ac-service/`
- Global Controller specialist owns `crates/gc-service/`
- etc.

This scoping is intentional and necessary to keep context manageable. However, it creates blind spots for cross-service code duplication.

**Problem discovered**: During GC Phase 2 implementation, the Global Controller specialist implemented JWT validation logic (`extract_kid`, `verify_token`, `MAX_JWT_SIZE_BYTES`) that already exists in Auth Controller (`extract_jwt_kid`, `verify_jwt`). The specialist had no visibility into AC's codebase and couldn't know the duplication existed.

**Impact**: Code duplication leads to:
- Maintenance burden (bugs fixed in one place but not the other)
- Inconsistent behavior across services
- Larger codebase than necessary

## Decision

**Add a DRY Reviewer specialist to code reviews with cross-service read access.**

The DRY (Don't Repeat Yourself) Reviewer participates in every code review alongside Security, Test, and Code Quality reviewers. Unlike service specialists, it has read-only access to ALL services and can detect duplication across service boundaries.

### Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Detection timing | Reactive (code review) | Proactive (pre-implementation) adds overhead to every task |
| Sensitivity | Balanced (50%+ similarity) | Too aggressive = noise; too conservative = misses duplication |
| Blocking behavior | Only BLOCKER severity blocks | Duplication is tech debt, not a showstopper (except for existing common code) |
| Who fixes | Orchestrator creates follow-up task | Cross-service changes shouldn't be done by scoped specialists |

### Severity Definitions

| Severity | Trigger | Blocking? | Action |
|----------|---------|-----------|--------|
| 🔴 BLOCKING | Code EXISTS in `common` but wasn't used | **Yes** | Must fix before approval |
| 📋 TECH_DEBT | Similar code exists in another service | No | Document, create follow-up task |

**Note**: The unified severity model uses TECH_DEBT for all non-blocking findings. This simplifies the blocking rule to: "All findings block EXCEPT TECH_DEBT."

### Blocking Behavior (Different from Other Reviewers)

This is the critical distinction:

| Reviewer | Blocking Behavior |
|----------|-------------------|
| Security | ALL findings must be fixed |
| Test | ALL findings must be fixed |
| Code Quality | ALL findings must be fixed |
| **DRY Reviewer** | Only BLOCKER blocks; others documented as tech debt |

**Why different?**
- Implementing something that **already exists in common** = Must use existing code (BLOCKER)
- Implementing something that **could be extracted to common** = Ship now, refactor later (non-blocking)

### Cross-Cutting Change Ownership

When DRY Reviewer flags duplication (non-BLOCKER):

1. Finding documented in devloop output under "Tech Debt: Cross-Service Duplication"
2. Current task completes with known duplication
3. New task created: "Extract shared {pattern} to common crate"
4. Orchestrator executes extraction task (not scoped to single service)

The orchestrator handles cross-service refactoring because:
- It's not scoped to a single service
- It can coordinate changes across multiple services + common
- It can invoke appropriate specialists for review

## Consequences

### Positive

- **Catches cross-service duplication** that scoped specialists miss
- **Shared code grows organically** as patterns are identified
- **Tech debt is tracked** rather than invisible
- **Doesn't block velocity** - non-BLOCKER findings don't stop work

### Negative

- **One more reviewer** in code review cycle (adds latency)
- **May generate noise** for similar-but-distinct code
- **Duplication may ship** before being refactored

### Neutral

- **Common crate grows** over time as shared patterns are extracted
- **Follow-up tasks created** for extraction work

## Implementation

### Files Created

- `.claude/agents/dry-reviewer.md` - Specialist definition
- This ADR

### Files Modified

- `.claude/workflows/code-review.md` - Add DRY reviewer to participants
- `.claude/workflows/development-loop.md` - Mention DRY reviewer, tech debt handling
- `AI_DEVELOPMENT.md` - Add to specialist list
- `CLAUDE.md` - Mention in code review section
- `docs/devloop-outputs/_template.md` - Add Tech Debt section

## References

- ADR-0016: Development Loop with Guard Integration
- ADR-0017: Specialist Self-Improvement via Dynamic Knowledge
- `.claude/workflows/code-review.md` - Code review process
- `.claude/workflows/development-loop.md` - Dev-loop workflow
