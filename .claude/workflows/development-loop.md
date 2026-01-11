# Development Loop Workflow

## Overview

The Development Loop is the primary workflow for implementing features. It combines:
- **Context injection** - Principles and patterns injected into specialist prompts
- **Iterative verification** - Guards and tests run after each attempt
- **Collaboration escalation** - Human involvement after 3 failed attempts

## When to Use

| Scenario | Use Loop? | Notes |
|----------|-----------|-------|
| Implement new feature | Yes | Standard flow |
| Bug fix | Yes | Unless trivial one-liner |
| Refactoring | Yes | Tests catch regressions |
| Documentation only | No | No verification needed |
| Exploration/research | No | No code to verify |

## Loop Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    Development Loop                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. DEBATE (if cross-cutting)                                   │
│     └─→ Produces ADR with design decisions                      │
│     └─→ See: multi-agent-debate.md                              │
│                                                                  │
│  2. SPECIALIST INVOCATION (iteration 1)                         │
│     └─→ Context injection: principles + ADR + task              │
│     └─→ Specialist implements                                   │
│                                                                  │
│  3. VERIFICATION                                                │
│     └─→ ./scripts/verify-completion.sh                          │
│     └─→ If PASS: Done ✓                                         │
│     └─→ If FAIL: Continue to step 4                             │
│                                                                  │
│  4. RETRY (iterations 2-3)                                      │
│     └─→ Same specialist + failure context                       │
│     └─→ Back to step 3                                          │
│                                                                  │
│  5. COLLABORATION (if still failing after 3 attempts)           │
│     └─→ Stop loop, present failures to human                    │
│     └─→ Work together to resolve                                │
│                                                                  │
│  [After loop completes: Code review before merge]               │
│     └─→ See: code-review.md                                     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Triggering the Loop

**Implicit trigger**: The orchestrator automatically uses this loop for implementation tasks.

When starting, announce:
> *"Starting development loop (max 3 iterations)"*

Report each iteration result as you go.

---

# Part 1: Context Injection

## Principle Categories

Self-contained principle files in `docs/principles/`:

| Category | File | Key Concerns |
|----------|------|--------------|
| Cryptography | `crypto.md` | EdDSA, bcrypt, CSPRNG, key rotation, no hardcoded secrets |
| JWT | `jwt.md` | Token validation, claims, expiry, size limits, algorithm attacks |
| Logging | `logging.md` | No PII, no secrets, SecretString, structured format |
| Queries | `queries.md` | Parameterized SQL, org_id filter, no dynamic SQL |
| Errors | `errors.md` | No panics, Result types, generic API messages |
| Input | `input.md` | Length limits, type validation, early rejection |
| Testing | `testing.md` | Test ownership, three tiers, determinism, coverage targets |
| Concurrency | `concurrency.md` | Actor pattern, message passing, no shared mutable state |
| API Design | `api-design.md` | URL versioning, deprecation, protobuf evolution |
| Observability | `observability.md` | Privacy-by-default, metrics naming, spans, SLOs |

## Task-to-Category Mapping

Match task description against patterns to determine which categories to inject:

```yaml
task_patterns:
  "password|hash|bcrypt|encrypt|decrypt|key|secret": [crypto, logging]
  "query|select|database|migration|sql": [queries, logging]
  "jwt|token|auth|oauth|bearer": [crypto, jwt, logging]
  "handler|endpoint|route|api": [logging, errors, input, api-design]
  "client|credential|oauth": [crypto, logging, errors]
  "parse|input|validate|request": [input, errors]
  "test|coverage|fuzz|integration|e2e": [testing, errors]
  "actor|channel|spawn|concurrent|async": [concurrency, errors]
  "version|deprecate|breaking|protobuf": [api-design, errors]
  "metric|trace|span|instrument|log": [observability, logging]
```

**Matching Rules**:
1. Case-insensitive regex match against task description
2. Multiple patterns can match → union of categories
3. Limit to 3-4 categories max per invocation (attention budget)
4. Always include `errors` for any production code

---

# Part 2: Specialist Prompts

## Iteration 1 Template (Initial)

```markdown
{Specialist definition from .claude/agents/{specialist}.md}

## Project Principles (MUST FOLLOW)

{Inject matched category files here}

## Design Context

{ADR summary if from debate, or "N/A" if no debate}

## Existing Patterns

{Relevant code snippets from similar implementations}

## Task

{The actual task description}

## Verification

When complete, the following checks will run:
- cargo check (must compile)
- cargo fmt (auto-formats code, fails if syntax broken)
- ./scripts/guards/run-guards.sh --simple-only (simple guards must pass)
- cargo test (all tests must pass)
- cargo clippy -- -D warnings (no new warnings)
- ./scripts/guards/run-guards.sh --semantic (semantic guards must pass)

## Deliverables

1. Implementation code
2. Unit tests for new/modified code
3. All verification checks passing
```

## Iterations 2-3 Template (Retry)

```markdown
{Same as iteration 1, plus:}

## Previous Attempt Failed (Iteration {N} of 3)

{Formatted failure report from verification script}

Please fix these issues. Focus on:
1. {Specific failure 1}
2. {Specific failure 2}
...

Do not change unrelated code. Make minimal fixes to pass verification.
```

---

# Part 3: Verification

## Running Verification

After specialist completes, run:

```bash
./scripts/verify-completion.sh --verbose
```

**Exit codes**:
- 0 = All checks passed → Loop complete
- 1 = Checks failed → Continue to retry or collaboration

**Verification layers** (all run for full verification):
1. `cargo check` - Compilation
2. `cargo fmt` - Auto-formatting (fixes in-place, fails only if syntax broken)
3. Simple guards - Pattern-based checks
4. Unit tests - `cargo test --lib`
5. All tests - `cargo test`
6. Clippy - Lint warnings
7. Semantic guards - LLM-based analysis (slowest, runs last)

## Formatting Failures for Retry

When verification fails, format the failure report for the retry prompt:

```markdown
## Previous Attempt Failed (Iteration 2 of 3)

**Failed at**: simple-guards
**Time**: 1.5 seconds

### Guard Failures

#### no-pii-in-logs.sh (2 violations)

```
src/handlers.rs:45: info!("User email: {}", email)
src/handlers.rs:52: debug!("IP: {}", ip_address)
```

**How to fix**: Use `[REDACTED]` placeholder or add `skip(email, ip_address)` to #[instrument]

### Summary

- Compilation: PASSED
- Guards: FAILED (2 violations)
- Tests: SKIPPED (blocked by guard failure)

Please fix the guard violations and ensure all checks pass.
```

---

# Part 4: Collaboration Mode

## When to Enter Collaboration

After **3 failed attempts**, stop the loop and present to user:

```markdown
## Development Loop: Collaboration Needed

The specialist attempted this task 3 times but verification still fails.

### Current Failures

{Remaining failures from last verification}

### Attempt History

| Attempt | Result | What Failed | What Was Fixed |
|---------|--------|-------------|----------------|
| 1 | FAIL | Compile error in auth.rs | Added missing import |
| 2 | FAIL | Guard: PII in logs | Used [REDACTED] for email |
| 3 | FAIL | Test: jwt_validation | Still failing |

### Analysis

The test failure appears to be:
- {Brief analysis of why the specialist couldn't fix it}
- {Possible root causes}

### Suggested Next Steps

1. Review test expectations - may need updating if requirements changed
2. Check if ADR design needs revision
3. Consider involving {other specialist} for fresh perspective
4. Manual debugging with `cargo test {test_name} -- --nocapture`
```

## Collaboration Options

Present to user:
1. **Provide guidance** - User gives hints, loop continues
2. **Adjust task** - Simplify or split task
3. **Involve another specialist** - Fresh perspective
4. **Debug together** - Interactive troubleshooting
5. **Accept partial** - Commit working parts, create TODO for rest

---

# Part 5: Integration with Other Workflows

## Multi-Agent Debate

See: `multi-agent-debate.md`

When a task requires cross-cutting design decisions:

1. Identify debate topic keywords
2. Match to principle categories
3. Inject matched categories into Round 1 context
4. All specialists see same principles during debate

After debate produces ADR:
- ADR summary injected as "Design Context" in specialist prompt
- Development loop implements the debated design

## Code Review

See: `code-review.md`

Code review happens **after** the development loop completes, **before** merge:

1. Loop completes (verification passes)
2. Present code for review per `code-review.md`
3. Code reviewers receive same principle categories as implementer
4. Fix review feedback (may restart loop if significant changes)
5. Merge when review passes

**Note**: Code review is currently a separate step after the loop. Future consideration: add code review as a verification step within the loop itself.

---

# Quick Reference

## Before Specialist Invocation

1. Match task → categories (see mapping above)
2. Build prompt with principles + existing patterns
3. Announce: "Starting development loop (max 3 iterations)"

## After Each Specialist Attempt

1. Run `./scripts/verify-completion.sh --verbose`
2. If PASS → Done, proceed to code review
3. If FAIL and iteration < 3 → Format failures, retry
4. If FAIL and iteration = 3 → Enter collaboration mode

## Verification Commands

```bash
# Full verification (default)
./scripts/verify-completion.sh

# Quick feedback during development
./scripts/verify-completion.sh --layer quick

# Machine-readable output
./scripts/verify-completion.sh --format json
```

## Categories Shorthand

- `crypto` - secrets, keys, hashing, encryption
- `jwt` - token validation, claims, expiry
- `logging` - no secrets in logs, structured format
- `queries` - parameterized SQL, no injection
- `errors` - no panics, proper types
- `input` - validation, limits, sanitization
- `testing` - test ownership, three tiers, determinism
- `concurrency` - actor pattern, message passing
- `api-design` - URL versioning, deprecation
- `observability` - privacy-by-default, metrics, spans

---

# Related Workflows

| Workflow | When to Use | Relationship to Loop |
|----------|-------------|---------------------|
| `multi-agent-debate.md` | Cross-cutting design | Happens BEFORE loop, produces ADR |
| `code-review.md` | Quality gate | Happens AFTER loop, before merge |
| `orchestrator-guide.md` | General orchestration | This loop is a key subprocess |
| `process-review-record.md` | Process failures | Use when loop reveals coordination gaps |
