# Auth Controller Specialist Checkpoint

**Date**: 2026-01-15
**Task**: Implement internal meeting and guest token endpoints

## Loop State

- **Phase**: Implementation Complete
- **Status**: Verification Passed
- **Blocking Issues**: None

## Patterns Discovered

### 1. JWT Claims Extension for Multiple Token Types

Instead of trying to fit different token types into the existing `crypto::Claims` structure, created dedicated claim structs for each token type:

```rust
struct MeetingTokenClaims {
    sub: String,
    token_type: String,  // Discriminator
    meeting_id: String,
    // ... meeting-specific fields
}

struct GuestTokenClaims {
    sub: String,
    token_type: String,
    display_name: String,
    waiting_room: bool,
    // ... guest-specific fields
}
```

**Why**: This keeps the token claims type-safe and prevents mixing incompatible fields.

### 2. Scope Validation at Handler Level

Rather than creating middleware for specific scopes, validate scopes in the handler itself:

```rust
let token_scopes: Vec<&str> = claims.scope.split_whitespace().collect();
if !token_scopes.contains(&REQUIRED_SCOPE) {
    return Err(AcError::InsufficientScope { ... });
}
```

**Why**: Different endpoints may require different scopes. Handler-level validation is more flexible and explicit.

### 3. Middleware for Claims Injection

Created `require_service_auth` middleware that validates the token but doesn't check specific scopes:

```rust
pub async fn require_service_auth(
    State(state): State<Arc<AuthMiddlewareState>>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, AcError> {
    // Validate token, inject claims, continue
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}
```

**Why**: Separates authentication (who are you?) from authorization (what can you do?).

### 4. TTL Capping

Always cap TTL at endpoint level, regardless of what the client requests:

```rust
const MAX_TOKEN_TTL_SECONDS: u32 = 900;
let ttl = payload.ttl_seconds.min(MAX_TOKEN_TTL_SECONDS);
```

**Why**: Defense in depth - even if validation is bypassed, tokens won't be too long-lived.

## Gotchas Encountered

### 1. JTI Generation

Meeting and guest tokens need unique `jti` (JWT ID) claims for tracking and revocation:

```rust
jti: uuid::Uuid::new_v4().to_string(),
```

**Lesson**: Always include jti for tokens that may need revocation.

### 2. Claims Extension Type

When using `Extension<crypto::Claims>` in handlers, the middleware must insert the exact same type:

```rust
// In middleware:
req.extensions_mut().insert(claims);  // crypto::Claims

// In handler:
Extension(claims): Extension<crypto::Claims>
```

**Lesson**: Type must match exactly - no trait objects or generics.

### 3. Signing Function Reuse

Tried to reuse `crypto::sign_jwt()` but it expects `crypto::Claims`. Created local signing functions for each token type instead.

**Lesson**: Generic signing function would need to accept `impl Serialize` to handle different claim types.

## Key Decisions

### 1. Separate Handler File

Created `internal_tokens.rs` instead of adding to `auth_handler.rs`.

**Rationale**:
- Internal endpoints have different authentication requirements
- Cleaner separation of concerns
- Easier to find internal-only endpoints

### 2. Fixed Guest Capabilities

Guest tokens always get `["video", "audio"]` capabilities, not configurable:

```rust
capabilities: vec!["video".to_string(), "audio".to_string()],
```

**Rationale**: Guests shouldn't have screen share by default - reduces attack surface.

### 3. Required Scope Name

Used `internal:meeting-token` as the required scope:

**Rationale**:
- `internal:` prefix makes it clear this is for service-to-service calls
- Single scope for both meeting and guest tokens simplifies management
- GC is the only service that needs to call these endpoints

## Integration Notes

### Required Scope Configuration

GC needs `internal:meeting-token` in its default scopes. Update in `models/mod.rs`:

```rust
ServiceType::GlobalController => vec![
    "meeting:create".to_string(),
    "meeting:list".to_string(),
    "meeting:read".to_string(),
    "service:register".to_string(),
    "internal:meeting-token".to_string(),  // Add this
],
```

### Token Validation

Meeting Controllers will need to validate these tokens. They should:
1. Fetch JWKS from AC's `/.well-known/jwks.json`
2. Verify signature using kid header
3. Check `token_type` claim to distinguish meeting vs guest tokens

## Files Changed

- `crates/ac-service/src/handlers/internal_tokens.rs` (new)
- `crates/ac-service/src/handlers/mod.rs`
- `crates/ac-service/src/models/mod.rs`
- `crates/ac-service/src/middleware/auth.rs`
- `crates/ac-service/src/routes/mod.rs`

## Reflection Summary

**Date Completed**: 2026-01-15

### Code Review Results

| Reviewer | Verdict | Notes |
|----------|---------|-------|
| Security | APPROVED | Strong security practices (TTL capping, scope validation) |
| Test | APPROVED | After adding scope/TTL boundary tests |
| Code Quality | APPROVED | Full ADR-0002 compliance (no panics) |
| DRY | TECH_DEBT | JWT signing duplication (non-blocking per ADR-0019) |

### Knowledge Captured

**Patterns added to `patterns.md`**:
1. JWT Claims Extension for Multiple Token Types - Separate structs per token type
2. Scope Validation at Handler Level - Authorization in handler, authentication in middleware
3. Middleware for Claims Injection - Decoupled auth pattern
4. TTL Capping (Defense in Depth) - Always cap regardless of client request

**Gotchas added to `gotchas.md`**:
1. JTI Required for Revocable Tokens - Always include for tokens needing revocation
2. Claims Extension Type Must Match Exactly - No generics/trait objects in Extension<T>
3. Signing Function Not Reusable Across Claim Types - Needs generic refactor

**Integration notes added to `integration.md`**:
1. Internal Token Endpoints - New endpoints and required scopes
2. Meeting Token Validation - How MC should validate these tokens
3. Tech Debt Backlog - TD-1 (generic signing) and TD-2 (key loading duplication)

### Lessons for Future Tasks

1. **Plan for token type proliferation**: When adding new token types, create dedicated claim structs upfront rather than retrofitting existing Claims
2. **Middleware vs handler authorization**: Use middleware for authentication (who are you?), handlers for authorization (what can you do?)
3. **Defense in depth**: Always add server-side caps on client-controlled values (TTL, size limits) even if client should validate
4. **Tech debt tracking**: Document DRY violations in integration.md for future consolidation sprints
