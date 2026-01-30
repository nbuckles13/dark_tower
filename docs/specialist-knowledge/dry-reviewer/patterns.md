# DRY Reviewer - Patterns That Work

Successful approaches for cross-service duplication detection in Dark Tower.

---

## Pattern: ADR-0019 BLOCKER vs TECH_DEBT Classification
**Added**: 2026-01-15
**Related files**: `docs/decisions/adr-0019-dry-reviewer.md`

Distinguish between BLOCKER duplication (must fix) and TECH_DEBT duplication (document and continue). BLOCKER: Code EXISTS in `common` crate but wasn't used, or new duplication of security-critical code. TECH_DEBT: Similar code exists in another service (parallel evolution). This allows feature velocity while tracking architectural debt.

---

## Pattern: Tech Debt Registry with Tracking IDs
**Added**: 2026-01-15
**Related files**: `docs/specialist-knowledge/dry-reviewer/integration.md`

Assign stable tech debt IDs (TD-1, TD-2, etc.) to known duplication patterns. Include: location, pattern description, severity, status, improvement path, and timeline. This enables quick classification of similar issues in future reviews and prevents repeated discovery of the same duplication.

---

## Pattern: Three-Tier Severity Assessment
**Added**: 2026-01-15
**Related files**: `docs/decisions/adr-0019-dry-reviewer.md`

Use severity tiers to assess duplication impact: Tier 1 (BLOCKER) for code that ignores existing common utilities or introduces new security duplication. Tier 2 (TECH_DEBT) for known patterns already documented. Tier 3 (ACCEPTABLE) for small isolated code where extraction cost exceeds benefit. Only Tier 1 blocks code review approval.

---

## Pattern: Review Flow with Registry Lookup
**Added**: 2026-01-15
**Related files**: `docs/specialist-knowledge/dry-reviewer/integration.md`

Before classifying duplication: (1) Identify all duplication points in changeset, (2) Check tech debt registry in integration.md for existing TD-N entries, (3) If found, classify as TECH_DEBT with existing ID, (4) If new, assess whether BLOCKER or new TECH_DEBT. This prevents re-flagging known debt and ensures consistent classification.

---

## Pattern: Improvement vs Duplication Assessment
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`, `crates/env-tests/src/fixtures/auth_client.rs`

When new code follows an existing pattern but adds enhancements, classify as IMPROVEMENT, not duplication. Example: `GcClient.sanitize_error_body()` is an enhancement not in `AuthClient`. The assessment should be: "This is an improvement - consider backporting" not "This duplicates AuthClient." Improvements flow forward (new code is better), duplication flows both ways (same code, neither better).

---

## Pattern: Mock Trait Pattern for gRPC Clients
**Added**: 2026-01-24
**Related files**: `crates/global-controller/src/mc_client.rs`

When reviewing gRPC client code, recognize the mock trait pattern: Define a trait (`McClientTrait`) with async methods, implement it for both the real client (`McClient`) and a mock (`MockMcClient`). This is NOT duplication - it's a standard testability pattern. The trait defines the contract, the real impl uses tonic channels, and the mock returns configurable responses. Mark as ACCEPTABLE when reviewing similar patterns for other gRPC clients (future MhClient, etc.).

---

## Pattern: Same Crypto Primitive, Different Purpose
**Added**: 2026-01-25
**Related files**: `crates/ac-service/src/crypto/`, `crates/meeting-controller/src/session/`

When the same cryptographic algorithm (e.g., HMAC-SHA256) appears in multiple services, assess semantic purpose before flagging as duplication. Example: AC uses HMAC-SHA256 for full session binding (security-critical token integrity), MC uses HMAC-SHA256 truncated for log correlation IDs (operational convenience). Same primitive, different purposes - NOT candidates for extraction. Mark as ACCEPTABLE and document the semantic distinction.

---

## Pattern: Dev-Dependency Precedent Check
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/Cargo.toml`, `crates/ac-service/Cargo.toml`

Before flagging dev-dependencies as duplication or questioning their necessity, check if existing services use the same pattern. Example: `tokio = { features = ["test-util"] }` in MC Cargo.toml matches AC's existing dev-dependencies. When a pattern follows established precedent in the codebase, classify as ACCEPTABLE. This prevents false positives on legitimate test infrastructure patterns.

---

## Pattern: Defer Extraction When Implementations Differ
**Added**: 2026-01-27
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`, `crates/global-controller/src/services/mc_client.rs`

When similar code exists in two services but with meaningful implementation differences, classify as TECH_DEBT rather than BLOCKER and defer extraction. Example: MC's GcClient uses single-channel caching, GC's McClient uses multi-channel pool - similar pattern, different strategies. The third implementation (e.g., MH client) will reveal which approach is canonical. Deferring allows: (1) implementations to mature independently, (2) third consumer to inform the right abstraction, (3) feature velocity to continue. Only consider extraction when a third consumer appears AND implementations have converged on a common approach.

---

## Pattern: Secret Wrapper Duplication Across Response Types
**Added**: 2026-01-28
**Related files**: `crates/ac-service/src/models/mod.rs`

When a single service wraps sensitive fields with SecretString or SecretBox across multiple response types, evaluate duplication based on scope. Example: `RegisterServiceResponse`, `CreateClientResponse`, and `RotateSecretResponse` all have `client_secret: SecretString` with identical custom Debug/Serialize impls. Scope = 3 types within single service (ac-service). Assessment: NOT BLOCKER - this is acceptable duplication for 3 response types in the same service. Only consider extraction to `common::secret` if: (1) a second service also needs identical response patterns, (2) the impl pattern becomes standardized across 4+ types in same service. Rationale: Custom Debug/Serialize impls for security wrappers are intentionally terse and service-specific; extracting prematurely creates coupling and makes intent less clear.

---

## Pattern: Service Error Enum Convergence Check
**Added**: 2026-01-28
**Related files**: `crates/global-controller/src/errors.rs`, `crates/meeting-controller/src/errors.rs`, `crates/ac-service/src/errors.rs`

When reviewing error enum changes across services, check for convergence on shared patterns from `common::error::DarkTowerError`. Example: GC and MC both use `Internal(String)` variant (matches common), while AC still uses `Internal` unit variant. This indicates parallel evolution toward a standard. Assessment: NOT BLOCKER when services converge on the common pattern - this is healthy architecture alignment. Only flag as TECH_DEBT if a service diverges from the established common pattern without justification. Rationale: Services cannot share error enums due to domain-specific variants, but should align on shared variant signatures (Internal, Database, etc.).

---

## Pattern: Tracing Instrument Patterns Are Infrastructure
**Added**: 2026-01-28
**Related files**: Code quality guard violations across services

When reviewing `#[instrument]` attribute patterns (e.g., `skip_all`, `skip(field1, field2)`, `fields(...)`), recognize these as infrastructure patterns, not business logic duplication. Each service will have service-specific span names (`gc.handler`, `mc.actor`, `ac.endpoint`) and field selections. Assessment: ACCEPTABLE - tracing patterns are intrinsic to observability infrastructure. Do not flag similar `skip_all` or `skip(...)` patterns across services as duplication requiring extraction. Only escalate if the actual instrumented business logic is duplicated, not the tracing boilerplate.

---

## Pattern: Error Preservation with Logging (Established)
**Added**: 2026-01-29
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/global-controller/src/auth/jwt.rs`, `crates/meeting-controller/src/config.rs`

The pattern `.map_err(|e| { tracing::error!(...); ServiceError::Variant(...) })` is now established across all three services (AC, MC, GC) as of Phase 4. This is idiomatic Rust for preserving error context while logging internal details before returning user-facing messages. Assessment: ACCEPTABLE - this is not duplication requiring extraction. Each service has its own error types, logging targets, and user messages. Only flag as duplication if the business logic inside the closure is identical across services.

---

## Pattern: Config Validation Duplication Threshold
**Added**: 2026-01-29
**Related files**: `crates/*/src/config.rs`

When reviewing config validation code (parsing env vars, range checks, error messages), apply the "3+ services" extraction threshold. For 2 services with similar validation patterns, classify as TECH_DEBT. For 3+ services, consider extraction to `common::config`. Rationale: Config validation is simple (~5-10 lines per field) and extraction requires generic error handling that may add more complexity than it removes. Defer until third consumer appears or validation logic becomes significantly more complex.

---
