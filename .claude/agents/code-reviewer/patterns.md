# Code Reviewer - Patterns

Reusable code quality patterns observed in Dark Tower codebase.

---

## Pattern: Configuration Value Pattern (Constants + Field + Parsing + Tests)
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

When adding configurable security parameters (e.g., bcrypt cost, JWT clock skew), follow the four-part pattern: (1) define constants with DEFAULT/MIN/MAX bounds, (2) add config struct field with serde defaults, (3) implement parsing with range validation, (4) add comprehensive tests. This ensures consistency and makes security boundaries explicit.

---

## Pattern: Defense-in-Depth Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto.rs`

Validate security-critical values at multiple layers: config parsing time AND at point of use. Even if config validation ensures valid ranges, crypto functions should independently verify inputs. Prevents bugs if validation is bypassed or config is constructed programmatically.

---

## Pattern: OWASP/NIST Reference Documentation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Document security-critical constants with references to authoritative sources (OWASP, NIST). Example: bcrypt cost factor 12 references OWASP password storage cheat sheet. This provides audit trail and justification for security decisions.

---

## Pattern: No Panic Production Code (ADR-0002)
**Added**: 2026-01-11
**Related files**: `docs/decisions/adr-0002-no-panic-policy.md`

All production code uses `Result<T, E>` for fallible operations. The `.unwrap()`, `.expect()`, and `panic!()` are only allowed in: test code, truly unreachable invariants with proof comments, and development tools. Grep for these patterns during review.
