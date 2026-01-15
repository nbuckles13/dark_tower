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

## Integration: Meeting CRUD (Phase 2)
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/models/mod.rs`

Phase 2 will add: `POST /v1/meetings`, `GET /v1/meetings/{id}`, `PUT /v1/meetings/{id}`, `DELETE /v1/meetings/{id}`. Requires valid JWT with appropriate scopes. Meeting state transitions managed by GC.

---
