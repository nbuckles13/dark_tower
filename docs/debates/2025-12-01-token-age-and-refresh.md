# Debate: Token Lifetime Strategy

**Date**: 2025-12-01
**Status**: Consensus Reached
**Participants**: Security, Test, Auth Controller, Global Controller, Meeting Controller, Media Handler

## Topic

Should we implement maximum token age validation (rejecting tokens with old `iat`), and if so, what should the threshold be? Consider alternatives like refresh tokens.

## Context

- Current tokens have 1-hour expiration (`exp` claim)
- `iat` validation rejects tokens with future `iat` beyond 5-minute clock skew
- No maximum age for past `iat` - tokens valid for full hour
- Security specialist initially recommended 15-minute max age, but this raised scalability concerns

## Options Considered

1. **No maximum age** (current) - Simple, tokens valid for full 1-hour lifetime
2. **Short access + refresh tokens** - 15 min access tokens, 24h refresh tokens
3. **Long maximum age** (45-50 min) - Compromise within single token lifetime
4. **Context-specific** - Different limits for services vs users
5. **Configurable** - Environment variable for deployment flexibility

## Specialist Positions

### Security Specialist (85/100 for Option 2)
- Recommends 15-minute access + 24-hour refresh tokens
- Key benefits: 75% reduction in attack window, revocation capability
- Requires: Refresh token rotation, family tracking, rate limiting
- References: NIST SP 800-63B, OWASP ASVS 3.0.1

### Test Specialist (85/100 for Option 2)
- Supports Option 2 as most testable (standard OAuth pattern)
- Estimates ~25 new tests needed for refresh flow
- Effort: 16-24 hours for full test coverage
- Concern: Option 3/4/5 introduce timing-sensitive or combinatorial tests

### Auth Controller Specialist (95/100 for Option 1 + Blacklist)
- **Dissenting view**: Refresh tokens solve a problem we don't have
- Client Credentials flow allows services to re-authenticate trivially
- Proposes JWT blacklist (Redis) for emergency revocation instead
- Concern: Refresh tokens introduce stateful validation, kill stateless design

### Global Controller Specialist (85/100 for Option 4 Hybrid)
- Proposes context-specific approach:
  - Service-to-service: No max age (trusted, network-isolated)
  - User tokens: 15-min access + refresh pattern
- Rationale: Different threat models for different token types
- Impact: Minimal for services, moderate for user auth implementation

### Meeting Controller Specialist (95/100 for Option 2)
- **Strongly supports** refresh tokens for meeting continuity
- Key insight: Token refresh happens out-of-band via signaling channel
- WebTransport connections persist during refresh - no disruption
- Proposed protocol additions: `TOKEN_EXPIRING_SOON`, `UPDATE_TOKEN` messages

### Media Handler Specialist (85/100 for Option 2)
- Confirms token validation is NOT in media hot path
- Validation at stream setup only, not per-packet
- Performance impact: Negligible (~1ms at connection, not ongoing)
- Supports 15-30 min access tokens + 24h refresh

## Consensus

**Agreed approach: Context-Specific Token Strategy (Option 4 + elements of Option 2)**

### Service-to-Service Tokens (Client Credentials Flow)
- **Access token lifetime**: 1 hour (current)
- **Maximum age validation**: None (rely on `exp`)
- **Refresh tokens**: Not needed (services re-authenticate via credentials)
- **Revocation**: JWT blacklist in Redis for emergency cases

### User Tokens (Future - Authorization Code Flow)
- **Access token lifetime**: 15 minutes
- **Refresh token lifetime**: 24 hours
- **Maximum age validation**: Implicit via short access token lifetime
- **Revocation**: Refresh tokens stored in database, revocable

### Implementation Phases
1. **Phase 4 (Current)**: No changes - defer max age to Phase 5
2. **Phase 5 (GC Implementation)**: Add JWT blacklist for emergency revocation
3. **Phase 8 (User Auth)**: Implement refresh token flow for users

## Satisfaction Scores

| Specialist | Score | Notes |
|------------|-------|-------|
| Security | 85 | Accepts phased approach, wants blacklist soon |
| Test | 85 | Appreciates deferral of complexity |
| Auth Controller | 95 | Happy to keep service tokens simple |
| Global Controller | 85 | Context-specific aligns with gateway role |
| Meeting Controller | 90 | Refresh pattern preserved for user sessions |
| Media Handler | 85 | No performance concerns with approach |

**Average Satisfaction**: 87.5/100

## Action Items

1. [ ] Create ADR-0007 documenting token lifetime strategy
2. [ ] Add JWT blacklist design to Phase 5 roadmap
3. [ ] Document refresh token flow for Phase 8 user auth
4. [ ] Update TODO.md with deferred items

## References

- RFC 6749: OAuth 2.0 Authorization Framework
- RFC 7519: JSON Web Token (JWT)
- NIST SP 800-63B: Digital Identity Guidelines
- OWASP ASVS 3.0.1: Session Management Requirements
