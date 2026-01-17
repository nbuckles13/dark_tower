# Global Controller Integration Guide

What other services need to know when integrating with the Global Controller.

---

## Integration: JWT Validation Flow
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/`, `crates/global-controller/src/middleware/auth.rs`

End-to-end JWT validation in GC:
1. Client sends: `Authorization: Bearer <token>`
2. Middleware extracts token, calls `JwtValidator::validate(token)`
3. JwtValidator:
   - Checks token size (< 8KB)
   - Extracts kid from header
   - Fetches JWK from cached JWKS (5 min TTL)
   - Validates JWK (kty=OKP, alg=EdDSA)
   - Verifies EdDSA signature using jsonwebtoken
   - Validates iat claim (with clock skew tolerance)
4. On success: Claims injected into request.extensions
5. Handler calls `req.extensions().get::<Claims>()`

---

## Integration: Protected Routes Pattern
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/routes/mod.rs`

Protected routes use `middleware::from_fn_with_state`:
```rust
.route(
    "/v1/me",
    get(handlers::me::get_me)
        .layer(middleware::from_fn_with_state(
            Arc::new(auth_state),
            require_auth,
        )),
)
```

The middleware chain:
- Layer wraps handler
- Middleware runs before handler (extracts/validates token)
- Handler receives Request with claims in extensions
- If middleware returns Err (401), handler never runs

---

## Integration: Claims Structure
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/claims.rs`

JWT claims struct from AC tokens:
```
sub: String       # Subject (user ID)
exp: i64          # Expiration time (Unix timestamp)
iat: i64          # Issued at (Unix timestamp)
scopes: Vec<String> # Authorization scopes
```

Handlers extract via request extensions. Claims implement Debug with redacted `sub` field to prevent leaking user IDs in logs.

---

## Integration: Testing with Mocked JWKS
**Added**: 2026-01-14
**Related files**: `crates/global-controller/tests/auth_tests.rs`

Integration tests use wiremock to mock AC's JWKS endpoint:
- Start mock server with `wiremock::MockServer::new()`
- Register JWKS endpoint response
- Pass mock URL to GC config
- GC fetches and caches JWKS from mock
- Tests verify auth behavior without depending on real AC

---

## Integration: AC Internal Token Endpoints
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/services/ac_client.rs`

GC calls AC internal endpoints for meeting tokens:
- `POST /api/v1/auth/internal/meeting-token` - Issue token for authenticated user joining meeting
- `POST /api/v1/auth/internal/guest-token` - Issue token for guest participant

Both require `Authorization: Bearer <GC_SERVICE_TOKEN>`. Request body includes meeting_code, user_id/guest_id, participant_type, role. Response contains signed JWT for WebTransport connection to MC.

---

## Integration: Meeting API Endpoints
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/routes/mod.rs`, `crates/global-controller/src/handlers/meetings.rs`

Meeting API endpoints:
- `GET /v1/meetings/{code}` - Join meeting (authenticated, returns meeting token)
- `POST /v1/meetings/{code}/guest-token` - Get guest token (public, requires captcha)
- `PATCH /v1/meetings/{id}/settings` - Update meeting settings (host only)

Join endpoint returns AC-issued meeting token for WebTransport connection. Guest endpoint allows unauthenticated access with captcha verification (placeholder). Settings endpoint allows host to toggle allow_guests, allow_external_participants, waiting_room_enabled.

---
