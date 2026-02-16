# DRY Reviewer Checkpoint

**Task**: Fix 26 error-context-preservation violations in the Authentication Controller

**Reviewer**: DRY Reviewer
**Date**: 2026-01-30
**Verdict**: APPROVED

---

## Summary

The implementation applies a consistent error-handling pattern across 3 files (26 fixes total):
- `crates/ac-service/src/crypto/mod.rs` - 19 fixes
- `crates/ac-service/src/handlers/internal_tokens.rs` - 4 fixes
- `crates/ac-service/src/handlers/auth_handler.rs` - 3 fixes

**Key Question**: Is this healthy architectural alignment or harmful duplication?

**Answer**: This is **healthy architectural alignment** - error handling boilerplate that follows a consistent convention across the codebase.

---

## Analysis

### Pattern Under Review

The error-context-preservation pattern:
```rust
.map_err(|e| AcError::Variant(format!("description: {}", e)))
```

This pattern:
1. Preserves the original error context in the error message
2. Follows Rust error handling best practices
3. Is documented in `docs/principles/errors.md`

### Cross-Service Usage

| Service | Occurrences | Notes |
|---------|-------------|-------|
| AC (ac-service) | 67+ | Primary authentication service |
| GC (global-controller) | 6 | Uses similar pattern |
| MC (meeting-controller) | 0 | Minimal implementation currently |

### Why This Is NOT Harmful Duplication

1. **Convention-Based Pattern**: This is error handling boilerplate, not business logic. The specialist definition explicitly lists "error handling boilerplate" as a healthy pattern that should NOT be blocked.

2. **Domain-Specific Error Types**: Each service has its own error type (`AcError`, `GcError`) with domain-specific variants. The format string pattern is how Rust `thiserror`-based errors conventionally preserve context.

3. **No Common Utility Possible**: You cannot extract `.map_err(|e| Error(format!(...)))` into a utility - it's idiomatic Rust that varies by:
   - Error variant used
   - Description context
   - Source error type

4. **Already Documented in Principles**: The `docs/principles/errors.md` file documents this pattern as standard practice.

---

## Findings

### BLOCKER: 0

No blocking findings. This is not copy-pasted business logic, duplicate utilities, or identical algorithms. It's error handling convention.

### TECH_DEBT: 0

No tech debt findings. This pattern is appropriate and does not warrant extraction:
- Error types are intentionally domain-specific per service
- The pattern is boilerplate, not extractable logic
- Extraction would add complexity without benefit

---

## Verdict Details

| Severity | Count | Blocking? |
|----------|-------|-----------|
| BLOCKER | 0 | Yes |
| TECH_DEBT | 0 | No |

**Verdict**: **APPROVED**

Per ADR-0019, the DRY Reviewer only blocks on genuine shared code requiring extraction. Error handling boilerplate across services is explicitly listed as healthy architectural alignment in the specialist definition.

---

## References

- **Specialist Definition**: `.claude/agents/dry-reviewer.md` - "Error handling boilerplate (healthy)"
- **Principles**: `docs/principles/errors.md` - Documents error handling patterns
- **ADR-0019**: DRY Reviewer blocking behavior rules
- **Integration Notes**: `docs/specialist-knowledge/dry-reviewer/integration.md` - Tech debt registry and blocking criteria

---

## Checkpoint Metadata

```
checkpoint_type: dry-review
task_id: 2026-01-30-fix-ac-error-context-preservation-violations-26
verdict: APPROVED
blocker_count: 0
tech_debt_count: 0
files_reviewed: 3
```
