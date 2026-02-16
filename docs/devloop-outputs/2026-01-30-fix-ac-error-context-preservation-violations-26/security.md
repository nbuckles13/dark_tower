# Security Review: Fix AC Error-Context-Preservation Violations

**Date**: 2026-01-30
**Reviewer**: Security Specialist
**Verdict**: APPROVED

---

## Files Reviewed

1. `crates/ac-service/src/crypto/mod.rs` - 19 fixes
2. `crates/ac-service/src/handlers/internal_tokens.rs` - 4 fixes
3. `crates/ac-service/src/handlers/auth_handler.rs` - 3 fixes

---

## Security Analysis

### 1. Crypto Error Messages (crypto/mod.rs)

**Assessment**: SECURE

The changes include error context from crypto library errors (ring, bcrypt, jsonwebtoken) in the returned `AcError::Crypto(String)` variant. These changes are security-safe for the following reasons:

1. **Crypto library errors are safe to include**: Errors from `ring::signature::Ed25519KeyPair::from_pkcs8()`, `ring::aead::UnboundKey::new()`, `bcrypt::hash()`, and `jsonwebtoken::encode()` indicate operation failure types (e.g., "KeyRejected", "InvalidSignature", "InvalidInput") but do NOT expose:
   - Key material
   - Plaintext data
   - Secret bytes
   - Internal state

2. **Error response sanitization at API boundary**: Looking at `errors.rs`, the `IntoResponse` implementation for `AcError::Crypto` returns a generic message to clients:
   ```rust
   AcError::Crypto(err) => {
       tracing::error!(target: "crypto", error = %err, "Cryptographic operation failed");
       (StatusCode::INTERNAL_SERVER_ERROR, "CRYPTO_ERROR", "An internal error occurred".to_string(), ...)
   }
   ```
   The actual error context is logged server-side but NOT returned to clients.

3. **#[instrument(skip_all)] on all crypto functions**: All functions continue to use `skip_all` which prevents tracing from capturing sensitive parameters.

4. **Pattern examples verified as secure**:
   - `format!("Keypair generation failed: {}", e)` - ring's error doesn't leak key bytes
   - `format!("Nonce generation failed: {}", e)` - only indicates RNG failure
   - `format!("Password hashing failed: {}", e)` - bcrypt error doesn't leak password
   - `format!("JWT signing operation failed: {}", e)` - jsonwebtoken error is generic

### 2. JWT Signing Errors (internal_tokens.rs)

**Assessment**: SECURE

The changes to `sign_meeting_jwt()` and `sign_guest_jwt()` follow the same pattern as crypto/mod.rs:
- Error context from `Ed25519KeyPair::from_pkcs8()` and `jsonwebtoken::encode()` is included in error
- These errors don't leak key material or token content
- Error sanitization happens at the handler level before client response

### 3. Credential Extraction Errors (auth_handler.rs)

**Assessment**: SECURE

**Critical finding properly handled**: The `extract_client_credentials()` function uses `InvalidCredentials` for authentication failures. The implementation correctly uses:

```rust
.map_err(|_| AcError::InvalidCredentials)
```

This is the CORRECT pattern because:
1. **Information leakage prevention**: Authentication failures should NOT include error details that help attackers (e.g., "base64 decode failed at position 15" reveals credential format)
2. **Enumeration attack prevention**: Generic error prevents distinguishing between "malformed credential" and "wrong credential"
3. **Consistent with ADR-0011**: Invalid credentials responses must be uniform regardless of failure mode

The three cases in `extract_client_credentials()`:
- `auth_header.to_str()` failure: Uses `|_|` - CORRECT (don't reveal header encoding issues)
- `general_purpose::STANDARD.decode()` failure: Uses `|_|` - CORRECT (don't reveal base64 format issues)
- `String::from_utf8()` failure: Uses `|_|` - CORRECT (don't reveal UTF-8 issues)

### 4. Sensitive Data Protection

**Verified**:
- `Claims` struct has custom Debug that redacts `sub` field
- `UserClaims` struct has custom Debug that redacts `sub`, `email`, `jti` fields
- `EncryptedKey` struct has custom Debug that redacts all fields
- All password/secret parameters use `SecretString` with automatic Debug redaction
- `#[instrument(skip_all)]` on all handlers and crypto functions

### 5. Error Response to Clients

**Verified in errors.rs**:
| Error Variant | Client Message | Internal Logging |
|--------------|----------------|------------------|
| `Crypto(String)` | "An internal error occurred" | Full context logged |
| `Database(String)` | "An internal database error occurred" | Full context logged |
| `InvalidCredentials` | "Invalid client credentials" | No details (intentional) |
| `InvalidToken(String)` | Token reason passed through | Token-specific message |

The `Crypto` variant correctly sanitizes error context from reaching clients while preserving it for debugging.

---

## Findings Summary

| Severity | Count | Description |
|----------|-------|-------------|
| BLOCKER | 0 | None |
| CRITICAL | 0 | None |
| MAJOR | 0 | None |
| MINOR | 0 | None |

---

## Security Patterns Validated

1. **Defense-in-Depth**: Error context preserved internally, sanitized at API boundary
2. **Information Leakage Prevention**: `InvalidCredentials` correctly hides error details
3. **Tracing-Safe Functions**: All `#[instrument(skip_all)]` annotations preserved
4. **Constant-Time Operations**: No changes to timing-sensitive code
5. **Secrets Handling**: `SecretString`/`SecretBox` usage unchanged

---

## Recommendations

None. The implementation correctly balances:
- **Debugging needs**: Error context is preserved in error types and logged server-side
- **Security requirements**: Client responses are sanitized, auth failures are uniform
- **Crypto safety**: Library errors don't leak sensitive material

---

## Verdict

**APPROVED**

The error-context-preservation fixes are security-safe. Crypto library error messages don't leak sensitive data, and the error sanitization at the API boundary prevents information disclosure to clients. The intentional exclusion of error context for `InvalidCredentials` follows security best practices for authentication failures.

---

## Checklist

- [x] Secrets exposure check: No secrets in error messages
- [x] Crypto correctness: Error messages from crypto libs are safe
- [x] Auth vulnerabilities: InvalidCredentials uses generic error (correct)
- [x] Information leakage: API boundary sanitizes internal errors
- [x] Timing attack resistance: No changes to timing-sensitive code
- [x] Input validation: Unchanged
- [x] Rate limiting: Unchanged
