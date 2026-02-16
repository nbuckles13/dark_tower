# DRY Reviewer Checkpoint

**Date**: 2026-01-16
**Task**: GC Integration Tests - Iteration 2 Review
**Verdict**: REQUEST_CHANGES

## Blocking Findings

### BLOCKING-1: `build_pkcs8_from_seed()` already exists in `ac-test-utils`

| Pattern | Location in meeting_tests.rs | Existing Location | Action Required |
|---------|------------------------------|-------------------|-----------------|
| `build_pkcs8_from_seed()` | Lines 143-168 | `crates/ac-test-utils/src/crypto_fixtures.rs` lines 69-103 | Use existing `ac_test_utils::crypto_fixtures::build_pkcs8_from_seed()` (note: currently private, may need to be made public) |

**Details**: The `build_pkcs8_from_seed()` function is copied verbatim from the `ac-test-utils` crate. This function already exists in a shared test utilities crate. The GC tests should depend on `ac-test-utils` and reuse this function.

**Note**: The function is currently private in `ac-test-utils`. Resolution options:
1. Make it public in `ac-test-utils` and add dependency
2. Move it to `crates/common` as a test-only utility if it should be more broadly shared

## Tech Debt (TECH_DEBT - non-blocking)

| Pattern | Location | Existing Location | Follow-up |
|---------|----------|-------------------|-----------|
| `TestClaims` struct | `meeting_tests.rs:42-50`, `auth_tests.rs:25-33` | Both in GC tests | Consider extracting to shared GC test module when there are 3+ test files |
| `TestKeypair` struct + `new()`, `sign_token()`, `jwk_json()` | `meeting_tests.rs:52-140`, `auth_tests.rs:35-83` | Both in GC tests | Consider extracting base `TestKeypair` to shared module; meeting_tests adds security attack methods |
| Test server setup patterns | `TestMeetingServer`, `TestAuthServer` | Both in GC tests | Consider shared `GcTestServer` base trait when patterns stabilize |
| Database fixture helpers | `meeting_tests.rs:355-454` | Only in meeting_tests | Document as patterns for future GC test files |

### Analysis Notes

1. **Cross-file duplication within GC tests**: The `TestClaims` and `TestKeypair` structs are duplicated between `auth_tests.rs` and `meeting_tests.rs`. This is acceptable TECH_DEBT for now since:
   - Only 2 files with duplication
   - Patterns are still evolving (meeting_tests adds security attack methods)
   - Can be refactored when a third test file is added

2. **Seed generation pattern**: The deterministic seed generation in `TestKeypair::new()` matches the pattern in `ac-test-utils`. This is acceptable because:
   - The seed generation is simple (2 lines)
   - Coupling GC tests to ac-test-utils for this would be overengineering

3. **Meeting-specific test helpers**: Functions like `create_test_org()`, `create_test_user()`, `create_test_meeting()` are meeting-specific and appropriately located in meeting_tests.rs.

## Recommendations for Future Work

1. **Immediate (blocking)**: Use or expose `build_pkcs8_from_seed()` from `ac-test-utils`
2. **When adding third GC test file**: Create `crates/global-controller/tests/common/mod.rs` with shared test utilities
3. **Consider for ac-test-utils**: Add `TestKeypair` with JWT signing to the shared test utils

## Status

Review complete. Verdict: **REQUEST_CHANGES**

The `build_pkcs8_from_seed()` function already exists in `ac-test-utils::crypto_fixtures` and should not be duplicated. This is a BLOCKING finding per ADR-0019.

**Resolution required before approval**: Either:
1. Add `ac-test-utils` as a dev-dependency and use the existing function (may require making it public)
2. Move the function to a new shared location accessible to both crates

All other duplication is categorized as TECH_DEBT and documented for future consolidation.
