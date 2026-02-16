# Security Specialist Review: GC Meeting Assignment

**Date**: 2026-01-21
**Task**: GC should assign users to MCs via load balancing per design in ADR-0010
**Reviewer**: Security Specialist

## Review Scope

### New Files
- `migrations/20260121000001_meeting_assignments.sql`
- `crates/global-controller/src/repositories/meeting_assignments.rs`
- `crates/global-controller/src/services/mc_assignment.rs`
- `crates/global-controller/tests/meeting_assignment_tests.rs`

### Modified Files
- `crates/global-controller/src/config.rs`
- `crates/global-controller/src/models/mod.rs`
- `crates/global-controller/src/handlers/meetings.rs`
- `crates/global-controller/src/repositories/mod.rs`
- `crates/global-controller/src/services/mod.rs`
- `crates/global-controller/tests/meeting_tests.rs`

---

## Security Checklist Results

### 1. SQL Injection Prevention - PASS

All database queries use parameterized statements via sqlx:

**meeting_assignments.rs:**
- `get_healthy_assignment`: Uses `$1`, `$2`, `$3` bind parameters
- `get_candidate_mcs`: Uses `$1`, `$2`, `$3` bind parameters
- `atomic_assign`: Uses `$1` through `$5` bind parameters
- `get_current_assignment`: Uses `$1`, `$2` bind parameters
- `end_assignment`: Uses `$1`, `$2` bind parameters
- `cleanup_old_assignments`: Uses `$1` bind parameter

**meetings.rs:**
- `find_meeting_by_code`: Uses `$1` bind parameter via `format!` for MEETING_SELECT_QUERY + `WHERE meeting_code = $1`
- `find_meeting_by_id`: Uses `$1` bind parameter
- `get_user_org_id`: Uses `$1` bind parameter
- `update_meeting_settings_in_db`: Uses `$1` through `$4` bind parameters

No string concatenation used for user-provided values in SQL queries.

### 2. Authentication Verification - PASS

**Authenticated Endpoints:**
- `join_meeting` (GET /v1/meetings/{code}): Requires `Extension(claims): Extension<Claims>` which is injected by auth middleware
- `update_meeting_settings` (PATCH /v1/meetings/{id}/settings): Requires `Extension(claims): Extension<Claims>`

**Public Endpoints:**
- `get_guest_token` (POST /v1/meetings/{code}/guest-token): Intentionally public but validates captcha_token

MC assignment only happens after authentication check passes (for authenticated users) or captcha validation (for guests).

### 3. Authorization Verification - PASS

**Meeting Join Authorization:**
- Same org users: Always allowed
- External users: Only if `allow_external_participants` is true
- Guests: Only if `allow_guests` is true
- Status check: Cancelled/ended meetings return 404

**Meeting Settings Update Authorization:**
- Only the meeting host (`created_by_user_id`) can update settings
- Non-hosts receive 403 Forbidden

**Evidence (meetings.rs lines 86-99):**
```rust
let is_same_org = user_org_id == meeting.org_id;
let is_host = meeting.created_by_user_id == user_id;

if !is_same_org && !meeting.allow_external_participants {
    warn!(...);
    return Err(GcError::Forbidden(...));
}
```

### 4. Race Condition Prevention - PASS

The `atomic_assign` function in `meeting_assignments.rs` uses a CTE (Common Table Expression) with proper atomic semantics:

1. Uses `ON CONFLICT (meeting_id, region) DO NOTHING` to handle concurrent inserts
2. The CTE structure ensures either the insert succeeds or returns null (race lost)
3. When race is lost, re-queries to get the winner's assignment
4. Database PRIMARY KEY constraint on `(meeting_id, region)` prevents duplicate assignments

**Evidence (meeting_assignments.rs lines 186-228):**
```rust
WITH ended AS (
    UPDATE meeting_assignments
    SET ended_at = NOW()
    WHERE ... AND meeting_controller_id IN (
        SELECT controller_id FROM meeting_controllers WHERE health_status != 'healthy' ...
    )
    RETURNING meeting_id
),
inserted AS (
    INSERT INTO meeting_assignments ...
    WHERE NOT EXISTS (
        SELECT 1 FROM meeting_assignments ma ...
    )
    ON CONFLICT (meeting_id, region) DO NOTHING
    RETURNING meeting_controller_id
)
SELECT meeting_controller_id FROM inserted
```

### 5. Information Disclosure Prevention - PASS

**Error Messages:**
- Generic error messages returned to clients via `GcError` types
- Database errors: "An internal database error occurred" (errors.rs line 94)
- Service unavailable: "Service temporarily unavailable" (errors.rs line 116)
- Internal errors: "An internal error occurred" (errors.rs line 121)
- Actual error details logged server-side only

**Logging:**
- Sensitive data not logged in info/warn logs
- Tracing spans redact sensitive values appropriately
- Meeting code, meeting ID, user ID, MC ID logged (appropriate for audit)

### 6. CSPRNG for Random Selection - PASS

**Load balancing random selection (meeting_assignments.rs lines 431-446):**
```rust
let rng = SystemRandom::new();
let mut random_bytes = [0u8; 8];
if rng.fill(&mut random_bytes).is_err() {
    // Fallback to first candidate if CSPRNG fails
    tracing::warn!(...);
    return candidates.first();
}
```

Uses `ring::rand::SystemRandom` which is a CSPRNG. Falls back to deterministic first candidate selection if CSPRNG fails (logged for monitoring).

**Guest ID generation (meetings.rs lines 512-526):**
```rust
fn generate_guest_id() -> Result<Uuid, GcError> {
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes).map_err(|_| {
        tracing::error!(target: "gc.handlers.meetings", "Failed to generate random bytes");
        GcError::Internal
    })?;
    // Set UUID version 4 and variant bits
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(Uuid::from_bytes(bytes))
}
```

Uses `ring::rand::SystemRandom` for cryptographically secure guest ID generation.

### 7. Input Validation - PASS

**GuestJoinRequest validation (models/mod.rs lines 193-214):**
- Display name: min 2 chars, max 100 chars, trimmed
- Captcha token: non-empty required

**Meeting code path parameter:**
- Passed directly to parameterized SQL query (no injection risk)
- Meeting not found returns 404

**Meeting ID path parameter (Uuid):**
- Axum's `Path<Uuid>` extractor performs type validation
- Invalid UUIDs rejected before handler code runs

**JWT subject parsing (meetings.rs lines 505-509):**
```rust
fn parse_user_id(sub: &str) -> Result<Uuid, GcError> {
    let uuid_str = sub.strip_prefix("user:").unwrap_or(sub);
    Uuid::parse_str(uuid_str)
        .map_err(|_| GcError::InvalidToken("Invalid user identifier in token".to_string()))
}
```

---

## Findings

### No BLOCKER Findings

### No CRITICAL Findings

### No MAJOR Findings

### No MINOR Findings

---

## Security Strengths Observed

1. **Defense in Depth**: Multiple layers of validation (middleware auth, handler checks, database constraints)
2. **Secure Defaults**: External participants disabled by default, guests disabled by default
3. **Proper CSPRNG Usage**: Using ring's SystemRandom for all security-sensitive random operations
4. **Atomic Operations**: Database operations designed to prevent race conditions
5. **Error Handling**: Generic error messages to clients, detailed logs server-side
6. **Input Validation**: Comprehensive validation with clear limits
7. **Test Coverage**: Extensive security-related tests including:
   - JWT algorithm confusion attacks (test_jwt_wrong_algorithm_returns_401)
   - JWT key substitution attacks (test_jwt_wrong_key_returns_401)
   - JWT tampering attacks (test_jwt_tampered_payload_returns_401)
   - Display name boundary tests (test_guest_token_max_display_name_boundary)
   - Concurrent request handling (test_concurrent_guest_requests_succeed)
   - Inactive user exclusion (test_join_meeting_inactive_user_denied)

---

## Verdict

**APPROVED**

The implementation follows security best practices:
- All SQL queries are parameterized (no SQL injection risk)
- Authentication is properly enforced on protected endpoints
- Authorization checks are correct (org membership, host-only settings)
- Race conditions are prevented via atomic database operations
- Error messages do not leak sensitive information
- CSPRNG is used for all security-sensitive random operations
- Input validation is comprehensive with proper bounds checking

The test suite includes comprehensive security tests covering common attack vectors.

---

## Summary

The GC meeting assignment implementation demonstrates strong security practices across all reviewed areas. The code uses parameterized queries throughout, enforces authentication and authorization correctly, handles race conditions atomically, and uses cryptographically secure random number generation. Error messages are properly sanitized to prevent information disclosure. No security findings requiring changes were identified.
