# ADR-0017: Specialist Self-Improvement via Dynamic Knowledge

**Status**: Accepted

**Date**: 2026-01-10

**Deciders**: Nathan, Claude Code Orchestrator

---

## Context

As the Dark Tower codebase grows, specialists face increasing cognitive load:
- More patterns to remember across the codebase
- More edge cases and gotchas discovered through implementation
- More integration points with other services
- Conventions that emerged after their core prompts were written

**Problem**: Specialist definitions in `.claude/agents/*.md` are static. They encode domain expertise but cannot accumulate project-specific knowledge over time. This means:
- Specialists re-discover the same gotchas repeatedly
- Patterns established in one session are forgotten in the next
- Integration notes between services aren't captured
- The "institutional memory" of the project lives only in code and comments

**Goal**: Allow specialists to accumulate domain knowledge over time without modifying their core prompts, while maintaining human oversight and keeping knowledge files in version control.

## Decision

**We adopt a layered knowledge architecture where specialists maintain dynamic knowledge files alongside their static definitions.**

### Architecture

```
.claude/agents/auth-controller.md           <- Static core (human-controlled)
.claude/agents/auth-controller/             <- Dynamic knowledge (specialist-maintained)
  ├── patterns.md                           <- "Here's how we do X"
  ├── gotchas.md                            <- "Watch out for Y"
  └── integration.md                        <- "When calling Z, remember..."
```

### Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Architecture | Layered (static + dynamic) | Clear separation between human-controlled core and specialist-learned knowledge |
| File updates | Specialists update directly | Files are in git, human reviews all diffs before committing |
| Sharing | No direct sharing | Cross-pollination happens naturally via debates and code reviews |
| Who reflects | All involved specialists | Implementer + reviewers all get reflection chance |
| When to reflect | After clean code review | Captures insights while context is fresh, before loop exit |
| Pruning | During reflection | Specialists identify outdated knowledge when code changes |

### Context Injection Order

When building specialist prompts, inject context in this order:

1. Specialist definition (`.claude/agents/{specialist}.md`)
2. Matched principles (`docs/principles/`)
3. Specialist knowledge (`.claude/agents/{specialist}/*.md` if exists)
4. Design context (ADR summary if from debate)
5. Task context (actual task and existing patterns)

### Knowledge File Format

Each knowledge file follows a structured format:

```markdown
# Patterns (or Gotchas, or Integration)

## Pattern: Descriptive Title
**Added**: YYYY-MM-DD
**Related files**: `src/path/to/file.rs`, `src/another/file.rs`

Brief description of the pattern, gotcha, or integration note.
Keep it concise (2-4 sentences max).
```

Guidelines:
- ~100 lines per file limit
- Each item has Added date and Related files
- Keep descriptions brief and actionable
- Use H2 headers for each item

### Reflection Step

After code review is clean but before exiting the development loop:

1. All involved specialists (implementer + reviewers) reflect
2. Each specialist updates their knowledge files directly
3. Changes appear in git diff alongside implementation
4. User reviews everything and commits when satisfied

### Bootstrap Process

When a specialist reflects for the first time (no knowledge directory exists):

1. Specialist creates `.claude/agents/{specialist}/` directory
2. Creates initial `patterns.md`, `gotchas.md`, `integration.md`
3. Populates with knowledge based on existing code and the task just completed
4. User sees new files in git diff and can review/approve

## Consequences

### Positive

- **Cumulative learning**: Specialists get better over time
- **Reduced context window**: Knowledge files are smaller than repeating full explanations
- **Version controlled**: All knowledge in git, reviewable, revertable
- **Human oversight**: User approves all changes via normal git workflow
- **No sharing conflicts**: Each specialist owns their own knowledge
- **Natural pruning**: Outdated knowledge removed when code changes

### Negative

- **File management overhead**: More files in `.claude/agents/`
- **Potential staleness**: Knowledge could become outdated if not pruned
- **Initial bootstrap**: First reflection takes longer as specialists seed their knowledge
- **Review burden**: User must review knowledge updates along with code

### Neutral

- **No automatic sharing**: Specialists don't directly read each other's knowledge (cross-pollination via debate/review instead)
- **Reflection happens every task**: Could skip if nothing learned, but explicit check is cheap

## Alternatives Considered

### Alternative A: Monolithic Knowledge

Store all learned knowledge in a single project-wide file.

- **Pros**: Single source of truth, easier to search
- **Cons**: All specialists modify same file (conflicts), no domain separation, harder to prune

### Alternative B: Database-backed Knowledge

Store knowledge in a database rather than files.

- **Pros**: Structured queries, automatic deduplication
- **Cons**: Not in git, harder to review, infrastructure dependency, loses human oversight

### Alternative C: No Persistent Knowledge

Keep specialists stateless, rely only on static definitions.

- **Pros**: Simpler, no file management
- **Cons**: Re-discover same issues repeatedly, no cumulative learning

## Implementation Notes

### Files Modified

- `.claude/agents/*.md` (all 12 specialists) - Added "Dynamic Knowledge" section
- `.claude/workflows/development-loop.md` - Added Part 7: Reflection, updated context injection
- `.claude/workflows/code-review.md` - Updated to inject reviewer knowledge
- `.claude/workflows/multi-agent-debate.md` - Updated to inject debater knowledge

### Knowledge Directories

Created on-demand by specialists during their first reflection:
- `.claude/agents/{specialist}/patterns.md`
- `.claude/agents/{specialist}/gotchas.md`
- `.claude/agents/{specialist}/integration.md`

### Rollout

1. **Phase 1 (This PR)**: Infrastructure only - workflow updates, no knowledge files yet
2. **Phase 2 (First Real Task)**: Specialists bootstrap their knowledge during first reflection
3. **Phase 3 (Organic Growth)**: Knowledge accumulates naturally as work continues

## References

- Development Loop: `.claude/workflows/development-loop.md`
- Code Review: `.claude/workflows/code-review.md`
- Multi-Agent Debate: `.claude/workflows/multi-agent-debate.md`
- Related: ADR-0016 (Development Loop with Guard Integration)
