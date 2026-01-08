# Principle: Error Handling

**Production code MUST NEVER panic.** All errors must be recoverable, loggable, and return proper responses to clients.

**ADR**: ADR-0002 (No-Panic Policy)

---

## DO

### Error Propagation
- **Use `Result<T, E>` for all fallible operations** - function signatures must reflect failure modes
- **Use `?` operator** for clean error propagation up the call stack
- **Convert `Option` to `Result`** with `.ok_or()` or `.ok_or_else()` for explicit error handling
- **Use `.ok_or_else()` with lazy construction** to avoid expensive error construction unless needed
- **Map errors at API boundaries** with `.map_err()` to convert internal errors to public types

### Collection Safety
- **Use `.get(idx)` instead of `[idx]`** for slices/vectors - returns `Option` instead of panicking
- **Use `.get(&key)` instead of `[key]`** for maps - returns `Option<&V>` instead of panicking
- **Use `.first()` instead of `[0]`** for accessing first element safely

### Error Types
- **Define custom error types per crate** using `thiserror` (e.g., `AcError`, `GcError`)
- **Implement `std::error::Error` trait** via `thiserror` derive macro
- **Include context in error variants** - use struct variants with named fields
- **Log internal details, return generic messages** - don't leak sensitive info to clients
- **Use `#[from]` attribute** for automatic error conversion from source errors

### Test Code
- **Tests CAN use `.unwrap()`** for known-good test data - tests should fail fast on setup errors
- **Prefer `Result<(), E>` return types** even in tests for better error messages

---

## DON'T

### Prohibited in Production Code
- **NEVER use `.unwrap()`** - panics on `None` or `Err`
- **NEVER use `.expect("message")`** - still panics, even with a message
- **NEVER use `panic!()` or `unreachable!()`** - crashes the service
- **NEVER use index operators `[idx]` or `[key]`** on collections - panic on invalid access

### Anti-Patterns
- **DON'T ignore errors silently** - always handle or propagate
- **DON'T use `String` as error type** - lacks structure, hard to handle programmatically
- **DON'T expose internal error details to clients** - security risk
- **DON'T use `#[allow(...)]`** - use `#[expect(..., reason = "...")]` instead

---

## Quick Reference

| Prohibited | Safe Alternative |
|------------|------------------|
| `.unwrap()` | `.ok_or()`, `?`, `.unwrap_or_default()` |
| `.expect()` | `.ok_or_else()`, `?` |
| `panic!()` | `return Err(...)` |
| `unreachable!()` | exhaustive match, `Result` |
| `vec[idx]` | `vec.get(idx)` |
| `map[key]` | `map.get(&key)` |
| `vec[0]` | `vec.first()` |
| `String` error | `thiserror` enum |

---

## Guards

**Clippy lints** (workspace `Cargo.toml`):
- `unwrap_used = "deny"` - forbids `.unwrap()`
- `expect_used = "deny"` - forbids `.expect()`
- `panic = "deny"` - forbids `panic!()`
- `indexing_slicing = "warn"` - warns on `[idx]` access
