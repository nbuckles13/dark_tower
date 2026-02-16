# Dev-Loop Output: Env-Tests Security Enhancements

**Date**: 2026-01-13
**Task**: Implement env-tests security validation for deployed services
**Branch**: `feature/env-tests-security`
**Duration**: Complete (implementation + reflection across sessions)

---

## Loop State (Internal)

<!-- This section is maintained by the orchestrator for state recovery after context compression. -->
<!-- Do not edit manually - the orchestrator updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Implementing Agent | `aec9aee` |
| Current Step | `complete` |
| Iteration | `1` |
| Security Reviewer | `a74e93f` |
| Infrastructure Reviewer | `a61eced` |
| Code Reviewer | `ac963ae` |

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

---

## Reflection

### What Went Well

1. **Comprehensive security test coverage**: Implemented 9 security tests covering JWKS exposure, time claims, header injection attacks (kid, jwk, jku), and NetworkPolicy enforcement.

2. **CanaryPod pattern worked cleanly**: The design of CanaryPod with `Drop`-based cleanup, `AtomicBool` for idempotency, and configurable labels proved robust. Using `std::process::Command` for kubectl was simple and effective for test code.

3. **Feature-gated tests prevent accidents**: The `#![cfg(feature = "flows")]` pattern ensures env-tests don't accidentally run in `cargo test --workspace`, which would fail without cluster infrastructure.

4. **Test pair pattern (positive/negative)**: For NetworkPolicy testing, implementing both a positive test (same namespace connectivity) and negative test (cross-namespace blocking) provides clear interpretation of results.

### What I Learned

1. **NetworkPolicy tests require label matching**: The positive test initially failed because CanaryPod had `app=canary` but AC's NetworkPolicy only allowed `app=global-controller`. Lesson: Positive tests must impersonate allowed services.

2. **Raw JSON for field checking**: For JWKS private key leakage tests, fetching raw JSON and checking for field existence is more reliable than typed deserialization, which may skip unknown fields.

3. **JWT signature validates header integrity**: Header injection attacks are automatically prevented because the JWT signature covers the header. Tampering any header field invalidates the signature, so explicit "header injection rejection" is really just "signature validation works."

4. **Synchronous subprocess in async is OK for tests**: Using `std::process::Command` (blocking) inside `async fn` works fine for tests where sequential execution is expected anyway.

### Knowledge Files Updated

- **patterns.md**: Added 5 new patterns
  - NetworkPolicy Positive/Negative Test Pair
  - Cluster-Dependent Test Structure
  - CanaryPod for In-Cluster Testing
  - JWT Header Injection Test Suite
  - JWKS Private Key Leakage Validation

- **gotchas.md**: Added 4 new gotchas
  - env-tests Feature Gates Require Explicit Flags
  - NetworkPolicy Tests Require Matching Pod Labels
  - Clippy Warns on Unused Structs in Tests
  - Synchronous kubectl in Async Context

- **integration.md**: Added 2 new notes
  - Infrastructure Specialist: env-tests Cluster Requirements
  - Security Specialist: JWT Security Test Coverage

### Future Improvements

1. **Rate limit test enhancement**: Current implementation checks metrics endpoint or warns on high thresholds. Could add test with known rate limit configuration to verify exact behavior.

2. **CanaryPod image pull policy**: Could add `--image-pull-policy=IfNotPresent` to avoid Docker Hub rate limits in CI environments.

3. **Async kubectl**: For production-quality test infrastructure, could migrate to `tokio::process::Command` or the `kube` crate for native async Kubernetes API.

### Code Quality Reviewer Reflection

**What I Observed**

This review demonstrated excellent test code quality across several dimensions:

1. **Assertion Message Quality**: The assertion messages in these tests are exemplary. Rather than just stating what failed, they explain what was expected, what the implications are, and provide debugging steps. The NetworkPolicy tests in `40_resilience.rs` are particularly good examples - they explain the three possible interpretations of test results (both pass = security gap, positive fails = misconfigured, etc.).

2. **Security Test Documentation**: Using CVE/CWE references in doc comments (e.g., CVE-2018-0114 for JWT embedded key attacks, CWE-321 for key exposure) provides an audit trail and helps future reviewers understand the specific threat being mitigated. This pattern should be adopted for all security-focused tests.

3. **Test Organization**: The feature-gated approach (`#![cfg(feature = "flows")]`) combined with `#[serial]` annotations provides clean separation between test categories and prevents race conditions. This is a pattern worth promoting.

4. **thiserror Usage**: The `CanaryError` enum in `canary.rs` is a clean example of proper error type definition - specific variants for each failure mode, clear messages, and proper `#[error(...)]` formatting.

**Knowledge Files Updated**

- **patterns.md**: Added 4 new patterns
  - Debugging-Friendly Assertion Messages
  - CVE/CWE Reference Documentation in Security Tests
  - Test Isolation with #[serial]
  - Feature-Gated Test Organization

- **gotchas.md**: Added 4 new gotchas
  - Unused Struct Fields in Test Deserialization
  - panic!() in Test Metrics Fallback
  - Magic Numbers for Timeouts in Test Infrastructure
  - Synchronous Subprocess in Async Context

**Recommendations for Future Reviews**

1. When reviewing security tests, check for CVE/CWE references in doc comments. Absence suggests either a known vulnerability test without proper documentation or a test that isn't targeting a specific attack vector.

2. For tests against shared infrastructure (cluster, database), verify `#[serial]` is applied to prevent flaky tests.

3. Assertion messages should be evaluated not just for correctness but for debugging utility. A good assertion message is a mini-runbook.

### Security Specialist Reflection

**What I Verified**

1. **JWKS Private Key Leakage Test**: Correctly checks all six RSA CRT fields (`d`, `p`, `q`, `dp`, `dq`, `qi`). For the Ed25519 keys Dark Tower uses, only `d` is relevant, but checking all fields provides defense-in-depth for any future algorithm changes. The use of raw JSON parsing is the correct approach - typed deserialization would silently ignore unexpected fields.

2. **JWT Header Injection Tests**: Comprehensive coverage of three attack surfaces:
   - `kid` injection: 7 attack vectors including path traversal, SQL injection, XSS, null byte, and header injection
   - `jwk` embedding (CVE-2018-0114): Properly validates the service doesn't trust embedded keys
   - `jku` SSRF: Tests external, internal, file protocol, and localhost vectors. Could optionally add cloud metadata endpoints (169.254.169.254) but current coverage is adequate.

3. **Signature-Based Protection**: The tests correctly demonstrate that header injection is prevented by JWT signature integrity, not input validation. Modifying any header field invalidates the signature. This is documented in test comments, which is good practice.

4. **CanaryPod Security**: No command injection risk. The implementation uses `Command::new().args([...])` pattern which passes arguments directly to the process, bypassing shell interpretation. Namespace and pod names cannot break out of their argument positions.

5. **Time-Based Claims**: 5-minute clock skew tolerance is appropriate for distributed systems. The tests correctly measure time windows with before/after timestamps rather than assuming synchronized clocks.

**Security Patterns Observed**

The test implementations demonstrate several security-aware patterns:
- **Explicit is better than implicit**: Using `expose_secret()` makes secret access auditable
- **Defense in depth**: Checking all possible private key fields even though only one is used
- **Clear failure messages**: Error messages explain the security implication (e.g., "CRITICAL SECURITY VULNERABILITY!")
- **Raw data validation**: Using untyped JSON to catch fields that typed structs would ignore

**Knowledge Files Updated**

- **patterns.md**: Added 4 new patterns
  - JWKS Private Key Field Validation
  - JWT Header Injection Test Suite
  - Security Test via Signature Integrity
  - Subprocess Command Array for Shell Injection Prevention

- **gotchas.md**: Added 5 new gotchas
  - CVE-2018-0114 - Embedded JWK in JWT Header
  - SSRF via JWT jku Header
  - Rate Limit Testing May Not Trigger
  - Clock Skew in Time-Based JWT Validation Tests
  - Typed Deserialization May Miss JWKS Leakage

**Recommendations for Future Work**

1. **Cloud metadata SSRF vectors**: Consider adding `http://169.254.169.254/` (AWS) and `http://metadata.google.internal/` (GCP) to jku SSRF tests for cloud-specific coverage.

2. **Stronger CVE-2018-0114 test**: Current test embeds a fake key and verifies rejection. An even stronger test would generate a real attacker keypair, sign the token with the attacker's private key, and embed the matching public key in `jwk`. However, this adds complexity and the current test is sufficient because:
   - If the service uses the embedded key: Signature validates but claims don't match expected issuer
   - If the service ignores the embedded key: Signature fails against the real JWKS key

3. **Rate limit test modes**: Consider adding a "strict" mode for security audits that fails (rather than warns) when rate limiting cannot be verified.

### Infrastructure Specialist Reflection

**What I Reviewed**

I performed a detailed infrastructure review of the CanaryPod implementation and NetworkPolicy tests, focusing on:
- kubectl interaction patterns and error handling
- Pod lifecycle management (deploy, wait, cleanup)
- Kubernetes best practices (labels, force delete, namespace handling)
- Drop implementation safety in async context

**Key Findings**

The implementation is solid and follows good Kubernetes patterns:

1. **Idempotent operations throughout**: Namespace creation handles "already exists" race conditions. Pod deletion uses `--ignore-not-found=true`. Cleanup tracking with `AtomicBool` prevents double-delete.

2. **Appropriate use of synchronous kubectl**: While I flagged this as "Important", it's actually the right choice for test code. Using `std::process::Command` instead of async alternatives keeps the implementation simple. Tests run sequentially with `#[serial]` anyway.

3. **Force delete is correct for test pods**: Using `--grace-period=0 --force` is appropriate since busybox canary pods don't need graceful shutdown. This speeds up test cleanup significantly.

4. **Service DNS handling is correct**: The positive test uses short DNS (same namespace), negative test uses FQDN (cross namespace). This is the correct approach - tests validate what they should validate.

**What I Learned**

1. **NetworkPolicy tests require label awareness**: The most important lesson is that NetworkPolicy selects by pod labels, not just namespace. A canary pod with default labels will be blocked by production NetworkPolicies. The solution (configurable labels via `CanaryConfig`) is elegant.

2. **Drop cleanup in Rust requires synchronous fallback**: You can't await in `Drop::drop()`. The pattern of implementing both `do_cleanup()` (sync) and `cleanup()` (async that calls sync) is the right approach.

3. **Test infrastructure code has different standards**: Synchronous subprocess calls, SeqCst atomic ordering, and simple polling loops are all acceptable in test utilities. Optimize for clarity over performance.

**Knowledge Files Created**

Created `.claude/agents/infrastructure/` with:

- **patterns.md** (9 patterns):
  - CanaryPod for NetworkPolicy Testing
  - Synchronous kubectl in Test Code
  - Idempotent Namespace Creation
  - Force Delete for Test Pods
  - Drop-based Resource Cleanup
  - Pod Labels for Test Identification
  - Service DNS in Kubernetes Tests
  - Pod Readiness Wait Loop
  - Test Namespace Cleanup Guard

- **gotchas.md** (9 gotchas):
  - Synchronous kubectl Blocks Async Executor
  - Missing Image Pull Policy on Test Pods
  - NetworkPolicy Tests Require Matching Pod Labels
  - Namespace Deletion is Async
  - Drop Cannot Await
  - kubectl exec Timeout vs Network Timeout
  - FQDN Required for Cross-Namespace Service Access
  - Pod Phase vs Container Ready
  - AtomicBool Ordering for Simple Flags

- **integration.md** (7 notes):
  - env-tests Cluster Requirements
  - Test Specialist - CanaryPod Design
  - Security Specialist - NetworkPolicy Validation
  - Operations Specialist - Test Pod Cleanup
  - Observability Specialist - Prometheus Metrics
  - Quick reference table for specialist responsibilities

**Improvement Suggestions for Future Work**

1. **Add `--image-pull-policy=IfNotPresent`**: Prevents Docker Hub rate limit issues in CI.

2. **Consider container readiness check**: For more complex test pods, check `containerStatuses[0].ready` not just pod phase.

3. **Add periodic cleanup job**: Operations runbook should include cleanup of orphaned canary pods.

4. **Document cluster prerequisites**: env-tests README should list required infrastructure (kind cluster, port-forwards, RBAC).

**Assessment**

The CanaryPod implementation is production-quality test infrastructure. The design decisions are well-reasoned, error handling is comprehensive, and the code follows Kubernetes best practices. Approved without blocking issues.
