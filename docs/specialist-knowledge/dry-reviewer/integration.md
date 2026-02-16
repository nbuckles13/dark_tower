# DRY Reviewer - Integration Notes

This file captures how the DRY Reviewer integrates with other specialists and components.

---

## ADR-0019 Blocking Behavior

**Added**: 2026-01-29
**Related files**: `docs/decisions/adr-0019-dry-reviewer-blocking.md`

**Integration Point**: Per ADR-0019, the DRY Reviewer has specific blocking behavior:
- **BLOCKER severity**: Blocks devloop completion (e.g., shared code requiring extraction)
- **TECH_DEBT severity**: Non-blocking, documented for future work

---

## Tech Debt Registry

Tracked duplication patterns with assigned IDs for consistent classification.

---

## Resolved Tech Debt

### TD-1: JWT Validation Duplication (AC vs GC)
**Added**: 2026-01-15 | **Resolved**: 2026-01-31
**Related files**: `crates/common/src/jwt.rs` (canonical location)

JWT utilities extracted to `crates/common/src/jwt.rs` on 2026-01-30. Includes `extract_kid()`, clock skew constants (`DEFAULT_CLOCK_SKEW`, `MAX_CLOCK_SKEW`), `MAX_JWT_SIZE_BYTES`, and `validate_iat()`. AC and GC now import from common instead of duplicating.

---

### TD-2: EdDSA Key Handling Patterns
**Added**: 2026-01-15 | **Resolved**: 2026-01-31
**Related files**: `crates/common/src/jwt.rs` (canonical location)

EdDSA key decoding functions (`decode_ed25519_public_key_pem`, `decode_ed25519_public_key_jwk`) extracted to `crates/common/src/jwt.rs` on 2026-01-30 as part of JWT utilities consolidation. AC and GC use shared implementation for public key decoding from PEM and JWK formats.

---

## Active Tech Debt

### TD-3: Enum Duplication (ParticipantType, MeetingRole)
**Added**: 2026-01-23
**Related files**: `crates/global-controller/src/services/ac_client.rs:28-45`, `crates/ac-service/src/models/mod.rs:75-110`

Both `ParticipantType` and `MeetingRole` enums are duplicated between AC and GC. AC defines canonical versions in models, GC duplicates for client requests. Severity: Low (small enums, 3 variants each). Improvement path: Extract to `common::types::ParticipantType` and `common::types::MeetingRole` when third service needs them. Timeline: Phase 7+ (when MH or MC needs participant/role concepts). Note: Current duplication acceptable - extraction cost exceeds benefit for 2 implementations.

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
**Added**: 2026-01-31 | **Updated**: 2026-02-12 (iteration 2) | **Status**: Resolved

Extracted to `generic_health_checker.rs` with closure-based generic function. Iteration 2 simplified the API: removed `HealthCheckerConfig` struct in favor of plain `entity_name: &'static str` parameter, removed `#[instrument]` from generic function in favor of `.instrument(tracing::info_span!(...))` chaining on wrappers. `health_checker.rs` and `mh_health_checker.rs` are now thin wrappers (~65 lines each, down from ~380/320). Approach used `Fn(PgPool, i64) -> Fut` closure (zero-cost, monomorphized) rather than trait objects. Wrapper function signatures unchanged -- no call site modifications needed.

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
**Added**: 2026-02-02 | **Updated**: 2026-02-11
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs:645-659`, `crates/meeting-controller/tests/gc_integration.rs:267-279`

Mock TokenReceiver helper function duplicated within MC (unit tests and integration tests). Uses OnceLock pattern for proper memory management. Severity: Low (2 occurrences in same service). Improvement path: Extract to `common::token_manager::test_helpers::mock_receiver()` when third occurrence appears (e.g., GC tests). Timeline: Next refactoring sprint or when GC adds similar test helper. Note: GC does not currently have this pattern - MC-only duplication.

---

### TD-17: OAuth Config Fields Pattern - NOT A VIOLATION
**Added**: 2026-02-02 | **Updated**: 2026-02-11 | **Status**: Closed (expected duplication)
**Related files**: `crates/meeting-controller/src/config.rs`, `crates/global-controller/src/config.rs`

Both MC and GC have OAuth credential fields for TokenManager integration. Severity: N/A (intentional, not a violation). Status: This is EXPECTED duplication, not a DRY violation. Each service authenticates to a different upstream (MC → GC, GC → AC), so separate configuration is architecturally correct. Closing as "not a tech debt item".

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

### TD-20: Increment/Decrement Pattern in Actor Metrics
### TD-20: Redundant test_default_check_interval in Health Checker Wrappers
**Added**: 2026-02-12
**Related files**: `crates/global-controller/src/tasks/generic_health_checker.rs:100`, `crates/global-controller/src/tasks/health_checker.rs:78`, `crates/global-controller/src/tasks/mh_health_checker.rs:78`

`test_default_check_interval` appears in 3 modules: the generic module (canonical) and both wrappers. Wrappers import `DEFAULT_CHECK_INTERVAL_SECONDS` from generic via `pub use`, so all three tests assert the same value. Severity: Low (trivial test, 1 line each). Improvement path: Remove `test_default_check_interval` from both wrapper test modules. Timeline: Next cleanup sprint. Note: Deferred in TD-13 PR to minimize diff and avoid changing test count mid-review.

---

### TD-19: Increment/Decrement Pattern in Actor Metrics
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/src/actors/metrics.rs:348-389`

ActorMetrics uses identical increment/decrement pattern in 4 methods (`meeting_created`, `meeting_removed`, `connection_created`, `connection_closed`): fetch_add/sub with Ordering::Relaxed, cast to u64, emit to Prometheus gauge. Severity: Low (code is clear, minor repetition ~6 lines × 4 = 24 lines). Improvement path: Could extract to generic helper `fn emit_metric_change(&self, field: &AtomicUsize, metric_fn: impl Fn(u64))` but the explicit form is actually clearer for readers. Timeline: Future cleanup only if this pattern adds 5+ methods. Note: Type safety and clarity of explicit form outweigh minor duplication. Do not extract prematurely.

---

### TD-21: Counter+Histogram Recording Pattern Inconsistency
**Added**: 2026-02-10
**Related files**: `crates/meeting-controller/src/observability/metrics.rs:154-174`, `crates/global-controller/src/observability/metrics.rs:43-63`, `crates/ac-service/src/observability/metrics.rs:30-35`

MC uses separate functions for counter and histogram recording (`record_gc_heartbeat()` + `record_gc_heartbeat_latency()`), while AC and GC combine them into single functions (`record_token_issuance()`, `record_http_request()` record both counter and histogram). Severity: Low (both approaches work correctly, stylistic inconsistency only). Improvement path: Consider unifying pattern across services when establishing shared metric patterns (if ever moved to common crate). Timeline: Phase 5+ (infrastructure cleanup). Note: MC pattern allows recording only counter OR only histogram if needed (flexibility), while AC/GC pattern reduces duplication at call sites (convenience). Both are valid per ADR-0011.

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

## Knowledge Documentation Recommendations

**Added**: 2026-01-29

**Integration Point**: When discovering widely-used patterns that aren't yet documented in specialist knowledge files, add TECH_DEBT findings recommending documentation updates. This helps future contributors understand established conventions without needing to search the codebase.

**Example**: The error preservation pattern `.map_err(|e| Error(format!("...: {}", e)))` is used 40+ times across services. If not documented in a specialist's knowledge files, recommend adding it.

---
