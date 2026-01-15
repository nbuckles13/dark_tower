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
