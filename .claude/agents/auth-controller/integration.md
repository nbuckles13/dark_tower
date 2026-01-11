# Auth Controller Integration Guide

What other services need to know when integrating with the Auth Controller.

---

## Integration: Environment Variables
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

**Required**: `DATABASE_URL`, `AC_MASTER_KEY` (32-byte base64)

**Optional**: `BIND_ADDRESS` (default: 0.0.0.0:8082), `JWT_CLOCK_SKEW_SECONDS` (default: 300, range: 1-600), `BCRYPT_COST` (default: 12, range: 10-14), `AC_HASH_SECRET` (set in production!), `OTLP_ENDPOINT`

---

## Integration: Token Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Services validating AC tokens must use same clock skew tolerance (default 300s). Tokens with `iat` beyond skew are rejected. Token expiry is 1 hour (not configurable). JWKS at `/.well-known/jwks.json`.

---

## Integration: Performance Expectations
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Bcrypt cost affects `/oauth/token` latency: cost 10 ~50ms, cost 12 ~200ms (default), cost 14 ~800ms. Load balancer timeouts should accommodate. Rate limiting: 5 failures in 15 min triggers lockout (HTTP 429).

---

## Integration: JWT Claims Structure
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Claims: `sub` (client_id), `exp`, `iat`, `scope` (space-separated), `service_type` (optional). Header includes `kid` for key rotation. Algorithm: EdDSA (Ed25519).

---

## Integration: Error Handling
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/errors.rs`

Auth errors are generic to prevent info leakage. Invalid client_id and invalid secret return identical errors. Do not parse error messages for failure reasons.

---

## Integration: Service Registration
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/registration_service.rs`

Valid types: `global-controller`, `meeting-controller`, `media-handler`. `client_secret` returned ONLY at creation - store immediately. Secret rotation invalidates old secret.

---

## Integration: Key Rotation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

Endpoint: `POST /internal/rotate-keys`. Scopes: `service.rotate-keys.ac` (6-day min) or `admin.force-rotate-keys.ac` (1-hour min). Old key valid 24 hours after rotation.
