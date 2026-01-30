# Security Review: AC Code Quality Fixes

**Reviewer**: Security Specialist
**Date**: 2026-01-29
**Verdict**: APPROVED

## Summary

Reviewed error hiding fixes in AC service. The changes improve debuggability by preserving original error context in server-side logs while maintaining all security properties.

## Files Reviewed

1. `crates/ac-service/src/crypto/mod.rs` - 19 error handling fixes
2. `crates/ac-service/src/handlers/internal_tokens.rs` - 4 error handling fixes
3. `crates/ac-service/src/handlers/auth_handler.rs` - 3 error handling fixes
4. `crates/ac-service/src/config.rs` - 2 error handling fixes

## Security Checklist

| Check | Result | Notes |
|-------|--------|-------|
| Error messages leak passwords/credentials? | NO | Passwords, secrets never in error context |
| Error messages leak key material? | NO | Crypto errors are generic "KeyRejected" style |
| Error messages leak JWT claims/tokens? | NO | Claims not included in error logging |
| Error messages leak PII? | NO | User identifiers not in error paths |
| Timing attack resistance preserved? | YES | Same error handling flow, just better logging |
| Client receives generic errors? | YES | All public errors remain generic |
| Server logs get details? | YES | tracing::error/debug captures `%e` |
| Log levels appropriate? | YES | debug for client validation, error for server ops |
| SecretBox/SecretString protection intact? | YES | All sensitive types properly wrapped |

## Detailed Analysis

### crypto/mod.rs

All crypto error logging uses the pattern:
```rust
.map_err(|e| {
    tracing::error!(target: "crypto", error = %e, "Operation failed");
    AcError::Crypto("Generic message".to_string())
})
```

This is the correct pattern because:
- `ring` and `bcrypt` library errors are generic operation failures
- They don't expose key material, plaintext, or ciphertext
- The public error message remains constant ("JWT signing failed", etc.)

### handlers/internal_tokens.rs

JWT signing error paths follow the same safe pattern. The `Ed25519KeyPair::from_pkcs8` error is a generic "KeyRejected" that indicates format issues, not key content.

### handlers/auth_handler.rs

Credential extraction errors log parsing failures:
- `to_str()` error: encoding issue
- Base64 decode error: malformed input
- UTF-8 error: byte sequence issue

None of these expose the actual credential content being parsed.

### config.rs

Configuration parsing errors log the string value that failed to parse. This is safe because:
- Values are configuration integers (JWT_CLOCK_SKEW_SECONDS, BCRYPT_COST)
- Not secrets (AC_MASTER_KEY, AC_HASH_SECRET errors don't log values)

## Findings

| Severity | Count | Description |
|----------|-------|-------------|
| CRITICAL | 0 | - |
| MAJOR | 0 | - |
| MINOR | 0 | - |
| TECH_DEBT | 0 | - |

## Verdict Rationale

The error handling changes are purely additive for debugging purposes. They preserve all existing security properties:

1. **Defense in depth**: Multiple layers still protect secrets
2. **Error message separation**: Client vs server error content properly isolated
3. **Crypto safety**: No cryptographic material exposed in any error path
4. **Timing resistance**: No new timing oracle introduced
5. **Log hygiene**: Appropriate log levels maintained

## Recommendation

APPROVED for merge. The changes improve operational visibility without compromising security.

---

## Reflection

**Knowledge Update**: Updated existing pattern "Server-Side Error Context with Generic Client Messages" in `docs/specialist-knowledge/security/patterns.md` to document that crypto library errors (ring, bcrypt, jsonwebtoken) are safe to log via `error = %e` because they indicate operation failure types without exposing key material or plaintext.

**Validation**: This review confirmed the error handling pattern is security-safe when library errors don't contain sensitive data and public error messages remain generic. The pattern already existed in knowledge base from 2026-01-28; this review validated it applies to crypto operations specifically.
