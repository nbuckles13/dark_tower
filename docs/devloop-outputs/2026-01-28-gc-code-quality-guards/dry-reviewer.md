# DRY Reviewer Checkpoint

**Task**: Fix GC code quality issues: 7 error hiding + 16 instrument skip-all violations
**Date**: 2026-01-28
**Reviewer**: DRY Specialist

---

## Reviewed Files

- `crates/global-controller/src/errors.rs`
- `crates/global-controller/src/config.rs`
- `crates/global-controller/src/handlers/meetings.rs`
- `crates/global-controller/src/services/mc_client.rs`
- `crates/global-controller/src/services/ac_client.rs`
- `crates/global-controller/src/services/mc_assignment.rs`
- `crates/global-controller/src/services/mh_selection.rs`
- `crates/global-controller/src/grpc/mc_service.rs`
- `crates/global-controller/src/auth/jwt.rs`
- `crates/global-controller/src/auth/jwks.rs`
- `crates/global-controller/src/middleware/auth.rs`

---

## Pattern Analysis

### 1. Error Variant Pattern: `Internal(String)`

**Global Controller Pattern (after fix)**:
```rust
// crates/global-controller/src/errors.rs:53-54
#[error("Internal server error: {0}")]
Internal(String),
```

**Meeting Controller Pattern (commit 840fc35)**:
```rust
// crates/meeting-controller/src/errors.rs:76-77
#[error("Internal error: {0}")]
Internal(String),
```

**Common Crate Pattern**:
```rust
// crates/common/src/error.rs:37-38
#[error("Internal error: {0}")]
Internal(String),
```

**AC Service Pattern**:
```rust
// crates/ac-service/src/errors.rs:41-42
#[error("Internal server error")]
Internal,  // Unit variant - STILL A UNIT VARIANT
```

**Analysis**:
- GC now matches the MC pattern (`Internal(String)`)
- Both GC and MC are consistent with `common::error::DarkTowerError::Internal(String)`
- AC Service still uses `Internal` unit variant (pre-existing tech debt, not introduced by this change)
- Each service has domain-specific error enums with unique variants (GC has HTTP-specific, MC has meeting-specific, AC has auth-specific)
- Cannot share error enum across services due to domain-specific variants

**Verdict**: NOT A BLOCKER - Pattern replication is acceptable since:
1. Each service needs its own domain-specific error enum
2. `common::DarkTowerError` exists but cannot replace domain-specific errors
3. The `Internal(String)` pattern is now consistent between GC and MC (AC is pre-existing tech debt)

---

### 2. Instrument Pattern: `#[instrument(skip_all, fields(...))]`

**Global Controller Pattern (after fix)**:
```rust
// Example from handlers/meetings.rs
#[instrument(skip_all, name = "gc.handler.join_meeting", fields(meeting_id = %path.meeting_id))]
async fn join_meeting(...) { ... }
```

**Meeting Controller Pattern**:
```rust
#[instrument(skip_all, name = "mc.actor.controller", fields(mc_id = %self.mc_id))]
async fn run(mut self) { ... }
```

**AC Service Pattern**:
```rust
#[instrument(skip_all)]
pub fn sign_claims(...) { ... }
```

**Analysis**:
- All three services now use consistent `skip_all` pattern (GC and MC fixed; AC has 4 remaining violations as pre-existing tech debt)
- Service-specific naming conventions: `gc.`, `mc.`, `ac.` prefixes
- This is standard tracing idiom, not duplicated business logic
- No abstraction in `common` could simplify this - it's intrinsic to how tracing works
- Cannot macro-ize service-specific span naming

**Verdict**: NOT A FINDING - This is a standard Rust tracing pattern, not code duplication.

---

### 3. Error Preservation Pattern: `.map_err(|e| Error::variant(format!("context: {}", e)))`

**Global Controller Pattern (after fix)**:
```rust
// crates/global-controller/src/config.rs:136
.map_err(|e| GcError::BadRequest(format!("Invalid duration format: {}", e)))

// crates/global-controller/src/handlers/meetings.rs:516
.map_err(|e| GcError::Internal(format!("failed to fill CSPRNG: {e}")))

// crates/global-controller/src/grpc/mc_service.rs:191
.map_err(|e| Status::invalid_argument(format!("invalid meeting_id: {e}")))
```

**Meeting Controller Pattern (commit 840fc35)**:
```rust
// crates/meeting-controller/src/actors/meeting.rs
.map_err(|e| McError::Internal(format!("channel send failed: {}", e)))
```

**Analysis**:
- Both GC and MC now use the same error preservation pattern
- Pattern varies appropriately based on error type:
  - GC/MC: Uses service-specific error types (`GcError`, `McError`)
  - gRPC: Uses `tonic::Status` with context
- This is the standard Rust pattern for error context preservation
- No shared helper in `common` would add value - it's idiomatic Rust

**Verdict**: NOT A FINDING - Standard Rust error handling pattern.

---

### 4. IntoResponse Implementation Pattern

**Global Controller Pattern**:
```rust
// crates/global-controller/src/errors.rs:85-150
impl IntoResponse for GcError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            GcError::Internal(reason) => {
                tracing::error!(target: "gc.internal", reason = %reason, "Internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", "An internal error occurred".to_string())
            }
            // ... other variants
        };
        // Build JSON response, add headers
    }
}
```

**AC Service Pattern**:
```rust
// crates/ac-service/src/errors.rs:75-198
impl IntoResponse for AcError {
    fn into_response(self) -> Response {
        let (status, code, message, ...) = match &self {
            AcError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", "An internal error occurred".to_string(), ...),
            // ... other variants
        };
        // Build JSON response, add headers
    }
}
```

**Analysis**:
- Both GC and AC use similar `IntoResponse` implementation patterns
- The implementations are nearly identical in structure:
  1. Match on error variant
  2. Determine status code, error code, and message
  3. Build `ErrorResponse` JSON
  4. Add authentication headers (WWW-Authenticate) where appropriate
- Key differences:
  - GC logs internal errors server-side (new)
  - AC has `required_scope` and `provided_scopes` fields for scope errors
  - Header handling differs slightly (retry-after, etc.)

**TECH_DEBT Opportunity Identified**: The `ErrorResponse` JSON structure and `IntoResponse` boilerplate could potentially be extracted to `common`. However:
- Services have different extra fields (`required_scope` in AC, none in GC)
- Header handling varies by service requirements
- Extracting would add complexity for marginal benefit
- Better addressed as part of a broader error handling ADR if needed

**Verdict**: TECH_DEBT (Minor) - Document for future consideration, not blocking.

---

## Cross-Service Consistency Check

| Pattern | GC | MC | AC | common | Status |
|---------|----|----|----|---------| -------|
| `Internal(String)` | ✅ Yes | ✅ Yes | ❌ Unit variant | ✅ Yes | Consistent (AC pre-existing debt) |
| `skip_all` instrument | ✅ Yes | ✅ Yes | ⚠️ 4 remaining | N/A | Consistent (AC pre-existing debt) |
| Error preservation | ✅ Yes | ✅ Yes | ⚠️ 30 remaining | N/A | Consistent (AC pre-existing debt) |
| IntoResponse pattern | ✅ Yes | N/A (gRPC) | ✅ Yes | ❌ Not shared | Similar but service-specific |

---

## Findings

### BLOCKER: 0

No code exists in `common` that GC should be using but isn't. The changes correctly use service-specific error types.

### TECH_DEBT: 1

**TD-001: ErrorResponse/IntoResponse boilerplate**
- **Severity**: Minor
- **Location**: `crates/global-controller/src/errors.rs`, `crates/ac-service/src/errors.rs`
- **Description**: Both GC and AC have similar `IntoResponse` implementations with `ErrorResponse` JSON structures. Future consideration: extract shared error response infrastructure to `common` if more services adopt the pattern.
- **Action**: Document for future extraction during Phase 5+ when more HTTP services exist.
- **NOT BLOCKING**: Extraction would add complexity now for marginal benefit.

### Pre-Existing Tech Debt (Not Introduced by This Change)

The following are pre-existing issues in AC service (not related to this GC fix):
- AC `Internal` is still a unit variant (30 error hiding violations)
- AC has 4 instrument skip-all violations

These should be addressed in a separate AC code quality dev-loop.

---

## Verdict

**APPROVED**

The GC implementation correctly applies patterns established in MC (commit 840fc35):
1. `Internal(String)` variant with context - Consistent with MC and `common`
2. `skip_all` instrument pattern - Standard tracing idiom
3. Error preservation pattern - Standard Rust error handling

No duplication of business logic was detected. The identified tech debt (ErrorResponse pattern) is minor and not blocking.

---

## Recommendations

1. **No action required for this implementation** - All patterns are appropriate.

2. **Future consideration** (Phase 5+): When adding more HTTP services, evaluate extracting shared error response infrastructure to `common`.

3. **AC Service cleanup** (separate dev-loop): Address the 30 error hiding + 4 instrument violations in ac-service to achieve full workspace consistency.

---

## Confidence Assessment

**Confidence**: HIGH

Cross-checked against:
- MC implementation (commit 840fc35, `docs/dev-loop-outputs/2026-01-27-mc-code-quality-guards/`)
- `common::error::DarkTowerError` patterns
- AC service patterns (for comparison)
- Previous DRY review checkpoint (2026-01-27)

---

## Reflection Summary

**Knowledge Changes**: 3 entries added/updated in specialist knowledge files

**Added to patterns.md**:
1. Service Error Enum Convergence Check - GC+MC alignment on `Internal(String)`
2. Tracing Instrument Patterns Are Infrastructure - `skip_all` is not duplication

**Updated in integration.md**:
1. Acceptable Duplication Patterns - added tracing and error preservation
2. TD-9: IntoResponse/ErrorResponse Boilerplate - new tech debt entry
3. Review Checkpoint: GC Code Quality Guards - captured findings

**Key Learning**: Error enum convergence across services (GC+MC both using `Internal(String)`) is healthy architecture alignment, not problematic duplication. When services independently adopt the same pattern from `common`, this indicates successful architectural guidance. Only flag as debt if divergence occurs without justification.

**Future Application**: When AC undergoes code quality refactor, expect similar convergence to `Internal(String)`. This review establishes the precedent for evaluating error enum changes across services.
