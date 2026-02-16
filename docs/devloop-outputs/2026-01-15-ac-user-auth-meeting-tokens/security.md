# Security Specialist Review: AC Internal Token Endpoints

**Date**: 2026-01-15
**Reviewer**: Security Specialist
**Implementation**: ADR-0020 Internal Token Endpoints (meeting-token, guest-token)

---

## Verdict: APPROVED

The implementation demonstrates strong security practices with proper authentication, authorization, cryptography, and input validation. No critical or high-severity findings identified.

---

## Security Assessment

### 1. Authentication - PASS

**Endpoints are properly protected by middleware:**

```rust
// routes/mod.rs:110-123
let internal_token_routes = Router::new()
    .route(
        "/api/v1/auth/internal/meeting-token",
        post(internal_tokens::handle_meeting_token),
    )
    .route(
        "/api/v1/auth/internal/guest-token",
        post(internal_tokens::handle_guest_token),
    )
    .layer(middleware::from_fn_with_state(
        auth_state.clone(),
        require_service_auth,
    ))
```

**Middleware validates JWT properly:**
- Extracts Bearer token from Authorization header
- Validates signature against active signing key via JWKS
- Respects configurable clock skew tolerance for iat validation
- Stores claims in request extensions for handler access

### 2. Authorization - PASS

**Scope validation is correctly implemented:**

```rust
// internal_tokens.rs:47-56
let token_scopes: Vec<&str> = claims.scope.split_whitespace().collect();
if !token_scopes.contains(&REQUIRED_SCOPE) {
    return Err(AcError::InsufficientScope {
        required: REQUIRED_SCOPE.to_string(),
        provided: token_scopes.iter().map(|s| s.to_string()).collect(),
    });
}
```

- Required scope: `internal:meeting-token` (correct per ADR-0020)
- Whitespace-separated scope parsing follows OAuth 2.0 spec (RFC 6749)
- Error response properly indicates required vs provided scopes

### 3. Cryptography - PASS

**JWT signing uses secure algorithms:**

```rust
// internal_tokens.rs:281-290
let encoding_key = EncodingKey::from_ed_der(private_key_pkcs8);

let mut header = Header::new(Algorithm::EdDSA);
header.typ = Some("JWT".to_string());
header.kid = Some(key_id.to_string());

encode(&header, claims, &encoding_key)
```

Security strengths:
- **EdDSA (Ed25519)**: Modern, secure signature algorithm
- **Key ID (kid)**: Included in header for key rotation support (ADR-0008)
- **Private key validation**: `Ed25519KeyPair::from_pkcs8()` validates key format before use
- **Encrypted at rest**: Private key decrypted from AES-256-GCM only when needed
- **Master key access**: Uses `expose_secret()` pattern for controlled access

**JTI generation is cryptographically secure:**

```rust
// internal_tokens.rs:170
jti: uuid::Uuid::new_v4().to_string(),
```

- UUID v4 uses CSPRNG (122 bits of entropy)
- Unique per token - enables revocation tracking per ADR-0020

### 4. Input Validation - PASS

**TTL is properly capped:**

```rust
// internal_tokens.rs:21
const MAX_TOKEN_TTL_SECONDS: u32 = 900;

// internal_tokens.rs:59
let ttl = payload.ttl_seconds.min(MAX_TOKEN_TTL_SECONDS);
```

- Maximum 15 minutes (900 seconds) per ADR-0020
- Server-side enforcement - client cannot request longer lifetime
- Uses `min()` for silent capping (appropriate for this use case)

**UUID fields use Uuid type for validation:**

```rust
// models/mod.rs:18-38
pub struct MeetingTokenRequest {
    pub subject_user_id: Uuid,
    pub meeting_id: Uuid,
    pub meeting_org_id: Uuid,
    pub home_org_id: Uuid,
    // ...
}
```

- Serde deserializes UUIDs directly - malformed UUIDs rejected at parse time
- No string manipulation on IDs - prevents injection attacks

**Guest capabilities are fixed server-side:**

```rust
// internal_tokens.rs:218
capabilities: vec!["video".to_string(), "audio".to_string()],
```

- Guest tokens always get fixed ["video", "audio"] per ADR-0020
- Client cannot specify elevated capabilities for guests
- Meeting tokens allow configurable capabilities (controlled by GC)

### 5. Information Leakage - PASS

**Error messages don't reveal sensitive details:**

```rust
// internal_tokens.rs:145, 195
AcError::Crypto("No active signing key available".to_string())
```

- Generic "No active signing key" - doesn't reveal key details
- Crypto errors logged server-side with target `crypto`, generic message to client

**Tracing instrumentation uses skip_all:**

```rust
// internal_tokens.rs:34-38
#[instrument(
    name = "ac.token.issue_meeting",
    skip_all,
    fields(grant_type = "internal_meeting", status)
)]
```

- `skip_all` prevents PII leakage in traces (per ADR-0011)
- Only grant_type and status recorded in spans

### 6. Timing Attacks - ACCEPTABLE

**Scope validation uses simple string comparison:**

```rust
if !token_scopes.contains(&REQUIRED_SCOPE) {
```

This is acceptable because:
- Scope strings are not secrets
- Early rejection on scope mismatch is appropriate
- Actual secret comparison (bcrypt verify) happens in service token issuance, not in these internal endpoints

**JWT validation in middleware:**
- `jsonwebtoken` crate uses constant-time signature verification internally
- Clock skew tolerance allows minor timing variations in iat checking

### 7. Logging - PASS

**Secrets properly redacted:**

- Claims use custom Debug impl with `[REDACTED]` for `sub` field
- EncryptedKey Debug impl redacts all cryptographic material
- SecretBox/SecretString enforce redaction at type level

**Error logging is appropriate:**

```rust
// internal_tokens.rs:277-278
let _key_pair = Ed25519KeyPair::from_pkcs8(private_key_pkcs8).map_err(|_| {
    tracing::error!(target: "crypto", "Invalid private key format");
```

- Errors logged to `crypto` target for filtering
- No key material in log messages

---

## ADR-0020 Compliance

| Requirement | Status | Evidence |
|-------------|--------|----------|
| Meeting tokens: 15 min max TTL | PASS | `MAX_TOKEN_TTL_SECONDS = 900` |
| Guest tokens: 15 min max TTL | PASS | Same constant used |
| All tokens AC-signed | PASS | EdDSA signing in `sign_meeting_jwt`/`sign_guest_jwt` |
| Guest tokens include `waiting_room` claim | PASS | Line 217: `waiting_room: payload.waiting_room` |
| Guest capabilities fixed to ["video", "audio"] | PASS | Line 218: hardcoded capabilities |
| Authorization requires `internal:meeting-token` scope | PASS | `REQUIRED_SCOPE = "internal:meeting-token"` |
| JTI included for revocation tracking | PASS | Lines 170, 221: UUID v4 jti |
| Token includes `token_type` claim | PASS | `"meeting"` or `"guest"` |
| Participant type in claims | PASS | `participant_type` field in both token types |

---

## Findings

No blocking findings identified.

---

## Recommendations (Non-Blocking)

### R1: Consider display_name length validation (LOW)

The `display_name` field in `GuestTokenRequest` has no explicit length limit in the models. While JSON parsing may have implicit limits, consider adding explicit validation:

```rust
// Recommendation: Add to GuestTokenRequest
#[validate(length(min = 1, max = 100))]
pub display_name: String,
```

**Rationale**: Prevents oversized display names from being included in JWTs. Current risk is low because:
- JWT size limit (4KB) provides defense-in-depth
- GC (the caller) should validate display name before calling AC

### R2: Consider rate limiting internal endpoints (LOW)

Internal token endpoints are only accessible with `internal:meeting-token` scope (service-to-service), but rate limiting could provide defense-in-depth against:
- Compromised service credentials
- Amplification if GC has a vulnerability

**Current mitigation**: Only GC should have `internal:meeting-token` scope, and GC should rate limit its guest-token endpoint.

### R3: Document capabilities allowlist (INFO)

Meeting tokens accept arbitrary capabilities from the request:

```rust
capabilities: payload.capabilities.clone(),
```

Consider documenting the expected values and whether MC should validate capabilities against an allowlist. This is informational - the security boundary is correct (GC controls what capabilities to request).

---

## Test Coverage Verification

The implementation includes unit tests for:
- Request/response serialization
- Participant type and meeting role enums
- TTL constant values
- Scope constant values

Integration tests should verify:
- [ ] Scope validation rejects unauthorized callers
- [ ] TTL capping works correctly
- [ ] JWT contains all required claims
- [ ] JWT validates against JWKS

---

## Conclusion

The AC internal token endpoints implementation follows security best practices and complies with ADR-0020 requirements. The code demonstrates:

1. **Defense in depth**: Multiple layers of validation (middleware + handler)
2. **Principle of least privilege**: Fixed capabilities for guests
3. **Secure defaults**: 15-min max TTL, waiting_room defaults to true
4. **No information leakage**: skip_all instrumentation, generic error messages
5. **Cryptographic hygiene**: EdDSA with kid, CSPRNG for JTI, encrypted keys at rest

**Approved for merge** pending integration test coverage.

---

*Security Specialist Review - 2026-01-15*
