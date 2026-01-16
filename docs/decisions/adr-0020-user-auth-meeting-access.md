# ADR-0020: User Authentication and Meeting Access Flows

**Status**: Accepted

**Date**: 2026-01-14

**Deciders**: Auth Controller specialist, Global Controller specialist, Security specialist, Test specialist, Operations specialist (5-specialist debate, 94% consensus)

---

## Context

Dark Tower needs user authentication for the Meeting API (GC Phase 3). The existing AC only handles service-to-service authentication via OAuth 2.0 Client Credentials. For the Meeting API, we need:

1. **User authentication** with organization membership (`org_id` in tokens)
2. **Guest access** for users without accounts joining meetings
3. **Cross-organization meeting access** for users joining meetings in other organizations
4. **Meeting-scoped authorization** with granular capabilities

### Requirements

- Multi-tenant isolation (users can only access their organization's data)
- Support for guest users without accounts
- External users joining meetings in other organizations
- Host controls (waiting room, kick participants)
- Immediate revocation capability for kicked participants
- Short-lived tokens for meeting access
- JWKS federation for token validation across services

## Decision

Implement a **three-tier token architecture** with subdomain-based organization identification.

### Token Types

| Token | Issuer | Lifetime | Scope | Use Case |
|-------|--------|----------|-------|----------|
| User Token | AC | 1 hour | org_id | User identity, API access |
| Meeting Token | AC (via GC request) | 15 min | meeting_id | Authenticated meeting participant |
| Guest Token | AC (via GC request) | 15 min | meeting_id | Unauthenticated meeting participant |

**Key architectural decision**: All JWTs are signed by AC. GC requests meeting/guest tokens from AC via internal endpoints, keeping key management centralized and avoiding JWKS duplication.

### Organization Identification

**Subdomain-based extraction** from HTTP `Host` header:

```
https://acme.darktower.com/api/v1/auth/user/token
        ^^^^^
        org_slug = "acme"
```

- AC extracts `org_slug` from subdomain
- Looks up `org_id` from `organizations` table
- Includes `org_id` in issued user token
- GC inherits `org_id` from validated user token

### Token Claims

**User Token (AC-issued)**:
```json
{
  "sub": "user_uuid",
  "org_id": "org_uuid",
  "email": "user@example.com",
  "roles": ["member", "admin"],
  "iat": 1705248000,
  "exp": 1705251600,
  "jti": "unique_token_id"
}
```

**Meeting Token (AC-issued via GC request)**:
```json
{
  "sub": "participant_uuid",
  "token_type": "meeting",
  "meeting_id": "meeting_uuid",
  "home_org_id": "users_org_uuid",
  "meeting_org_id": "meetings_org_uuid",
  "participant_type": "member|external",
  "role": "host|participant",
  "capabilities": ["video", "audio", "screen_share"],
  "iat": 1705248000,
  "exp": 1705248900,
  "jti": "unique_token_id"
}
```

**Guest Token (AC-issued via GC request)**:
```json
{
  "sub": "guest_uuid",
  "token_type": "guest",
  "meeting_id": "meeting_uuid",
  "meeting_org_id": "meetings_org_uuid",
  "participant_type": "guest",
  "role": "guest",
  "display_name": "Alice",
  "waiting_room": true,
  "capabilities": ["video", "audio"],
  "iat": 1705248000,
  "exp": 1705248900,
  "jti": "unique_token_id"
}
```

### JWKS Trust Chain

```
AC publishes: /.well-known/jwks.json (for ALL token types)
       |
       +---> GC validates user tokens against AC JWKS
       |
       +---> MC validates meeting/guest tokens against AC JWKS
```

**Single JWKS source**: AC is the sole JWT authority. All services validate all tokens against AC's JWKS. This:
- Eliminates key management duplication
- Simplifies cross-cluster validation (AC JWKS already federated)
- Maintains consistent security posture

AC key management:
- EdDSA (Ed25519) signing keys
- Encrypted at rest (AES-256-GCM)
- Loaded into memory at startup
- Rotation via ADR-0008/ADR-0009 mechanism

### Join Flows

#### 1. Authenticated User (Same Org)

```
1. User authenticates via subdomain login
2. AC issues user token with org_id
3. User calls GET /v1/meetings/{code} (with user token)
4. GC validates user token against AC JWKS
5. GC checks org_id matches meeting.org_id
6. GC calls AC: POST /api/v1/auth/internal/meeting-token
7. AC issues meeting token (signed by AC)
8. GC returns meeting token to user
9. User connects to MC with meeting token
10. MC validates against AC JWKS
```

#### 2. External User (Cross-Org)

```
1. External user (org B) clicks invite link to meeting in org A
2. User authenticates to their org (B) via subdomain
3. User gets user token with org_id = B
4. User calls GET /v1/meetings/{code} (with user token)
5. GC validates user token against AC JWKS
6. GC checks meeting.allow_external_participants
7. If allowed, GC calls AC: POST /api/v1/auth/internal/meeting-token
   with home_org_id: B, meeting_org_id: A, participant_type: "external"
8. AC issues meeting token (signed by AC)
9. GC returns meeting token to user
10. User connects to MC with meeting token
```

#### 3. Guest User (No Account)

```
1. Guest visits /m/{code} -> frontend shows join form
2. Guest enters display name, completes captcha
3. Frontend calls POST /v1/meetings/{code}/guest-token
4. GC validates captcha, checks meeting.allow_guests
5. GC generates guest_id (CSPRNG)
6. GC calls AC: POST /api/v1/auth/internal/guest-token
7. AC issues guest token with waiting_room: true (signed by AC)
8. GC returns guest token to guest
9. Guest connects to MC with token
10. MC validates against AC JWKS, holds guest in waiting room
11. On host approval, guest can participate
```

### Guest Token Endpoint

```
POST /v1/meetings/{meeting_code}/guest-token

Request:
{
  "display_name": "Alice",
  "captcha_token": "hcaptcha_response"
}

Response (200):
{
  "token": "eyJ...",
  "expires_in": 900
}

Errors:
  400: Invalid captcha or display name
  403: Meeting doesn't allow guests
  404: Meeting not found
  429: Rate limited (5 req/min per IP)
```

### AC Internal Token Endpoints

These endpoints are called by GC (authenticated with its service token) to request meeting/guest tokens from AC.

**Meeting Token Request**:
```
POST /api/v1/auth/internal/meeting-token
Authorization: Bearer <GC's service token>

Request:
{
  "subject_user_id": "uuid",
  "meeting_id": "uuid",
  "meeting_org_id": "uuid",
  "home_org_id": "uuid",           // null if same-org
  "participant_type": "member|external",
  "role": "host|participant",
  "capabilities": ["video", "audio", "screen_share"],
  "ttl_seconds": 900               // optional, default 900
}

Response (200):
{
  "token": "eyJ...",
  "expires_in": 900
}
```

**Guest Token Request**:
```
POST /api/v1/auth/internal/guest-token
Authorization: Bearer <GC's service token>

Request:
{
  "guest_id": "uuid",              // GC generates with CSPRNG
  "display_name": "Alice",
  "meeting_id": "uuid",
  "meeting_org_id": "uuid",
  "waiting_room": true,
  "ttl_seconds": 900               // optional, default 900
}

Response (200):
{
  "token": "eyJ...",
  "expires_in": 900
}
```

**Authorization**: Only services with `internal:meeting-token` scope can call these endpoints.

### Token Revocation

For kicked participants, a three-layer approach:

1. **Short TTL (15 min)**: Natural expiration handles most cases
2. **In-memory revoked set**: MC maintains `revoked_tokens: HashSet<Jti>`
   - When host kicks participant, add their token's `jti`
   - Check on every WebTransport message
   - Auto-cleanup after 15 min (token max lifetime)
3. **Cluster-local sync**: Redis pub/sub broadcasts revocation to all MC instances **within the same cluster**

**Cross-cluster revocation**: If GC routes to MC in a different cluster, immediate revocation is not supported. Token expires naturally within 15 minutes. Future work may add cross-cluster pub/sub (Kafka/NATS) or AC-level revocation list if needed.

### Meeting Access Control

Meeting settings control access:

| Setting | Default | Effect |
|---------|---------|--------|
| `allow_guests` | false | Enables guest token issuance |
| `allow_external_participants` | false | Enables cross-org join |
| `waiting_room_enabled` | true | Holds guests until approved |
| `require_authentication` | true | If false, treat all as guests |

**Mutability**: These settings can be:
- Set when creating a meeting (`POST /v1/meetings`)
- Changed during a meeting by the host (`PATCH /v1/meetings/{id}/settings`)

Setting changes apply to **new join attempts only**, not to participants already in the meeting.

### Security Controls

| Control | Implementation |
|---------|----------------|
| CSPRNG | `ring::rand::SystemRandom` for all security-critical randomness |
| Meeting codes | 72 bits entropy (12 base62 chars) |
| Guest IDs | UUID v4 from CSPRNG |
| Rate limiting | 5 req/min per IP on guest-token endpoint |
| Captcha | Required for guest tokens (hCaptcha/reCAPTCHA) |
| Waiting room | Server-side state in MC (not in token claims) |

## Consequences

### Positive

1. **Clean separation of concerns**: AC owns identity and all token issuance, GC owns meeting authorization logic, MC owns real-time state
2. **Multi-tenant isolation**: `org_id` in all user tokens, enforced in all queries
3. **Guest flexibility**: Support anonymous users while maintaining security
4. **Cross-org support**: External users can join meetings with proper isolation
5. **Revocation capability**: Immediate effect for kicked participants (cluster-local)
6. **Unified JWKS**: Single source (AC) for all token validation - no key management duplication
7. **Cross-cluster support**: MC in any cluster can validate tokens against federated AC JWKS

### Negative

1. **Three token types**: More complexity than single token approach
2. **AC dependency for joins**: GC must call AC for every meeting/guest token (mitigated by 15-min token lifetime)
3. **Short token lifetimes**: More frequent token refresh needed
4. **Redis dependency**: Cluster-local revocation sync requires Redis pub/sub

### Neutral

1. **Subdomain-based orgs**: Industry-standard approach (Slack, Notion, etc.)
2. **Waiting room state in MC**: Centralized but requires MC availability

## Alternatives Considered

### Alternative A: GC Issues Own Tokens (Original Design)
GC maintains its own EdDSA signing key and JWKS endpoint for meeting/guest tokens.
- Pros: GC is self-sufficient for token issuance
- Cons: Duplicates AC's key management complexity, requires GC JWKS federation for cross-cluster, operational overhead
- **Decision**: Rejected in favor of AC issuing all tokens

### Alternative B: Single Token Type
Issue one token type from AC with all claims (meeting, org, role).
- Pros: Simpler architecture, single issuer
- Cons: AC needs meeting knowledge, tokens too long-lived for meetings, can't revoke granularly

### Alternative C: Header-Based Org Identification
Pass `X-Org-Id` header instead of subdomain.
- Pros: Works with single domain, more explicit
- Cons: Client must manage header, less intuitive UX, doesn't match industry patterns

### Alternative D: Stateful Sessions Instead of JWTs
Use opaque session IDs that GC stores in database, MC validates by calling GC or shared Redis.
- Pros: Instant revocation, no JWKS needed
- Cons: MC depends on GC/Redis availability for validation, higher latency, cross-cluster requires shared state

### Alternative E: No Guest Support
Require all users to have accounts.
- Pros: Simpler auth model, no anonymous risk
- Cons: Poor UX for ad-hoc meetings, doesn't match competitor features

## Implementation Notes

### Phase 1: AC User Auth + Internal Token Endpoints
1. Add `/api/v1/auth/user/token` endpoint (user login with subdomain org)
2. Implement subdomain extraction from Host header
3. Add `org_id` to user token claims
4. Add `/api/v1/auth/internal/meeting-token` endpoint (GC calls this)
5. Add `/api/v1/auth/internal/guest-token` endpoint (GC calls this)
6. Add `internal:meeting-token` scope for GC service credentials

### Phase 2: GC Meeting API
1. Implement `GET /v1/meetings/{code}` to validate user and request meeting token from AC
2. Implement `POST /v1/meetings/{code}/guest-token` to validate captcha and request guest token from AC
3. Implement `PATCH /v1/meetings/{id}/settings` for host to change meeting settings
4. GC validates user tokens against AC JWKS (no GC signing key needed)

### Phase 3: MC Token Validation
1. MC validates meeting/guest tokens against AC JWKS (same as all other tokens)
2. Implement revoked token set with cluster-local Redis pub/sub
3. Add waiting room state management

### Files Affected

**AC Changes**:
- `crates/ac-service/src/handlers/user_auth.rs` (new)
- `crates/ac-service/src/handlers/internal_tokens.rs` (new)
- `crates/ac-service/src/services/token_service.rs` (add meeting/guest token generation)
- `crates/ac-service/src/routes.rs` (add user and internal routes)

**GC Changes**:
- `crates/global-controller/src/handlers/meetings.rs`
- `crates/global-controller/src/handlers/guest_token.rs` (new)
- `crates/global-controller/src/services/ac_client.rs` (new - client for AC internal endpoints)

**MC Changes** (future):
- Token validation against AC JWKS
- Revocation set management with Redis

### Runbooks

See `docs/runbooks/` for:
- `auth-cross-org-join.md` - Cross-org join failures
- `auth-meeting-token-issuance.md` - Meeting token issuance failures (AC internal endpoint)
- `gc-guest-token-abuse.md` - Guest token abuse

### Severity Classifications

| Alert | Severity | Response |
|-------|----------|----------|
| AC signing key failure | P1 | 5 min |
| AC internal endpoint unavailable | P1 | 5 min |
| Token validation >10% failures | P1 | 15 min |
| Cross-org join >5% failures | P2 | 30 min |
| Guest captcha >20% failures | P2 | 30 min |
| GC→AC latency p99 >500ms | P3 | 4 hours |
| Token refresh p99 >1s | P3 | 4 hours |

## References

- ADR-0003: Service Authentication (OAuth 2.0 Client Credentials)
- ADR-0007: Token Lifetime Strategy
- ADR-0008: Key Rotation Strategy
- ADR-0002: No-Panic Policy (all code uses `Result<T, E>`)
- ADR-0010: Global Controller Architecture
- Debate record: 2026-01-14 User Auth Design (5 specialists, 3 rounds, 94% consensus)

---

## Revision History

| Date | Change | Reason |
|------|--------|--------|
| 2026-01-14 | Initial version | 5-specialist debate consensus |
| 2026-01-15 | **Revised**: AC issues all tokens | User feedback: GC JWKS duplicates operational complexity, conflicts with AC's role. Changed to AC internal endpoints for meeting/guest tokens. |
