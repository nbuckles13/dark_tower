# Global Controller Patterns

Reusable patterns discovered and established in the Global Controller codebase.

---

## Pattern: Token Size Check Before Parsing (DoS Prevention)
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

ALWAYS check token size in bytes BEFORE any parsing or cryptographic operations. Set MAX_JWT_SIZE_BYTES constant (8KB default), check `token.len() > MAX_JWT_SIZE_BYTES` at function entry. Prevents DoS via oversized tokens consuming CPU/memory. Return generic error message to avoid info leakage.

---

## Pattern: JWK Validation Before Signature Verification
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

Validate JWK structure BEFORE using it for signature verification:
- Check `jwk.kty == "OKP"` (reject if not, log warning)
- Check `jwk.alg == "EdDSA"` if present (reject if different, log warning)
- This prevents algorithm confusion attacks where attacker manipulates JWK to use weak algorithms

---

## Pattern: Algorithm Pinning in jsonwebtoken Validation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

Use `Validation::new(Algorithm::EdDSA)` to explicitly set the expected algorithm BEFORE calling `decode()`. Never use `Validation::default()` which accepts multiple algorithms. Pinning prevents algorithm confusion attacks from alg:none or alg:HS256 tokens.

---

## Pattern: JWKS Caching with TTL and Refresh
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwks.rs`

Implement JWKS caching with:
- In-memory cache (HashMap<kid, Jwk>) wrapped in Arc<RwLock<Option<CachedJwks>>>
- Expiry time (Instant::now() + cache_ttl) stored with cached data
- Cache miss or expired triggers async refresh_cache()
- Read lock for cache hits, write lock only for updates
- Default 5-minute TTL balances key rotation latency vs AC load

---

## Pattern: kid Extraction Without Full Token Parsing
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

Extract kid for key lookup by:
- Split token on '.' to get exactly 3 parts
- Decode header (first part) from base64url
- Parse as JSON (handle parse failure gracefully)
- Extract kid as string from header object
- Return Option to allow error propagation upstream
This avoids full JWT parsing before signature validation - kid selection is data-only.

---

## Pattern: AC Client Service for Internal Endpoints
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/services/ac_client.rs`

HTTP client for calling AC internal token endpoints. Uses Bearer auth with GC_SERVICE_TOKEN, configurable timeout (default 10s), and proper error mapping (network errors -> ServiceUnavailable, 4xx -> Unauthorized/Forbidden). Client is reusable via Arc in AppState.

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

## Pattern: Testing JWKS Cache with Short TTL
**Added**: 2026-01-18
**Related files**: `crates/global-controller/src/auth/jwks.rs`

To test cache expiration behavior, create JwksClient with very short TTL (1ms) and use `tokio::time::sleep()` to trigger expiration. Use wiremock's `expect(N)` to verify cache hits vs fetches. This avoids flaky time-dependent tests while still exercising cache expiration paths.

---

## Pattern: HTTP Status Code Branch Coverage
**Added**: 2026-01-18
**Related files**: `crates/global-controller/src/services/ac_client.rs`

When testing HTTP client response handling, test ALL status code branches: success (200), client errors (400, 401, 403, 404), server errors (500, 502), and unexpected codes (418). Use wiremock to return each status and verify error mapping. This ensures full branch coverage of `handle_response()` logic.

---
