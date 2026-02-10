# DRY Reviewer - Integration Notes

This file captures how the DRY Reviewer integrates with other specialists and components.

---

## ADR-0019 Blocking Behavior

**Added**: 2026-01-29
**Related files**: `docs/decisions/adr-0019-dry-reviewer-blocking.md`

**Integration Point**: Per ADR-0019, the DRY Reviewer has specific blocking behavior:
- **BLOCKER severity**: Blocks dev-loop completion (e.g., shared code requiring extraction)
- **TECH_DEBT severity**: Non-blocking, documented for future work

---

## Tech Debt Registry

Tracked duplication patterns with assigned IDs for consistent classification.

---

### TD-1: JWT Validation Duplication (AC vs GC) - RESOLVED
**Added**: 2026-01-15 | **Resolved**: 2026-01-31
**Related files**: `crates/common/src/jwt.rs` (canonical location)

**RESOLVED**: JWT utilities extracted to `crates/common/src/jwt.rs` per commits babd7f7 and 2b4b70f. Includes `extract_kid()`, clock skew constants (`DEFAULT_CLOCK_SKEW_SECONDS`, `MAX_CLOCK_SKEW_SECONDS`), and `MAX_JWT_SIZE_BYTES`. AC and GC now re-export from common for backwards compatibility.

---

### TD-2: EdDSA Key Handling Patterns - RESOLVED
**Added**: 2026-01-15 | **Resolved**: 2026-01-31
**Related files**: `crates/common/src/jwt.rs` (canonical location)

**RESOLVED**: EdDSA key handling consolidated into `crates/common/src/jwt.rs` as part of JWT utilities extraction (commits babd7f7, 2b4b70f). Public key decoding from base64url and DecodingKey creation now shared.

---

### TD-3: ID Validation Function Patterns
**Added**: 2026-01-23
**Related files**: `crates/global-controller/src/auth/`, `crates/ac-service/src/`

`validate_meeting_id` (GC) duplicates structural logic from `validate_controller_id` (AC) - both validate identifier format with similar length/character checks. Severity: Low (validation is intentionally strict per-type). Improvement path: Consider extracting to `common::validation::id` trait when Meeting Controller adds its own ID validation. Timeline: Phase 6+ (when MC implementation begins). Note: Different ID types have different semantic requirements, so some duplication may be acceptable.

---

### TD-4: Weighted Random Selection for Load Balancing
**Added**: 2026-01-24 | **Updated**: 2026-02-09
**Related files**: `crates/global-controller/src/services/mh_selection.rs`, `crates/global-controller/src/repositories/meeting_controllers.rs`

Both MH selection and MC selection use weighted random selection based on inverse load ratio: calculate weight as `(1.0 - load_ratio)`, use weighted distribution to select instance. Severity: Low (algorithm is simple, 15-20 lines). Improvement path: Consider extracting to `common::load_balancing::WeightedSelector<T>` trait if a third use case appears (e.g., client load balancing). Timeline: Phase 7+ (when Media Handler internal routing is implemented). Note: Current duplication is acceptable as both implementations are in the same crate (GC) and the code is small. Update 2026-02-09: MH selection now has dedicated service (`mh_selection.rs`) with `weighted_random_select()` function using CSPRNG, confirming pattern stability.

---

### TD-5: Instance ID Generation
**Added**: 2026-01-25
**Related files**: `crates/global-controller/src/`, `crates/meeting-controller/src/`

Instance ID generation pattern duplicated between GC and MC (~6 lines each): generate UUID, format as service-prefixed string (e.g., "gc-{uuid}" or "mc-{uuid}"). Severity: Low (small code, 6 lines). Improvement path: Consider extracting to `common::instance::generate_instance_id(prefix: &str)` when Media Handler adds similar pattern. Timeline: Phase 7+ (when MH implementation begins). Note: Current duplication acceptable - extraction cost exceeds benefit for 2 implementations.

---

### TD-6: ActorMetrics Pattern (MC)
**Added**: 2026-01-25 | **Updated**: 2026-01-30
**Related files**: `crates/meeting-controller/src/actors/metrics.rs`

MC's actor system uses `ActorMetrics` and `ControllerMetrics` structs to track actor lifecycle metrics (message counts, processing times, queue depths) and heartbeat reporting metrics (meetings, participants). Severity: Low (MC-specific actor model). Improvement path: Consider extracting to `common::metrics::ActorMetrics<T>` trait if GC or MH implement similar actor patterns with metrics. Timeline: Phase 7+ (when second actor implementation appears). Note: Single implementation - do not extract prematurely. Monitor for pattern emergence in other services.

---

### TD-7: gRPC Client Channel Pattern
**Added**: 2026-01-26 | **Updated**: 2026-01-30
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`, `crates/global-controller/src/services/mc_client.rs`

Both MC and GC implement gRPC clients with channel management and auth header injection. Pattern includes: `add_auth()` helper for Bearer token, configurable timeouts, channel clone for concurrent use. Severity: Low (implementations differ in strategy). MC uses direct `Channel` (eager init, single endpoint), GC uses `HashMap<String, Channel>` pool (multiple MC endpoints). Improvement path: Consider `common::grpc::AuthenticatedClient<C>` trait when third gRPC client appears (MH). Timeline: Phase 7+ (when MH implementation begins). Note: Current duplication acceptable - different strategies are appropriate for their use cases and extraction cost exceeds benefit for 2 implementations.

---

### TD-8: gRPC Auth Interceptor Pattern
**Added**: 2026-01-27
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`, `crates/global-controller/src/services/mc_client.rs`

Both MC and GC implement similar `add_auth()` helper functions for injecting Bearer tokens into gRPC request metadata. Pattern includes: create `MetadataValue` from token string, insert into request metadata with "authorization" key. Severity: Low (parallel evolution, ~5 lines each). Improvement path: Consider extracting to `common::grpc::auth_interceptor()` or `common::grpc::BearerAuth` trait when third gRPC client appears (MH). Timeline: Phase 7+ (when MH implementation begins). Note: Current duplication acceptable - small code, parallel evolution. Could be combined with TD-7 extraction into unified `common::grpc::AuthenticatedClient` module.

---

### TD-9: IntoResponse/ErrorResponse Boilerplate (HTTP Services)
**Added**: 2026-01-28
**Related files**: `crates/global-controller/src/errors.rs`, `crates/ac-service/src/errors.rs`

Both GC and AC implement similar `IntoResponse` trait impls for their error types with `ErrorResponse` JSON structures. Pattern includes: match on error variant, determine status code/error code/message, build JSON response, add service-specific headers (WWW-Authenticate, Retry-After). Severity: Low (implementations differ slightly - AC has scope fields, GC logs internal errors). Improvement path: Consider extracting shared `ErrorResponse` JSON structure and base `IntoResponse` logic to `common::http::errors` when third HTTP service appears or when implementations converge. Timeline: Phase 5+ (when more HTTP services exist beyond GC and AC). Note: Current duplication acceptable - services have different header requirements and error metadata. Premature extraction would add complexity for marginal benefit.

---

### TD-10: JWT Clock Skew Configuration Validation
**Added**: 2026-01-29
**Related files**: `crates/ac-service/src/config.rs:184-219`, `crates/global-controller/src/config.rs:138-163`

Both AC and GC implement nearly identical JWT clock skew validation: `DEFAULT_JWT_CLOCK_SKEW_SECONDS` (300), `MAX_JWT_CLOCK_SKEW_SECONDS` (600), and validation logic (positive, under max, parse errors). Pattern includes: parse from env var, validate range, return ConfigError if invalid. ~40 lines duplicated. Severity: Low (small code, straightforward). Improvement path: Extract to `common::config::parse_jwt_clock_skew(vars: &HashMap, key: &str) -> Result<i64, ConfigError>`. Timeline: Phase 5+ (when third service requires JWT clock skew config). Note: Current duplication acceptable for 2 services - defer extraction until third consumer appears.

---

### TD-11: Shutdown Signal Handler
**Added**: 2026-01-30
**Related files**: `crates/meeting-controller/src/main.rs:289-319`, `crates/global-controller/src/main.rs:204-253`, `crates/ac-service/src/main.rs:136-189`

All three services implement nearly identical `shutdown_signal()` async functions (~30 lines each): listen for SIGINT/SIGTERM, handle Unix vs non-Unix conditionally with `#[cfg(unix)]`, optional drain period with env var override. Severity: Low (small code, straightforward). Improvement path: Extract to `common::shutdown::shutdown_signal()` with optional drain period parameter. Timeline: Phase 5+ (infrastructure cleanup). Note: Current duplication acceptable - extraction would provide minor benefit. MC's version is slightly simpler (no drain period).

---

### TD-12: Tracing Initialization
**Added**: 2026-01-30
**Related files**: `crates/meeting-controller/src/main.rs:56-62`, `crates/global-controller/src/main.rs:46-53`, `crates/ac-service/src/main.rs:24-32`

All three services have identical tracing_subscriber initialization (~7 lines each): `registry().with(EnvFilter).with(fmt::layer()).init()`. Only difference is the default filter string (service name). Severity: Low (small code, boilerplate). Improvement path: Extract to `common::observability::init_tracing(default_filter: &str)`. Timeline: Phase 5+ (infrastructure cleanup). Note: Very low priority - the code is small and unlikely to diverge.

---

### TD-13: Health Checker Background Task Pattern
**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/src/health/checker.rs`, `crates/media-handler/src/health/checker.rs`

Both MC and MH implement similar health checker background tasks (~150 lines each, ~300 total): spawn tokio task, periodic check loop with configurable interval, aggregate component health into overall status, expose via gRPC health service. Pattern includes: `HealthChecker` struct with `CancellationToken`, check interval, and atomic health state. Severity: Low (infrastructure, acceptable for 2 services). Improvement path: Consider `common::health::HealthChecker<T: HealthCheckable>` trait when third service needs same pattern. Timeline: Phase 5+ (infrastructure cleanup). Note: Current duplication acceptable - services have different components to check and different aggregation strategies.

---

### TD-14: Exponential Backoff Pattern
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs:68-71`, `crates/meeting-controller/src/grpc/gc_client.rs:60-63`

TokenManager and GcClient both implement exponential backoff with similar constants (1s base, 30s max) and calculation `(delay * 2).min(max)`. Severity: Low (different semantics - TokenManager uses infinite retry for OAuth acquisition, GcClient uses bounded retry with deadline). Improvement path: Consider `common::retry::ExponentialBackoff` struct or helper when third use case appears (e.g., MH-AC integration). Timeline: Phase 7+ (when MH implementation begins). Note: Current duplication acceptable - different types (`u64` ms vs `Duration`), different retry semantics, extraction would require complex generalization.

---

### TD-15: HTTP Client Builder Boilerplate
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs:319-324`, `crates/global-controller/src/services/ac_client.rs:133-136`

TokenManager and AcClient both construct reqwest clients with similar pattern: `Client::builder().timeout(...).connect_timeout(Duration::from_secs(5)).build()`. Severity: Low (~4 lines each, different timeout configuration). Improvement path: Consider `common::http::build_client(timeout, connect_timeout)` helper when third HTTP client appears. Timeline: Phase 5+ (when more HTTP clients needed). Note: Current duplication acceptable - small code, TokenManager uses configurable timeout while AcClient uses constant.

---

### TD-16: Mock TokenReceiver Test Helper
**Added**: 2026-02-02 | **Updated**: 2026-02-02
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs:642-659`, `crates/meeting-controller/tests/gc_integration.rs:264-279`, `crates/global-controller/src/services/ac_client.rs` (GC integration)

Mock TokenReceiver helper functions now exist in both MC (unit tests and integration tests) and GC. All use OnceLock pattern for proper memory management. Severity: Low (3 occurrences across services). Improvement path: Extract to `common::token_manager::test_helpers::mock_receiver()` since both services now use the pattern. Timeline: Next refactoring sprint. Note: Pattern is identical across services and ready for extraction.

---

### TD-17: OAuth Config Fields Pattern
**Added**: 2026-02-02 | **Updated**: 2026-02-02 | **RESOLVED**
**Related files**: `crates/meeting-controller/src/config.rs:96-105`, `crates/global-controller/src/config.rs`

Both MC and GC now have OAuth credential fields (`ac_endpoint`, `client_id`, `client_secret`). Severity: Low (acceptable service-specific config). Status: RESOLVED - Both services integrated. No extraction needed - credentials are service-specific (MC authenticates to GC, GC authenticates to AC). Note: This is expected duplication, not a DRY violation.

---

### TD-18: Master Secret Loading Pattern
**Added**: 2026-02-02
**Related files**: `crates/meeting-controller/src/main.rs:145-170`

MC loads session binding token master secret via: base64 decode -> length validation (32 bytes min) -> wrap in SecretBox. First occurrence of this pattern. Severity: Low (single occurrence, MC-specific). Improvement path: Consider `common::secret::load_master_secret()` if GC/MH need similar secret loading. Timeline: Phase 7+ (if MH needs secrets). Note: Session binding is MC-specific per ADR-0023, so duplication may not occur.

---

### TD-19: HTTP Metrics Middleware + Path Normalization
**Added**: 2026-02-04
**Related files**: `crates/global-controller/src/middleware/http_metrics.rs`, `crates/ac-service/src/middleware/http_metrics.rs`, `crates/global-controller/src/observability/metrics.rs`, `crates/ac-service/src/observability/metrics.rs`

**HTTP Middleware** (~95% identical): Both GC and AC have identical `http_metrics_middleware` functions - capture start time, method, path, execute request, record metrics with duration. Only difference is import path for `record_http_request`.

**Path Normalization** (~80% similar): Both services implement the same algorithm: check known static paths, normalize dynamic segments with placeholders, return `/other` for unknown. AC has `normalize_path()` + `normalize_dynamic_path()` + `is_uuid()`, GC has `normalize_endpoint()` + `normalize_dynamic_endpoint()` + `categorize_status_code()`.

Severity: Medium (significant code, affects maintainability across services). Improvement path: Extract to `common::observability`:
1. `common::middleware::http_metrics_middleware<R: HttpMetricsRecorder>` with trait-based recorder
2. `common::observability::PathNormalizer` struct with configurable static paths and dynamic patterns
3. `common::observability::is_uuid()` utility
4. `common::observability::categorize_status_code()` utility

Timeline: Phase 5+ (before third HTTP service). Note: First significant observability duplication - common crate currently has NO observability module, so this establishes the pattern for future extraction.

---

This differs from Security, Test, and Code Quality reviewers where ALL findings block. Only genuine shared code requiring extraction should be classified as BLOCKER.

**When to block**: Copy-pasted business logic, duplicate utilities that should be in `common/`, identical algorithms across services.

**When NOT to block**: Convention-based patterns, domain-specific error handling, consistent logging/metrics approaches.

---

## Refactors That Improve DRY

**Added**: 2026-01-31
**Related files**: ADR-0023 Phase 6c Iteration 3

**Integration Point**: When reviewing implementation iterations, track whether refactors improve or worsen DRY. In Iteration 3:
- **Removed Arc<GcClient> duplication**: Unified task now owns client directly
- **Centralized re-registration logic**: `handle_heartbeat_error()` helper eliminates duplication in caller
- **Single registration/heartbeat loop**: `run_gc_task()` consolidates what could have been separate tasks

**Note in reviews**: Explicitly call out DRY improvements in checkpoint positive observations. This reinforces good refactoring patterns.

**Tracking**: Compare rounds - if Round N has fewer TECH_DEBT items than Round N-1 (and no new blockers), the iteration improved DRY.

---

## Principles Documentation Recommendations

**Added**: 2026-01-29
**Related files**: `docs/principles/errors.md`

**Integration Point**: When discovering widely-used patterns that aren't yet documented in principles files, add TECH_DEBT findings recommending documentation updates. This helps future contributors understand established conventions without needing to search the codebase.

**Example**: The error preservation pattern `.map_err(|e| Error(format!("...: {}", e)))` is used 40+ times across services but may not be documented in `docs/principles/errors.md`.

---
