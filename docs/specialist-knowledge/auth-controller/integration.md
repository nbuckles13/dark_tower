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

## Integration: Service JWT Claims Structure
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Claims: `sub` (client_id), `exp`, `iat`, `scope` (space-separated), `service_type` (optional). Header includes `kid` for key rotation. Algorithm: EdDSA (Ed25519).

---

## Integration: User JWT Claims Structure (ADR-0020)
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/crypto/mod.rs`

User tokens follow ADR-0020 claim structure:
- `sub`: User UUID (not email)
- `org_id`: Organization UUID
- `email`: User email address
- `roles`: Array of role strings (e.g., ["admin", "member"])
- `iat`: Issued-at timestamp
- `exp`: Expiration timestamp (1 hour from issuance)
- `jti`: Unique token ID for revocation tracking

Use `verify_user_jwt()` to validate user tokens (different from `verify_jwt()` for service tokens).

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

---

## Integration: Internal Token Endpoints
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/handlers/internal_tokens.rs`

**Endpoints**:
- `POST /api/v1/auth/internal/meeting-token` - Issue token for authenticated meeting participant
- `POST /api/v1/auth/internal/guest-token` - Issue token for guest (waiting room) participant

**Required scope**: `internal:meeting-token` (GC must have this scope)

**Token characteristics**:
- Max TTL: 900 seconds (15 minutes), client requests capped
- Includes `jti` for revocation tracking
- `token_type` claim distinguishes meeting vs guest tokens

---

## Integration: Subdomain Requirement for User Endpoints
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/middleware/org_extraction.rs`, `crates/ac-service/src/routes/mod.rs`

User-facing endpoints (`/api/v1/auth/register`, `/api/v1/auth/user/token`) require organization subdomain in Host header. Requests to these endpoints without valid subdomain receive 400 Bad Request. Integration tests must set Host header: `Host: acme.example.com`. The subdomain identifies the organization context for user operations.

---

## Integration: TokenManager for Service-to-Service Auth
**Added**: 2026-02-02
**Updated**: 2026-02-10
**Related files**: `crates/common/src/token_manager.rs`

GC and MC use `spawn_token_manager()` to acquire and refresh OAuth tokens from AC. The function blocks until first token is acquired (wrap in timeout for startup limits). Returns `(JoinHandle, TokenReceiver)` - pass `token_rx.clone()` to any task needing tokens. Token is automatically refreshed before expiration with 30-second clock drift margin. Use `TokenManagerConfig::new_secure()` in production to enforce HTTPS. Call `token_rx.token()` to get current token (always returns valid SecretString after spawn completes).

---

## Integration: AC Hash Secret for Correlation Logging
**Added**: 2026-02-10
**Related files**: `crates/ac-service/src/observability/mod.rs`

AC requires `AC_HASH_SECRET` environment variable (base64-encoded, minimum 32 bytes) for HMAC-SHA256 correlation hashing. Defaults to 32 zero bytes for tests - MUST set in production. Used to hash PII fields (client_id) in logs via `hash_for_correlation()`. Do not share this secret across services or store in version control.

---

## Integration: Observability Module Structure
**Added**: 2026-02-10
**Related files**: `crates/ac-service/src/observability/mod.rs`, `crates/ac-service/src/observability/metrics.rs`

AC observability follows ADR-0011 structure: `observability/mod.rs` contains `ErrorCategory` enum and `hash_for_correlation()`, `observability/metrics.rs` contains Prometheus metric recording functions. Handlers import metrics via `use crate::observability::metrics::*`. All instrumentation uses `#[instrument(skip_all)]` with explicit safe field allow-listing.

---
