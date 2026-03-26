# Devloop Output: Extract JWKS client + JWT validator to common crate

**Date**: 2026-03-26
**Task**: Extract `JwksClient` and generic `JwtValidator` from GC to `crates/common/`, with `JwtError` enum and wiremock tests (R-23)
**Specialist**: auth-controller
**Mode**: Agent Teams (v2)
**Branch**: `feature/meeting-join-user-story-devloop-task7`
**Duration**: ~45m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `7b89d2af36e2e5a11de2d4265262aed555cce658` |
| Branch | `feature/meeting-join-user-story-devloop-task7` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `auth-controller` |
| Implementing Specialist | `auth-controller` |
| Iteration | `2` |
| Security | `RESOLVED` |
| Test | `RESOLVED` |
| Observability | `CLEAR` |
| Code Quality | `RESOLVED` |
| DRY | `RESOLVED` |
| Operations | `CLEAR` |

---

## Task Overview

### Objective
Extract `JwksClient` and generic `JwtValidator` from `crates/gc-service/src/auth/` into `crates/common/` so both GC and MC can share JWKS fetching and JWT validation code without duplication (R-23).

### Scope
- **Service(s)**: common crate (new code), gc-service (extraction source)
- **Schema**: No
- **Cross-cutting**: Yes (common crate affects all services)

### Debate Decision
NOT NEEDED - This is a straightforward code extraction from GC to common crate, following the established pattern (ServiceClaims, UserClaims extractions). No architectural decisions needed.

---

## Planning

TBD

---

## Pre-Work

None - Task 1 (MeetingTokenClaims/GuestTokenClaims in common) is already completed.

---

## Implementation Summary

### JWKS Client + JWT Validator Extraction (R-23)
| Item | Before | After |
|------|--------|-------|
| `JwksClient` | `gc-service/src/auth/jwks.rs` | `common/src/jwt.rs` |
| `JwtValidator` | `gc-service/src/auth/jwt.rs` | `common/src/jwt.rs` |
| `verify_token<T>` | `gc-service/src/auth/jwt.rs` | `common/src/jwt.rs` |
| `Jwk`, `JwksResponse` | `gc-service/src/auth/jwks.rs` | `common/src/jwt.rs` |
| Error type | `GcError` (service-specific) | `JwtError` (generic) |
| `JwksClient::new()` | Infallible (unwrap_or_else) | Returns `Result` (ADR-0002) |

### New Additions
- `JwtError` enum with 7 variants (extends old `JwtValidationError`)
- `HasIat` trait for compile-time iat field enforcement
- `From<JwtError> for GcError` error mapping
- 32+ new tests in common (wiremock JWKS, verify_token, round-trip Ed25519)
- Constructor clamps `clock_skew_seconds` to `[0, MAX_CLOCK_SKEW]`

### GC Auth Refactoring
- `gc-service/src/auth/jwks.rs` — re-export from common
- `gc-service/src/auth/jwt.rs` — thin wrapper delegating to common
- `gc-service/Cargo.toml` — `jsonwebtoken`/`base64` moved to dev-dependencies

---

## Files Modified

```
 crates/common/Cargo.toml                           |    3 +
 crates/common/src/jwt.rs                           | 1419 +++++++++++++++++++-
 crates/gc-service/Cargo.toml                       |    7 +-
 crates/gc-service/src/auth/claims.rs               |    7 +
 crates/gc-service/src/auth/jwks.rs                 |  786 +----------
 crates/gc-service/src/auth/jwt.rs                  |  441 +-----
 crates/gc-service/src/auth/mod.rs                  |    4 +-
 crates/gc-service/src/errors.rs                    |   10 +
 crates/gc-service/src/grpc/auth_layer.rs           |   17 +-
 crates/gc-service/src/main.rs                      |   12 +-
 crates/gc-service/src/routes/mod.rs                |   11 +-
 crates/gc-service/tests/auth_tests.rs              |    3 +-
 crates/gc-service/tests/meeting_create_tests.rs    |    3 +-
 crates/gc-service/tests/meeting_tests.rs           |    3 +-
 crates/gc-test-utils/src/server_harness.rs         |    3 +-
 docs/specialist-knowledge/auth-controller/INDEX.md |   13 +-
 docs/specialist-knowledge/code-reviewer/INDEX.md   |   13 +-
 docs/specialist-knowledge/dry-reviewer/INDEX.md    |   14 +-
 docs/specialist-knowledge/observability/INDEX.md   |    4 +
 docs/specialist-knowledge/operations/INDEX.md      |   12 +-
 docs/specialist-knowledge/security/INDEX.md        |   13 +-
 docs/specialist-knowledge/test/INDEX.md            |   19 +-
 23 files changed, 1733 insertions(+), 1232 deletions(-)
```

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS

### Layer 3: Simple Guards
**Status**: ALL PASS (14/14)

### Layer 4: Tests
**Status**: PASS (all workspace tests pass, 0 failures)

### Layer 5: Clippy
**Status**: PASS

### Layer 6: Audit
**Status**: PASS (3 pre-existing advisories, none introduced by this task)

### Layer 7: Semantic Guards
**Status**: PASS

---

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed, 0 deferred

1. `validate_iat_from_token` silently skipped iat when absent — fixed: now errors with `JwtError::MalformedToken`
2. `clock_skew_seconds` negative values wrapped to huge u64 — fixed: constructor clamps to `[0, MAX_CLOCK_SKEW]`

### Test Specialist
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed, 0 deferred

1. Missing `From<JwtError> for GcError` variant coverage (3 of 7 variants untested) — fixed: all 7 variants now tested

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 0 found

### Code Quality Reviewer
**Verdict**: RESOLVED
**Findings**: 6 found, 6 fixed, 0 deferred

1. `#[allow(clippy::cast_sign_loss)]` x2 → `#[expect]` with reason (ADR-0002) — fixed
2. `validate_iat_from_token` double-parses JWT payload — fixed: replaced with `HasIat` trait
3. `#[allow(dead_code)]` on `Jwk` → removed (fields now used) — fixed
4. `#[allow(dead_code)]` on `force_refresh` → removed (method now used) — fixed
5. `#[allow(dead_code)]` on `clear_cache` → `#[expect]` with reason — fixed
6. `clock_skew_seconds` i64→u64 cast without validation — fixed (constructor clamps)

### DRY Reviewer
**Verdict**: RESOLVED
**Findings**: 0 true duplication findings

**Extraction opportunities** (tech debt observations):
- `gc-service::auth::claims::Claims` remains structurally identical to `common::jwt::ServiceClaims`

### Operations Reviewer
**Verdict**: CLEAR
**Findings**: 0 found

---

## Tech Debt

### Deferred Findings

| Finding | Reviewer | Location | Deferral Justification | Follow-up Task |
|---------|----------|----------|------------------------|----------------|
| Claims/ServiceClaims duplication | DRY, Code Quality | `crates/gc-service/src/auth/claims.rs` | Consolidation requires changing all GC code importing `crate::auth::Claims` (middleware, handlers, gRPC) — beyond JWKS extraction scope | Existing in docs/TODO.md |

### Cross-Service Duplication (from DRY Reviewer)

| Pattern | New Location | Existing Location | Follow-up Task |
|---------|--------------|-------------------|----------------|
| Claims type | `crates/gc-service/src/auth/claims.rs` | `crates/common/src/jwt.rs:ServiceClaims` | Consolidate GC Claims to use ServiceClaims |

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `7b89d2af36e2e5a11de2d4265262aed555cce658`
2. Review all changes: `git diff 7b89d2af36e2e5a11de2d4265262aed555cce658..HEAD`
3. Soft reset (preserves changes): `git reset --soft 7b89d2af36e2e5a11de2d4265262aed555cce658`
4. Hard reset (clean revert): `git reset --hard 7b89d2af36e2e5a11de2d4265262aed555cce658`

---

## Reflection

TBD

---

## Issues Encountered & Resolutions

None

---

## Lessons Learned

TBD

---

## Appendix: Verification Commands

```bash
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo audit
```
