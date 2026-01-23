# DRY Reviewer Checkpoint: GC Meeting Assignment

**Date**: 2026-01-21
**Task**: GC should assign users to MCs via load balancing per design in ADR-0010
**Reviewer**: DRY Reviewer Specialist

## Files Reviewed

### New Files
- `crates/global-controller/src/repositories/meeting_assignments.rs`
- `crates/global-controller/src/services/mc_assignment.rs`

### Modified Files
- `crates/global-controller/src/config.rs`
- `crates/global-controller/src/models/mod.rs`
- `crates/global-controller/src/handlers/meetings.rs`
- `crates/global-controller/src/services/mod.rs`

## Cross-Service Duplication Analysis

### 1. CSPRNG Usage (ring::rand::SystemRandom)

**Location**:
- `meeting_assignments.rs:432-441` (weighted_random_select)
- `meetings.rs:512-526` (generate_guest_id)

**Similar code in AC service**:
- `ac-service/src/crypto/mod.rs:446-454` (generate_random_bytes)

**Severity**: TECH_DEBT

**Analysis**: Both GC and AC use `ring::rand::SystemRandom` for cryptographically secure random number generation. The AC service has a reusable `generate_random_bytes()` function, but GC implements its own inline random generation. This is not a BLOCKER because:
1. `generate_random_bytes()` is in ac-service, not in common
2. The use cases are different (byte array vs f64 for weighted selection)
3. The code is small and self-contained

**Recommendation**: Consider extracting CSPRNG utilities to `common` crate if more services need random generation.

---

### 2. Config Debug Redaction Pattern

**Location**: `config.rs:69-87` (GC Config Debug impl)

**Similar code in AC service**: `ac-service/src/config.rs:90-102` (AC Config Debug impl)

**Severity**: TECH_DEBT

**Analysis**: Both services implement custom `Debug` for `Config` to redact `database_url`. The pattern is identical but cannot be easily extracted because:
1. Each Config struct has different fields
2. The redaction is embedded in the Debug implementation
3. Rust macros would be needed for a generic solution

**Recommendation**: Document this as an intentional pattern in developer guidelines. No immediate action needed.

---

### 3. Error Type Patterns

**Location**: `errors.rs` (GcError)

**Similar code**: `ac-service/src/errors.rs` (AcError), `common/src/error.rs` (DarkTowerError)

**Severity**: TECH_DEBT

**Analysis**: GcError and AcError have similar variants (Database, NotFound, etc.) but with service-specific HTTP mappings and response formats. The `common::error::DarkTowerError` exists but is not used because:
1. Each service needs custom HTTP status code mappings
2. Error messages are service-specific
3. `IntoResponse` implementations differ

**Recommendation**: This is intentional per-service error handling. No change needed.

---

### 4. UUID Generation for Guest IDs

**Location**: `meetings.rs:512-526` (generate_guest_id)

**Similar patterns**:
- `ac-service/src/crypto/mod.rs` uses `generate_random_bytes()` + encoding
- `ac-service/src/services/registration_service.rs:35` uses `Uuid::new_v4()`
- `common/src/types.rs` has ID wrapper types

**Severity**: TECH_DEBT

**Analysis**: GC generates UUIDs manually from CSPRNG bytes to ensure cryptographic randomness. AC service uses `Uuid::new_v4()` for non-security-critical IDs. The GC approach is correct for guest IDs but could be extracted.

**Recommendation**: Consider adding a `common::crypto::generate_secure_uuid()` function if this pattern is needed in MC or MH.

---

### 5. Repository Pattern

**Location**: `meeting_assignments.rs` (MeetingAssignmentsRepository)

**Similar patterns**:
- `ac-service/src/repositories/service_credentials.rs`
- `ac-service/src/repositories/organizations.rs`

**Severity**: Not a finding (intentional pattern)

**Analysis**: The repository pattern is consistent across services. Each service has domain-specific repositories with sqlx compile-time checked queries. This is the correct architecture.

---

## Findings Summary

| Finding | Severity | File | Description |
|---------|----------|------|-------------|
| 1 | TECH_DEBT | meeting_assignments.rs, meetings.rs | CSPRNG usage could be centralized |
| 2 | TECH_DEBT | config.rs | Config Debug redaction pattern duplicated |
| 3 | TECH_DEBT | errors.rs | Error type patterns similar but intentionally separate |
| 4 | TECH_DEBT | meetings.rs | Secure UUID generation could be centralized |

## Verdict

**APPROVED**

**Rationale**: No BLOCKER findings. All identified patterns are TECH_DEBT level, meaning similar code exists in other services but has not been extracted to `common`. The implementation correctly:
1. Uses `common::secret` types where available (not applicable here - no secrets in these files)
2. Follows established repository patterns
3. Uses sqlx for compile-time query checking
4. Does not duplicate code that exists in `common`

The TECH_DEBT items are documented for future consideration when:
- A third service needs the same patterns
- A refactoring sprint is planned
- Cross-service utilities are being consolidated

## Recommendations for Future Work

1. **If MC or MH need CSPRNG**: Extract `common::crypto::generate_random_bytes()` and `common::crypto::generate_secure_uuid()`
2. **Documentation**: Add a "Common Patterns" section to CLAUDE.md documenting the Config Debug redaction pattern
3. **ADR consideration**: If more than 3 services end up with similar error types, consider ADR for unified error framework
