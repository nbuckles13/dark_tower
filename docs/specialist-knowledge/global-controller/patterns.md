# Global Controller Patterns

Reusable patterns discovered and established in the Global Controller codebase.

---

## Pattern: Handler -> Service -> Repository Foundation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/routes/mod.rs`, `crates/global-controller/src/handlers/health.rs`

GC follows the same layered architecture as AC. Handlers receive Axum extractors (State, Path, Json), call service functions (Phase 2), which call repository functions. AppState holds pool and config.

---

## Pattern: AppState with Pool and Config
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/routes/mod.rs`

AppState struct holds `PgPool` and `Config`. Passed to handlers via `State<Arc<AppState>>`. All handlers access via `state.pool` and `state.config`. Matches ac-service pattern exactly.

---

## Pattern: Error Variants Map to HTTP Status Codes
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/errors.rs`

GcError enum has 9 variants each mapping to appropriate HTTP status: InvalidInput(400), Unauthorized(401), Forbidden(403), NotFound(404), Conflict(409), DatabaseError(500), InternalError(500), ServiceUnavailable(503), RateLimitExceeded(429).

---

## Pattern: IntoResponse for Error Types
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/errors.rs`

GcError implements Axum's `IntoResponse` trait. Converts to JSON response with `error` field. Internal details logged server-side, generic messages to clients. Matches ac-service error handling.

---

## Pattern: Environment-Based Config with Validation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

Config loads from env vars with `from_env()`. Validates bounds (JWT clock skew 1-600s, rate limit 10-10000 RPM). Required: DATABASE_URL, AC_JWKS_URL. Optional: BIND_ADDRESS, GC_REGION with sensible defaults.

---

## Pattern: TestGcServer Harness
**Added**: 2026-01-14
**Related files**: `crates/gc-test-utils/src/server_harness.rs`

TestGcServer::spawn(pool) starts real Axum server on random port. Returns base_url for reqwest calls. Server drops when handle dropped. Matches TestAcServer pattern for E2E testing.

---

## Pattern: Graceful Shutdown with Drain Period
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/main.rs`

Server uses tokio::signal for SIGTERM/SIGINT. Configurable drain period (GC_DRAIN_SECONDS, default 5) allows in-flight requests to complete. Production-ready shutdown pattern.

---

## Pattern: Dead Code Annotations for Foundation Components
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/errors.rs`, `crates/global-controller/src/models/mod.rs`

Foundation components not yet used (MeetingStatus, some error variants) annotated with `#[allow(dead_code)]` and comment explaining they're for Phase 2+. Prevents clippy warnings while maintaining code.

---

## Pattern: Token Size Check Before Parsing (DoS Prevention)
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs:72-84`

ALWAYS check token size in bytes BEFORE any parsing or cryptographic operations. Set MAX_JWT_SIZE_BYTES constant (8KB default), check `token.len() > MAX_JWT_SIZE_BYTES` at function entry. Prevents DoS via oversized tokens consuming CPU/memory. Return generic error message to avoid info leakage.

---

## Pattern: JWK Validation Before Signature Verification
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs:145-160`

Validate JWK structure BEFORE using it for signature verification:
- Check `jwk.kty == "OKP"` (reject if not, log warning)
- Check `jwk.alg == "EdDSA"` if present (reject if different, log warning)
- This prevents algorithm confusion attacks where attacker manipulates JWK to use weak algorithms

---

## Pattern: Algorithm Pinning in jsonwebtoken Validation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs:177-179`

Use `Validation::new(Algorithm::EdDSA)` to explicitly set the expected algorithm BEFORE calling `decode()`. Never use `Validation::default()` which accepts multiple algorithms. Pinning prevents algorithm confusion attacks from alg:none or alg:HS256 tokens.

---

## Pattern: JWKS Caching with TTL and Refresh
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwks.rs`

Implement JWKS caching with:
- In-memory cache (HashMap<kid, Jwk>) wrapped in Arc<RwLock<Option<CachedJwks>>>
- Expiry time (Instant::now() + cache_ttl) stored with cached data
- Cache miss or expired → trigger async refresh_cache()
- Read lock for cache hits, write lock only for updates
- Default 5-minute TTL balances key rotation latency vs AC load

---

## Pattern: HTTP Client Error Handling in Initialization
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwks.rs:101-107`

When building HTTP client during initialization (not request-time), use `unwrap_or_else()` to fall back to `reqwest::Client::new()` but log warning. Never silently ignore HTTP client build failures - surface them via tracing for observability. This catches configuration errors early but doesn't panic.

---

## Pattern: Bearer Token Extraction with Prefix Validation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/middleware/auth.rs:44-58`

Extract Bearer token with strict validation:
- Get Authorization header → to_str() → unwrap
- Use `strip_prefix("Bearer ")` to extract token
- Both steps can fail independently - handle each
- Return generic error message in both cases
- This prevents header injection and format confusion attacks

---

## Pattern: JWT Claims Validation with Clock Skew
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs:98-114`

Validate iat claim AFTER signature verification:
- Use `chrono::Utc::now().timestamp()` for current time
- Calculate `max_iat = now + clock_skew_seconds`
- Reject if `claims.iat > max_iat` (future tokens)
- Log current time, token iat, and skew tolerance for debugging
- Clock skew tolerance should match AC's value (default 300s)

---

## Pattern: kid Extraction Without Full Token Parsing
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs:121-140`

Extract kid for key lookup by:
- Split token on '.' → exactly 3 parts
- Decode header (first part) from base64url
- Parse as JSON (handle parse failure gracefully)
- Extract kid as string from header object
- Return Option to allow error propagation upstream
This avoids full JWT parsing before signature validation - kid selection is data-only.

---

## Pattern: AC Client Service for Internal Endpoints
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/services/ac_client.rs`

HTTP client for calling AC internal token endpoints. Uses Bearer auth with GC_SERVICE_TOKEN, configurable timeout (default 10s), and proper error mapping (network errors → ServiceUnavailable, 4xx → Unauthorized/Forbidden). Client is reusable via Arc in AppState.

---

## Pattern: Runtime sqlx Queries for CI/CD Flexibility
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/services/meeting_service.rs`

Use runtime-checked queries (`sqlx::query_as::<_, T>()`) instead of compile-time macros (`query_as!()`) when DATABASE_URL may not be available during CI builds. Trade compile-time safety for deployment flexibility. Document decision in code comments.

---

## Pattern: CSPRNG Guest ID Generation
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/handlers/meetings.rs`

Generate guest IDs using `ring::rand::SystemRandom` for CSPRNG security. Fill 16-byte buffer, then apply UUID v4 bit manipulation (version nibble = 4, variant bits = 10xx). Format as hyphenated UUID string. Never use thread_rng() for security-critical IDs.

---

## Pattern: Host-Only Authorization Check
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/handlers/meetings.rs`

For host-only endpoints (settings, kick participant), compare `meeting.created_by_user_id` against `claims.sub`. Return 403 Forbidden if mismatch. This check happens AFTER meeting lookup to avoid leaking meeting existence via 403 vs 404.

---

## Pattern: COALESCE for Partial Updates (PATCH)
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/services/meeting_service.rs`

Use SQL `COALESCE($N, column_name)` pattern for PATCH endpoints. Client sends only fields to update, NULL/None means "keep existing". Example: `SET allow_guests = COALESCE($2, allow_guests)`. Avoids multiple queries or complex conditionals.

---
