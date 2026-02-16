# GC Code Quality Guards - Global Controller Checkpoint

**Date**: 2026-01-28
**Specialist**: global-controller
**Task**: Fix GC code quality issues: 7 error hiding + 16 instrument skip-all violations

---

## Patterns Discovered

### 1. GcError::Internal Variant Migration
When adding context to error variants that were previously unit variants (like `GcError::Internal`), the pattern is:

```rust
// Before
#[error("Internal server error")]
Internal,

// After
#[error("Internal server error: {0}")]
Internal(String),
```

Then update all usages:
- In production code: `GcError::Internal(format!("context: {}", e))`
- In tests: `GcError::Internal(_)` or `GcError::Internal("test".to_string())`
- In match arms for status_code: `GcError::Internal(_)`

### 2. Error Hiding Fix Pattern
When fixing `.map_err(|_| ...)` violations:

```rust
// Before (error hidden)
.map_err(|_| GcError::Internal)?

// After (error preserved in context)
.map_err(|e| GcError::Internal(format!("context: {}", e)))?
```

For security-sensitive paths (like parsing user input), log the error at debug level but keep the user-facing message generic:

```rust
.map_err(|e| {
    tracing::debug!(target: "...", error = %e, "Failed to parse");
    GcError::InvalidToken("Invalid user identifier in token".to_string())
})
```

### 3. Instrument Allowlist Migration
When converting from denylist to allowlist:

```rust
// Before (denylist - BAD)
#[instrument(skip(self, request), fields(meeting_id = %meeting_id))]

// After (allowlist - GOOD)
#[instrument(skip_all, fields(meeting_id = %meeting_id))]
```

Key insight: The `fields()` clause can still reference function parameters - `skip_all` just prevents automatic capture.

---

## Gotchas Encountered

### 1. All GcError::Internal Usages Must Be Updated
Changing `Internal` from a unit variant to `Internal(String)` requires updating:
- All production code that creates the error
- All test code that matches the error (use `GcError::Internal(_)` for pattern matching)
- The `status_code()` match arm
- The `IntoResponse` match arm

### 2. Formatting Changes After map_err Edits
When adding the error parameter `|e|` to closures, the line may exceed the formatter's line length limit. Running `cargo fmt` will split these across multiple lines automatically.

### 3. Guard Violations in Other Crates Don't Block GC
The `run-guards.sh` script runs across the entire workspace. Violations in other crates (like ac-service) are not related to GC changes. Use crate-specific guard checks to verify GC is clean:
```bash
./scripts/guards/simple/no-error-hiding.sh crates/global-controller/
./scripts/guards/simple/instrument-skip-all.sh crates/global-controller/
```

### 4. Integration Tests Need DATABASE_URL
Some tests in the lib crate are actually integration tests (using `#[sqlx::test]`). Running `cargo test -p global-controller --lib` without `DATABASE_URL` set will fail those tests. Use the test script or set the env var manually.

---

## Key Decisions

### 1. Error Context Granularity
Decided to include the original error in the `Internal` variant message for debugging purposes. The `IntoResponse` implementation logs this server-side but returns a generic message to clients.

### 2. Consistent Debug-Level Logging for User Input Errors
For errors in parsing user-provided data (like UUIDs from JWT claims), we log at `debug` level to avoid log spam from invalid tokens while still preserving forensic capability.

### 3. Using format!() for Error Context
Used `format!("context: {}", e)` instead of `e.to_string()` to maintain consistent error message structure and make it easier to parse logs.

---

## Current Status

**Completed**: All 23 violations fixed

**Error Hiding Fixes (7 total)**:
1. `config.rs:136` - JWT clock skew parsing
2. `config.rs:164` - Rate limit parsing
3. `handlers/meetings.rs:507-509` - UUID parsing
4. `handlers/meetings.rs:516-518` - RNG failure
5. `services/mc_client.rs:183-186` - Header parsing
6. `grpc/mc_service.rs:191-192` - max_meetings conversion
7. `grpc/mc_service.rs:193-194` - max_participants conversion

**Instrument Skip-All Fixes (16 total)**:
1. `auth/jwt.rs:71` - validate()
2. `auth/jwks.rs:129` - get_key()
3. `auth/jwks.rs:168` - refresh_cache()
4. `handlers/meetings.rs:64` - join_meeting()
5. `handlers/meetings.rs:193` - get_guest_token()
6. `handlers/meetings.rs:303` - update_meeting_settings()
7. `middleware/auth.rs:38` - require_auth()
8. `services/ac_client.rs:160` - request_meeting_token()
9. `services/ac_client.rs:194` - request_guest_token()
10. `services/mc_assignment.rs:72` - assign_meeting()
11. `services/mc_assignment.rs:166` - end_assignment()
12. `services/mc_assignment.rs:198` - get_assignment()
13. `services/mc_assignment.rs:230` - assign_meeting_with_mh()
14. `services/mh_selection.rs:62` - select_mhs_for_meeting()
15. `services/mc_client.rs:146` - assign_meeting() (McClient method)

Plus 3 additional Internal error variant updates in ac_client.rs.

**Verification**:
- `cargo check --workspace`: PASSED
- `cargo fmt --all --check`: PASSED
- `./scripts/guards/simple/no-error-hiding.sh crates/global-controller/`: 0 violations
- `./scripts/guards/simple/instrument-skip-all.sh crates/global-controller/`: 0 violations
- `cargo test -p global-controller --lib`: 259 passed
- `cargo clippy --workspace -- -D warnings`: PASSED

---

## Reflection

This implementation followed a systematic pattern established by the MC code quality fixes. The error variant migration (Internal → Internal(String)) was straightforward following the MC precedent, but required careful attention to all usages across tests and production code.

Key insight: The allowlist tracing approach (`skip_all`) is fundamentally about fail-safe defaults - new parameters are safe by default unless explicitly opted in. This mirrors security principles like allowlist vs denylist in input validation.

The most valuable discovery was recognizing the formatter's role in code quality: running `cargo fmt` after map_err fixes ensures consistent style and prevents debates about closure formatting. This is a workflow pattern, not just a formatting concern.

**Knowledge updates**: Added 3 new patterns (error variant migration, error context preservation, tracing allowlist) and 2 new gotchas (error variant test updates, formatter behavior). These generalize well beyond this specific task and will help future specialists working on error handling or observability improvements.
