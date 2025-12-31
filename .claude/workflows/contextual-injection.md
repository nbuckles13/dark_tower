# Contextual Injection Workflow

## Purpose

Proactively inject relevant project principles and context to specialists during implementation, keeping them aligned with project standards and reducing violations caught late in review.

## When to Use

| Phase | Injection Type | What to Inject |
|-------|---------------|----------------|
| **Implementation** | Pre-Task | Category-matched principles based on task keywords |
| **During Work** | Pattern-Triggered | Guards + fix guidance when violations detected |
| **Multi-Agent Debate** | Cross-Cutting | Relevant categories for debate participants |
| **Code Review** | Verification | Same categories given to implementer |

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

## Task-to-Category Mapping

Match task description against patterns to determine which categories to inject:

```yaml
task_patterns:
  "password|hash|bcrypt|encrypt|decrypt|key|secret": [crypto, logging]
  "query|select|database|migration|sql": [queries, logging]
  "jwt|token|auth|oauth|bearer": [crypto, jwt, logging]
  "handler|endpoint|route|api": [logging, errors, input]
  "client|credential|oauth": [crypto, logging, errors]
  "parse|input|validate|request": [input, errors]
```

**Matching Rules**:
1. Case-insensitive regex match against task description
2. Multiple patterns can match → union of categories
3. Limit to 3-4 categories max per invocation (attention budget)
4. Always include `errors` for any production code

## Pre-Task Injection Template

When invoking a specialist, structure the prompt as:

```markdown
{specialist definition from .claude/agents/{specialist}.md}

## Project Principles (MUST FOLLOW)

{Inject matched category files here, verbatim or summarized}

## Existing Patterns (MATCH THESE)

{Relevant code snippets from similar implementations}

## Task

{The actual task description}

## Deliverables

1. {What the specialist should produce}
2. Write unit tests for new/modified code
3. Ensure tests pass
```

### Example: Auth Handler Task

Task: "Implement client secret rotation endpoint"

**Matched patterns**: `client|secret` + `handler|endpoint`
**Categories**: crypto, logging, errors, input

```markdown
You are the Auth Controller specialist.

## Project Principles (MUST FOLLOW)

### Cryptography (crypto.md)
[Contents of docs/principles/crypto.md]

### Logging Safety (logging.md)
[Contents of docs/principles/logging.md]

### Error Handling (errors.md)
[Contents of docs/principles/errors.md]

### Input Validation (input.md)
[Contents of docs/principles/input.md]

## Existing Patterns

See: crates/ac-service/src/routes/admin_handler.rs
- rotate_service_secret() function for pattern reference

## Task

Implement POST /admin/clients/{id}/rotate-secret endpoint...
```

## Guard Integration

### During Specialist Work

Run category-matched guards after specialist produces code:

```bash
# For crypto category
./scripts/guards/simple/no-hardcoded-secrets.sh crates/ac-service/src/

# For logging category
./scripts/guards/simple/no-secrets-in-logs.sh crates/ac-service/src/

# For complex cases
./scripts/guards/semantic/credential-leak.sh <file>
```

### Pre-Commit (ALL Guards)

Before committing, run all simple guards regardless of categories:

```bash
./scripts/guards/run-guards.sh
```

### CI Pipeline

Same as pre-commit - all guards must pass.

## Hybrid Iteration Protocol

When guards find violations:

### Auto-Fix (Simple Violations)
- Single-line fixes (add `skip()`, remove secret from log)
- Clear pattern match to principle rule
- No architectural changes needed

**Action**: Show violation + fix, apply automatically

### Escalate (Complex Violations)
- Multi-file changes required
- Unclear which principle applies
- Architectural decision needed
- Security implications

**Action**: Show violation, ask user for guidance

### Iteration Flow

```
1. Specialist produces code
2. Run category-matched guards
3. If violations:
   a. Simple → auto-fix + re-run guards
   b. Complex → escalate to user
4. Repeat until clean (max 3 iterations)
5. Run ALL guards before commit
```

## Integration with Other Workflows

### Multi-Agent Debate

When building debate context (see `multi-agent-debate.md`):

1. Identify debate topic keywords
2. Match to principle categories
3. Inject matched categories into Round 1 context
4. All specialists see same principles during debate

**Example**: Debate on "JWT refresh token strategy"
- Categories: jwt, crypto, logging
- All debate participants receive these principles

### Code Review

When specialist produces code for review:

1. Code reviewer receives same categories as implementer
2. Review checklist includes "verify principles followed"
3. Violations caught here indicate:
   - Missing guard coverage
   - Principle needs clarification
   - Guard pattern needs update

### Orchestrator Guide

The orchestrator should:

1. Match task to categories before invoking specialist
2. Include category files in specialist prompt
3. Run category guards after specialist work
4. Apply hybrid iteration for violations
5. Run all guards before presenting to user

## Metrics to Track

During experiment and ongoing:

| Metric | What it Measures |
|--------|------------------|
| First-pass violations | Guards caught on initial code |
| Iteration count | Rounds before clean code |
| Escalation rate | Complex violations needing user input |
| Category coverage | Which categories caught most violations |
| Missing principles | Violations not covered by any category |

## Quick Reference

**Before specialist invocation**:
1. Match task → categories
2. Build prompt with principles
3. Include existing patterns

**After specialist work**:
1. Run category guards
2. Apply hybrid iteration
3. Run all guards before commit

**Categories shorthand**:
- `crypto` - secrets, keys, hashing, encryption
- `jwt` - token validation, claims, expiry
- `logging` - no secrets in logs, structured format
- `queries` - parameterized SQL, no injection
- `errors` - no panics, proper types
- `input` - validation, limits, sanitization
