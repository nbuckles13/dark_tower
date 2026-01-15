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
