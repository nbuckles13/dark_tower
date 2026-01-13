# Code Reviewer - Gotchas

Common code smells and anti-patterns to watch for in Dark Tower codebase.

---

## Gotcha: Single-Layer Security Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto.rs`

If security parameters are only validated at config parse time, bugs or programmatic construction can bypass checks. Always validate at point of use too. Example: bcrypt cost should be checked both when loading config AND when hashing passwords.

---

## Gotcha: Magic Numbers Without Constants
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Security-critical numeric values (cost factors, timeouts, limits) should be defined as named constants with documentation, not inline literals. Bad: `if cost < 4`. Good: `if cost < BCRYPT_COST_MIN` with constant documenting why 4 is minimum.

---

## Gotcha: Missing Range Tests for Config Values
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Config validation tests should cover: below minimum (rejected), at minimum (accepted), default value (accepted), at maximum (accepted), above maximum (rejected). Missing boundary tests allow edge case bugs to slip through.

---

## Gotcha: Inconsistent Pattern Between Similar Features
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

When adding features similar to existing ones (e.g., bcrypt cost like JWT clock skew), verify exact pattern match: same constant naming, same validation approach, same test coverage style. Inconsistency creates maintenance burden and hides bugs.

---

## Gotcha: String Concatenation in SQL Queries
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/repository/`

Never use format!() or string concatenation for SQL. Always use sqlx compile-time checked queries with parameterized values. This is enforced by project convention and prevents SQL injection by design.

---

## Gotcha: Deriving Debug on Structs with SecretBox Fields
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

Do NOT use `#[derive(Debug)]` on structs containing `SecretBox<T>` or `SecretString`. While `SecretBox` itself redacts in Debug output, the struct's derived Debug may expose other sensitive context (like database URLs with credentials). Always implement Debug manually to control exactly what's shown. Look for structs with secret fields that derive Debug - they need manual impl.

---

## Gotcha: Missing Documentation on Custom Serialize for Secrets
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

When implementing custom `Serialize` that exposes a `SecretString` via `.expose_secret()`, always add a doc comment explaining this is intentional. Without documentation, future reviewers may flag it as a security bug. Pattern: `/// Custom Serialize that exposes client_secret for API response. This is intentional: [reason].`

---

## Gotcha: Forgetting Clone Impl When Using SecretBox
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`

`SecretBox<T>` does not derive `Clone` by design. If your struct needs Clone and contains SecretBox fields, you must implement Clone manually. Compiler error will catch this, but watch for workarounds like removing Clone requirement entirely - sometimes Clone is actually needed (e.g., Config shared across threads via Arc).

---

## Gotcha: Inconsistent Redaction Placeholder Strings
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/`

Use consistent `"[REDACTED]"` string across all Debug implementations. Inconsistent placeholders (e.g., `"***"`, `"<hidden>"`, `"[SECRET]"`) make log analysis harder and suggest incomplete refactoring. Grep for redaction patterns to verify consistency.
