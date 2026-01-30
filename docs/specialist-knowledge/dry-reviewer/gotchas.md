# DRY Reviewer - Gotchas to Avoid

This file captures pitfalls and anti-patterns discovered during DRY reviews.

---

## Don't Flag Convention-Based Patterns as Duplication

**Added**: 2026-01-29
**Related files**: N/A (general principle)

**Gotcha**: Don't flag repeated patterns as duplication if they represent architectural conventions that are intentionally consistent across services. Examples include error handling patterns, logging formats, or metric naming schemes. Each service should own its domain-specific implementations while following project-wide conventions.

**How to distinguish**:
- **Harmful duplication**: Copy-pasted business logic, shared utilities coded multiple times, identical algorithms
- **Healthy alignment**: Consistent patterns with domain-specific context (error types, service names, operation descriptions)

**Rule of thumb**: If extracting the pattern would require creating abstractions that are more complex than the repetition itself, it's likely a convention, not duplication.

---
