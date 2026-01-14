# Dev-Loop Output: Env-Tests Security Enhancements

**Date**: 2026-01-13
**Task**: Implement env-tests security validation for deployed services
**Branch**: `feature/env-tests-security`
**Duration**: In progress

---

## Loop State (Internal)

<!-- This section is maintained by the orchestrator for state recovery after context compression. -->
<!-- Do not edit manually - the orchestrator updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Implementing Agent | `aec9aee` |
| Current Step | `code_review` |
| Iteration | `1` |
| Security Reviewer | `pending` |
| Infrastructure Reviewer | `pending` |
| Code Reviewer | `pending` |

---

## Task Overview

### Objective
Implement 5 security testing enhancements for the env-tests crate, validating security properties of deployed services.

### Scope
- **Service(s)**: env-tests (tests against deployed AC service)
- **Schema**: No
- **Cross-cutting**: No (contained to env-tests crate)

### Debate Decision
NOT NEEDED - Contained to env-tests crate, no cross-service impact, well-defined scope.

---

## Pre-Work

- Created branch `feature/env-tests-security` from `feature/guard-pipeline-phase1`
- Updated `.claude/TODO.md` to defer TLS testing as infrastructure concern

---

## Implementation Summary

### Items to Implement

| # | Item | Status | File |
|---|------|--------|------|
| 1 | JWKS private key leakage test | Complete | 25_auth_security.rs |
| 2 | Time-based claims validation (iat, lifetime) | Complete | 25_auth_security.rs |
| 3 | JWT header injection attacks (kid, jwk, jku) | Complete | 25_auth_security.rs |
| 4 | Rate limit smoke test | Complete | 10_auth_smoke.rs |
| 5 | NetworkPolicy tests (CanaryPod) | Complete | canary.rs + 40_resilience.rs |

### Test Count Summary
- **25_auth_security.rs**: 3 existing + 6 new = 9 tests
  - `test_jwks_no_private_key_leakage` - CWE-321 validation
  - `test_iat_claim_is_current` - Time claims validation
  - `test_token_lifetime_is_reasonable` - ADR-0007 validation
  - `test_kid_injection_rejected` - Path traversal, SQL injection
  - `test_jwk_header_injection_rejected` - CVE-2018-0114
  - `test_jku_header_injection_rejected` - URL injection
- **10_auth_smoke.rs**: 4 existing + 1 new = 5 tests
  - `test_rate_limiting_enabled` - Rate limit validation
- **40_resilience.rs**: 4 stubs + 2 new = 6 tests
  - `test_same_namespace_connectivity` - Positive NetworkPolicy test
  - `test_network_policy_blocks_cross_namespace` - Negative NetworkPolicy test

---

## Files Modified

```
 .claude/TODO.md                            |  34 ++-
 crates/env-tests/Cargo.toml                |   3 +
 crates/env-tests/src/canary.rs             | 396 +++++++++++++++++++++++++--
 crates/env-tests/tests/10_auth_smoke.rs    | 136 +++++++++
 crates/env-tests/tests/25_auth_security.rs | 424 ++++++++++++++++++++++++++++-
 crates/env-tests/tests/40_resilience.rs    | 123 ++++++++-
 6 files changed, 1083 insertions(+), 33 deletions(-)
```

---

## Dev-Loop Verification Steps

### Layer 1: cargo check
**Status**: PASS
**Output**: Compiled successfully

### Layer 2: cargo fmt
**Status**: PASS
**Output**: No formatting changes required

### Layer 3: Simple Guards
**Status**: ALL PASS
**Duration**: ~2s

| Guard | Status |
|-------|--------|
| api-version-check | PASS |
| no-hardcoded-secrets | PASS |
| no-pii-in-logs | PASS |
| no-secrets-in-logs | PASS |
| no-test-removal | PASS |
| test-coverage | PASS |

### Layer 4: Unit Tests
**Status**: PASS
**Output**: All tests passed

### Layer 5: All Tests (Integration)
**Status**: PASS
**Output**: All tests passed (env-tests feature-gated, not run in workspace tests)

### Layer 6: Clippy
**Status**: PASS
**Output**: No warnings

### Layer 7: Semantic Guards
**Status**: Pending code review

---

## Code Review Results

### Security Specialist
**Verdict**: APPROVED

Key findings:
- All private key fields (d, p, q, dp, dq, qi) checked in JWKS test
- Comprehensive header injection vectors (path traversal, SQL injection, XSS, null byte)
- CVE-2018-0114 and jku SSRF tests properly validate protection
- CanaryPod avoids command injection through proper `Command` API usage
- No blocking issues identified

### Infrastructure Specialist
**Verdict**: APPROVED (with minor suggestions)

Key findings:
- Important: Synchronous kubectl calls in async functions (acceptable for test code)
- Important: Missing `--image-pull-policy=IfNotPresent` (could add)
- Minor: Pod readiness only checks phase (sufficient for busybox/sleep)
- Proper error handling and idempotent operations throughout

### Code Quality Reviewer
**Verdict**: APPROVED

Key findings:
- Test names follow `test_<function>_<scenario>_<expected>` pattern
- Detailed assertion messages with debugging guidance
- Excellent use of `thiserror` for error types
- Good documentation with CVE/CWE references
- Proper test isolation with `#[serial]` annotations

---

## Issues Encountered & Resolutions

### Issue 1: NetworkPolicy blocking positive test
**Problem**: The positive test (`test_same_namespace_connectivity`) failed because the canary pod had `app=canary` label, but the AC service's NetworkPolicy only allows ingress from pods with `app=global-controller` label.

**Resolution**: Added `CanaryConfig` struct with configurable labels. The positive test now uses `app=global-controller` label to match the NetworkPolicy rules, validating that allowed traffic works. The negative test continues to use default `app=canary` labels to verify blocking.

### Issue 2: Unused Prometheus structs causing warnings
**Problem**: The rate limit test had unused `PrometheusResponse`, `PrometheusData`, `PrometheusResult` structs that caused clippy warnings.

**Resolution**: Removed the unused structs since the implementation checks for metric patterns in the raw response text rather than parsing JSON.

---

## Lessons Learned

1. **NetworkPolicy tests require matching labels**: When testing NetworkPolicy enforcement, positive tests must use labels that match the allowed ingress rules. The canary should impersonate an allowed service.

2. **Feature-gated tests require explicit running**: env-tests don't run with `cargo test` - they require `--features flows` or similar. Always run feature-gated tests before committing.

3. **CanaryPod needs configurable labels**: Made the canary pod labels configurable via `CanaryConfig` to support different NetworkPolicy testing scenarios.

---

## Next Steps

1. Test specialist implements security tests
2. 7-layer verification
3. Code review by Security + Infrastructure + Code reviewers
4. Reflection and knowledge capture

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/dev/iterate.sh

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=postgresql://postgres:postgres@localhost:5433/dark_tower_test cargo test --workspace
DATABASE_URL=postgresql://postgres:postgres@localhost:5433/dark_tower_test cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh <changed_files>
```
