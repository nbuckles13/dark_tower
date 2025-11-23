# ADR-0004: API Versioning Strategy

**Status**: Accepted

**Date**: 2025-01-22

**Deciders**: All Specialists

---

## Context

Dark Tower exposes multiple HTTP APIs:
- Global Controller: Meeting management, authentication
- Auth Controller: Token issuance, JWKS
- Future services with HTTP interfaces

**Problems**:
- APIs evolve over time (new features, breaking changes)
- Clients expect stability (mobile apps, web clients can't update instantly)
- Mixed versioning approaches confusing (some paths have `/v1/`, some don't)

**Requirements**:
- Clear API evolution strategy
- Backward compatibility for non-breaking changes
- Explicit versioning for breaking changes
- Consistency across all services

## Decision

**We adopt URL path versioning for all HTTP APIs.**

### Versioning Pattern

**All API endpoints include version in path**: `/api/v{N}/...`

**Examples**:
```
Global Controller:
  POST   /api/v1/meetings
  GET    /api/v1/meetings/{id}
  DELETE /api/v1/meetings/{id}

Auth Controller:
  POST /api/v1/auth/user/token
  POST /api/v1/auth/service/token
  POST /api/v1/admin/services/register
```

### Exception: Well-Known URIs

**Standard paths** (defined by RFCs) don't use versioning:
```
GET /.well-known/jwks.json      (RFC 8414)
GET /.well-known/openid-configuration  (future)
```

**Rationale**: Standards define exact paths, clients expect these locations.

### Version Numbering

**Version format**: `v{integer}` (v1, v2, v3, etc.)

**When to increment**:
- **Major version** (v1 → v2): Breaking changes
  - Remove endpoint
  - Remove required field
  - Change field type/semantics
  - Change authentication method

- **No version change**: Non-breaking changes
  - Add new endpoint
  - Add optional field
  - Add new enum value (with default handling)
  - Performance improvements
  - Bug fixes

### Version Support Policy

**Support N and N-1 simultaneously**:
- v2 released → v1 and v2 both supported
- v3 released → v2 and v3 both supported, v1 deprecated (6-month sunset)

**Deprecation process**:
1. Announce deprecation (release notes, logs, headers)
2. Add deprecation header: `Deprecation: true`
3. Wait 6 months minimum
4. Remove deprecated version

### Internal APIs (Service-to-Service)

**gRPC services use protobuf versioning** (not path versioning):
```protobuf
syntax = "proto3";

package dark_tower.internal.v1;  // Version in package name

service MediaHandlerService {
  rpc RouteMedia(RouteMediaCommand) returns (RouteMediaResponse);
}
```

**Why different from HTTP**:
- Protobuf has built-in versioning (field numbers, package names)
- gRPC services are internal (we control both client and server)
- Can coordinate breaking changes across services

### Error Responses

**Version mismatch errors**:
```json
{
  "error": {
    "code": "UNSUPPORTED_API_VERSION",
    "message": "API version v5 does not exist. Current version: v2. Use /v2/... endpoints.",
    "supported_versions": ["v1", "v2"],
    "details": {
      "requested_version": "v5",
      "latest_version": "v2"
    }
  }
}
```

**Deprecated version warnings**:
```http
HTTP/1.1 200 OK
Deprecation: true
Sunset: Sat, 1 Jul 2025 23:59:59 GMT
Link: </v2/meetings>; rel="successor-version"

{
  "meeting_id": "...",
  "deprecation_notice": "v1 will be sunset on 2025-07-01. Please migrate to /v2/ endpoints."
}
```

## Consequences

### Positive

- ✅ **Clear evolution path**: Version in URL makes it obvious
- ✅ **Stable clients**: Old clients continue working during transitions
- ✅ **Explicit breaking changes**: v1 → v2 signals incompatibility
- ✅ **Gradual migration**: Support multiple versions simultaneously
- ✅ **Predictable**: Industry standard (Stripe, GitHub, AWS use path versioning)

### Negative

- ❌ **Code duplication**: May need to maintain v1 and v2 handlers
- ❌ **Routing complexity**: Router must handle multiple versions
- ❌ **URL length**: `/v1/` adds characters to every request

### Neutral

- Version is part of URL (affects caching, logs)
- Clients must explicitly choose version

## Alternatives Considered

### Alternative 1: Header Versioning

**Approach**: Version in `Accept` or custom header
```http
GET /meetings
Accept: application/vnd.darktower.v1+json
```

**Pros**:
- Cleaner URLs
- Follows REST "best practices"

**Cons**:
- Less visible (developers miss it)
- Harder to test (can't just paste URL in browser)
- Cache-unfriendly (same URL, different responses)

**Why not chosen**: URL versioning more pragmatic, easier to debug

### Alternative 2: Subdomain Versioning

**Approach**: Version in subdomain
```
https://v1.api.dark.com/meetings
https://v2.api.dark.com/meetings
```

**Pros**:
- Clear separation
- Can deploy different versions to different infrastructure

**Cons**:
- DNS/TLS certificate management per version
- CORS complexity (different origins)
- Overkill for minor API changes

**Why not chosen**: Too heavyweight for API versioning

### Alternative 3: No Versioning

**Approach**: Always maintain backward compatibility, never break

**Pros**:
- Simple (one version)
- No migration needed

**Cons**:
- Technical debt accumulates
- Eventually impossible to evolve
- Deprecated fields linger forever

**Why not chosen**: Unrealistic for long-term project

### Alternative 4: Query Parameter Versioning

**Approach**: Version in query string
```
GET /meetings?api_version=v1
```

**Pros**:
- Flexible (easy to add)

**Cons**:
- Inconsistent (optional vs required)
- Ugly URLs
- Not RESTful

**Why not chosen**: Less clean than path versioning

## Implementation Notes

### Routing

**Axum example**:
```rust
async fn configure_routes() -> Router {
    Router::new()
        // v1 routes
        .nest("/api/v1", v1_routes())

        // v2 routes (future)
        .nest("/api/v2", v2_routes())

        // Well-known URIs (no version)
        .route("/.well-known/jwks.json", get(jwks_handler))
}

fn v1_routes() -> Router {
    Router::new()
        .route("/meetings", post(create_meeting_v1))
        .route("/meetings/:id", get(get_meeting_v1))
        .route("/auth/user/token", post(user_token_v1))
}
```

### Shared Logic

**Avoid duplication** by extracting common logic:
```rust
// Shared domain logic
async fn create_meeting_domain(
    request: CreateMeetingRequest
) -> Result<Meeting, MeetingError> {
    // Common business logic
}

// v1 handler (thin wrapper)
async fn create_meeting_v1(
    Json(request): Json<CreateMeetingRequestV1>
) -> Result<Json<MeetingResponseV1>, ApiError> {
    let meeting = create_meeting_domain(request.into()).await?;
    Ok(Json(meeting.into()))
}

// v2 handler (different request/response types, same logic)
async fn create_meeting_v2(
    Json(request): Json<CreateMeetingRequestV2>
) -> Result<Json<MeetingResponseV2>, ApiError> {
    let meeting = create_meeting_domain(request.into()).await?;
    Ok(Json(meeting.into()))
}
```

### Documentation

**OpenAPI/Swagger**: Generate separate specs per version
```
/api/v1/openapi.json
/api/v2/openapi.json
```

**Changelog**: Document version changes
```markdown
# API Changelog

## v2 (2025-06-01)

### Breaking Changes
- Removed deprecated `max_participants` field from meeting response
- Changed `created_at` from Unix timestamp to ISO 8601 string

### Additions
- Added `/api/v2/meetings/{id}/participants` endpoint
- Added `layout_type` field to meeting settings

## v1 (2025-01-01)

Initial release
```

### Migration Strategy

**When introducing v2**:
1. Implement v2 endpoints alongside v1
2. Update clients to use v2 gradually
3. Mark v1 as deprecated (6 months before sunset)
4. Monitor v1 usage (should decrease)
5. Remove v1 after 6 months if usage < 1%

### Client Libraries

**Version in client package**:
```rust
// Rust client
use dark_tower_client_v1::Client;
use dark_tower_client_v2::Client;

// Or single client with version selection
let client = Client::new("https://api.dark.com")
    .version(ApiVersion::V2);
```

## References

- REST API Versioning: https://restfulapi.net/versioning/
- Stripe API Versioning: https://stripe.com/docs/api/versioning
- GitHub API Versioning: https://docs.github.com/en/rest/about-the-rest-api/api-versions
- RFC 8594 (Sunset Header): https://www.rfc-editor.org/rfc/rfc8594.html
- Related: ADR-0003 (Auth Controller uses /v1/ prefix)
