# ADR-0026: Knowledge System Restructure

## Status

Accepted

## Context

Dark Tower uses 14 specialist agents with layered context: CLAUDE.md (501 lines, always loaded), agent definitions (~1,016 lines), specialist knowledge files (48 files, ~7,000 lines), and skill files. Through 13 user-story test runs, we discovered:

1. **Knowledge contamination**: Knowledge files cached stale facts that overrode protocol instructions (e.g., auth-controller's "Scopes: {principal}.{operation}.{component}" overrode ADR-0020's roles model)
2. **Context overload**: Research shows 150-200 instruction ceiling; agents received 500-1,500+ lines
3. **Self-loading fails**: 56% failure rate for agents self-loading knowledge; confirmed experimentally

## Decision

### Framework: Identity / Navigation / Facts

- **Agent prompt** (~40-50 lines): Identity + principles + boundaries (auto-loaded via `subagent_type`)
- **INDEX.md** (~30-50 lines per specialist): Navigation pointers to code/ADRs, max 5 cross-cutting gotchas
- **Facts**: Live in code and ADRs only — never duplicated into prompts or knowledge files
- **Enforceable rules**: Enforced by tooling (clippy, guards, CI) — NOT instructions
- **Injection**: Lead deterministically injects INDEX.md content into spawn prompts (100% compliance)

### Changes Made

1. **CLAUDE.md slimmed** from 501 to ~71 lines (project identity + orchestrator behavior + navigation)
2. **DEVELOPMENT_WORKFLOW.md deleted** (all content duplicated in CLAUDE.md or skill files)
3. **Key Patterns stripped** from 5 agent definitions (auth-controller, global-controller, meeting-controller, media-handler, database) to remove contaminating facts
4. **INDEX.md created** for all 14 specialists (navigation-only, 50-line cap)
5. **48 old knowledge files archived** (patterns.md, gotchas.md, integration.md, domain-specific) — git history preserves content
6. **Step 0 ("Load Knowledge") removed** from devloop and debate skills — replaced by Lead-injected INDEX.md
7. **Reflection rewritten** to maintain navigation pointers, not accumulate facts
8. **knowledge-audit skill deleted** — replaced by `validate-knowledge-index.sh` guard
9. **approved-crypto.md moved** to ADR-0027

### Rules Removed from CLAUDE.md (already enforced by tooling)

| Instruction | Enforced By |
|-------------|-------------|
| No `unwrap()`/`expect()` in production | `clippy::unwrap_used/expect_used = "deny"` |
| No `panic!` in production | `clippy::panic = "deny"` |
| Use sqlx compile-time checking | Compiler (sqlx won't compile without it) |
| No string concatenation for SQL | sqlx compile-time checking |
| Format code / Linting | CI + devloop validation pipeline |

## Consequences

### Positive
- ~85% reduction in always-loaded context (501 → 71 lines CLAUDE.md)
- Eliminates knowledge contamination (no facts in agent prompts)
- 100% INDEX injection compliance (Lead reads file, not agent)
- Navigation pointers fail loudly (stale pointer → guard catches); facts fail silently
- Guard validates INDEX.md pointers resolve on every run

### Negative
- Specialists lose pre-loaded domain knowledge (must navigate to it)
- Initial INDEX.md files may need refinement through use

### Neutral
- Knowledge file git history preserved for recovery
- Reflection produces pointer updates instead of knowledge dumps

## Evidence

- 13 user-story test runs documenting contamination and self-load failures
- Scopes-vs-roles incident (auth-controller knowledge overriding ADR-0020)
- env-test deferral cascade (5 consecutive runs deferring due to stale "blocked" knowledge)

## Participants

- Orchestrator: System design and execution
- All 14 specialists: INDEX.md creation via domain exploration
