# ADR-0008: Key Rotation Strategy

**Status**: Accepted

**Date**: 2025-12-02

**Deciders**: Auth Controller, Test, Security specialists (multi-agent debate)

---

## Context

The Auth Controller (AC) uses EdDSA (Ed25519) signing keys to issue JWTs. These keys need periodic rotation for security hygiene. The system must support:

1. **Multi-instance deployment**: Multiple AC instances per region for HA
2. **Exactly-once rotation**: Only one instance should perform rotation
3. **Zero-downtime**: Tokens signed with old keys remain valid during transition
4. **Emergency rotation**: Ability to force rotation if keys are compromised
5. **Deployment flexibility**: Must work with K8s, Docker Compose, bare metal

### Current State

Existing implementation in `key_management_service.rs`:
- `initialize_signing_key()` - Creates initial key on startup
- `rotate_signing_key()` - Creates new key, deactivates old
- `get_jwks()` - Returns RFC 7517 formatted public keys

**Gaps identified**:
- No `kid` in JWT headers (tokens don't identify signing key)
- No API endpoint to trigger rotation
- `expire_old_keys()` is a placeholder
- No coordination for multi-instance deployments

## Decision

### External Scheduler with OAuth 2.0 Authentication

Rotation is triggered by an external scheduler (K8s CronJob, AWS EventBridge, cron, etc.) that authenticates via OAuth 2.0 Client Credentials and calls a dedicated rotation endpoint.

### Two-Tier Scope System

Two scopes control rotation, each with different rate limits:

| Scope | Purpose | Rate Limit | Use Case |
|-------|---------|------------|----------|
| `service.rotate-keys.ac` | Normal scheduled rotation | 1 per 6 days | Automated weekly scheduler |
| `admin.force-rotate-keys.ac` | Emergency rotation | 1 per hour | Manual break-glass for key compromise |

Scope format follows established pattern: `{principal}.{operation}.{component}`

### Rate Limiting

Database-driven rate limiting ensures protection across multiple AC instances:

```rust
// Query most recent key creation time
let last_rotation = sqlx::query_scalar!(
    "SELECT created_at FROM signing_keys ORDER BY created_at DESC LIMIT 1"
).fetch_optional(pool).await?;

let min_interval = match scope {
    s if s.contains("force-rotate-keys") => Duration::hours(1),
    s if s.contains("rotate-keys") => Duration::days(6),
    _ => return Err(InsufficientScope),
};

if let Some(last) = last_rotation {
    if Utc::now() - last < min_interval {
        return Err(TooManyRequests { retry_after: ... });
    }
}
```

### Endpoint Specification

**`POST /internal/rotate-keys`**

**Request**:
```
Authorization: Bearer <token-with-rotation-scope>
```

**Success Response** (200 OK):
```json
{
  "rotated": true,
  "new_key_id": "uuid-v4",
  "old_key_id": "uuid-v4",
  "old_key_valid_until": "2025-12-09T00:00:00Z"
}
```

**Rate Limited Response** (429 Too Many Requests):
```
Retry-After: 518400
```
```json
{
  "error": {
    "code": "TOO_MANY_REQUESTS",
    "message": "Key rotation allowed every 6 days",
    "retry_after_seconds": 518400
  }
}
```

**Forbidden Response** (403):
```json
{
  "error": {
    "code": "INSUFFICIENT_SCOPE",
    "message": "Requires scope: service.rotate-keys.ac",
    "required_scope": "service.rotate-keys.ac"
  }
}
```

### Key Validity Overlap

Keys have a 1-week overlap period to ensure in-flight tokens remain valid:

| Week | Signing Key | JWKS Contains |
|------|-------------|---------------|
| 1 | keyA | [keyA] |
| 2 | keyB | [keyA, keyB] |
| 3 | keyC | [keyB, keyC] |

The `valid_until` timestamp is set to 1 week after rotation. Keys are removed from JWKS when `valid_until` passes.

### JWT `kid` Header

All issued JWTs must include the `kid` (Key ID) header:

```rust
pub fn sign_jwt(claims: &Claims, private_key: &[u8], key_id: &str) -> Result<String, AcError> {
    let header = Header {
        algorithm: Algorithm::EdDSA,
        key_id: Some(key_id.to_string()),
        ..Default::default()
    };
    // ...
}
```

This allows validators to identify which key signed a token without trial-and-error.

### Client Provisioning

Two service clients are created via database migration:

1. **key-rotation-scheduler**: Has `service.rotate-keys.ac` scope
   - Used by automated scheduler (CronJob, etc.)
   - Credentials stored in secrets management

2. **key-rotation-breakglass**: Has `admin.force-rotate-keys.ac` scope
   - Used for emergency manual rotation
   - Credentials stored in secure vault with access logging

**User tokens can NEVER have rotation scopes** - enforced at token issuance.

### Audit Logging

All rotation attempts are logged:

```json
{
  "event": "key_rotation_attempt",
  "timestamp": "2025-12-02T10:30:00Z",
  "client_id": "key-rotation-scheduler",
  "success": true,
  "forced": false,
  "new_key_id": "...",
  "old_key_id": "...",
  "ip_address": "10.0.1.50"
}
```

The `forced` flag is `true` when `admin.force-rotate-keys.ac` scope is used.

## Consequences

**Positive**:
- Deployment-agnostic (works with any scheduler)
- Uses existing OAuth 2.0 infrastructure
- Rate limiting prevents key churn attacks
- Emergency rotation path for compromised keys
- Clear audit trail for compliance
- Multi-instance safe via database-driven rate limits

**Negative**:
- Requires external scheduler setup
- Two sets of credentials to manage
- Slightly more complex than background task approach

**Neutral**:
- Scheduler configuration is outside AC codebase
- Break-glass credentials require secure storage

## Alternatives Considered

### PostgreSQL Advisory Locks

Each AC instance runs a background task and uses `pg_try_advisory_lock()`.

- **Pros**: Zero new dependencies, simple implementation
- **Cons**: Every instance has rotation code (larger attack surface), lock key could be targeted for DoS

### K8s ServiceAccount JWT

K8s CronJob authenticates via ServiceAccount token validated through TokenReview API.

- **Pros**: Tight K8s integration, no OAuth overhead
- **Cons**: Hard K8s dependency, breaks local dev and bare metal

### Leader Election (Redis/Consul)

Distributed consensus elects one AC instance as rotation leader.

- **Pros**: Real-time failover
- **Cons**: New infrastructure dependency, complex failure modes, over-engineering

## Implementation Notes

### Files to Modify

1. `crypto/mod.rs` - Add `key_id` parameter to `sign_jwt()`
2. `services/token_service.rs` - Pass `key_id` to `sign_jwt()`
3. `handlers/admin_handler.rs` - Add `handle_rotate_keys()` endpoint
4. `routes/mod.rs` - Wire up `/internal/rotate-keys` route
5. `services/key_management_service.rs` - Implement `expire_old_keys()`
6. `errors.rs` - Add `TooManyRequests` error variant
7. `handlers/jwks_handler.rs` - Add `Cache-Control` header

### Database Migration

```sql
-- Add scheduler client
INSERT INTO service_credentials (client_id, client_secret_hash, service_type, scopes, ...)
VALUES ('key-rotation-scheduler', '...', 'internal', ARRAY['service.rotate-keys.ac'], ...);

-- Add break-glass client
INSERT INTO service_credentials (client_id, client_secret_hash, service_type, scopes, ...)
VALUES ('key-rotation-breakglass', '...', 'internal', ARRAY['admin.force-rotate-keys.ac'], ...);
```

### Test Requirements

**P0 (Security-Critical)**:
- Valid token with `service.rotate-keys.ac` succeeds
- Valid token without rotation scope returns 403
- User token with rotation scope returns 403
- Rotation within 6 days returns 429

**P1 (Important)**:
- Force rotation within 1 hour returns 429
- Force rotation after 1 hour succeeds
- JWT contains `kid` header matching signing key
- Old key remains valid for 1 week after rotation

## References

- [RFC 7517: JSON Web Key (JWK)](https://tools.ietf.org/html/rfc7517)
- [RFC 7519: JSON Web Token (JWT)](https://tools.ietf.org/html/rfc7519)
- [ADR-0003: Service Authentication](./adr-0003-service-authentication.md)
- [ADR-0007: Token Lifetime Strategy](./adr-0007-token-lifetime-strategy.md)
- [Debate: Key Rotation Coordination](../debates/2025-12-02-key-rotation-coordination.md)
