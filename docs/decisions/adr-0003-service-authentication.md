# ADR-0003: Service Authentication & Federation

**Status**: Accepted

**Date**: 2025-01-22

> **Scope note**: This ADR covers **service-to-service authentication** (Client Credentials, service tokens, mTLS). For **user authentication and authorization**, see [ADR-0020](adr-0020-user-auth-meeting-access.md).

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

### Component 2: OAuth 2.0 Scopes (Service Tokens)

**Scope Format**: `{principal}.{operation}.{component}`

**Principal Types**:
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
- `service.write.mh` - Service can route media
- `service.read.gc` - Service can read from GC
- `service.admin.gc` - Service can perform admin operations

### Component 3: Token Types

**User Tokens**: See [ADR-0020](adr-0020-user-auth-meeting-access.md) for user token format and authorization model.

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
3. If not found, fetch JWKS from all federated clusters (with certificate pinning)
4. Validate signature using public key
5. Accept token from any trusted cluster

**JWKS Security** (Protection against JWKS poisoning):
- **Certificate pinning**: Services pin AC's TLS certificate to prevent MITM attacks
- **HTTPS only**: JWKS fetched over TLS 1.3
- Implementation example:
```rust
let ac_cert = include_bytes!("../certs/auth-us.crt");
let jwks_client = reqwest::Client::builder()
    .add_root_certificate(reqwest::Certificate::from_pem(ac_cert)?)
    .build()?;
```

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
2. Call Auth Controller: POST /api/v1/admin/services/register
   Body: { "service_type": "meeting-controller", "region": "us-west-1" }
3. Auth Controller returns: { "client_id": "...", "client_secret": "..." }
4. Deployment injects as env vars: AC_CLIENT_ID, AC_CLIENT_SECRET
```

**Token Exchange** (at service startup):
```
1. MC calls Auth Controller: POST /api/v1/auth/service/token
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
       │ POST /api/v1/auth/service/token  │
       │ Basic auth(client_id, secret)    │
       │─────────────────────────────────>│
       │                                  │
       │                                  │ Validate credentials
       │                                  │ Check client_id in DB
       │                                  │ Verify client_secret hash
       │                                  │
       │   200 OK                         │
       │   { access_token, expires_in,    │
       │     scope, token_type }          │
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

### OAuth 2.0 Token Response Format

**Service Token Response** (RFC 6749 Section 5.1):
```json
{
  "access_token": "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9...",
  "token_type": "Bearer",
  "expires_in": 7200,
  "scope": "service.write.mh service.read.gc"
}
```

**User Token Response**: See [ADR-0020](adr-0020-user-auth-meeting-access.md).

**Field Requirements** (service tokens):
- `access_token` (REQUIRED): The JWT token
- `token_type` (REQUIRED): Always "Bearer"
- `expires_in` (REQUIRED): Token lifetime in seconds
- `scope` (REQUIRED): Space-separated list of granted scopes

**Note**: RFC 6749 makes `scope` optional if identical to requested scope, but Dark Tower **always includes it** to avoid client confusion about granted permissions.

### OAuth 2.0 Error Responses

**401 Unauthorized** (invalid, expired, or missing token):
```http
HTTP/1.1 401 Unauthorized
WWW-Authenticate: Bearer realm="dark-tower-api", error="invalid_token", error_description="The access token expired"
Content-Type: application/json

{
  "error": {
    "code": "UNAUTHORIZED",
    "message": "The access token expired"
  }
}
```

**403 Forbidden** (insufficient scope):
```http
HTTP/1.1 403 Forbidden
WWW-Authenticate: Bearer realm="dark-tower-api", error="insufficient_scope", scope="service.admin.gc"
Content-Type: application/json

{
  "error": {
    "code": "FORBIDDEN",
    "message": "Requires scope: service.admin.gc",
    "required_scope": "service.admin.gc",
    "provided_scopes": ["service.read.gc", "service.write.mh"]
  }
}
```

**400 Bad Request** (malformed request):
```http
HTTP/1.1 400 Bad Request
WWW-Authenticate: Bearer realm="dark-tower-api", error="invalid_request", error_description="Missing grant_type parameter"
Content-Type: application/json

{
  "error": {
    "code": "INVALID_REQUEST",
    "message": "Missing grant_type parameter"
  }
}
```

**WWW-Authenticate Header** (RFC 6750 Section 3):
- **REQUIRED** on all 401 responses involving Bearer tokens
- **error** parameter: `invalid_token`, `invalid_request`, `insufficient_scope`
- **error_description** parameter: Human-readable description
- **scope** parameter (403 only): Required scope for the operation

### Security Considerations

**Token Storage**:
- Services: Store in memory only (never persist)
- Clients: Store in memory or secure storage (not localStorage)

**Token Transmission**:
- Always over TLS/QUIC (never plaintext)
- HTTP: `Authorization: Bearer <token>`
- gRPC: `metadata["authorization"] = "Bearer <token>"`
- WebTransport: In protobuf message field

**Scope Validation** (service tokens only — user tokens use roles per [ADR-0020](adr-0020-user-auth-meeting-access.md)):
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

### Cryptographic Requirements

**CSPRNG (Cryptographically Secure RNG)**:

All random values MUST use `ring::rand::SystemRandom`:

```rust
use ring::rand::{SecureRandom, SystemRandom};

// Signing key generation
let rng = SystemRandom::new();
let pkcs8_bytes = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng)
    .map_err(|_| CryptoError::KeyGenerationFailed)?;

// Client secret generation (32 bytes = 256 bits)
let mut secret = [0u8; 32];
rng.fill(&mut secret)
    .map_err(|_| CryptoError::RandomGenerationFailed)?;
let client_secret = base64::encode_config(&secret, base64::URL_SAFE_NO_PAD);
```

**DO NOT USE**:
- `rand::thread_rng()` - Not guaranteed cryptographically secure
- `std::collections::hash_map::RandomState` - Not cryptographic

**Key Encryption at Rest**:

Private signing keys encrypted with AES-256-GCM before storing in database:

```rust
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};

// Encryption
fn encrypt_private_key(
    plaintext: &[u8],
    master_key: &[u8; 32],
) -> Result<EncryptedKey, CryptoError> {
    let unbound_key = UnboundKey::new(&AES_256_GCM, master_key)
        .map_err(|_| CryptoError::InvalidKey)?;
    let key = LessSafeKey::new(unbound_key);

    // Generate random 96-bit nonce
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes)?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut ciphertext = plaintext.to_vec();
    let tag = key.seal_in_place_separate_tag(nonce, Aad::empty(), &mut ciphertext)
        .map_err(|_| CryptoError::EncryptionFailed)?;

    Ok(EncryptedKey {
        ciphertext,
        nonce: nonce_bytes.to_vec(),
        tag: tag.as_ref().to_vec(),
    })
}

// Decryption
fn decrypt_private_key(
    encrypted: &EncryptedKey,
    master_key: &[u8; 32],
) -> Result<Vec<u8>, CryptoError> {
    let unbound_key = UnboundKey::new(&AES_256_GCM, master_key)
        .map_err(|_| CryptoError::InvalidKey)?;
    let key = LessSafeKey::new(unbound_key);

    let nonce = Nonce::try_assume_unique_for_key(&encrypted.nonce)
        .map_err(|_| CryptoError::InvalidNonce)?;

    let mut plaintext = encrypted.ciphertext.clone();
    plaintext.extend_from_slice(&encrypted.tag);

    key.open_in_place(nonce, Aad::empty(), &mut plaintext)
        .map_err(|_| CryptoError::DecryptionFailed)?;

    // Remove tag from end
    plaintext.truncate(plaintext.len() - 16);
    Ok(plaintext)
}
```

**Master Key Management**:
- **Production**: Environment variable `AC_MASTER_KEY` (base64-encoded 32 bytes)
- **Future**: Migrate to HashiCorp Vault for cloud-independent key management
- **Never**: Commit master key to git, log master key, expose in APIs
- **Rotation**: Master key rotated annually, all signing keys re-encrypted

**Database Storage**:
```sql
CREATE TABLE signing_keys (
    key_id VARCHAR(50) PRIMARY KEY,
    public_key TEXT NOT NULL,  -- PEM format, plaintext
    private_key_encrypted BYTEA NOT NULL,  -- AES-256-GCM ciphertext
    encryption_nonce BYTEA NOT NULL,  -- 96-bit nonce
    encryption_tag BYTEA NOT NULL,  -- 128-bit authentication tag
    encryption_algorithm VARCHAR(50) NOT NULL DEFAULT 'AES-256-GCM',
    -- ...
);
```

### Rate Limiting & Brute Force Protection

**Multi-Layer Rate Limiting**:

**Layer 1: IP-Based** (prevents distributed attacks):
- User login: 10 attempts per 15 minutes per IP
- Service token: 60 requests per hour per IP
- JWKS endpoint: 100 requests per minute per IP

**Layer 2: Account-Based** (prevents credential stuffing):
- User account: 10 failed attempts → lock account for 1 hour
- Service credential: 20 failed attempts → disable credential, require admin re-enable

**Layer 3: Exponential Backoff** (slows attackers):
```rust
fn calculate_backoff(failed_attempts: u32) -> Duration {
    match failed_attempts {
        0..=2 => Duration::from_secs(0),      // No delay
        3..=5 => Duration::from_secs(5),      // 5 second delay
        6..=8 => Duration::from_secs(30),     // 30 second delay
        9..=10 => Duration::from_secs(300),   // 5 minute delay
        _ => Duration::from_secs(3600),       // 1 hour lockout
    }
}
```

**Layer 4: CAPTCHA** (future):
- Require CAPTCHA after 3 failed login attempts
- Prevents automated brute force

**Layer 5: Alerting**:
- Email user on 5+ failed login attempts
- Alert ops team on unusual patterns (100+ failures/min)

**Implementation**:
- **MVP**: In-memory rate limiting (single AC instance)
- **Production**: Redis-based distributed rate limiting (multi-instance AC)

**Rate Limit Response** (RFC 6585):
```http
HTTP/1.1 429 Too Many Requests
Retry-After: 300
X-RateLimit-Limit: 10
X-RateLimit-Remaining: 0
X-RateLimit-Reset: 1234567890

{
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Too many failed login attempts. Try again in 5 minutes.",
    "retry_after": 300
  }
}
```

## Implementation Status

| Component | Status | Commit/PR | Notes |
|-----------|--------|-----------|-------|
| OAuth 2.0 Client Credentials | ✅ Done | | Token issuance for service-to-service auth |
| JWT EdDSA Signatures | ✅ Done | | Ed25519 signing with JWKS |
| JWKS Endpoint | ✅ Done | | Public key distribution |
| Token Validation | ✅ Done | | Claim validation with clock skew tolerance |
| Rate Limiting | ✅ Done | | Token bucket algorithm |
| Bcrypt Password Hashing | ✅ Done | | Cost factor 12 |
| AES-256-GCM Key Encryption | ✅ Done | | Private key encryption at rest |
| Error Counter Metrics | ❌ Pending | | Add `ac_errors_total` (counter by error_type, endpoint) to enable error budget calculations in SLO dashboards. Track token issuance errors, validation errors, JWKS errors, HTTP 4xx/5xx responses. Required for `ac-slos.json` error budget panels which currently show -74914% due to missing metrics. Label dimensions: `error_type` (validation_failed, token_expired, rate_limited, internal_error), `endpoint` (/token, /jwks, /validate). Align with ADR-0011 observability framework. |
| Token Validation Metrics | ❌ Pending | | Wire `ac_token_validations_total` at call sites. Recording function exists in `metrics.rs` (`#[allow(dead_code)]`) but is not called from token validation code paths. |
| Key Management Metrics | ❌ Pending | | Wire `ac_signing_key_age_days`, `ac_active_signing_keys`, `ac_key_rotation_last_success_timestamp` at call sites. Recording functions exist in `metrics.rs` (`#[allow(dead_code)]`) but are not called from key rotation code paths. |
| Rate Limit Metrics | ❌ Pending | | Wire `ac_rate_limit_decisions_total` at call sites. Recording function exists in `metrics.rs` (`#[allow(dead_code)]`) but is not called from rate limiting middleware. |
| Database Query Metrics | ❌ Pending | | Wire `ac_db_queries_total` and `ac_db_query_duration_seconds` at call sites. Recording function exists in `metrics.rs` (`#[allow(dead_code)]`) but is not called from repository query methods. |
| Bcrypt Duration Metrics | ❌ Pending | | Wire `ac_bcrypt_duration_seconds` at call sites. Recording function exists in `metrics.rs` (`#[allow(dead_code)]`) but is not called from password hashing code paths. |
| Audit Log Metrics | ❌ Pending | | Wire `ac_audit_log_failures_total` at call sites. Recording function exists in `metrics.rs` (`#[allow(dead_code)]`) but audit logging is not yet implemented. |
| Credential Operations Metrics | ✅ Done | | `ac_credential_operations_total` (renamed from `ac_admin_operations_total`) wired at all admin handler call sites via `record_credential_operation()`. |
| Redis-based Rate Limiting | ❌ Pending | | Multi-instance distributed rate limiting |
| CAPTCHA Integration | ❌ Pending | | After 3 failed attempts (Layer 4) |
| Failed Login Alerting | ❌ Pending | | Email user, alert ops team (Layer 5) |

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
