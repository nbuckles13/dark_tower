# Code Review: AC Code Quality Fixes

**Reviewer**: Code Quality Reviewer
**Date**: 2026-01-29
**Task**: Review error hiding fixes (28 `.map_err(|_|...)` -> `.map_err(|e|...)`)

## Files Reviewed

1. `crates/ac-service/src/crypto/mod.rs` - 19 fixes
2. `crates/ac-service/src/handlers/internal_tokens.rs` - 4 fixes
3. `crates/ac-service/src/handlers/auth_handler.rs` - 3 fixes
4. `crates/ac-service/src/config.rs` - 2 fixes

## Review Summary

The implementation correctly addresses the error hiding violations identified by the guard pipeline. All 28 instances of `.map_err(|_| ...)` have been changed to `.map_err(|e| ...)` with appropriate error logging.

### Pattern Analysis

The fix pattern used throughout is consistent and correct:

```rust
// Before (error hiding):
.map_err(|_| AcError::Crypto("Error message".to_string()))

// After (error preserved):
.map_err(|e| {
    tracing::error!(target: "crypto", error = %e, "Descriptive error message");
    AcError::Crypto("Generic client-facing message".to_string())
})
```

This pattern:
- Preserves the original error for debugging via structured logging
- Maintains generic client-facing error messages (security best practice)
- Uses appropriate tracing targets (`crypto`, `auth`)
- Correctly distinguishes between `tracing::error!` (for actual errors) and `tracing::debug!` (for validation failures)

### Detailed Review by File

#### 1. `crypto/mod.rs` (19 fixes)

**Findings**: No issues found.

All fixes correctly:
- Log the underlying error with `error = %e`
- Use `tracing::error!` for cryptographic operation failures
- Use `tracing::debug!` for input validation failures (token verification)
- Maintain consistent error message formatting

Examples of good patterns:
- Line 112-114: Keypair generation logs the error
- Line 148-150: Nonce generation failure logged
- Line 263-265: JWT signing failure logged
- Line 354-357: Public key decode uses `debug!` (correct - validation failure, not internal error)

#### 2. `handlers/internal_tokens.rs` (4 fixes)

**Findings**: No issues found.

The file contains 4 error mapping fixes in `sign_meeting_jwt` and `sign_guest_jwt`:
- Line 276-278: Private key format validation
- Line 287-289: JWT encode operation
- Line 303-305: Private key format validation (guest)
- Line 314-316: JWT encode operation (guest)

All correctly use `tracing::error!` with `target: "crypto"`.

#### 3. `handlers/auth_handler.rs` (3 fixes)

**Findings**: No issues found.

The file contains 3 error mapping fixes in `extract_client_credentials`:
- Line 303-305: Authorization header encoding validation
- Line 309-311: Base64 decode validation
- Line 314-316: UTF-8 decode validation

All correctly use `tracing::debug!` (appropriate for input validation failures from external requests).

#### 4. `config.rs` (2 fixes)

**Findings**: No issues found.

The file contains 2 error mapping fixes:
- Line 185-189: JWT clock skew parsing error
- Line 226-230: Bcrypt cost parsing error

Both correctly preserve the error in the error message and provide context about what was expected.

## Code Quality Assessment

### Strengths

1. **Consistent Pattern**: All fixes follow the same pattern, making the codebase predictable and maintainable.

2. **Security Awareness**: The implementation correctly separates internal logging (detailed) from client-facing messages (generic), preventing information leakage.

3. **Tracing Targets**: Consistent use of `target: "crypto"` and `target: "auth"` enables filtered log analysis.

4. **Error Level Appropriateness**: Correct use of `error!` for internal failures vs `debug!` for input validation failures.

### No Issues Found

After thorough review:
- No MAJOR issues
- No MINOR issues
- No TECH_DEBT suggestions

The implementation is clean, consistent, and follows Rust idioms and project patterns.

## Verdict

**APPROVED**

The error hiding fixes are well-implemented and follow security best practices. All 28 fixes:
- Preserve error context for debugging
- Maintain generic client-facing messages
- Use appropriate logging levels
- Follow consistent patterns

---

**Finding Count**:
- Blocker: 0
- Critical: 0
- Major: 0
- Minor: 0
- Tech Debt: 0

---

## Reflection Summary

**Knowledge Updates**: Added 1 new pattern to code-reviewer knowledge base.

This review reinforced the importance of error context preservation with security-aware logging. The consistent pattern across all 28 fixes demonstrated a mature understanding of the tradeoff between debugging needs and security:

1. **Structured logging with `error = %e`** captures full error context server-side for debugging
2. **Generic client-facing messages** prevent information leakage about internal systems
3. **Appropriate log levels** distinguish expected validation failures (`debug!`) from unexpected internal failures (`error!`)
4. **Consistent tracing targets** enable filtered log analysis by subsystem

Added new pattern "Error Context Preservation with Security-Aware Logging" to `patterns.md` as this is a reusable pattern for all error handling in the codebase. The pattern generalizes beyond this specific fix and will help future reviewers understand the security implications of error handling.

No pruning needed - existing entries remain relevant.
