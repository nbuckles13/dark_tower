# Principle: Input Validation

**All external inputs MUST be validated at system boundaries.** Fail fast, reject early, trust nothing from external sources.

**ADRs**: ADR-0002 (No-Panic), ADR-0003 (Service Auth), ADR-0005 (Integration Testing), ADR-0006 (Fuzz Testing)

---

## DO

### Boundary Validation
- **Validate at system boundaries** - API handlers, message receivers, all external inputs before processing
- **Reject invalid input immediately** - return errors instead of silent fallbacks or defaults
- **Validate "internal" services too** - defense-in-depth, all inputs get validated

### Size Limits
- **Enforce size limits BEFORE parsing** - check `len()` before base64 decode, JSON parse, etc.
- **Limit all variable-length inputs** - strings, arrays, request bodies, file uploads
- **Use constants for limits** - `MAX_JWT_SIZE_BYTES`, `MAX_BODY_SIZE`, etc.

### Type System
- **Parse, don't validate** - convert strings into typed wrappers (`ClientId`, `UserId`, `Scope`)
- **Use enums for known values** - `ServiceType` enum instead of freeform strings
- **Use `#[serde(deny_unknown_fields)]`** - reject unexpected JSON fields

### Character Validation
- **Whitelist allowed characters** - define exact character sets per field type
- **Validate formats early** - UUIDs, timestamps, base64 before any use
- **Check numeric ranges** - timestamps, ports, counts within valid bounds

---

## DON'T

### Trust
- **NEVER trust client-provided data** - always validate and parse into typed values
- **NEVER skip validation for "internal" calls** - all inputs need validation

### SQL
- **NEVER use string concatenation for SQL** - use sqlx parameterized queries exclusively
- **NEVER pass user input to shell commands** - use APIs instead

### Errors
- **NEVER `unwrap()` on user input** - return proper errors for invalid input
- **NEVER leak information in validation errors** - use consistent messages ("invalid credentials" not "user not found")
- **NEVER normalize input silently** - reject invalid format, don't auto-correct

### Resources
- **NEVER allow unbounded input sizes** - always enforce limits before allocation

---

## Quick Reference

| Field Type | Max Length | Allowed Characters |
|------------|------------|-------------------|
| client_id | 64 | `[a-zA-Z0-9_-]` |
| scope | 256 | `[a-zA-Z0-9:_-]` (colon-separated) |
| key_id | 64 | `[a-zA-Z0-9-]` |
| JWT token | 8KB | base64 |
| Request body | varies | JSON/Protobuf |

| Numeric | Valid Range | Notes |
|---------|-------------|-------|
| `iat` (issued-at) | now ± 5 min | clock skew tolerance |
| `exp` (expiry) | future, ≤24hr | must be > iat |
| port | 1-65535 | network config |
| pagination | 1-100 | limit batch sizes |

---

## Guards

**Compile-Time**:
- `sqlx::query!` - parameterization enforced
- Strong typing - parse into typed wrappers
- `#[serde(deny_unknown_fields)]`

**Runtime**:
- Size limits before allocation
- Enum parsing with `FromStr`
- Rate limiting on auth endpoints

**Tests** (P0/P1):
- SQL injection prevention
- Fuzz tests for all parsers (JWT, Protobuf, base64, JSON)
