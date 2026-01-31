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

## Acceptable Internal Duplication (Same-File, Same-Purpose)

**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs:357-363, 436-442` (NOT_FOUND detection), `crates/meeting-controller/src/grpc/gc_client.rs:178-195, 471-488` (RegisterMcRequest construction)

**Gotcha**: Don't immediately flag duplication within a single file if:
1. Only 2 occurrences (not N occurrences)
2. Both serve similar purposes (e.g., fast_heartbeat vs comprehensive_heartbeat)
3. Change together (if one changes, the other should too)
4. Extraction would create helper with single caller pair

**Examples from ADR-0023 Phase 6c**:
- **NOT_FOUND detection** in both `fast_heartbeat()` and `comprehensive_heartbeat()` - identical 4-line pattern
- **RegisterMcRequest construction** in both `register()` and `attempt_reregistration()` - identical struct initialization

**Severity**: TECH_DEBT (not BLOCKER) - Note for future consolidation but don't block.

**When to escalate to BLOCKER**: If duplication appears 3+ times, spans multiple files, or represents business logic that could diverge.

**Rule of thumb**: 2 occurrences in same file = TECH_DEBT. 3+ occurrences or cross-file = consider BLOCKER.

---

## Test Code Structural Similarity is Often Justified

**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/tests/gc_integration.rs:547-611` (test_heartbeat_not_found_detection vs test_comprehensive_heartbeat_not_found_detection)

**Gotcha**: Don't flag test code duplication if tests are structurally similar but test different code paths. Tests should prioritize clarity over DRY.

**When structural similarity is acceptable**:
- Each test calls a different method (fast_heartbeat vs comprehensive_heartbeat)
- The production code has duplication (e.g., TECH_DEBT-008 NOT_FOUND detection)
- Tests would be harder to understand if combined (parameterized tests can be opaque)

**Counter-pattern**: If tests are identical AND call the same code path with different inputs, use parameterized tests or table-driven tests.

**When reviewing**: For test code, ask "Does this test a different code path?" If yes, structural similarity is fine. If no, suggest consolidation.

---
