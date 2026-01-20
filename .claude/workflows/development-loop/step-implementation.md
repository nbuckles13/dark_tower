# Step: Implementation

This file covers the implementation step of the dev-loop.

**Invocation mechanism**: See `specialist-invocation.md` for how to invoke specialists via Task tool.

---

## Context Injection

When building specialist prompts, inject context in this order:

1. **Specialist definition** - From `.claude/agents/{specialist}.md`
2. **Matched principles** - From `docs/principles/` based on task keywords
3. **Specialist knowledge** - From `docs/specialist-knowledge/{specialist}/` (if exists)
4. **Design context** - ADR summary + file path if from debate
5. **Task context** - The actual task description

## Specialist Knowledge Files

Each specialist may have accumulated knowledge in `docs/specialist-knowledge/{specialist}/`:
- `patterns.md` - Established approaches for common tasks
- `gotchas.md` - Mistakes to avoid, learned from experience
- `integration.md` - Notes on working with other services

**If these files exist**: Inject their contents after principles, before task context.

**If these files don't exist**: Skip (specialist will bootstrap them during first reflection).

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

## What to Show User Before Spawning

Before invoking the specialist, show the user:

1. **Matched principles** (file paths):
   ```
   - docs/principles/errors.md
   - docs/principles/testing.md
   ```

2. **Task prompt** (what specialist will see):
   ```
   Replace all #[allow(...)] attributes with #[expect(...)] in AC service
   production code. Test code may continue to use #[allow(...)] as needed.
   ```

User approves, then spawn the specialist.

---

## Resume for Fixes

When resuming a specialist to fix code review findings:

```markdown
## Findings to Address

Iteration 2 of 5.

### Security Specialist Findings
1. [BLOCKER] Missing input validation in `src/handlers/auth.rs:45`
   - Suggested fix: Add length check before processing

### Test Specialist Findings
1. [REQUIRED] No test coverage for error path in `src/services/token.rs:120`
   - Suggested fix: Add test for invalid token case

Fix all findings, re-run verification, update output, and return with ---RESULT--- block.
```

---

## Resume for Reflection

When resuming for reflection (after code review clean):

```markdown
## Reflection

Code review passed. Please:
1. Review your checkpoint file for patterns/gotchas discovered
2. Update `docs/specialist-knowledge/{specialist}/patterns.md` with new patterns
3. Update `docs/specialist-knowledge/{specialist}/gotchas.md` with new gotchas
4. Append reflection summary to main.md
```

---

## Checkpoint Files

Specialists write checkpoints during implementation:

**Location**: `docs/dev-loop-outputs/YYYY-MM-DD-{task}/{specialist}.md`

**Content**:
- Patterns Discovered
- Gotchas Encountered
- Key Decisions
- Status

This enables recovery if session is interrupted.
