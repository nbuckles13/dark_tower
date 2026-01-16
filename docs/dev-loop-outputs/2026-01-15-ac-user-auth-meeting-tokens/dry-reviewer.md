# DRY Reviewer Code Review - AC Internal Token Endpoints

**Date**: 2026-01-15
**Reviewer**: DRY Reviewer Specialist
**Files Reviewed**: `crates/ac-service/src/handlers/internal_tokens.rs` (438 lines)

---

## Verdict: TECH_DEBT

The new code does not use any existing utilities from `crates/common/` that it should have (no BLOCKER), but there is significant duplication within the file and a pattern that appears 3+ times across the codebase that is a candidate for extraction.

---

## DRY Assessment

### Summary

1. **No BLOCKER findings**: The `crates/common/` crate does not contain JWT signing utilities. It only provides:
   - `secret.rs`: `SecretBox`, `SecretString`, `ExposeSecret` (correctly used)
   - `types.rs`: ID types (`OrganizationId`, `MeetingId`, etc.) - not applicable here
   - `error.rs`: `DarkTowerError` - AC has its own `AcError` (acceptable per-service error types)
   - `config.rs`: Common config types - not applicable here

2. **TECH_DEBT: JWT signing pattern duplicated 3+ times**:
   - `crates/ac-service/src/crypto/mod.rs:244-269` - `sign_jwt()` for `Claims`
   - `crates/ac-service/src/handlers/internal_tokens.rs:267-291` - `sign_meeting_jwt()` for `MeetingTokenClaims`
   - `crates/ac-service/src/handlers/internal_tokens.rs:294-318` - `sign_guest_jwt()` for `GuestTokenClaims`
   - `crates/global-controller/tests/auth_tests.rs:64-71` - Test helper `sign_token()` (acceptable for tests)

3. **TECH_DEBT: Key loading/decryption pattern duplicated**:
   - `issue_meeting_token_internal()` lines 142-155
   - `issue_guest_token_internal()` lines 193-205
   - Both have identical code for loading active key and decrypting

---

## BLOCKER Findings (must fix)

**None**. The `crates/common/` crate does not have JWT-related utilities that should have been used.

---

## TECH_DEBT Findings (document for follow-up)

### TD-1: Generic JWT Signing Function (HIGH priority)

**Pattern**: JWT signing with EdDSA appears 3 times in production code with nearly identical implementation.

**Current State**: Three separate functions that all do the same thing:
1. Validate private key format with `Ed25519KeyPair::from_pkcs8()`
2. Create `EncodingKey::from_ed_der()`
3. Build `Header::new(Algorithm::EdDSA)` with `typ = "JWT"` and `kid`
4. Call `encode()`
5. Map error to `AcError::Crypto`

**Recommended Extraction**: Create a generic signing function in `crypto/mod.rs`:

```rust
/// Sign any serializable claims as a JWT with EdDSA.
pub fn sign_jwt_generic<T: Serialize>(
    claims: &T,
    private_key_pkcs8: &[u8],
    key_id: &str,
) -> Result<String, AcError>
```

**Files Affected**:
- `crates/ac-service/src/crypto/mod.rs` - already has `sign_jwt()` for `Claims`
- `crates/ac-service/src/handlers/internal_tokens.rs` - `sign_meeting_jwt()`, `sign_guest_jwt()`

**Migration Path**:
1. Rename existing `sign_jwt()` to `sign_jwt_generic<T: Serialize>()`
2. Create backward-compatible `sign_jwt(claims: &Claims, ...)` that calls generic version
3. Update `internal_tokens.rs` to use generic version
4. Delete `sign_meeting_jwt()` and `sign_guest_jwt()`

**Estimated Savings**: ~50 lines of duplicated code

---

### TD-2: Key Loading and Decryption Pattern (MEDIUM priority)

**Pattern**: Loading active signing key and decrypting private key appears 2 times with identical code.

**Current State** (both functions have this block):
```rust
// Load active signing key
let signing_key = signing_keys::get_active_key(&state.pool)
    .await?
    .ok_or_else(|| AcError::Crypto("No active signing key available".to_string()))?;

// Decrypt private key
let encrypted_key = EncryptedKey {
    encrypted_data: SecretBox::new(Box::new(signing_key.private_key_encrypted)),
    nonce: signing_key.encryption_nonce,
    tag: signing_key.encryption_tag,
};

let private_key_pkcs8 =
    crypto::decrypt_private_key(&encrypted_key, state.config.master_key.expose_secret())?;
```

**Recommended Extraction**: Create helper function in `crypto/mod.rs` or `handlers/internal_tokens.rs`:

```rust
/// Load the active signing key and decrypt the private key.
async fn load_decrypted_signing_key(
    pool: &PgPool,
    master_key: &[u8],
) -> Result<(Vec<u8>, String), AcError>  // Returns (private_key_pkcs8, key_id)
```

**Files Affected**:
- `crates/ac-service/src/handlers/internal_tokens.rs` - both `issue_meeting_token_internal()` and `issue_guest_token_internal()`
- Potentially other handlers that issue tokens in the future

**Estimated Savings**: ~20 lines of duplicated code

---

### TD-3: Handler Response Pattern (LOW priority)

**Pattern**: Both `handle_meeting_token` and `handle_guest_token` have identical structure for:
- Timing/metrics collection
- Scope validation
- Error recording

This is borderline acceptable since handler code often follows similar patterns, but could be extracted if more internal token types are added.

---

## Location Cross-Reference

| Pattern | New Location | Existing Location | Verdict |
|---------|--------------|-------------------|---------|
| JWT signing (EdDSA) | `internal_tokens.rs:267-291, 294-318` | `crypto/mod.rs:244-269` | TECH_DEBT (3+ instances) |
| Key load + decrypt | `internal_tokens.rs:142-155, 193-205` | N/A (internal duplication) | TECH_DEBT (2 instances) |
| SecretBox/ExposeSecret usage | `internal_tokens.rs:15, 140, 149, 190, 199` | `common/secret.rs` | APPROVED (correctly used) |
| EncryptedKey struct | `internal_tokens.rs:148-152, 198-202` | `crypto/mod.rs:76-82` | APPROVED (correctly used) |

---

## Recommendation

**Do NOT block merge**. The duplication identified is internal to the AC service and does not violate the ADR-0019 BLOCKER criteria (code exists in `crates/common/` but wasn't used).

**Create follow-up tickets**:
1. **[HIGH]** TD-1: Extract generic JWT signing function
2. **[MEDIUM]** TD-2: Extract key loading helper function

These can be addressed in a follow-up refactoring PR without blocking the current feature work.

---

## Compliance with ADR-0019

- [x] Checked `crates/common/` for existing utilities
- [x] No BLOCKER: common crate has no JWT utilities to reuse
- [x] Identified patterns appearing 3+ times for TECH_DEBT tracking
- [x] Documented findings with location cross-reference
- [x] Provided actionable recommendations for future extraction
