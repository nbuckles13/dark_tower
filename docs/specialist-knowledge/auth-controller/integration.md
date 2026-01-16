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

## Integration: Meeting Token Validation
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/handlers/internal_tokens.rs`

Meeting Controllers validating these tokens should:
1. Fetch JWKS from AC's `/.well-known/jwks.json`
2. Verify signature using `kid` header
3. Check `token_type` claim: `"meeting"` or `"guest"`
4. Validate `meeting_id` matches expected meeting

Guest tokens have `waiting_room: true` and fixed capabilities `["video", "audio"]`.

---

## Integration: Tech Debt Backlog
**Added**: 2026-01-15
**Related files**: Various

From DRY reviewer findings (non-blocking per ADR-0019):

**TD-1**: JWT signing pattern appears 3+ times. Candidate for generic function accepting `impl Serialize`:
```rust
pub fn sign_jwt_generic<T: Serialize>(claims: &T, key: &SigningKey) -> Result<String, AcError>
```
Files: `crypto/mod.rs`, `handlers/internal_tokens.rs`

**TD-2**: Key loading/decryption block duplicated across handlers. Consider extracting to shared helper or service.
Files: `handlers/auth_handler.rs`, `handlers/internal_tokens.rs`

---

## Integration: User Token Claims Structure (ADR-0020)
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `docs/decisions/adr-0020-user-tokens.md`

User tokens follow ADR-0020 claim structure:
- `sub`: User UUID (not email)
- `org_id`: Organization UUID
- `email`: User email address
- `roles`: Array of role strings (e.g., ["admin", "member"])
- `iat`: Issued-at timestamp
- `exp`: Expiration timestamp (1 hour from issuance)
- `jti`: Unique token ID for revocation tracking

GC and MC should use `verify_user_jwt()` from ac-service crypto module to validate user tokens.

---

## Integration: Subdomain Requirement for User Endpoints
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/middleware/org_extraction.rs`, `crates/ac-service/src/routes/mod.rs`

User-facing endpoints (`/api/v1/auth/register`, `/api/v1/auth/user/token`) require organization subdomain in Host header. Requests to these endpoints without valid subdomain receive 400 Bad Request. Integration tests must set Host header: `Host: acme.example.com`. The subdomain identifies the organization context for user operations.

---

## Integration: verify_user_jwt() for GC/MC Token Validation
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/crypto/mod.rs`

GC and MC services should use `verify_user_jwt()` function to validate user tokens. This function:
1. Fetches JWKS from AC's `/.well-known/jwks.json`
2. Verifies EdDSA signature using `kid` header
3. Validates `exp`, `iat`, and clock skew
4. Returns `UserClaims` struct with all claim fields

Different from `verify_jwt()` which returns service `Claims`. Token type should be checked before calling appropriate verification function.
