# Code Quality Review: TokenManager (OAuth 2.0 Client Credentials)

**Reviewer**: Code Reviewer Specialist
**Date**: 2026-02-02
**Files Reviewed**:
- `crates/common/src/token_manager.rs` (832 lines)
- `crates/common/Cargo.toml` (36 lines)
- `crates/common/src/lib.rs` (22 lines)

---

## Summary

The TokenManager implementation provides OAuth 2.0 client credentials flow with automatic token refresh, exponential backoff, and thread-safe access via `tokio::sync::watch` channel. The code demonstrates excellent Rust idioms, proper error handling with custom types via `thiserror`, and good documentation. The implementation correctly uses `SecretString` for credentials and tokens, with custom `Debug` implementations that redact sensitive data. One significant issue: the code uses `unwrap_or_default()` in error handling which silently swallows error context.

---

## Positive Highlights

1. **Excellent module documentation** - The module doc comments explain features, provide usage examples, and reference ADRs (lines 1-47)
2. **Proper use of `SecretString`** - Client secret and tokens are wrapped in `SecretString` per crypto.md principles
3. **Custom Debug implementations** - Both `TokenManagerConfig` and `TokenReceiver` manually implement `Debug` with `[REDACTED]` for secrets (lines 128-138, 206-212)
4. **Builder pattern with `#[must_use]`** - Configuration methods return modified self with compiler warnings if unused (lines 142-166)
5. **Clean error types** - `TokenError` enum with `thiserror` provides structured, typed errors (lines 77-103)
6. **`tokio::sync::watch` for thread safety** - Clever use of watch channel provides efficient, non-blocking token access without `Arc<RwLock<>>` (per concurrency.md principles)
7. **Comprehensive test coverage** - Tests cover success path, retries, refresh, cloning, and error cases
8. **Exponential backoff** - Proper retry logic with capped backoff (1s -> 30s max)
9. **`#[instrument(skip_all)]` pattern not needed** - No `#[instrument]` on functions with secrets, which is correct since raw tracing would bypass `skip`
10. **Logging follows logging.md** - Logs metadata (client_id, expires_in_secs, status) not values (token contents)

---

## Findings

### 🔴 BLOCKER Issues

**None**

### 🟠 CRITICAL Issues

**None**

### 🟡 MAJOR Issues

**None**

### 🟢 MINOR Issues

1. **`unwrap_or_default()` silently swallows error body** - `token_manager.rs:443`
   - **Problem**: When AC returns 401/400, `response.text().await.unwrap_or_default()` silently discards any error if body read fails
   - **Impact**: Diagnostic information lost if body read fails (e.g., connection reset mid-response)
   - **Fix**: Use `.unwrap_or_else(|e| format!("(failed to read body: {e})"))`
   ```rust
   let body = response.text().await.unwrap_or_else(|e| format!("(failed to read body: {e})"));
   ```
   - **ADR**: ADR-0002 discourages silent error swallowing

### 💡 SUGGESTIONS

1. **Consider adding `#[instrument(skip_all)]` to public functions** - `token_manager.rs:257`
   - While the current implementation avoids logging secrets, adding `#[instrument(skip_all, fields(client_id = %config.client_id))]` to `spawn_token_manager` would provide automatic tracing spans with safe fields
   - **Why not MINOR**: Current logging is sufficient, this is stylistic improvement

2. **Document why `#[allow(dead_code)]` on OAuth response fields** - `token_manager.rs:222-227`
   - The `token_type` and `scope` fields are allowed-dead but it's not immediately clear they're part of OAuth response structure
   - Consider comment: `// Included for OAuth response compliance; may be used in future`

3. **Consider dedicated HTTP client for AC communication** - `token_manager.rs:261`
   - The current implementation creates a client per `spawn_token_manager` call
   - For services spawning multiple token managers (unlikely but possible), a shared client could be passed in
   - **Why not MINOR**: Single-client pattern is typical usage; config-based creation is cleaner API

### 📋 TECH_DEBT

1. **Magic number for connect timeout** - `token_manager.rs:263`
   - **Type**: Hardcoded value
   - **Details**: `.connect_timeout(Duration::from_secs(5))` uses inline magic number
   - **Recommendation**: Extract to `const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);` alongside other constants
   - **Action**: Add to tech debt tracking for future cleanup

---

## ADR Compliance Check

**Relevant ADRs checked**:

- ✅ **ADR-0002** (No-Panic Policy) - **COMPLIANT**
  - No `unwrap()`, `expect()`, or `panic!()` in production code
  - Tests appropriately use `#[allow(clippy::unwrap_used, clippy::expect_used)]` (line 476)
  - `#[allow(clippy::cast_possible_wrap)]` used correctly with documentation (lines 309, 372, 432)
  - `#[allow(clippy::cast_sign_loss)]` used correctly (line 376)
  - Error propagation via `Result<T, E>` throughout

- ✅ **ADR-0003** (Service Authentication) - **COMPLIANT**
  - Uses OAuth 2.0 client credentials flow as specified
  - Token endpoint matches `/api/v1/auth/service/token`
  - Form body uses `grant_type=client_credentials`
  - Parses standard OAuth response (`access_token`, `token_type`, `expires_in`, `scope`)
  - Client secret stored as `SecretString`

---

## Code Organization Assessment

**Module Structure**: Excellent
- Clear section separators with consistent formatting (Constants, Error Types, Configuration, etc.)
- Single-file implementation appropriate for self-contained functionality
- Public API minimal: `spawn_token_manager`, `TokenManagerConfig`, `TokenReceiver`, `TokenError`

**Separation of Concerns**: Good
- Configuration is separate from runtime state
- Token acquisition logic separated from refresh loop
- Error types clearly defined at top of file

**Function Size**: Good
- `token_refresh_loop` (90 lines) is at the upper limit but logically cohesive
- `acquire_token` (75 lines) handles all HTTP interaction in one place
- Helper functions could be extracted but current structure is readable

---

## Documentation Assessment

**Module-level docs**: Excellent (47 lines of documentation with examples, features, security notes)

**Public API docs**: Good
- `spawn_token_manager` has comprehensive docs including error conditions and panic guarantees
- `TokenReceiver::token()` and `changed()` documented
- Config builder methods documented

**Inline comments**: Adequate
- Cast annotations explain why casts are safe
- Could benefit from more "why" comments in refresh logic

---

## Maintainability Score

**Score: 8/10**

**Justification**:
- Clear error types with good error messages
- Well-documented public API
- Follows established patterns (builder, watch channel)
- Tests cover main functionality

**Deductions**:
- -1: One MINOR finding (silent error swallowing)
- -1: One TECH_DEBT finding (magic number)

---

## Summary Statistics

- Files reviewed: 3
- Lines reviewed: 890
- Issues found: 3 (Blocker: 0, Critical: 0, Major: 0, Minor: 1, Tech Debt: 1, Suggestions: 3)

---

## Recommendation

- [x] ⚠️ **REQUEST_CHANGES** - Must address MINOR issues before approval

---

## Next Steps

1. **REQUIRED**: Fix MINOR-001 (unwrap_or_default -> unwrap_or_else with error context)
2. **Optional**: Consider extracting connect timeout to named constant (TECH_DEBT-001)
3. **Optional**: Add comment explaining OAuth response field usage (SUGGESTION-002)

---

## Verdict Summary

```
verdict: REQUEST_CHANGES
finding_count:
  blocker: 0
  critical: 0
  major: 0
  minor: 1
  tech_debt: 1
checkpoint_exists: true
summary: Strong implementation with proper secret handling, good error types, and comprehensive tests. One MINOR issue with silent error swallowing on body read failure needs to be fixed before approval.
```
