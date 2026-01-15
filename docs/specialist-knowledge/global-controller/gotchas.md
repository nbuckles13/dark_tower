# Global Controller Gotchas

Mistakes to avoid and edge cases discovered in the Global Controller codebase.

---

## Gotcha: AC JWKS URL Must Be Reachable
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

GC validates JWTs using AC's JWKS endpoint. AC_JWKS_URL must be reachable at runtime. In tests, use mock or real TestAcServer. In production, ensure network connectivity to AC.

---

## Gotcha: Database URL Redacted in Debug
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

Config's Debug impl redacts DATABASE_URL to prevent credential leaks in logs. Never log Config with `{:?}` expecting to see DB credentials - they're intentionally hidden.

---

## Gotcha: Clock Skew Shared with AC
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

JWT_CLOCK_SKEW_SECONDS should match AC's value (default 300s). Mismatched skew can cause valid tokens to be rejected. Both services read from same env var name.

---

## Gotcha: Rate Limit Per-Minute Not Per-Second
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

RATE_LIMIT_RPM is requests per MINUTE (default 60). Not per-second. Config validation ensures range 10-10000 RPM. Token bucket implementation handles actual rate limiting.

---

## Gotcha: Health Endpoint Pings Database
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/handlers/health.rs`

`/v1/health` executes `SELECT 1` to verify database connectivity. If DB is down, health returns 503. Don't use for liveness probe if DB issues should not restart pod.

---

## Gotcha: Request Timeout is 30 Seconds
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/routes/mod.rs`

Tower ServiceBuilder sets 30s request timeout. Long operations (meeting creation with many participants) must complete within this window. Increase if needed for specific routes.

---

## Gotcha: Region Defaults to "unknown"
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

GC_REGION env var is optional, defaults to "unknown". Production deployments MUST set this for proper geographic routing. Health endpoint includes region in response.

---

## Gotcha: MeetingStatus Enum Not Yet Persisted
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/models/mod.rs`

MeetingStatus enum (Scheduled, Active, Ended, Cancelled) is defined but not yet mapped to database. Phase 2 will add migrations. Do not assume it maps to VARCHAR or enum type yet.

---

## Gotcha: Dual Module Declarations in lib.rs and main.rs
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/lib.rs`, `crates/global-controller/src/main.rs`

When a crate has both lib.rs and main.rs, modules must be declared in BOTH files. If you add a new module (e.g., `mod auth;`), it must appear in:
- lib.rs (so tests and other crates can use it)
- main.rs (so the binary entry point sees it)
Missing either declaration causes compilation errors. Check both files when adding modules.

---

## Gotcha: Borrow Checker with Partial Move in Struct Initialization
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/handlers/me.rs`

When constructing a response from Claims, avoid partial moves:
- Wrong: `MeResponse { sub: claims.sub, scopes: claims.scopes() }`
- Right: Extract `scopes` to local var first, then construct response
The borrow checker won't allow moving `claims.sub` then borrowing `claims` for method call in same expression. Solve by extracting computed values to local variables before struct construction.

---

## Gotcha: Token Size Limit is Bytes, Not Characters
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs:74`

MAX_JWT_SIZE_BYTES checks `token.len()` which counts UTF-8 bytes, not characters. Most JWT tokens are ASCII so this is equivalent, but be explicit in comments. Test boundary cases: 8191 bytes (accept), 8192 bytes (accept), 8193 bytes (reject).

---

## Gotcha: kid Extraction Happens BEFORE Signature Validation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs:86-90`

The kid is extracted from JWT header without signature verification. This is correct for key lookup but:
- Never trust kid value
- Always validate JWK (kty, alg) after fetching
- Attacker can claim any kid, but must have valid signature from that key
If JWK doesn't exist, return "invalid or expired" not "kid not found" (info leak prevention).

---

## Gotcha: JWKS Cache TTL Affects Key Rotation Latency
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwks.rs:21, 104-107`

5-minute cache TTL means AC key rotations take up to 5 minutes to propagate. If AC rotates keys and GC still has old key cached, tokens signed with new key will fail until cache expires. This is intentional tradeoff:
- Shorter TTL (1 min): Faster rotation but higher load on AC
- Longer TTL (10 min): Lower AC load but slower rotation
Verify TTL matches operational requirements during deployment.

---

## Gotcha: JWKS HTTP Client Timeout is 10 Seconds
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwks.rs:101-102`

reqwest client built with 10-second timeout. If AC is slow to respond, JWKS fetch will timeout and return ServiceUnavailable (503). This is correct behavior - stale cache is better than hanging. If AC is consistently slow, increase timeout and add observability (metrics).

---

## Gotcha: Generic Error Messages Hide JWT Validation Details
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs:82, 111, 150, 156, 165, 171, 185`

All JWT validation failures return "The access token is invalid or expired" to clients. This is intentional - don't leak:
- Whether kid was found
- Why JWK validation failed
- Specific signature error details
Log detailed error internally, but never include in HTTP response. This prevents attackers from probing token format.

---

## Gotcha: Algorithm Confusion Attacks Can Use alg:none
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs:177-179`

The jsonwebtoken library defaults to accepting `alg:none` if not explicitly pinned. ALWAYS use `Validation::new(Algorithm::EdDSA)` - never `Validation::default()`. Test for this specifically:
- Token with `alg:none` should be rejected
- Token with `alg:HS256` should be rejected
- Only `alg:EdDSA` should be accepted
Review code review findings from 2026-01-14 for test coverage.

---
