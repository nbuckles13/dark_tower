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

### TD-1: JWT Validation Duplication (AC vs GC)
**Added**: 2026-01-15 | **Updated**: 2026-01-24
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/global-controller/src/auth/jwt.rs`

JWT validation logic duplicated between AC and GC: `extract_jwt_kid` (AC) vs `extract_kid` (GC), `verify_jwt` (AC) vs `verify_token` (GC), and `MAX_JWT_SIZE_BYTES` constant (4KB in AC, 8KB in GC). Additionally, JWT clock skew constants (e.g., `CLOCK_SKEW_SECONDS`) are duplicated in both services for `iat`/`exp` validation. Severity: Medium. Improvement path: Extract to `common::crypto::jwt` utilities module with configurable constants. Timeline: Phase 5+ (post-Phase 4 hardening). Note: GC uses JWKS client for key fetching while AC uses database - extraction must preserve these different key sources.

---

### TD-2: EdDSA Key Handling Patterns
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/global-controller/src/auth/jwt.rs`

Both services implement EdDSA public key decoding from base64url and DecodingKey creation. Severity: Low (small code). Improvement path: Consider extraction when a third service (MC or MH) needs the same pattern. Timeline: Phase 5+ or when third consumer appears.

---

### TD-3: ID Validation Function Patterns
**Added**: 2026-01-23
**Related files**: `crates/global-controller/src/auth/`, `crates/ac-service/src/`

`validate_meeting_id` (GC) duplicates structural logic from `validate_controller_id` (AC) - both validate identifier format with similar length/character checks. Severity: Low (validation is intentionally strict per-type). Improvement path: Consider extracting to `common::validation::id` trait when Meeting Controller adds its own ID validation. Timeline: Phase 6+ (when MC implementation begins). Note: Different ID types have different semantic requirements, so some duplication may be acceptable.

---

### TD-4: Weighted Random Selection for Load Balancing
**Added**: 2026-01-24
**Related files**: `crates/global-controller/src/repositories/media_handlers.rs`, `crates/global-controller/src/repositories/meeting_controllers.rs`

Both MH selection and MC selection use weighted random selection based on inverse load ratio: calculate weight as `(1.0 - load_ratio) * 100`, use weighted distribution to select instance. Severity: Low (algorithm is simple, 5-10 lines). Improvement path: Consider extracting to `common::load_balancing::WeightedSelector<T>` trait if a third use case appears (e.g., client load balancing). Timeline: Phase 7+ (when Media Handler internal routing is implemented). Note: Current duplication is acceptable as both implementations are in the same crate (GC) and the code is small.

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
