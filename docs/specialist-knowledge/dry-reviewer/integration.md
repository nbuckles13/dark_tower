# DRY Reviewer - Cross-Service Integration Notes

Known duplication patterns and cross-service coordination for Dark Tower.

---

## Tech Debt Registry

Tracked duplication patterns with assigned IDs for consistent classification.

---

### TD-1: JWT Validation Duplication (AC vs GC)
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/global-controller/src/auth/jwt.rs`

JWT validation logic duplicated between AC and GC: `extract_jwt_kid` (AC) vs `extract_kid` (GC), `verify_jwt` (AC) vs `verify_token` (GC), and `MAX_JWT_SIZE_BYTES` constant (4KB in AC, 8KB in GC). Severity: Medium. Improvement path: Extract to `common::crypto::jwt` utilities module. Timeline: Phase 5+ (post-Phase 4 hardening). Note: GC uses JWKS client for key fetching while AC uses database - extraction must preserve these different key sources.

---

### TD-2: EdDSA Key Handling Patterns
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/global-controller/src/auth/jwt.rs`

Both services implement EdDSA public key decoding from base64url and DecodingKey creation. Severity: Low (small code). Improvement path: Consider extraction when a third service (MC or MH) needs the same pattern. Timeline: Phase 5+ or when third consumer appears.

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
