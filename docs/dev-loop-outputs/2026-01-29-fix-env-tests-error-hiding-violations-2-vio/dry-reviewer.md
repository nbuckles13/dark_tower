# DRY Reviewer: Fix Env-Tests Error Hiding Violations

**Date**: 2026-01-29
**Reviewer**: DRY Reviewer Specialist
**Verdict**: APPROVED

---

## Review Summary

Reviewed the error hiding fixes in `crates/env-tests/src/cluster.rs` for cross-service duplication and pattern extraction opportunities.

---

## Analysis

### Pattern Alignment Assessment

The changes follow the **established error preservation pattern** used across AC, MC, and GC services:

```rust
// Established pattern (AC, MC, GC)
.map_err(|e| ErrorType::Variant(format!("Context: {}", e)))
```

**Evidence of established precedent**:

1. **AC Service** (`crates/ac-service/src/`): 40+ instances of this pattern
   - `repositories/users.rs`: `.map_err(|e| AcError::Database(format!("Failed to fetch user: {}", e)))`
   - `repositories/signing_keys.rs`: `.map_err(|e| AcError::Database(format!("Failed to create signing key: {}", e)))`
   - `handlers/admin_handler.rs`: `.map_err(|e| AcError::Database(format!("Failed to begin transaction: {}", e)))`

2. **GC Service** (`crates/global-controller/src/`):
   - `services/ac_client.rs`: `.map_err(|e| GcError::Internal(format!("Failed to build HTTP client: {}", e)))`
   - `handlers/meetings.rs`: `.map_err(|e| GcError::Internal(format!("RNG failure: {}", e)))`

3. **Env-Tests** (this fix): Now uses the same pattern
   - `cluster.rs:124`: `.map_err(|e| ClusterError::HealthCheckFailed { message: format!("Invalid address '{}': {}", addr, e) })`
   - `cluster.rs:129`: `.map_err(|e| ClusterError::HealthCheckFailed { message: format!("... TCP error: {}", e) })`

### Duplication Assessment

| Category | Finding | Severity |
|----------|---------|----------|
| Pattern reuse | Error preservation pattern is repeated across AC, MC, GC, env-tests | **GOOD** (architectural alignment) |
| Error types | Each crate defines its own error type (`AcError`, `GcError`, `ClusterError`) | **APPROPRIATE** (domain separation) |
| Format strings | Similar but contextually different messages | **ACCEPTABLE** (domain-specific context) |

### Should This Be Extracted?

**No.** The pattern follows healthy architectural alignment:

1. **Domain-specific error types**: Each crate should own its error types (AC -> `AcError`, GC -> `GcError`, env-tests -> `ClusterError`)
2. **Consistent pattern, different contexts**: The `.map_err(|e| Error(format!("...: {}", e)))` pattern is intentionally repeated - it's a convention, not duplication
3. **No shared utility needed**: Creating a macro or helper for this pattern would add complexity without reducing maintenance burden

**ADR-0019 Classification**: This is **healthy pattern replication** (following an established convention), NOT **harmful duplication** (copy-paste code requiring extraction).

---

## Findings

### TECH_DEBT (Non-blocking)

| ID | Description | Recommendation |
|----|-------------|----------------|
| TD-1 | Consider documenting the error preservation pattern in `docs/principles/errors.md` if not already present | Add pattern to principles documentation |

No BLOCKER findings.

---

## Verdict

**APPROVED**

The changes correctly apply the established error preservation pattern. This is architectural alignment (good), not duplication requiring extraction.

---

## Checklist

- [x] Checked for copy-paste duplication: None found
- [x] Verified pattern follows established precedent: Yes, matches AC/MC/GC
- [x] Assessed extraction opportunities: Not warranted for convention-based patterns
- [x] Classified findings per ADR-0019: TECH_DEBT only

---

## Files Reviewed

- `/home/nathan/code/dark_tower/crates/env-tests/src/cluster.rs`

## Cross-Reference Checks

- `/home/nathan/code/dark_tower/crates/ac-service/src/repositories/` - 40+ instances of pattern
- `/home/nathan/code/dark_tower/crates/global-controller/src/services/ac_client.rs` - Pattern present
- `/home/nathan/code/dark_tower/crates/global-controller/src/handlers/meetings.rs` - Pattern present

---

## Reflection

### Key Learnings

1. **Architectural Alignment vs. Duplication**: The `.map_err(|e| Error(format!("...: {}", e)))` pattern appears 40+ times across services. This is healthy architectural alignment (convention-based consistency), NOT harmful duplication requiring extraction. Each service correctly owns its domain-specific error types while following the same error preservation pattern.

2. **ADR-0019 Blocking Criteria**: Successfully applied the distinction between BLOCKER (shared code requiring extraction) and TECH_DEBT (non-blocking observations). Convention-based patterns should not block, even when widely repeated.

3. **Documentation Recommendations**: When widely-used patterns aren't documented in principles files, TECH_DEBT findings can guide future documentation improvements without blocking current work.

### Knowledge Updates

Created initial DRY Reviewer knowledge files:
- `docs/specialist-knowledge/dry-reviewer/patterns.md` - Added architectural alignment pattern
- `docs/specialist-knowledge/dry-reviewer/gotchas.md` - Added convention vs. duplication distinction
- `docs/specialist-knowledge/dry-reviewer/integration.md` - Added ADR-0019 blocking behavior and principles documentation guidance

### Reusable Insights

- **Pattern recognition**: Error preservation patterns, logging formats, and metric naming are conventions, not duplication
- **Extraction threshold**: Don't extract if the abstraction is more complex than the repetition
- **Documentation gaps**: Use TECH_DEBT to recommend principle documentation without blocking
