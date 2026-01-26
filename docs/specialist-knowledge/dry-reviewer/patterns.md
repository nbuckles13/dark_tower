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
