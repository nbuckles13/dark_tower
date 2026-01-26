# DRY Review - ADR-0023 Phase 6a MC Foundation

**Reviewer**: DRY Reviewer
**Date**: 2026-01-25
**Verdict**: APPROVED

## Executive Summary

The Phase 6a implementation follows established patterns from existing services (AC, GC) with acceptable variation for service-specific needs. No BLOCKER duplication found. Several TECH_DEBT items identified for future consolidation.

## Files Reviewed

### New Files
- `crates/meeting-controller/src/lib.rs`
- `crates/meeting-controller/src/config.rs`
- `crates/meeting-controller/src/errors.rs`
- `crates/mc-test-utils/src/lib.rs`
- `crates/mc-test-utils/src/mock_gc.rs`
- `crates/mc-test-utils/src/mock_mh.rs`
- `crates/mc-test-utils/src/mock_redis.rs`
- `crates/mc-test-utils/src/fixtures/mod.rs`
- `crates/mc-test-utils/Cargo.toml`
- `crates/meeting-controller/Cargo.toml`

### Comparison References
- `crates/global-controller/src/config.rs`
- `crates/ac-service/src/config.rs`
- `crates/gc-test-utils/`
- `crates/ac-test-utils/`

---

## Findings

### TECH_DEBT-001: Config Module Pattern Duplication
**Severity**: TECH_DEBT
**Files**: `mc/config.rs`, `gc/config.rs`, `ac/config.rs`

**Pattern**: All three service configs follow the same structure:
- `Config` struct with custom `Debug` impl for redaction
- `ConfigError` enum with `MissingEnvVar` and specific validation errors
- `from_env()` + `from_vars()` pattern
- Test helper `base_vars()` function

**Observation**: This is a well-established pattern across services. Each service has different fields so complete unification isn't appropriate, but a config builder macro or trait could reduce boilerplate.

**Recommendation**: Consider extracting common patterns to `common::config` in a future refactor:
- `ConfigBuilder` trait
- `#[derive(ConfigEnv)]` macro for common env var loading
- `RedactedDebug` derive macro

**Not a blocker because**: The pattern works well, is consistent, and unification would require significant macro infrastructure. Each service legitimately has different required fields.

---

### TECH_DEBT-002: Instance ID Generation Pattern
**Severity**: TECH_DEBT
**Files**: `mc/config.rs` (lines 200-205), `gc/config.rs` (lines 199-206)

**Duplicated Code**:
```rust
// MC version
let mc_id = vars.get("MC_ID").cloned().unwrap_or_else(|| {
    let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
    let uuid_suffix = uuid::Uuid::new_v4().to_string();
    let short_suffix = uuid_suffix.get(..8).unwrap_or("00000000");
    format!("{DEFAULT_MC_ID_PREFIX}-{hostname}-{short_suffix}")
});

// GC version (identical pattern)
let gc_id = vars.get("GC_ID").cloned().unwrap_or_else(|| {
    let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
    let uuid_suffix = uuid::Uuid::new_v4().to_string();
    let short_suffix = uuid_suffix.get(..8).unwrap_or("00000000");
    format!("{}-{}-{}", DEFAULT_GC_ID_PREFIX, hostname, short_suffix)
});
```

**Recommendation**: Extract to `common::config::generate_instance_id(prefix: &str, env_key: &str, vars: &HashMap) -> String`

**Not a blocker because**: The code is only ~6 lines and appears in exactly 2 places. Simple refactor when MH adds similar pattern.

---

### TECH_DEBT-003: Test Utils Crate Structure
**Severity**: TECH_DEBT
**Files**: `mc-test-utils/src/lib.rs`, `gc-test-utils/src/lib.rs`, `ac-test-utils/src/lib.rs`

**Pattern**: All three test utility crates follow similar structure with different module compositions:
- `mc-test-utils`: mock_gc, mock_mh, mock_redis, fixtures
- `gc-test-utils`: server_harness only
- `ac-test-utils`: crypto_fixtures, assertions, token_builders, etc.

**Observation**: Test utility patterns diverge based on testing needs. MC needs mocks for its dependencies (GC, MH, Redis). GC needs a real server harness. AC needs crypto fixtures.

**Recommendation**: No action needed - the variation is appropriate for each service's testing domain.

---

### TECH_DEBT-004: Cargo.toml Comment Pattern
**Severity**: TECH_DEBT (cosmetic)
**Files**: `mc-test-utils/Cargo.toml`, `gc-test-utils/Cargo.toml`, `ac-test-utils/Cargo.toml`

**Duplicated Text** (verbatim in all three):
```toml
# Note: {service}-test-utils is a test utility library providing assertion helpers.
# Test assertion methods intentionally use unwrap/expect/panic to fail tests.
# Unlike production crates, this crate does NOT inherit workspace lints because
# test utilities need to use unwrap/expect/panic for assertions.
```

**Recommendation**: Document this pattern once in CONTRIBUTING.md or workspace README, reference in Cargo.toml comments.

**Not a blocker because**: Cosmetic documentation duplication that aids understanding of each crate in isolation.

---

### TECH_DEBT-005: Mock Builder Pattern
**Severity**: TECH_DEBT
**Files**: `mc-test-utils/src/mock_gc.rs`, `mc-test-utils/src/mock_mh.rs`

**Pattern**: Both mocks use builder pattern with `builder()`, specific methods, and `build()`.

**Observation**: This is a good pattern that will likely be reused for `mock_webtransport` and potentially mock AC. The pattern is consistent.

**Recommendation**: When implementing more mocks, consider extracting a `MockBuilder` trait to ensure consistency. Not needed now with only 2 mocks.

---

### APPROVED: Config Field Differences Are Appropriate

The MC config has unique fields not shared with GC or AC:
- `redis_url` (MC uses Redis, GC uses PostgreSQL)
- `webtransport_bind_address` (WebTransport is MC-specific)
- `binding_token_ttl_seconds`, `clock_skew_seconds`, `nonce_grace_window_seconds` (ADR-0023 session binding)
- `disconnect_grace_period_seconds` (participant timeout)
- `max_meetings`, `max_participants` (capacity limits)

These are legitimate domain differences, not copy-paste duplication.

---

### APPROVED: Error Types Are Service-Specific

`McError` has domain-specific variants:
- `SessionBinding(SessionBindingError)` - unique to MC
- `MeetingNotFound`, `ParticipantNotFound` - meeting domain
- `Draining`, `Migrating`, `FencedOut` - MC lifecycle states

These map appropriately to the signaling `ErrorCode` enum and cannot be shared with AC/GC.

---

### APPROVED: Proto Additions Follow Established Patterns

New proto messages (MuteRequest, HostMuteRequest, etc.) follow existing signaling.proto conventions:
- Field numbering continues from existing messages
- Comments reference ADR-0023
- Enum usage consistent with existing patterns

---

## Summary

| Severity | Count | Description |
|----------|-------|-------------|
| BLOCKER | 0 | - |
| TECH_DEBT | 5 | Config pattern, instance ID, test utils structure, Cargo.toml comments, mock builder |

**Per ADR-0019**: Only BLOCKER findings block merges. TECH_DEBT items are documented for future consolidation.

## Verdict: APPROVED

The implementation correctly follows established patterns with appropriate service-specific variations. The identified tech debt is low-impact and can be addressed in future refactoring when patterns stabilize across all services.
