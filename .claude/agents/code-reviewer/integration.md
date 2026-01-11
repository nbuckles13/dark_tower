# Code Reviewer - Integration Notes

Notes on working with other specialists in the Dark Tower project.

---

## Integration: Security Specialist Handoff
**Added**: 2026-01-11
**Related files**: `.claude/agents/security.md`

When reviewing security-critical code (crypto, auth, validation), flag findings as MAJOR or CRITICAL for security specialist review. Defense-in-depth recommendations should be explicitly requested if not already implemented. Security specialist should verify cryptographic parameter choices match OWASP/NIST guidance.

---

## Integration: Test Specialist Collaboration
**Added**: 2026-01-11
**Related files**: `.claude/agents/test.md`

After code review, coordinate with test specialist to ensure: boundary conditions have tests, error paths are exercised, security-critical paths have P0 priority tests. For config changes, verify both valid and invalid input tests exist.

---

## Integration: Auth Controller Specialist Context
**Added**: 2026-01-11
**Related files**: `crates/ac-service/`

Auth Controller has established patterns for config, crypto, and validation. When reviewing AC changes, verify pattern consistency with existing code. Key files to check: `config.rs` for configuration patterns, `crypto.rs` for cryptographic operations, `error.rs` for error handling patterns.

---

## Integration: Pre-Review Checklist
**Added**: 2026-01-11
**Related files**: `.claude/workflows/code-review.md`

Before starting review, verify: (1) no unwrap/expect/panic in production paths, (2) sqlx used for all database queries, (3) Result<T,E> used for fallible operations, (4) documentation includes security references where applicable, (5) tests cover boundary conditions.

---

## Integration: ADR Compliance Check
**Added**: 2026-01-11
**Related files**: `docs/decisions/`

Cross-reference code changes against existing ADRs. Key ADRs: ADR-0002 (no-panic policy), ADR-0003 (error handling). Flag violations as MAJOR findings requiring remediation before approval.
