# Test Specialist Checkpoint: Fix Env-Tests Error Hiding Violations

**Date**: 2026-01-29
**Task**: Fix 2 error hiding violations in env-tests crate
**Status**: Complete

---

## Patterns Discovered

### Error Preservation Pattern

The established pattern for error preservation in Dark Tower is straightforward:

```rust
// Before (error hidden)
.map_err(|_| ErrorVariant::SomeError("context".to_string()))

// After (error preserved)
.map_err(|e| ErrorVariant::SomeError(format!("context: {}", e)))
```

This pattern has been applied consistently across AC (28 violations), MC, and GC, and now env-tests.

### Error Variant Selection

When the original error variant (e.g., `PortForwardNotFound`) only has a `port` field with no room for the original error message, it's acceptable to switch to a more flexible variant like `HealthCheckFailed` that accepts a message string - as long as the user-facing guidance is preserved.

---

## Gotchas Encountered

### None

This was a straightforward application of an established pattern. No gotchas were encountered.

---

## Key Decisions

### 1. Used HealthCheckFailed for TCP Connection Error

The original code used `PortForwardNotFound { port }` for TCP connection errors. This variant has a hardcoded message and no place for the original error. Rather than modifying the error enum (which would have broader impact), I used `HealthCheckFailed` with a message that:

1. Preserves the actionable guidance ("Run './infra/kind/scripts/setup.sh' to start port-forwards")
2. Includes the port number
3. Appends the original TCP error details

This approach:
- Preserves diagnostic information
- Maintains user-friendly actionable messages
- Follows the established error preservation pattern

### 2. Preserved Actionable Error Messages

The original error messages contained actionable guidance for operators (e.g., "Run './infra/kind/scripts/setup.sh'"). This guidance was preserved in the refactored error messages while adding the original error context.

---

## Current Status

- [x] Fixed line 124: Address parsing error now preserved
- [x] Fixed line 129: TCP connection error now preserved
- [x] All 7 verification layers passed
- [x] Guard compliance verified (`no-error-hiding` guard passes)

---

## Files Modified

- `crates/env-tests/src/cluster.rs` - Fixed 2 error hiding violations

---

## Verification Results

| Layer | Command | Result |
|-------|---------|--------|
| 1 | `cargo check --workspace` | PASSED |
| 2 | `cargo fmt --all --check` | PASSED |
| 3 | `./scripts/guards/run-guards.sh` | PASSED (10/10 guards) |
| 4 | `./scripts/test.sh --workspace --lib` | PASSED |
| 5 | `./scripts/test.sh --workspace` | PASSED |
| 6 | `cargo clippy --workspace -- -D warnings` | PASSED |
| 7 | `./scripts/guards/run-guards.sh --semantic` | PASSED (11/11 guards) |

---

## Code Review (Test Specialist)

**Reviewer**: Test Specialist
**Review Date**: 2026-01-29

### Test Impact Analysis

#### Existing Unit Tests
| Test | Status | Notes |
|------|--------|-------|
| `test_default_ports` | PASS | No behavioral change |

#### Integration Test Impact
The `ClusterConnection` is consumed via `.expect()` in test helpers. Error messages now include TCP error details (e.g., "Connection refused"), improving debugging when port-forwards are unavailable.

#### Error Type Change Assessment
TCP errors now return `ClusterError::HealthCheckFailed` instead of `ClusterError::PortForwardNotFound`.

**Risk**: LOW - No code pattern-matches on specific variants; all errors consumed via `.expect()` or `?`

### Test Environment Verification

```
cargo test --package env-tests --lib
# Result: 20 passed; 0 failed

cargo clippy --package env-tests -- -D warnings
# Result: No warnings
```

### Findings

| ID | Severity | Finding |
|----|----------|---------|
| (none) | - | No issues found |

### Verdict

**APPROVED**

Changes correctly preserve error context without breaking existing test behavior.

---

## Reflection

**Knowledge Review**: Reviewed all three specialist knowledge files (patterns.md, gotchas.md, integration.md) to identify learnings from this implementation.

**Result**: No updates needed. The implementation validated existing pattern #30 in patterns.md ("Error Path Testing for Pure Refactors") without discovering new insights. The error hiding fix pattern established in AC/MC/GC applies cleanly to env-tests with no special considerations.

**Pattern Validation**: This task confirmed that error hiding fixes are:
- Straightforward mechanical refactors (`|_|` → `|e|` with error context preservation)
- Compiler-verified (type mismatches caught immediately)
- No new test cases required (existing tests validate error paths)
- Pure observability improvements (no behavioral changes)

The existing knowledge entry already captures these characteristics. No additions, updates, or pruning needed.
