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
