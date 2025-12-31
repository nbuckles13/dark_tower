# Principle: JWT Handling

## DO

- **Validate signature using EdDSA (Ed25519)** - All JWTs must be signed and verified with EdDSA algorithm
- **Reject tokens exceeding 8KB** - Check token size BEFORE parsing to prevent DoS attacks via oversized payloads
- **Validate `exp` claim** - Reject tokens with expiration timestamp (`exp`) in the past
- **Validate `iat` claim with clock skew** - Reject tokens with issued-at (`iat`) more than 5 minutes in the future
- **Check `kid` header** - Use Key ID to select the correct public key from JWKS during key rotation
- **Enforce EdDSA algorithm explicitly** - Specify `Algorithm::EdDSA` when decoding to prevent algorithm confusion attacks
- **Use JWKS endpoints for federation** - Fetch public keys from trusted JWKS endpoints over TLS with certificate pinning
- **Validate required claims** - Ensure `sub`, `exp`, `iat`, and `scope` claims are present and non-empty
- **Apply context-specific lifetimes** - Service tokens: 1 hour, User tokens: 15 minutes (when implemented)
- **Use constant-time comparison** - Validate tokens with timing-attack-resistant operations

## DON'T

- **Don't accept `alg: "none"` tokens** - Reject any JWT with algorithm set to "none" (CVE-2015-2951)
- **Don't trust `alg` header from token** - Always specify expected algorithm explicitly during verification
- **Don't accept HS256 with EdDSA public keys** - Prevent algorithm confusion attacks (attacker uses public key as HMAC secret)
- **Don't skip signature verification** - ALWAYS verify signature before trusting any claims
- **Don't use `kid` for key fetching without whitelist** - Only fetch keys matching known key IDs from trusted sources
- **Don't validate max age for service tokens** - Per ADR-0007, service tokens rely on `exp` only (no `iat` age check beyond clock skew)
- **Don't expose token validation errors** - Return generic "invalid or expired" messages to prevent information leakage
- **Don't parse tokens before size check** - Check token length BEFORE base64 decode or signature verification
- **Don't allow tokens with >10 custom claims** - Limit claim count to prevent resource exhaustion
- **Don't trust `typ` header for security** - The `typ` field is informational only, not security-critical

## Examples

### Good: Secure JWT Validation

```rust
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

pub fn verify_jwt(token: &str, public_key_pem: &str) -> Result<Claims, Error> {
    // 1. Check size BEFORE parsing (DoS prevention)
    if token.len() > MAX_JWT_SIZE_BYTES {
        return Err(Error::InvalidToken);
    }

    // 2. Extract public key from PEM
    let public_key_bytes = decode_pem_public_key(public_key_pem)?;
    let decoding_key = DecodingKey::from_ed_der(&public_key_bytes);

    // 3. Configure validation with EXPLICIT algorithm
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = true;
    validation.set_audience(&["dark-tower-services"]);
    validation.set_issuer(&["auth.dark.com"]);

    // 4. Verify signature and decode claims
    let token_data = decode::<Claims>(token, &decoding_key, &validation)?;

    // 5. Custom validation: iat within clock skew
    let now = Utc::now().timestamp();
    if token_data.claims.iat > now + JWT_CLOCK_SKEW_SECONDS {
        return Err(Error::InvalidToken);
    }

    Ok(token_data.claims)
}
```

### Bad: Insecure JWT Validation

```rust
// ❌ WRONG: Accepts any algorithm from token header
let validation = Validation::default();
let claims = decode::<Claims>(token, &key, &validation)?;

// ❌ WRONG: No size check (DoS vulnerable)
let claims = decode::<Claims>(token, &key, &validation)?;

// ❌ WRONG: Trusts kid without whitelist
let kid = extract_kid(token)?;
let key = fetch_key_from_url(&format!("https://{}/jwks", kid))?;

// ❌ WRONG: Detailed error messages leak information
if !user_exists {
    return Err("User not found in database");
} else if !password_valid {
    return Err("Invalid password");
}
```

### Good: JWT Signing with Rotation

```rust
pub fn sign_jwt(claims: &Claims, private_key: &[u8], key_id: &str) -> Result<String, Error> {
    // 1. Create header with algorithm and kid
    let mut header = Header::new(Algorithm::EdDSA);
    header.typ = Some("JWT".to_string());
    header.kid = Some(key_id.to_string());

    // 2. Sign with EdDSA private key
    let encoding_key = EncodingKey::from_ed_der(private_key);
    let token = encode(&header, claims, &encoding_key)?;

    Ok(token)
}
```

### Good: Service Token Claims (1 hour)

```rust
let claims = Claims {
    sub: "service-id-123".to_string(),
    exp: Utc::now().timestamp() + 3600,  // 1 hour
    iat: Utc::now().timestamp(),
    scope: "service.write.mh service.read.gc".to_string(),
    service_type: Some("meeting-controller".to_string()),
};
```

### Good: User Token Claims (15 minutes)

```rust
let claims = Claims {
    sub: "user-uuid-456".to_string(),
    exp: Utc::now().timestamp() + 900,  // 15 minutes
    iat: Utc::now().timestamp(),
    scope: "user.read.gc user.write.mc".to_string(),
    service_type: None,
};
```

## Security Tests Required

### P0 (Critical - Must Pass)

- **Signature Verification**: Token signed with wrong key rejected
- **Algorithm Confusion**: Tokens with `alg: "none"` or `alg: "HS256"` rejected
- **Expiration**: Tokens with `exp < now` rejected
- **Size Limit**: Tokens >8KB rejected before parsing
- **Required Claims**: Tokens missing `sub`, `exp`, `iat`, or `scope` rejected

### P1 (Important - Should Pass)

- **JWT Payload Tampering**: Modified claims detected by signature mismatch
- **Header Injection**: `kid` pointing to attacker-controlled keys rejected
- **Future iat Beyond Clock Skew**: Tokens with `iat` >5 min in future rejected
- **Stripped Signature**: Tokens with missing signature component rejected
- **Extra Claims**: Unknown claims safely ignored during deserialization

### P2 (Defense-in-Depth)

- **Token Size Boundary**: Test exact 8KB boundary (4095, 4096, 4097 bytes)
- **Clock Skew Boundary**: Test iat at exact 5-minute boundary
- **Claim Count Limit**: Tokens with >10 claims rejected

## Token Lifetime Strategy (ADR-0007)

### Service-to-Service Tokens

| Parameter | Value | Validation |
|-----------|-------|------------|
| Access token lifetime | 1 hour | `exp` claim |
| Maximum age validation | **None** | No `iat` age check (only future `iat` beyond clock skew) |
| Refresh tokens | Not used | Services re-authenticate via client credentials |
| Revocation | JWT blacklist (Redis) | Emergency revocation only |

**Rationale**: Services possess permanent credentials and can easily re-authenticate. Short lifetimes provide minimal security benefit while creating unnecessary Auth Controller load.

### User Tokens (Future - Phase 8)

| Parameter | Value | Validation |
|-----------|-------|------------|
| Access token lifetime | 15 minutes | `exp` claim |
| Refresh token lifetime | 24 hours | Database-stored, revocable |
| Maximum age validation | Implicit | Via short access token lifetime |
| Revocation | Per-session | Database-tracked refresh tokens |

**Rationale**: User tokens cannot be easily re-obtained. Refresh tokens enable short-lived access tokens while maintaining seamless UX.

## Attack Scenarios & Defenses

### Algorithm Confusion (CVE-2015-2951)

**Attack**: Attacker changes `alg: "EdDSA"` to `alg: "HS256"`, signs token with HMAC using public key as secret.

**Defense**:
- Specify `Algorithm::EdDSA` explicitly in `Validation::new()`
- jsonwebtoken library enforces algorithm match
- Signature verification fails if algorithm doesn't match

### Token Pre-Generation

**Attack**: Attacker generates tokens with future `iat` for replay after compromise.

**Defense**:
- Validate `iat <= now + JWT_CLOCK_SKEW_SECONDS`
- Reject tokens issued >5 minutes in the future
- Detects compromised systems with incorrect clocks

### DoS via Oversized Tokens

**Attack**: Attacker sends 10MB JWT to exhaust server resources.

**Defense**:
- Check `token.len() > MAX_JWT_SIZE_BYTES` BEFORE parsing
- Reject immediately without base64 decode or signature verification
- Typical tokens: 200-500 bytes, limit: 8KB

### Kid Injection

**Attack**: Attacker sets `kid: "../../../etc/passwd"` or `kid: "https://attacker.com/jwks"`.

**Defense**:
- Never fetch keys using `kid` directly
- Whitelist known key IDs before lookup
- Only fetch from trusted JWKS endpoints with certificate pinning

### Username Enumeration via Timing

**Attack**: Measure response time differences between "user not found" vs "wrong password".

**Defense**:
- Always run bcrypt verification (even for non-existent users with dummy hash)
- Return identical error messages for all authentication failures
- Log generic "Invalid credentials" regardless of failure reason

## Guards

**Guard**: `test_jwt_signature_verification`
- **What**: Rejects tokens signed with incorrect private key
- **Where**: `crates/ac-service/src/services/token_service.rs::test_jwt_wrong_signature_rejected`

**Guard**: `test_jwt_algorithm_confusion`
- **What**: Rejects tokens with tampered `alg` header (EdDSA→HS256, EdDSA→none)
- **Where**: `crates/ac-service/src/services/token_service.rs::test_jwt_algorithm_confusion_rejected`

**Guard**: `test_jwt_size_limit`
- **What**: Rejects tokens >8KB before parsing
- **Where**: `crates/ac-service/src/services/token_service.rs::test_jwt_oversized_token_rejected`

**Guard**: `test_jwt_expiration`
- **What**: Rejects tokens with `exp` in the past
- **Where**: `crates/ac-service/src/services/token_service.rs::test_jwt_expired_token_rejected`

**Guard**: `test_jwt_future_iat`
- **What**: Rejects tokens with `iat` >5 min in future
- **Where**: `crates/ac-service/src/services/token_service.rs::test_jwt_future_iat_beyond_clock_skew_rejected`

**Guard**: `test_jwt_kid_injection`
- **What**: Rejects tokens signed with attacker's key despite spoofed `kid`
- **Where**: `crates/ac-service/src/services/token_service.rs::test_jwt_header_kid_injection`

**Guard**: `test_jwt_payload_tampering`
- **What**: Detects modified claims via signature mismatch
- **Where**: `crates/ac-service/src/services/token_service.rs::test_jwt_payload_tampering_rejected`

**Guard**: `test_timing_attack_prevention`
- **What**: Authentication attempts take constant time regardless of failure reason
- **Where**: `crates/ac-service/src/services/token_service.rs::test_timing_attack_prevention_invalid_client_id`

## ADR References

- **ADR-0003: Service Authentication** - EdDSA signing, JWT structure, JWKS federation, token validation
- **ADR-0007: Token Lifetime Strategy** - Service tokens 1hr lifetime (no max age), user tokens 15min + refresh, no `iat` age validation for service tokens beyond clock skew tolerance

## Implementation Reference

- **JWT Signing**: `crates/ac-service/src/crypto/mod.rs::sign_jwt()`
- **JWT Verification**: `crates/ac-service/src/crypto/mod.rs::verify_jwt()`
- **Token Issuance**: `crates/ac-service/src/services/token_service.rs::issue_service_token()`
- **Security Tests**: `crates/ac-service/src/services/token_service.rs::tests` (P1 JWT validation tests)
- **Size Limit Constant**: `crates/ac-service/src/crypto/mod.rs::MAX_JWT_SIZE_BYTES`
- **Clock Skew Constant**: `crates/ac-service/src/crypto/mod.rs::JWT_CLOCK_SKEW_SECONDS`
