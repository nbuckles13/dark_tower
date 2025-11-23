# ADR-0003: Service Authentication & Federation

**Status**: Accepted

**Date**: 2025-01-22

**Deciders**: Security Specialist, Auth Controller Specialist, Global Controller Specialist, Meeting Controller Specialist, Media Handler Specialist

---

## Context

Dark Tower is a distributed system with multiple services that need to communicate securely:
- Global Controller ↔ Meeting Controller
- Meeting Controller ↔ Media Handler
- Auth Controller ↔ All Services

**Security Requirements**:
- Zero-trust architecture (authenticate every service call)
- Federation support (multiple clusters, cross-cluster token validation)
- Scalable (thousands of services across multiple regions)
- OAuth 2.0 scope-based authorization

**Problems to solve**:
- How do services authenticate to each other?
- How are tokens validated without calling issuer?
- How do we support federated clusters?
- How do we prevent token theft and replay?

## Decision

**We adopt a dual-layer security model: mTLS for transport + JWT for authorization.**

### Component 1: Auth Controller

**New subsystem**: Auth Controller issues and validates all authentication tokens.

**Responsibilities**:
- User authentication (username/password, OAuth future)
- Service authentication (client credentials)
- Token issuance (user tokens, service tokens)
- JWKS endpoint (public key distribution)
- Key rotation
- Token revocation (future)

**Deployment**:
- Multiple instances per cluster (HA)
- One cluster per continent or customer
- All instances in cluster share same signing key

### Component 2: OAuth 2.0 Scopes

**Scope Format**: `{principal}.{operation}.{component}`

**Principal Types**:
- `user` - End user tokens
- `service` - Service-to-service tokens

**Operations**:
- `read` - Read-only (GET, query)
- `write` - Mutating (POST, PUT, DELETE)
- `admin` - Administrative (elevated permissions)

**Components**:
- `gc` - Global Controller
- `mc` - Meeting Controller
- `mh` - Media Handler
- `ac` - Auth Controller (future)

**Examples**:
- `user.read.gc` - User can read from GC
- `user.write.mc` - User can publish streams, subscribe to layouts
- `service.write.mh` - Service can route media
- `service.admin.gc` - Service can perform admin operations

### Component 3: Token Types

**User Tokens** (issued by Auth Controller):
```json
{
  "sub": "user_id",
  "org_id": "org_id",
  "email": "user@example.com",
  "scopes": ["user.read.gc", "user.write.gc", "user.read.mc", "user.write.mc"],
  "iss": "auth.us.dark.com",
  "iat": 1234567890,
  "exp": 1234571490,  // 1 hour
  "aud": "dark-tower-api"
}
```

**Service Tokens** (issued by Auth Controller):
```json
{
  "sub": "service_id",
  "service_type": "meeting-controller",
  "region": "us-west-1",
  "scopes": ["service.write.mh", "service.read.gc"],
  "iss": "auth.us.dark.com",
  "iat": 1234567890,
  "exp": 1234578690,  // 2 hours
  "aud": "dark-tower-internal"
}
```

**Connection Tokens** (issued by Meeting Controller for Media Handler access):
```json
{
  "sub": "participant_id",
  "meeting_id": "meeting_id",
  "user_id": "0x123456",  // 8-byte user ID
  "media_handler_id": "mh-abc123",  // Restrict to specific MH
  "client_fingerprint": "sha256...",  // Bind to specific client
  "scopes": ["media.publish", "media.subscribe"],
  "iss": "mc-instance-xyz",
  "iat": 1234567890,
  "exp": null,  // Valid until meeting ends
  "aud": "dark-tower-mh"
}
```

### Component 4: Federation

**Federation Model**: Multiple clusters, cross-cluster token validation

**Federation Config** (distributed to all services):
```yaml
federation:
  clusters:
    - name: "us-primary"
      auth_controller_url: "https://auth.us.dark.com"
      jwks_url: "https://auth.us.dark.com/.well-known/jwks.json"

    - name: "eu-primary"
      auth_controller_url: "https://auth.eu.dark.com"
      jwks_url: "https://auth.eu.dark.com/.well-known/jwks.json"

  local_cluster: "us-primary"
```

**JWKS Format**:
```json
{
  "keys": [
    {
      "kid": "auth-us-2025-01",
      "kty": "OKP",
      "crv": "Ed25519",
      "x": "base64_public_key",
      "use": "sig",
      "alg": "EdDSA"
    }
  ]
}
```

**Cross-Cluster Validation**:
1. Service receives token with `kid: auth-eu-2025-01`
2. Service checks JWKS cache for `auth-eu-2025-01`
3. If not found, fetch JWKS from all federated clusters
4. Validate signature using public key
5. Accept token from any trusted cluster

### Component 5: mTLS for Transport Security

**gRPC Service-to-Service**:
- All internal gRPC uses mTLS
- Mutual certificate verification
- Prevents MITM attacks
- Complements JWT authorization

**Flow**:
```
Meeting Controller → Media Handler (RouteMedia):
1. mTLS handshake (both verify each other's certs)
2. MC sends gRPC request with service token in metadata:
   metadata["authorization"] = "Bearer <JWT>"
3. MH validates mTLS cert (transport authenticated)
4. MH validates JWT signature (token authentic)
5. MH checks JWT scopes: requires "service.write.mh"
6. If valid, process request
```

### Component 6: Token Validation (Actor-Based)

**JwksManagerActor** (see ADR-0001):
- Manages JWKS cache from all federated clusters
- Refreshes JWKS hourly
- Handles unknown kid by force-refresh
- Rate limits refresh to prevent abuse

**Validation Flow**:
```rust
async fn validate_token(
    token: &str,
    jwks_manager: &JwksManagerHandle,
) -> Result<Claims, ValidationError> {
    let header = decode_jwt_header(token)?;
    let kid = header.kid.ok_or(ValidationError::MissingKid)?;

    // Try to get key (fast path)
    let public_key = match jwks_manager.get_key(&kid).await? {
        Some(key) => key,
        None => {
            // Unknown kid - refresh and retry
            jwks_manager.force_refresh().await?;
            jwks_manager.get_key(&kid).await?
                .ok_or(ValidationError::UnknownKid { kid })?
        }
    };

    verify_jwt_signature(token, &public_key)?;
    validate_expiration(claims.exp)?;
    Ok(claims)
}
```

### Component 7: Service Credentials (Client Credentials Flow)

**Service Registration** (at deployment):
```
1. Deploy new MC instance
2. Call Auth Controller: POST /admin/services/register
   Body: { "service_type": "meeting-controller", "region": "us-west-1" }
3. Auth Controller returns: { "client_id": "...", "client_secret": "..." }
4. Deployment injects as env vars: AC_CLIENT_ID, AC_CLIENT_SECRET
```

**Token Exchange** (at service startup):
```
1. MC calls Auth Controller: POST /auth/service/token
   Headers: Authorization: Basic base64(client_id:client_secret)
   Body: { "grant_type": "client_credentials" }
2. Auth Controller validates credentials
3. Auth Controller issues service token (2-hour lifetime)
4. MC uses token in all service-to-service calls
5. MC refreshes token before expiration
```

### Component 8: Key Rotation

**Weekly rotation schedule**:
```
Week 1: Keys [keyA]          - sign with keyA
Week 2: Keys [keyA, keyB]    - sign with keyB, validate both
Week 3: Keys [keyB, keyC]    - sign with keyC, validate both
Week 4: Keys [keyC, keyD]    - sign with keyD, validate both
```

**One-week overlap** allows services to refresh JWKS without rejecting valid tokens.

## Consequences

### Positive

- ✅ **Zero-trust**: Every service call authenticated and authorized
- ✅ **Federation**: Users/services can cross clusters seamlessly
- ✅ **Scalable**: No network call for token validation (local signature check)
- ✅ **OAuth 2.0 standard**: Well-understood, library support
- ✅ **Defense in depth**: mTLS + JWT
- ✅ **Granular permissions**: Scope-based authorization
- ✅ **Key rotation**: Weekly rotation limits exposure
- ✅ **Connection token binding**: Prevents token theft

### Negative

- ❌ **Complexity**: Multiple token types, federation config
- ❌ **Key distribution**: All services need JWKS from all clusters
- ❌ **Token size**: JWTs are ~500 bytes (vs 16-byte session IDs)
- ❌ **Revocation delay**: Tokens valid until expiration (no instant revocation)

### Neutral

- All services must implement token validation
- Federation config must be deployed to all services
- Clock sync required (JWT expiration checks)

## Alternatives Considered

### Alternative 1: Shared Secrets

**Approach**: Each service pair shares a secret for HMAC signing

**Pros**:
- Simple to implement
- Fast validation

**Cons**:
- N² secrets to manage (every service pair)
- Compromised secret affects all services
- No federation support
- Difficult rotation

**Why not chosen**: Doesn't scale, no federation

### Alternative 2: Central Auth Service (Token Introspection)

**Approach**: Services call Auth Controller to validate every token

**Pros**:
- Instant revocation
- No JWKS distribution

**Cons**:
- Network call on every request (latency)
- Auth Controller bottleneck
- Single point of failure

**Why not chosen**: Latency and scalability concerns

### Alternative 3: Mutual TLS Only (No JWTs)

**Approach**: Use mTLS certificates for both transport and authorization

**Pros**:
- One security mechanism
- Strong cryptography

**Cons**:
- Certificate management complexity
- No fine-grained scopes (all-or-nothing access)
- Hard to revoke (CRL distribution)
- Difficult to support federation

**Why not chosen**: Insufficient authorization granularity

### Alternative 4: Macaroons

**Approach**: Use macaroons (bearer tokens with caveats)

**Pros**:
- Delegatable credentials
- Attenuation (add restrictions to token)

**Cons**:
- Less mature ecosystem
- More complex than JWT
- Not widely understood

**Why not chosen**: JWT is standard, well-supported

## Implementation Notes

### Service Token Issuance Flow

```
┌─────────────┐                  ┌──────────────────┐
│   Service   │                  │ Auth Controller  │
│(MC instance)│                  │                  │
└──────┬──────┘                  └────────┬─────────┘
       │                                  │
       │ POST /auth/service/token         │
       │ Basic auth(client_id, secret)    │
       │─────────────────────────────────>│
       │                                  │
       │                                  │ Validate credentials
       │                                  │ Check client_id in DB
       │                                  │ Verify client_secret hash
       │                                  │
       │   200 OK                         │
       │   { access_token, expires_in }   │
       │<─────────────────────────────────│
       │                                  │
```

### Service-to-Service Call Flow

```
┌──────────────────┐         ┌──────────────┐
│Meeting Controller│         │Media Handler │
└────────┬─────────┘         └──────┬───────┘
         │                          │
         │ gRPC: RouteMedia         │
         │ mTLS + JWT in metadata   │
         │─────────────────────────>│
         │                          │
         │                          │ 1. Verify mTLS cert
         │                          │ 2. Extract JWT from metadata
         │                          │ 3. Validate JWT (JwksManagerActor)
         │                          │ 4. Check scopes
         │                          │ 5. Process request
         │                          │
         │    Response              │
         │<─────────────────────────│
         │                          │
```

### Migration Path

**Phase 1: MVP** (current)
- Implement Auth Controller
- Client credentials for service auth
- JWKS distribution via config file
- Single cluster (US)

**Phase 2: Federation**
- Add EU cluster
- JWKS endpoints (not config file)
- Cross-cluster token validation

**Phase 3: Certificate-Based Auth**
- Replace client credentials with certs
- Automate cert issuance via cert-manager

**Phase 4: Advanced**
- Token revocation list (Redis)
- Short-lived tokens (5 minutes) with refresh
- OIDC integration for enterprise customers

### Security Considerations

**Token Storage**:
- Services: Store in memory only (never persist)
- Clients: Store in memory or secure storage (not localStorage)

**Token Transmission**:
- Always over TLS/QUIC (never plaintext)
- HTTP: `Authorization: Bearer <token>`
- gRPC: `metadata["authorization"] = "Bearer <token>"`
- WebTransport: In protobuf message field

**Scope Validation**:
```rust
fn check_scope(claims: &Claims, required: &str) -> Result<(), AuthError> {
    if !claims.scopes.contains(&required.to_string()) {
        return Err(AuthError::InsufficientScope {
            required: required.to_string(),
            provided: claims.scopes.clone(),
        });
    }
    Ok(())
}
```

## References

- OAuth 2.0 RFC 6749: https://datatracker.ietf.org/doc/html/rfc6749
- JWT RFC 7519: https://datatracker.ietf.org/doc/html/rfc7519
- JWKS RFC 7517: https://datatracker.ietf.org/doc/html/rfc7517
- EdDSA (Ed25519): RFC 8032
- Implementation: See `JwksManagerActor`, `Auth Controller` crate
- Related: ADR-0001 (Actor Pattern for JWKS management)
- Related: ADR-0002 (Error handling in validation code)
- Architecture: `docs/ARCHITECTURE.md` (Auth Controller section)
- Security: `.claude/agents/security.md`
