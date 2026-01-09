# ADR-0016: Development Loop with Guard Integration

**Status**: Accepted

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

**We adopt a Development Loop workflow with 3-attempt limit and human collaboration escalation.**

### Core Design

```
┌─────────────────────────────────────────┐
│           Development Loop               │
├─────────────────────────────────────────┤
│  1. Specialist invocation (iteration 1) │
│  2. Run verification                    │
│     - compile → guards → tests → clippy │
│  3. If FAIL and iteration < 3 → retry   │
│  4. If FAIL and iteration = 3 →         │
│     → Stop and collaborate with human   │
│  5. If PASS → Done, proceed to review   │
└─────────────────────────────────────────┘
```

### Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Implementation | Custom (not Ralph plugin) | Tighter integration with specialist workflow |
| Failure routing | Same specialist retries | Simpler; they wrote the code, they fix it |
| Verification level | Full (all checks) | All guards + tests + clippy must pass |
| Max iterations | 3 attempts | Prevent infinite loops; human insight for hard problems |
| Workflow files | Combined | Single `development-loop.md` replaces `contextual-injection.md` |
| Loop trigger | Implicit + announcement | Auto-use for implementation; announce "Starting development loop" |

### Verification Script

Created `scripts/verify-completion.sh` that runs layered checks:

| Layer | Check | Speed |
|-------|-------|-------|
| 1 | `cargo check` | ~5s |
| 2 | Simple guards | ~2s |
| 3 | `cargo test --lib` | ~30s |
| 4 | `cargo test` (all) | ~1-2min |
| 5 | `cargo clippy` | ~30s |

Exit codes: 0 = pass, 1 = fail with report

### Collaboration Mode

After 3 failed attempts, the loop stops and presents:
- Current failures with details
- History of what was attempted
- Suggested next steps

Human can then: provide guidance, adjust task, involve another specialist, or debug together.

### Integration Points

| Workflow | Relationship |
|----------|--------------|
| `multi-agent-debate.md` | BEFORE loop - produces ADR for design context |
| `code-review.md` | AFTER loop - quality gate before merge |
| `development-loop.md` | Central implementation workflow |

## Consequences

### Positive

- **Clear expectations**: Specialists know exactly what checks will run
- **Fast feedback**: Layered verification catches simple issues quickly
- **Bounded iteration**: 3-attempt limit prevents runaway costs
- **Human escalation**: Hard problems get human insight instead of infinite retries
- **Unified workflow**: Context injection + verification in one document

### Negative

- **Latency**: Full verification adds ~2-3 minutes per iteration
- **False failures**: Flaky tests could waste iterations
- **Orchestrator complexity**: Orchestrator must track iteration count and format failures

### Neutral

- **Code review remains separate**: Could add to loop later if desired
- **3-attempt limit is tunable**: May adjust based on experience

## Implementation

Files created:
- `scripts/verify-completion.sh` - Verification script
- `.claude/workflows/development-loop.md` - Combined workflow
- `docs/decisions/adr-0016-development-loop.md` - This ADR

Files deprecated:
- `.claude/workflows/contextual-injection.md` - Content merged into development-loop.md

## Future Considerations

1. **Code review in loop**: Could add specialist code review as a verification step
2. **Parallel verification**: Run guards and tests concurrently for speed
3. **Selective retests**: Only rerun failed tests on retry iterations
4. **Metrics tracking**: Track iteration counts, failure patterns, collaboration rates

## References

- Ralph-Wiggum plugin: https://github.com/anthropics/claude-code/tree/main/plugins/ralph-wiggum
- Guard infrastructure: ADR-0015
- Existing workflows: `.claude/workflows/*.md`
