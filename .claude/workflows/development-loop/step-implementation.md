# Step: Implementation

This file is read when entering the `implementation` step of the dev-loop.

**Invocation mechanism**: See `specialist-invocation.md` for how to invoke specialists via `claude --print`.

---

## Context Injection

When building specialist prompts, inject context in this order:

1. **Specialist definition** - From `.claude/agents/{specialist}.md`
2. **Matched principles** - From `docs/principles/` based on task keywords
3. **Specialist knowledge** - From `docs/specialist-knowledge/{specialist}/` (if exists)
4. **Design context** - ADR summary + file path if from debate (e.g., `docs/decisions/adr-0020-...`)
5. **Task context** - The actual task description and existing patterns

## Specialist Knowledge Files

Each specialist may have accumulated knowledge in `docs/specialist-knowledge/{specialist}/`:
- `patterns.md` - Established approaches for common tasks
- `gotchas.md` - Mistakes to avoid, learned from experience
- `integration.md` - Notes on working with other services

**If these files exist**: Inject their contents after principles, before task context.

**If these files don't exist**: Skip this step (specialist will bootstrap them during first reflection).

---

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

## Specialist Prompts

### Initial Invocation

When invoking an implementing specialist, include these sections in order:

1. **Specialist definition** - From `.claude/agents/{specialist}.md`
2. **Implementation mode header** - "You are being invoked to IMPLEMENT" + expected behavior
3. **Project principles** - Matched category files from `docs/principles/`
4. **Accumulated knowledge** - From `docs/specialist-knowledge/{specialist}/*.md` (if exists)
5. **Design context** - ADR summary + file path if from debate (so specialist can reference full ADR)
6. **Existing patterns** - Relevant code snippets
7. **Task description** - The actual task
8. **Responsibilities** - Implement → verify (7 layers) → fix → write output + checkpoint → return

**Key instructions to include**:
- If requirements unclear: Return to orchestrator with questions (don't guess)
- Run all 7 verification layers and fix failures before returning
- Write output to `docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/main.md`
- Write checkpoint to `docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/{your-name}.md`
- End response with structured `---RESULT---` block (see specialist-invocation.md)

**Templates**: See `docs/dev-loop-outputs/_template/` for output and checkpoint formats.

### Required Output Format

All specialist responses must end with:

```
---RESULT---
STATUS: SUCCESS or FAILURE
SUMMARY: Brief description of what was done
FILES_MODIFIED: Comma-separated list of files changed
TESTS_ADDED: Number of tests added (0 if none)
VERIFICATION: PASSED or FAILED (did all 7 layers pass?)
ERROR: Error message if FAILURE, or "none" if SUCCESS
---END---
```

This enables step-runners to reliably detect success/failure.

### Resume for Fixes

Use `--resume "$session_id"` to continue the specialist's session (preserves context, reduces cost).

When resuming to fix code review findings, provide:
- Iteration number (N of 5)
- List of findings with severity, file:line, description, suggested fix
- Instruction: Fix all → re-run verification → update output → return
- Reminder to end with `---RESULT---` block

### Resume for Reflection

When resuming for reflection (after code review clean):
- Instruction: Review knowledge files → add/update/remove entries → append reflection to output
- Reference: `docs/specialist-knowledge/{specialist}/*.md`

---

## Checkpoint Writes

During implementation, specialists write to checkpoint files:

**Location**: `docs/dev-loop-outputs/YYYY-MM-DD-{task}/{specialist}.md`

**Content** (written as work progresses):
- **Patterns Discovered** - What approaches worked well
- **Gotchas Encountered** - What was tricky, what to warn others about
- **Key Decisions** - Choices made and why
- **Status** - Current step, timestamp

This enables session recovery if context is compressed mid-loop.

## Session Tracking

Step-runners must log session_id to `main.md` after each specialist invocation:

```markdown
## Session Tracking

| Specialist | Session ID | Iteration | Status |
|------------|------------|-----------|--------|
| auth-controller | 9e956e47-... | 1 | SUCCESS |
```

This enables:
- Resume capability for iteration 2+ (fixing findings)
- Orchestrator recovery after context compression
- Cost tracking (resumed sessions use cached prompts)
