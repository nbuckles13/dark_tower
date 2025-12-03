# Debate: Key Rotation Coordination

**Date**: 2025-12-02
**Status**: Consensus Reached
**Participants**: Auth Controller, Test, Security

## Topic

How should key rotation be coordinated in a multi-instance Auth Controller deployment?

## Context

- Auth Controller (AC) will have multiple instances per region for high availability
- Key rotation must happen exactly once (not duplicated across instances)
- All instances must pick up the new key after rotation
- Existing infrastructure: PostgreSQL (required), Kubernetes (likely), Redis (planned for Phase 5)

### Existing Key Rotation Implementation

Already implemented in `key_management_service.rs`:
- `initialize_signing_key()` - Creates initial key on startup
- `rotate_signing_key()` - Creates new key, deactivates old
- `get_jwks()` - Returns RFC 7517 formatted public keys

### Critical Gaps Identified

1. **No `kid` in JWT headers** - Tokens don't identify which key signed them
2. **No rotation trigger** - Can't initiate rotation via API
3. **`expire_old_keys()` is placeholder** - Returns empty Vec
4. **No coordination mechanism** - Multiple instances would conflict

## Options Considered

### Option 1: PostgreSQL Advisory Locks

Each instance runs background task, uses `pg_try_advisory_lock()` before rotating.

**Pros**:
- Zero new dependencies
- Simple implementation (~20 lines)
- Automatic lock release on crash
- PostgreSQL guarantees exactly-once

**Cons**:
- Every instance has rotation code (larger attack surface)
- Lock key constant could be targeted for DoS
- Advisory lock acquisition not logged by default

### Option 2: External Scheduler with K8s ServiceAccount JWT

K8s CronJob triggers rotation, authenticates via ServiceAccount JWT.

**Pros**:
- Rotation logic isolated from production instances
- Tamper-evident audit trail (K8s events)
- Clear separation of concerns

**Cons**:
- Hard dependency on Kubernetes
- Breaks local dev, Docker Compose, bare metal
- Complex TokenReview API integration
- Still needs internal coordination

### Option 3: External Scheduler with OAuth 2.0 (SELECTED)

External scheduler authenticates via OAuth 2.0 Client Credentials, calls rotation endpoint.

**Pros**:
- Deployment-agnostic (works anywhere)
- Uses existing authentication infrastructure
- Clear audit trail (OAuth tokens logged)
- Scheduler choice flexible (K8s CronJob, AWS EventBridge, cron)

**Cons**:
- Need dedicated privileged scope
- Scheduler credentials require secure management
- Endpoint still needs idempotency handling

### Option 4: Leader Election (Redis/Consul)

Distributed consensus elects one AC instance as leader.

**Pros**:
- Real-time leader failover
- Standard distributed systems pattern

**Cons**:
- New infrastructure dependency
- Complex failure modes (split-brain)
- Testing nightmare
- Over-engineering for hourly task

## Specialist Positions

### Round 1

| Specialist | Option 1 | Option 2 | Option 3 | Preferred |
|------------|----------|----------|----------|-----------|
| Auth Controller | 95% | 70% | - | Option 1 |
| Test | 90% | 65% | - | Option 1 |
| Security | 75% | 90% | - | Option 2 |

### Round 2

Auth Controller raised critical concern: K8s dependency (Option 2) breaks deployment flexibility.

Counter-proposed: Option 3 (OAuth-based scheduler) as deployment-agnostic alternative.

User selected Option 3 with dedicated OAuth scope requirement.

### Final Consensus (Option 3)

| Specialist | Score | Decision |
|------------|-------|----------|
| Auth Controller | 95% | YES |
| Test | 92% | YES |
| Security | 97% | YES |

**Average Satisfaction**: 94.7%

## Consensus Design

### Two-Tier Scope System

To protect against scheduler misconfiguration and credential compromise while allowing emergency rotations:

1. **`service.rotate-keys.ac`** - Normal scheduler scope
   - Rate limited: 1 successful rotation per 6 days
   - Returns 429 if rotation attempted too soon
   - Used by automated scheduler

2. **`admin.force-rotate-keys.ac`** - Emergency override scope
   - Minimal rate limit: 1 per hour
   - Additional audit logging ("forced_rotation": true)
   - Only granted to break-glass credentials
   - Manual operation by ops team for secret leaks

### Rate Limiting Implementation

Database-driven rate limiting (works across multiple AC instances):

```rust
// Get most recent key's created_at timestamp
let last_rotation = query("SELECT created_at FROM signing_keys
                           ORDER BY created_at DESC LIMIT 1");

let min_interval = match scope {
    "admin.force-rotate-keys.ac" => Duration::hours(1),
    "service.rotate-keys.ac" => Duration::days(6),
};

if elapsed < min_interval {
    return Err(TooManyRequests { retry_after: ... });
}
```

### Endpoint

- `POST /internal/rotate-keys`
- Requires bearer token with `service.rotate-keys.ac` OR `admin.force-rotate-keys.ac`
- Rate limit determined by scope
- Returns `429 Too Many Requests` with `Retry-After` header if too soon

### Authentication Requirements

1. **User tokens**: NEVER have rotation scopes (enforced at issuance)
2. **Service tokens**:
   - Scheduler client has `service.rotate-keys.ac`
   - Break-glass client has `admin.force-rotate-keys.ac`
3. **Client provisioning**: Via migration or admin tool, NOT public API

### Rotation Flow

1. External scheduler (K8s CronJob, EventBridge, etc.) triggers weekly
2. Scheduler authenticates via OAuth 2.0 Client Credentials
3. Calls `POST /internal/rotate-keys` with bearer token
4. AC validates token and scope
5. AC checks `signing_keys.created_at` for rate limiting
6. Creates new signing key, sets 1-week validity overlap
7. Old key remains in JWKS for grace period
8. Logs rotation event with audit details

### Key Validity Overlap

Per ADR-0003, keys have weekly rotation with 1-week overlap:
- Week 1: Sign with keyA, JWKS returns [keyA]
- Week 2: Sign with keyB, JWKS returns [keyA, keyB]
- Week 3: Sign with keyC, JWKS returns [keyB, keyC]

JWKS cache header: `Cache-Control: max-age=3600` (1 hour)

### Audit Requirements

All rotation attempts logged with:
- Timestamp
- Client ID (from token)
- Success/failure status
- Forced flag (true if `admin.force-rotate-keys.ac` scope used)
- New key fingerprint (on success)
- Old key fingerprint (on success)

### Security Requirements

1. Rotation scopes NEVER granted to user tokens
2. Dedicated scheduler client for normal rotations
3. Separate break-glass client for emergency rotations
4. Short token lifetime for both clients (5-15 minutes)
5. Rotation endpoint does NOT return private keys
6. Database-driven rate limiting prevents key churn attacks

### Test Requirements (P0)

1. Valid token with `service.rotate-keys.ac` scope succeeds
2. Valid token WITHOUT rotation scope returns 403
3. User token with rotation scope (injected) returns 403
4. Invalid/expired token returns 401
5. Rotation within 6 days returns 429 (for normal scope)
6. Force rotation within 1 hour returns 429 (for admin scope)
7. Force rotation after 1 hour succeeds

## Action Items

1. [ ] Create ADR-0008 documenting key rotation strategy
2. [ ] Add `kid` to JWT headers
3. [ ] Implement `/internal/rotate-keys` endpoint
4. [ ] Add `service.rotate-keys.ac` scope to allowed scopes
5. [ ] Add `admin.force-rotate-keys.ac` scope to allowed scopes
6. [ ] Implement database-driven rate limiting
7. [ ] Implement scope enforcement (user tokens blocked)
8. [ ] Add audit logging for rotation events
9. [ ] Implement `expire_old_keys()` function
10. [ ] Create scheduler client credentials in migration
11. [ ] Create break-glass client credentials in migration

## References

- RFC 7517: JSON Web Key (JWK)
- RFC 7519: JSON Web Token (JWT)
- ADR-0003: Service Authentication
- ADR-0007: Token Lifetime Strategy
