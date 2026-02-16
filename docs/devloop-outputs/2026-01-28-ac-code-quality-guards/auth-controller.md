# Auth Controller Specialist Checkpoint

**Date**: 2026-01-28
**Task**: Fix AC code quality violations (28 error hiding + 4 instrument skip-all)
**Status**: Complete

---

## Patterns Discovered

### Pattern: Error Logging with Context Preservation
When preserving error context in crypto operations, the pattern is:
```rust
.map_err(|e| {
    tracing::error!(target: "crypto", error = %e, "Operation failed");
    AcError::Crypto("Generic message to client".to_string())
})
```

This logs the actual error server-side for debugging while returning a generic message to clients.

### Pattern: Debug-Level Logging for Authentication Failures
For credential parsing failures that could reveal information to attackers, use debug-level:
```rust
.map_err(|e| {
    tracing::debug!(target: "auth", error = %e, "Invalid authorization header");
    AcError::InvalidCredentials
})
```

This prevents credential enumeration attacks while still logging for debugging.

### Pattern: Config Error Messages Include Parse Error
For configuration parsing, the original error can be included since it's not security-sensitive:
```rust
.map_err(|e| {
    ConfigError::InvalidValue(format!("ENV_VAR must be valid, got '{}': {}", value_str, e))
})
```

---

## Gotchas Encountered

### Gotcha: Multi-line Instrument Attributes Cause False Positives
The `instrument-skip-all` guard uses grep to detect `#[instrument` without `skip_all`. When the attribute spans multiple lines:
```rust
#[instrument(
    name = "ac.token.issue_meeting",
    skip_all,               // This is on line N+2
    fields(...)
)]
```
The grep sees line N (`#[instrument(`) without `skip_all` and flags it as a violation, even though `skip_all` is present on line N+2.

**Resolution**: These are false positives. The code already uses the correct allowlist pattern. The guard has known limitations with multi-line attributes.

### Gotcha: ring::signature Errors Don't Implement Display
The `ring` crate's error types (like `KeyRejected`) implement `std::error::Error` but their Display implementation is minimal. Using `%e` in tracing still works and captures the error type, though the message may be brief like "unspecified".

---

## Key Decisions

### Decision: Use `error = %e` for All Captured Errors
Consistently use `error = %e` in tracing statements even when the error type has limited Display output. This:
1. Captures whatever information is available
2. Creates a consistent pattern across the codebase
3. Enables searching logs for `error=`

### Decision: Keep Generic Client Messages
Even when preserving error context in logs, client-facing error messages remain generic:
- `"Key generation failed"` - not `"PKCS8 generation failed: invalid seed"`
- `"JWT signing failed"` - not `"Ed25519 keypair rejected: invalid length"`
- `"Decryption failed"` - not `"AES-GCM authentication tag mismatch"`

This is defense-in-depth against information leakage.

### Decision: No Changes to AcError Variants
The existing `AcError::Crypto(String)` variant was sufficient for all error hiding fixes. The `AcError::Internal` variant (which has no context) was not used by any of the violations.

---

## Current Status

### Completed
- [x] Fixed 19 error hiding violations in `crypto/mod.rs`
- [x] Fixed 4 error hiding violations in `handlers/internal_tokens.rs`
- [x] Fixed 3 error hiding violations in `handlers/auth_handler.rs`
- [x] Fixed 2 error hiding violations in `config.rs`
- [x] All 370 unit tests pass
- [x] All 77 integration tests pass
- [x] Clippy passes with no warnings
- [x] Format check passes

### Not Changed (False Positives)
- 4 instrument-skip-all "violations" are false positives
  - `handlers/internal_tokens.rs:34` - already has `skip_all`
  - `handlers/internal_tokens.rs:87` - already has `skip_all`
  - `handlers/auth_handler.rs:86` - already has `skip_all`
  - `handlers/auth_handler.rs:209` - already has `skip_all`

---

## Files Modified

1. `crates/ac-service/src/crypto/mod.rs` - 19 fixes
2. `crates/ac-service/src/handlers/internal_tokens.rs` - 4 fixes
3. `crates/ac-service/src/handlers/auth_handler.rs` - 3 fixes
4. `crates/ac-service/src/config.rs` - 2 fixes

**Total**: 28 error hiding fixes across 4 files

---

## Reflection Summary

This implementation reinforced the importance of preserving error context while maintaining security boundaries. The key insight is that server-side debugging needs detailed errors, but clients should only receive generic messages to prevent information leakage.

The pattern of using different logging levels (error vs debug) based on security sensitivity is now codified in the patterns file. For cryptographic operations, we log actual errors at error level since they indicate system issues. For credential parsing, we use debug level since detailed errors could aid enumeration attacks.

The false positives from the instrument-skip-all guard highlight a limitation with line-based grep patterns for multi-line Rust attributes. This has been documented as a gotcha to help future developers distinguish real violations from tool limitations.

### Knowledge Updates
- **Added 1 pattern**: Error context preservation with security-aware logging
- **Added 1 gotcha**: Multi-line instrument attributes causing guard false positives

These entries will help future implementations maintain the balance between debuggability and security.
