# ADR-0007: Token Lifetime Strategy

**Status**: Accepted

**Date**: 2025-12-01

**Deciders**: Security Specialist, Test Specialist, Auth Controller Specialist, Global Controller Specialist, Meeting Controller Specialist, Media Handler Specialist

**Debate Record**: [docs/debates/2025-12-01-token-age-and-refresh.md](../debates/2025-12-01-token-age-and-refresh.md)

---

## Context

The Authentication Controller issues JWT access tokens for service-to-service authentication. During security testing, we considered adding maximum token age validation (rejecting tokens with `iat` too far in the past) to reduce replay attack windows. However, this raised scalability concerns:

- **Current state**: Tokens have 1-hour expiration (`exp` claim), `iat` validated for future timestamps only (5-minute clock skew tolerance)
- **Security concern**: A stolen token remains valid for its full lifetime (up to 60 minutes)
- **Scalability concern**: Short token lifetimes (e.g., 15 minutes) would require frequent token refresh, increasing Auth Controller load significantly at scale

We needed to balance security (short token lifetimes) against scalability (reduced authentication load) while considering the different security postures of service-to-service vs user authentication.

## Decision

**Adopt a context-specific token strategy** that applies different token lifetime and refresh patterns based on the authentication context:

### Service-to-Service Tokens (Client Credentials Flow)

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Access token lifetime | 1 hour | Services can easily re-authenticate |
| Maximum age validation | None | Rely on `exp` claim only |
| Refresh tokens | Not implemented | Services authenticate via credentials |
| Revocation mechanism | JWT blacklist (Redis) | For emergency credential compromise |

**Rationale**: Services authenticate using client credentials that they possess permanently. Re-authentication is trivial (no user interaction), so short-lived tokens provide minimal security benefit while creating unnecessary load. A JWT blacklist provides emergency revocation capability.

### User Tokens (Authorization Code Flow - Future)

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Access token lifetime | 15 minutes | Minimize replay attack window |
| Refresh token lifetime | 24 hours | Balance security and UX |
| Maximum age validation | Implicit | Via short access token lifetime |
| Revocation mechanism | Database-stored refresh tokens | Revocable per session |

**Rationale**: User tokens cannot be easily re-obtained (requires user interaction). Refresh tokens allow short-lived access tokens while maintaining session continuity. This pattern:
- Reduces replay attack window by 75% (60 min → 15 min)
- Enables per-session revocation
- Maintains seamless UX via background refresh

### Implementation Phases

1. **Phase 4 (Current)**: No changes to token lifetime
2. **Phase 5 (GC Implementation)**: Add JWT blacklist in Redis for emergency revocation
3. **Phase 8 (User Auth)**: Implement refresh token flow with:
   - Refresh token rotation (new refresh token on each use)
   - Token family tracking (detect reuse attacks)
   - Rate limiting on refresh endpoint

## Consequences

**Positive**:
- Simple, stateless validation for service-to-service tokens
- No performance overhead for service authentication at scale
- Strong security for user sessions (when implemented)
- Emergency revocation capability via blacklist
- Aligns with OAuth 2.0 best practices

**Negative**:
- Two different token patterns to maintain
- JWT blacklist adds Redis dependency for revocation
- Refresh token implementation adds complexity (Phase 8)
- Different security guarantees for different contexts (must document clearly)

**Neutral**:
- Service tokens remain unchanged from current implementation
- User token implementation deferred to Phase 8

## Alternatives Considered

### Alternative 1: Short Access Tokens for All (15 minutes)
- **Pros**: Uniform security posture, reduced replay window
- **Cons**: Massive increase in Auth Controller load at scale; services would need to refresh every ~10 minutes; unnecessary complexity for service-to-service calls

### Alternative 2: Maximum Token Age Validation (45-50 minutes)
- **Pros**: Single token pattern, some replay protection
- **Cons**: Arbitrary threshold, marginal security benefit, adds complexity without addressing core concerns

### Alternative 3: Fully Configurable via Environment
- **Pros**: Maximum flexibility for deployments
- **Cons**: Combinatorial testing burden, harder to reason about security guarantees, potential for misconfiguration

## Implementation Notes

### Phase 5: JWT Blacklist

```rust
// Redis key pattern for blacklisted JWTs
// Key: blacklist:jwt:{jti}
// Value: 1 (presence indicates blacklisted)
// TTL: Remaining token lifetime at blacklist time

async fn is_token_blacklisted(jti: &str) -> Result<bool, Error> {
    redis.exists(format!("blacklist:jwt:{}", jti)).await
}
```

### Phase 8: Refresh Token Flow

For meeting sessions, token refresh happens out-of-band:
1. Server sends `TOKEN_EXPIRING_SOON` via signaling channel (~2 min before expiry)
2. Client requests refresh via REST API
3. Client sends `UPDATE_TOKEN` with new access token
4. WebTransport connection persists throughout (no disruption)

## References

- [RFC 6749: OAuth 2.0 Authorization Framework](https://tools.ietf.org/html/rfc6749)
- [RFC 7519: JSON Web Token (JWT)](https://tools.ietf.org/html/rfc7519)
- [NIST SP 800-63B: Digital Identity Guidelines](https://pages.nist.gov/800-63-3/sp800-63b.html)
- [OWASP ASVS 3.0.1: Session Management Requirements](https://owasp.org/www-project-application-security-verification-standard/)
- ADR-0003: Service Authentication (EdDSA, bcrypt)
- Debate: [2025-12-01-token-age-and-refresh.md](../debates/2025-12-01-token-age-and-refresh.md)
