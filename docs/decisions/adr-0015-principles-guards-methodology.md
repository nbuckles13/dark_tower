# ADR-0015: Principles and Guards Methodology

**Status**: Accepted

**Date**: 2026-01-08

**Deciders**: All Specialists

---

## Context

Dark Tower uses AI specialists to implement features, each with domain expertise (Auth Controller, Database, Security, etc.). These specialists need consistent guidance on project standards to produce compliant code on the first attempt.

**Problems addressed**:
- Specialists may not know project-specific rules (e.g., "never log secrets")
- Rules scattered across ADRs are hard to find and inject into prompts
- Manual code review catches violations late in the process
- No clear criteria for when to automate enforcement vs. rely on review

**Requirements**:
- Specialists receive relevant principles before implementation
- Violations are caught early (shift-left)
- Clear taxonomy for enforcement mechanisms
- Self-documenting system that stays in sync

## Decision

**We adopt a three-layer enforcement system: Principles, Guards, and Contextual Injection.**

### Layer 1: Principles (`docs/principles/*.md`)

Human-readable rule files, each covering a cross-cutting concern.

**Format**:
- ~100-120 lines maximum (attention budget for injection)
- Sections: DO / DON'T / Quick Reference / Guards
- No code examples (prose generalizes better than specific code)
- ADR references for deeper context

**Current principles**: crypto, jwt, logging, queries, errors, input, testing, concurrency, api-design, observability

### Layer 2: Guards (`scripts/guards/`)

Automated enforcement of principles. Guards are categorized by speed and capability:

| Type | Location | Speed | Use When |
|------|----------|-------|----------|
| Simple | `scripts/guards/simple/` | <1s | Grep/regex patterns reliably catch violations |
| Semantic | `scripts/guards/semantic/` | ~30s | Complex control flow or context needed (LLM-based) |
| Clippy | `Cargo.toml` lints | Build time | Rust-specific patterns (unwrap, panic, indexing) |
| Compile-time | sqlx, type system | Build time | Query safety, type constraints |
| Tests | P0/P1 security tests | CI | Runtime behavior validation |
| Coverage | `test-coverage.sh` | CI | Ensure changed code is tested |
| Test Protection | `no-test-removal.sh` | Pre-commit | Warn on test removal/weakening |

### Tests as Guards

P0/P1 security tests serve as **runtime guards** - they verify actual behavior rather than code patterns. Tests are the preferred enforcement mechanism when:

| Condition | Example | Why Tests > Static Guards |
|-----------|---------|---------------------------|
| Behavior matters more than pattern | JWT algorithm rejection | Need to verify token is actually rejected, not just that EdDSA is mentioned |
| Implementation is centralized | `verify_jwt()` function | One well-tested function, called by many places |
| Multiple valid implementations | Error handling | Many ways to handle errors correctly |
| Edge cases are complex | Clock skew boundaries | Boundary conditions need runtime verification |

**Tests are NOT sufficient alone** - they require protection:
1. **Tests must run before merge** - CI enforcement
2. **Tests must be exhaustive** - Coverage guard on changed code
3. **Tests must not be weakened** - Test modification guard warns on removals

### Layer 3: Contextual Injection (`.claude/workflows/contextual-injection.md`)

Principles are injected into specialist prompts based on task keywords:
- Task description matched against patterns (e.g., "jwt|token|auth" → crypto, jwt, logging)
- Matched principles included in specialist prompt before implementation
- Category-specific guards run after specialist produces code

### Guard Coverage

Each principle file (`docs/principles/*.md`) documents its enforcement mechanism in a `## Guards` section. This is the authoritative source for guard coverage - principle files are self-documenting.

### Adding New Guards - Decision Tree

1. Can grep/regex catch it reliably? → **Simple guard**
2. Does it need control flow analysis or context? → **Semantic guard**
3. Is it a Rust-specific pattern? → **Clippy lint**
4. Does it need runtime behavior validation? → **P0/P1 test** (ensure coverage guard protects it)
5. Is it too context-dependent for automation? → **Code review** (document in principle's Guards section)

**When to use tests instead of static guards**:
- The principle is about behavior, not code patterns
- Implementation is centralized and well-tested
- There are multiple valid ways to implement correctly
- Edge cases require runtime verification

### Adding New Principles

1. Identify cross-cutting concern from ADRs or recurring review feedback
2. Create `docs/principles/{name}.md` following format (~100-120 lines)
3. Include Guards section specifying enforcement mechanism
4. Update `.claude/workflows/contextual-injection.md`:
   - Add to Principle Categories table
   - Add task patterns for automatic matching
   - Add to Categories shorthand

### Guard Execution

| Context | Guards Run | Command |
|---------|------------|---------|
| During specialist work | Category-matched guards | `./scripts/guards/simple/no-secrets-in-logs.sh <file>` |
| Pre-commit | All simple guards | `./scripts/guards/run-guards.sh --simple-only` |
| CI | All guards (simple + semantic) | `./scripts/guards/run-guards.sh --semantic` |

## Consequences

### Positive

- Specialists receive relevant guidance via injection → fewer violations
- Violations caught early (simple guards <1s, semantic ~30s)
- Self-documenting: principle files declare their enforcement in Guards section
- Clear criteria for adding new guards vs. relying on review
- Consistent format makes principles easy to scan and inject

### Negative

- Maintenance burden: guards must stay in sync with principles
- Semantic guards have LLM cost (~$0.01-0.05 per invocation)
- Some principles can only be enforced via code review (context-dependent)

### Neutral

- Principles are opinion files (reflect project decisions, not universal truths)
- Guard coverage will expand over time as patterns emerge

## Implementation Notes

**Testing guards**:
```bash
./scripts/guards/run-guards.sh --verbose          # All guards
./scripts/guards/run-guards.sh --simple-only      # Fast guards only
./scripts/guards/simple/no-hardcoded-secrets.sh . # Single guard on directory
```

**Guard exit codes**:
- 0 = Pass (no violations)
- 1 = Fail (violations found)
- 2 = Script error
- 3 = Unclear (semantic only, requires manual review)

**Environment variables**:
- `GUARD_SEMANTIC_MODEL`: Override LLM model for semantic guards (default: claude-sonnet-4-20250514)

## References

- Principle files: `docs/principles/*.md`
- Guard scripts: `scripts/guards/`
- Contextual injection workflow: `.claude/workflows/contextual-injection.md`
- Related ADRs: All ADRs referenced in principle files
