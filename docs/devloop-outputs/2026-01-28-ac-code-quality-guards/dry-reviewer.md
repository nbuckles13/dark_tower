# DRY Reviewer Checkpoint: AC Code Quality Guards

**Date**: 2026-01-29
**Reviewer**: DRY Reviewer (dry-reviewer specialist)
**Task**: Fix AC code quality violations (28 error hiding + 4 instrument skip-all)

## Summary

Reviewed the code quality fixes in AC service. The changes follow the established pattern from MC and GC fixes. While there is significant code duplication in error handling patterns across services, this is intentional per the service-specific error types architecture and does not constitute architectural duplication that would block approval.

## Files Reviewed

1. **`crates/ac-service/src/crypto/mod.rs`** - 19 error handling fixes
2. **`crates/ac-service/src/handlers/internal_tokens.rs`** - 4 error handling fixes
3. **`crates/ac-service/src/handlers/auth_handler.rs`** - 3 error handling fixes
4. **`crates/ac-service/src/config.rs`** - 2 error handling fixes

## Analysis

### Error Handling Pattern

The error handling pattern across all three services (AC, MC, GC) follows the same approach:

```rust
.map_err(|e| {
    tracing::error!(target: "...", error = %e, "Description");
    ServiceError::Variant("User-facing message".to_string())
})?
```

This pattern is consistent across:
- **AC**: `AcError::Crypto`, `AcError::InvalidToken`, `AcError::InvalidCredentials`
- **GC**: `GcError::*` variants in jwt.rs, jwks.rs, handlers/meetings.rs
- **MC**: Uses `common::DarkTowerError` (no service-specific error types yet)

### Cross-Service Patterns Observed

| Pattern | AC | GC | MC | Duplication? |
|---------|----|----|----|--------------|
| JWT verification with logging | Yes (crypto/mod.rs) | Yes (auth/jwt.rs) | No (skeleton) | Intentional - different claims types |
| Config validation with tracing | Yes (config.rs) | Yes (config.rs) | Yes (config.rs) | TECH_DEBT: Same validation logic |
| Base64 decode with error mapping | Yes (auth_handler.rs) | Yes (auth/jwt.rs) | No | Intentional - service boundary |
| PKCS8 key validation | Yes (crypto/mod.rs, internal_tokens.rs) | No | No | AC-specific |

### Detailed Findings

#### 1. JWT Clock Skew Configuration (TECH_DEBT)

**Location**: AC and GC both have nearly identical config validation:
- `crates/ac-service/src/config.rs:184-219`
- `crates/global-controller/src/config.rs:138-163`

Both implement:
- Same `DEFAULT_JWT_CLOCK_SKEW_SECONDS` constant (300)
- Same `MAX_JWT_CLOCK_SKEW_SECONDS` constant (600)
- Same validation logic (positive, under max, parse errors)

**Recommendation**: Extract to `common::config::parse_jwt_clock_skew()` utility function. This would reduce ~40 lines of duplicated validation code.

**Severity**: TECH_DEBT (non-blocking)

#### 2. Config Error Types (TECH_DEBT)

**Location**: Each service defines its own ConfigError:
- `AcError::InvalidJwtClockSkew`, `AcError::InvalidBcryptCost`
- `GcConfigError::InvalidJwtClockSkew`, `GcConfigError::InvalidRateLimit`
- `McConfigError::MissingEnvVar`, `McConfigError::InvalidValue`

These could use a shared `common::config::ConfigValidationError` with service-specific wrapping.

**Severity**: TECH_DEBT (non-blocking, affects maintainability)

#### 3. Debug Redaction Pattern (Acceptable)

All three services implement custom `Debug` for config with `[REDACTED]` for sensitive fields. This is acceptable duplication because:
- Each service has different sensitive fields
- The pattern is simple and unlikely to diverge incorrectly
- Extracting to a macro would add complexity without significant benefit

**Severity**: Not a finding (acceptable pattern)

#### 4. JWT Signing Functions in AC (Acceptable)

The `sign_meeting_jwt` and `sign_guest_jwt` functions in `internal_tokens.rs` are nearly identical (lines 267-291 and 294-318). However:
- They have different claim types (`MeetingTokenClaims` vs `GuestTokenClaims`)
- Merging would require generics that may reduce clarity
- This is internal to AC, not cross-service

**Severity**: Not a finding (internal AC decision)

### No BLOCKER Findings

All duplication identified is:
1. **Intentional** due to service isolation and type safety requirements, OR
2. **TECH_DEBT** that should be addressed but does not block this change

The code quality fixes themselves are correctly implemented and follow established patterns.

## Verdict

```
verdict: APPROVED
finding_count:
  blocker: 0
  critical: 0
  major: 0
  minor: 0
  tech_debt: 2
checkpoint_exists: true
summary: No blocking duplication. Two TECH_DEBT items identified (JWT clock skew validation and config error types) that could be extracted to common crate in future refactoring. The error handling fixes follow established patterns from MC/GC and are correctly implemented.
```

## Tech Debt Recommendations (Non-Blocking)

### TD-1: Extract JWT Clock Skew Validation
- **Effort**: Low (1-2 hours)
- **Impact**: Reduces ~40 lines of duplicated validation code
- **Suggested location**: `common::config::validate_jwt_clock_skew()`

### TD-2: Shared Config Validation Utilities
- **Effort**: Medium (half day)
- **Impact**: Unified config parsing patterns across services
- **Suggested location**: `common::config` module with `parse_positive_i64()`, `parse_bounded_u32()` helpers

These should be tracked in `.claude/TODO.md` for future cleanup sprints.

---

## Reflection Summary

**Knowledge Updates**: Added 2 patterns, 2 tech debt registry entries, 1 review checkpoint.

**Key Learning**: The error preservation pattern (`.map_err(|e| { tracing::error!(...); Error::Variant(...) })`) is now fully established across all three services (AC, MC, GC). This pattern emerged organically through parallel evolution during Phase 4 code quality work and represents architectural alignment without requiring extraction. Future reviews should recognize this as acceptable infrastructure pattern, not duplication.

**Tech Debt Identified**: Two config validation patterns (JWT clock skew validation and general config parsing) are duplicated between AC and GC. Both classified as TECH_DEBT (TD-10, TD-11) - defer extraction until third service requires the pattern or complexity increases.

**Process Observation**: The "3+ services" extraction threshold continues to serve the project well. Deferring extraction of 2-service patterns allows implementations to mature independently while avoiding premature abstraction. The config validation patterns will be reassessed when MC or MH require similar validation logic.

---

*Reviewed per ADR-0019: DRY Review Process. TECH_DEBT findings documented but non-blocking.*
