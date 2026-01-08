# Principle: JWT Handling

**All JWTs MUST use EdDSA (Ed25519) signing and size-checked validation.** Never trust token headers for security decisions.

**ADRs**: ADR-0003 (Service Auth), ADR-0007 (Token Lifetime)

---

## DO

### Validation
- **Validate signature using EdDSA (Ed25519)** - specify `Algorithm::EdDSA` explicitly to prevent algorithm confusion
- **Reject tokens >8KB BEFORE parsing** - check `token.len()` before any base64 decode or signature verification
- **Validate `exp` claim** - reject tokens with expiration in the past
- **Validate `iat` with clock skew** - reject tokens issued >5 minutes in the future
- **Check `kid` header** - use Key ID to select correct public key from JWKS during rotation
- **Validate required claims** - ensure `sub`, `exp`, `iat`, `scope` are present and non-empty
- **Use constant-time comparison** - timing-attack-resistant operations for all token validation

### Federation
- **Use JWKS endpoints** - fetch public keys from trusted endpoints over TLS with certificate pinning
- **Whitelist known key IDs** - only accept `kid` values matching known keys from trusted sources

---

## DON'T

### Security Critical
- **NEVER accept `alg: "none"`** - reject any JWT with algorithm set to "none" (CVE-2015-2951)
- **NEVER trust `alg` header** - always specify expected algorithm explicitly, don't read from token
- **NEVER accept HS256 with EdDSA keys** - prevents algorithm confusion (attacker uses public key as HMAC secret)
- **NEVER skip signature verification** - always verify before trusting claims
- **NEVER parse before size check** - DoS prevention requires checking length first

### Information Leakage
- **Don't expose validation errors** - return generic "invalid or expired" messages only
- **Don't use `kid` for direct key fetching** - attackers can inject paths or URLs

### Limits
- **Don't allow >10 custom claims** - prevents resource exhaustion
- **Don't trust `typ` header** - informational only, not security-critical

---

## Quick Reference

| Parameter | Value | Notes |
|-----------|-------|-------|
| Algorithm | EdDSA (Ed25519) | Only supported algorithm |
| Max token size | 8KB (8192 bytes) | Check BEFORE parsing |
| Clock skew tolerance | 5 minutes (300s) | For `iat` validation |
| Service token lifetime | 1 hour | `exp` claim only, no `iat` age check |
| User token lifetime | 15 minutes | Plus 24hr refresh token (future) |

| Claim | Required | Validation |
|-------|----------|------------|
| `sub` | Yes | Subject identifier (service/user ID) |
| `exp` | Yes | Must be in future |
| `iat` | Yes | Must not be >5min in future |
| `scope` | Yes | Space-separated permissions |
| `kid` | Header | Must match known key IDs |

| Attack | Defense |
|--------|---------|
| Algorithm confusion | Explicit `Algorithm::EdDSA` in validation |
| Oversized token DoS | Size check before parsing |
| Kid injection | Whitelist known key IDs |
| Timing attack | Constant-time comparison, bcrypt for non-existent users |

---

## Guards

**Security tests** (P0/P1):
- Signature verification with wrong key
- Algorithm confusion (`alg: none`, `alg: HS256`)
- Expiration validation
- Size limit enforcement
- `kid` injection prevention
- Payload tampering detection
