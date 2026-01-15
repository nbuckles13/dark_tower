# Global Controller Integration Guide

What other services need to know when integrating with the Global Controller.

---

## Integration: Environment Variables
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

**Required**: `DATABASE_URL`, `AC_JWKS_URL`

**Optional**: `BIND_ADDRESS` (default: 0.0.0.0:8080), `GC_REGION` (default: "unknown"), `JWT_CLOCK_SKEW_SECONDS` (default: 300, range: 1-600), `RATE_LIMIT_RPM` (default: 60, range: 10-10000), `GC_DRAIN_SECONDS` (default: 5)

---

## Integration: Health Check Endpoint
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/handlers/health.rs`

Endpoint: `GET /v1/health`

Response: `{"status": "ok", "region": "<GC_REGION>"}`

Returns 503 if database unreachable. Use for readiness probe. For liveness, consider `/v1/health?skip_db=true` (Phase 2).

---

## Integration: JWT Validation via AC
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

GC validates JWTs by fetching JWKS from AC. Set `AC_JWKS_URL` to AC's `/.well-known/jwks.json` endpoint. JWKS is cached (Phase 2 will add refresh logic). Token clock skew tolerance configurable.

---

## Integration: API Versioning
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/routes/mod.rs`

All endpoints prefixed with `/v1/`. Future versions will use `/v2/` etc. Version is path-based, not header-based. Matches ADR-0010 API design.

---

## Integration: Error Response Format
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/errors.rs`

Errors return JSON: `{"error": "<message>"}` with appropriate HTTP status. Internal errors (500) return generic "Internal server error" - details logged server-side only.

---

## Integration: Rate Limiting
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

Rate limiting configured via RATE_LIMIT_RPM. Exceeding limit returns HTTP 429 with `Retry-After` header (Phase 2). Token bucket algorithm with per-client tracking.

---

## Integration: Database Connection Pool
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/main.rs`

GC uses sqlx PgPool. Pool settings from DATABASE_URL. Recommended: `?max_connections=20` for production. Health check uses pool connection to verify DB reachability.

---

## Integration: Meeting CRUD (Phase 3)
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/models/mod.rs`

Phase 3 will add: `POST /v1/meetings`, `GET /v1/meetings/{id}`, `PUT /v1/meetings/{id}`, `DELETE /v1/meetings/{id}`. Requires valid JWT with appropriate scopes. Meeting state transitions managed by GC.

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
- Register JWKS endpoint response: `POST /expected_path(path_regex("/\.well-known/jwks\.json"))`
- Pass mock URL to GC config
- GC fetches and caches JWKS from mock
- Tests verify auth behavior without depending on real AC

---

## Integration: Bearer Token Format Requirements
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/middleware/auth.rs:44-58`

Authorization header requirements:
- MUST be present (returns 401 if missing)
- MUST start with "Bearer " (6 characters including space)
- Token follows after "Bearer "
- No other formats accepted (Basic, Digest, etc.)
- Header value MUST be valid UTF-8 (HTTP spec)

Example valid headers:
```
Authorization: Bearer eyJhbGciOiJFZERTQSI...
Authorization: Bearer short_token
```

Invalid headers (return 401):
```
Authorization: bearer eyJ... (lowercase b - case sensitive)
Authorization: eyJ... (missing Bearer prefix)
Authorization: Token eyJ... (wrong scheme)
```

---
