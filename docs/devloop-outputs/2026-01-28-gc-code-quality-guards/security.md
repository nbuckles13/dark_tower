# Security Review: GC Code Quality Guards

**Date**: 2026-01-28
**Reviewer**: Security Specialist
**Scope**: Global Controller code quality fixes (error hiding + instrument violations)
**Changes**: 7 error hiding fixes, 16 instrument skip-all fixes

---

## Summary

**Verdict**: APPROVED

The code quality refactoring maintains strong security posture. Error context preservation is done safely with server-side logging only - no sensitive data is exposed to clients. The `#[instrument]` changes use explicit field allowlists rather than `skip_all`, which is the correct privacy-by-default approach.

---

## Review Areas

### 1. Error Information Disclosure

**File**: `crates/global-controller/src/errors.rs`

**Change**: `GcError::Internal` now accepts a String parameter: `Internal(String)`

**Analysis**: SAFE
- The `IntoResponse` impl logs the internal reason server-side at `gc.internal` target
- Client-facing response returns ONLY generic message: `"An internal error occurred"`
- This pattern matches the existing `Database` and `ServiceUnavailable` error handling
- Internal error context is preserved for debugging without client exposure

**Evidence** (lines 118-126):
```rust
GcError::Internal(reason) => {
    // Log actual reason server-side, return generic message to client
    tracing::error!(target: "gc.internal", reason = %reason, "Internal error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
        "An internal error occurred".to_string(),
    )
}
```

### 2. Error Context Preservation (Multiple Files)

**Affected Files**:
- `config.rs` - JWT clock skew and rate limit parsing errors
- `handlers/meetings.rs` - UUID parsing and RNG errors
- `services/mc_client.rs` - gRPC endpoint and token format errors
- `grpc/mc_service.rs` - MC registration and heartbeat errors

**Analysis**: SAFE
- All error context is logged at appropriate levels (debug, warn, error)
- Error messages passed to `GcError::Internal(String)` contain operational context
- No sensitive data (tokens, credentials, user data) is included in error strings

**Specific Review**:

1. **config.rs**: Parse errors include the user-provided value (e.g., `"five-minutes"`) which is safe since these are config values, not secrets.

2. **handlers/meetings.rs (line 520)**: RNG failure logged server-side, error message is `"RNG failure: {e}"` - acceptable as it only contains the ring error type, no sensitive context.

3. **mc_client.rs (line 185)**: Token format error logged at error level, message is `"Invalid service token format: {e}"` - only includes the parse error, NOT the token itself.

4. **grpc/mc_service.rs**: Database errors are logged server-side with `error = %e`, but Status returned is generic `"Registration failed"` / `"Heartbeat update failed"`.

### 3. Logging Safety (Instrument Changes)

**Pattern Change**: From `#[instrument(skip_all)]` to `#[instrument(skip_all, fields(...))]` with explicit field allowlists

**Analysis**: EXCELLENT
This is the correct approach - explicit field allowlists prevent accidental logging of sensitive data.

**Review of Allowed Fields**:

| File | Function | Allowed Fields | Assessment |
|------|----------|----------------|------------|
| `jwt.rs:71` | `validate` | (none) | SAFE - no fields logged |
| `jwks.rs:129` | `get_key` | `kid` | SAFE - key ID is non-secret |
| `jwks.rs:168` | `refresh_cache` | (none) | SAFE - no fields logged |
| `middleware/auth.rs:38` | `require_auth` | `name` | SAFE - span name only |
| `ac_client.rs:160` | `request_meeting_token` | `meeting_id`, `user_id` | SAFE - IDs are non-secret |
| `ac_client.rs:194` | `request_guest_token` | `meeting_id`, `guest_id` | SAFE - IDs are non-secret |
| `mc_client.rs:146` | `assign_meeting` | `mc_endpoint`, `meeting_id`, `gc_id` | SAFE - operational data |
| `mc_assignment.rs:72,166,198,230` | Various | `meeting_id`, `region`, `gc_id` | SAFE - operational data |
| `mh_selection.rs:62` | `select_mhs_for_meeting` | `region` | SAFE - non-sensitive |
| `handlers/meetings.rs:64,193,303` | Various | `meeting_code`, `meeting_id` | SAFE - public identifiers |

**NOT Logged** (correctly excluded):
- Service tokens
- JWT tokens
- Authorization headers
- Database URLs
- Private keys
- User credentials

### 4. Secret Exposure

**mc_client.rs**: Service token is stored in `SecretString` (line 78) and accessed via `expose_secret()` only when needed for authorization header (line 181). Token is NOT logged.

**config.rs**: Database URL is redacted in Debug impl (line 72): `field("database_url", &"[REDACTED]")`

**No regressions identified**.

---

## Findings

### Blockers: 0

### Critical: 0

### Major: 0

### Minor: 0

### Tech Debt: 1

**TD-001**: Consider adding structured error codes for internal errors
- Current: `GcError::Internal("RNG failure: {e}")` uses free-form strings
- Recommendation: Consider enum-based error causes for better categorization
- Priority: Low (current approach is safe, just harder to categorize programmatically)
- Not blocking: Error context is logged server-side and not exposed to clients

---

## Conclusion

The code quality fixes are security-neutral or security-positive:

1. **Error handling**: Internal error context is correctly logged server-side while clients receive generic messages
2. **Instrument fields**: Explicit allowlists prevent accidental credential/token logging
3. **Secret protection**: Tokens remain in SecretString, database URLs remain redacted
4. **No new attack surface**: No new user inputs or external interfaces introduced

**Recommendation**: APPROVED for merge.

---

## Reflection Summary

**Date**: 2026-01-28
**Knowledge Changes**: 2 patterns added

### Patterns Added

1. **Explicit Instrument Field Allowlists for Privacy-by-Default**: The change from `skip_all` to `skip_all, fields(...)` with explicit allowlists is the correct privacy-by-default approach. This pattern generalizes across all services - when tracing functions that handle requests, explicitly list only safe identifiers (IDs, regions, etc.) and never include credentials/tokens. Added to `patterns.md`.

2. **Server-Side Error Context with Generic Client Messages**: The refactor of `GcError::Internal` to accept a String parameter demonstrates the correct pattern for error handling in security-sensitive services. Log full context server-side for debugging, return generic messages to clients to prevent information disclosure. This pattern applies to database errors, parsing errors, service communication failures. Added to `patterns.md`.

### Why These Were Added

Both patterns are **reusable across services** (GC, MC, MH, AC) and represent **architectural best practices** for privacy and information disclosure prevention. They meet the curation criteria:
- Fresh specialist would benefit: Yes - these are non-obvious patterns that require security knowledge
- Reusable: Yes - applicable to all services with tracing and error handling
- Project-specific: Yes - Dark Tower's approach to tracing and error handling
- Not covered by existing entries: Existing entries cover SecretBox, JWKS, timing attacks, but not the tracing allowlist or error context split patterns

### No Gotchas or Integration Changes

No new gotchas discovered - the implementation correctly applied existing security principles (SecretString for tokens, redacted Debug impls, generic error messages). No integration guide updates needed - the changes maintain existing security contracts.
