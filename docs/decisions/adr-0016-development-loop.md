# ADR-0016: Development Loop with Guard and Code Review Integration

**Status**: Accepted (Updated 2026-01-10)

**Superseded By**: ADR-0022 (Skill-Based Development Loop) - The workflow files described below have been replaced by executable skills. See ADR-0022 for the current approach.

**Date**: 2026-01-08

**Deciders**: Nathan, Claude Code Orchestrator

---

## Context

Dark Tower uses specialist agents to implement features. Without structured verification, specialists may produce code that:
- Compiles but fails tests
- Passes tests but violates project principles (caught by guards)
- Requires multiple back-and-forth iterations to fix

We observed the [Ralph-Wiggum](https://github.com/anthropics/claude-code/tree/main/plugins/ralph-wiggum) pattern: iterative loops where Claude retries until verification passes. This approach works well when completion criteria are deterministic.

**Our completion criteria are deterministic**:
- `cargo check` - compilation
- `./scripts/guards/run-guards.sh` - principle enforcement
- `cargo test` - behavioral correctness
- `cargo clippy` - lint warnings

**Problem**: How do we integrate these checks into a structured development flow that:
1. Gives specialists clear verification expectations upfront
2. Provides actionable feedback on failures
3. Escalates to human when automation can't resolve issues
4. Integrates with existing debate and code review workflows

## Decision

**We adopt a Development Loop workflow with integrated code review, 5-attempt limit, and human collaboration escalation.**

### Core Design

```
┌──────────────────────────────────────────────┐
│    Development Loop (with Code Review)        │
├──────────────────────────────────────────────┤
│  1. Specialist invocation (iteration N)      │
│  2. Run verification (7 layers)              │
│     - compile → fmt → guards → tests →       │
│       clippy → semantic guards               │
│  3. If FAIL → retry (back to 1)              │
│  4. Run code review (per code-review.md)     │
│  5. If findings → retry with findings        │
│  6. If clean → Done, ready to commit         │
│  COLLABORATION: If iteration > 5             │
│     → Stop and collaborate with human        │
└──────────────────────────────────────────────┘
```

### Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Implementation | Custom (not Ralph plugin) | Tighter integration with specialist workflow |
| Failure routing | Same specialist retries | Simpler; they wrote the code, they fix it |
| Verification level | Full (all checks) | All guards + tests + clippy must pass |
| Max iterations | 5 attempts | Increased from 3 to accommodate code review fixes |
| Code review integration | All findings blocking | Any review finding triggers retry until clean |
| Code review workflow | Reference existing | `code-review.md` used, not duplicated |
| Workflow files | Combined | Single `development-loop.md` replaces `contextual-injection.md` |
| Loop trigger | Implicit + announcement | Auto-use; announce "Starting development loop (max 5 iterations, includes code review)" |

### Verification Script

Created `scripts/verify-completion.sh` that runs layered checks:

| Layer | Check | Speed |
|-------|-------|-------|
| 1 | `cargo check` | ~5s |
| 2 | `cargo fmt` | ~2s (auto-formats in place) |
| 3 | Simple guards | ~2s |
| 4 | `cargo test --lib` | ~30s |
| 5 | `cargo test` (all) | ~1-2min |
| 6 | `cargo clippy` | ~30s |
| 7 | Semantic guards | ~30s+ (LLM-based, runs last) |

Exit codes: 0 = pass, 1 = fail with report

### Code Review Step

After verification passes, code review runs per `code-review.md`:
- Same specialist reviewers (Code Reviewer, Security, Test, Observability)
- All findings are blocking (BLOCKER through SUGGESTION)
- Findings formatted like verification failures for retry prompt

### Collaboration Mode

After 5 failed attempts (covering both verification and code review fixes), the loop stops and presents:
- Current failures with details (verification OR review)
- History of what was attempted at each stage
- Suggested next steps

Human can then: provide guidance, adjust task, involve another specialist, or debug together.

### Integration Points

| Workflow | Relationship |
|----------|--------------|
| `multi-agent-debate.md` | BEFORE loop - produces ADR for design context |
| `code-review.md` | INTEGRATED into loop (step 4) - also usable standalone |
| `development-loop.md` | Central implementation workflow |

## Consequences

### Positive

- **Clear expectations**: Specialists know exactly what checks will run
- **Fast feedback**: Layered verification catches simple issues quickly
- **Bounded iteration**: 5-attempt limit prevents runaway costs
- **Human escalation**: Hard problems get human insight instead of infinite retries
- **Unified workflow**: Context injection + verification + code review in one flow
- **End-to-end quality**: Code review integrated means no manual step before commit
- **Clean reviews**: All findings blocking ensures high-quality output

### Negative

- **Latency**: Full verification + code review adds ~5-10 minutes per iteration
- **False failures**: Flaky tests could waste iterations
- **Orchestrator complexity**: Orchestrator must track iteration count and format failures
- **Strict by default**: All review findings blocking may require tuning

### Neutral

- **5-attempt limit is tunable**: May adjust based on experience
- **Code review can still run standalone**: `code-review.md` usable outside loop

## Implementation

Files created:
- `scripts/verify-completion.sh` - Verification script
- `.claude/workflows/development-loop.md` - Combined workflow
- `docs/decisions/adr-0016-development-loop.md` - This ADR

Files deprecated:
- `.claude/workflows/contextual-injection.md` - Content merged into development-loop.md

## Future Considerations

1. **Parallel verification**: Run guards and tests concurrently for speed
2. **Selective retests**: Only rerun failed tests on retry iterations
3. **Metrics tracking**: Track iteration counts, failure patterns, collaboration rates
4. **Configurable strictness**: Allow some review findings to be advisory vs blocking

## References

- Ralph-Wiggum plugin: https://github.com/anthropics/claude-code/tree/main/plugins/ralph-wiggum
- Guard infrastructure: ADR-0015
- Existing workflows: `.claude/workflows/*.md`
