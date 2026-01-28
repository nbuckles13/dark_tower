# DRY Reviewer - Cross-Service Integration Notes

Known duplication patterns and cross-service coordination for Dark Tower.

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
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/session/actor.rs`

MC's SessionActor uses an ActorMetrics struct to track actor lifecycle metrics (message counts, processing times, queue depths). Severity: Low (first implementation, MC-specific). Improvement path: Consider extracting to `common::metrics::ActorMetrics<T>` trait if GC or MH implement similar actor patterns with metrics. Timeline: Phase 7+ (when second actor implementation appears). Note: Single implementation - do not extract prematurely. Monitor for pattern emergence in other services.

---

### TD-7: gRPC Client Channel Caching Pattern
**Added**: 2026-01-26
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`, `crates/global-controller/src/services/mc_client.rs`

Both MC and GC implement gRPC clients with channel caching and auth header injection. Pattern includes: `get_channel()` with RwLock-protected cache, `add_auth()` helper for Bearer token, configurable timeouts. Severity: Low (implementations differ in cache strategy - single vs pool). Improvement path: Consider `common::grpc::AuthenticatedClient<C>` trait when third gRPC client appears (MH). Timeline: Phase 7+ (when MH implementation begins). Note: Current duplication acceptable - implementations differ (single-channel vs multi-channel pool) and extraction cost exceeds benefit for 2 implementations.

---

### TD-8: gRPC Auth Interceptor Pattern
**Added**: 2026-01-27
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`, `crates/global-controller/src/services/mc_client.rs`

Both MC and GC implement similar `add_auth()` helper functions for injecting Bearer tokens into gRPC request metadata. Pattern includes: create `MetadataValue` from token string, insert into request metadata with "authorization" key. Severity: Low (parallel evolution, ~5 lines each). Improvement path: Consider extracting to `common::grpc::auth_interceptor()` or `common::grpc::BearerAuth` trait when third gRPC client appears (MH). Timeline: Phase 7+ (when MH implementation begins). Note: Current duplication acceptable - small code, parallel evolution. Could be combined with TD-7 extraction into unified `common::grpc::AuthenticatedClient` module.

---

## Specialist Coordination
**Added**: 2026-01-15
**Related files**: `.claude/agents/security.md`, `.claude/agents/code-reviewer.md`, `.claude/agents/test.md`

Security specialist handoff: Escalate duplication in cryptographic code to Security specialist. Security may accept duplication if it reduces coupling - document their rationale. Code reviewer handoff: DRY reviewer focuses on cross-service duplication; Code Quality specialist focuses on single-service structure. Share findings if both identify the same issue. Test specialist handoff: Test utility duplication (e.g., ac-test-utils) may warrant extraction; Test specialist has final say on test code organization.

---

## Acceptable Duplication Patterns
**Added**: 2026-01-15
**Related files**: `crates/*/src/config.rs`, `crates/*/src/errors.rs`

These patterns are acceptable and should NOT be flagged: (1) Per-service configuration loading (each service has different env vars), (2) Service-specific error types (each service defines its own error.rs), (3) Protocol message handling (each service may interpret messages differently), (4) Logging/metrics initialization (boilerplate is expected per OWASP guidelines).

---

## Escalation Criteria
**Added**: 2026-01-15
**Related files**: `.claude/agents/security.md`

Escalate to Architecture specialist if: duplication spans 3+ services, extraction requires database schema changes, pattern impacts protocol or API contracts, uncertainty about service boundary placement. Escalate to Security specialist if: duplication involves cryptography, authentication, or authorization, or pattern impacts threat model.

---
