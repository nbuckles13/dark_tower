# Devloop Output: Move UserClaims to common::jwt + GC Default Scopes Fix

**Date**: 2026-02-25
**Task**: Move UserClaims struct from AC crypto to common::jwt, add internal:meeting-token to GC default scopes, update AC to use shared type
**Specialist**: auth-controller
**Mode**: Agent Teams (full)
**Branch**: `feature/meeting-create-task0`
**Duration**: ~14m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `db2c3a3245a49b3c9f544880adcc53d41b405f97` |
| Branch | `feature/meeting-create-task0` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-userclaims` |
| Implementing Specialist | `auth-controller` |
| Iteration | `1` |
| Security | `security@devloop-userclaims` |
| Test | `test@devloop-userclaims` |
| Observability | `observability@devloop-userclaims` |
| Code Quality | `code-reviewer@devloop-userclaims` |
| DRY | `dry-reviewer@devloop-userclaims` |
| Operations | `operations@devloop-userclaims` |

---

## Task Overview

### Objective
Move `UserClaims` struct from `crates/ac-service/src/crypto/mod.rs` to `crates/common/src/jwt.rs` (alongside existing `ServiceClaims`). Add `pub use common::jwt::UserClaims;` alias in AC's crypto/mod.rs. Add `"internal:meeting-token"` to `GlobalController::default_scopes()` in `crates/ac-service/src/models/mod.rs`. This is Task 0 of the "Create a Meeting" user story.

### Scope
- **Service(s)**: AC Service, Common crate
- **Schema**: No
- **Cross-cutting**: Yes (common crate affects all services)

### Debate Decision
NOT NEEDED - This is a straightforward type extraction following an established pattern (ServiceClaims already lives in common::jwt).

---

## Planning

### Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

### Approved Plan
Move `UserClaims` struct from `crates/ac-service/src/crypto/mod.rs` to `crates/common/src/jwt.rs`, add re-export in AC, add `"internal:meeting-token"` to GC default scopes. 4 files changed. All reviewers confirmed no blocking concerns.

---

## Pre-Work

None

---

## Implementation Summary

### UserClaims Extraction
| Item | Before | After |
|------|--------|-------|
| UserClaims location | `crates/ac-service/src/crypto/mod.rs:362-386` | `crates/common/src/jwt.rs` |
| Debug impl location | `crates/ac-service/src/crypto/mod.rs:388-404` | `crates/common/src/jwt.rs` (redacts sub, email, jti) |
| AC access path | Direct struct definition | `pub use common::jwt::UserClaims;` re-export |

### GC Default Scopes Fix
| Item | Before | After |
|------|--------|-------|
| GC scopes | Missing `internal:meeting-token` | Added to `GlobalController::default_scopes()` |

### Additional Changes
- Removed unused `serde::{Deserialize, Serialize}` import from AC crypto/mod.rs
- Added 3 unit tests in common::jwt (Debug redaction, serde roundtrip, Clone)
- Updated `test_service_type_scopes` test to assert new scope

---

## Files Modified

```
 .claude/TODO.md                                   |   1 +
 crates/ac-service/src/crypto/mod.rs               |  48 ++------
 crates/ac-service/src/models/mod.rs               |   5 +
 crates/common/src/jwt.rs                          | 143 ++++++++++++++++++++++
 docs/specialist-knowledge/code-reviewer/INDEX.md  |   1 +
 docs/specialist-knowledge/dry-reviewer/INDEX.md   |   4 +
 docs/specialist-knowledge/security/INDEX.md       |   2 +
 docs/specialist-knowledge/semantic-guard/INDEX.md |   1 +
 8 files changed, 165 insertions(+), 40 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `crates/common/src/jwt.rs` | Added `UserClaims` struct, custom `Debug` impl (PII redaction), 3 unit tests |
| `crates/ac-service/src/crypto/mod.rs` | Removed `UserClaims` struct + Debug impl, added re-export, removed unused serde import |
| `crates/ac-service/src/models/mod.rs` | Added `"internal:meeting-token"` to GC default scopes + test assertion |
| `.claude/TODO.md` | DRY reviewer added GC Claims tech debt entry |
| `docs/specialist-knowledge/*.md` | INDEX.md updates from reflection phase |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS
**Output**: Clean compilation, all workspace crates

### Layer 2: cargo fmt
**Status**: PASS
**Output**: No formatting issues

### Layer 3: Simple Guards
**Status**: ALL PASS (14/14)

| Guard | Status |
|-------|--------|
| api-version-check | PASS |
| grafana-datasources | PASS |
| instrument-skip-all | PASS |
| no-hardcoded-secrets | PASS |
| no-pii-in-logs | PASS |
| no-secrets-in-logs | PASS |
| no-test-removal | PASS |
| test-coverage | PASS |
| test-registration | PASS |
| test-rigidity | PASS |
| validate-application-metrics | PASS |
| validate-histogram-buckets | PASS |
| validate-infrastructure-metrics | PASS |
| validate-knowledge-index | PASS |

### Layer 4: Tests
**Status**: PASS
**Tests**: All passed, 0 failures across workspace

### Layer 5: Clippy
**Status**: PASS
**Output**: No warnings

### Layer 6: Audit
**Status**: PRE-EXISTING ONLY
**Output**: 2 pre-existing vulnerabilities (ring 0.16.20 via wtransport, rsa 0.9.10 via sqlx) — not introduced by this change

### Layer 7: Semantic Guards
**Status**: PASS
**Output**: No semantic issues found. Re-export preserves API surface, struct fields identical, serde import removal safe, scope addition correct per ADR-0020.

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

PII redaction preserved, all fields required (non-Optional), scope addition matches ADR-0020, no new attack surface.

### Test Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

All existing test paths preserved, re-export transparent, new scope tested.

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

All `#[instrument(skip_all)]` preserved, Debug redaction maintained, no metrics/traces affected.

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

ADR-0002 (no-panic): PASS. ADR-0003 (error handling): PASS. Clippy deny list: PASS. Pattern consistent with ServiceClaims extraction.

### DRY Reviewer
**Verdict**: CLEAR

**True duplication findings**: None

**Extraction opportunities** (tech debt observations):
- GC `Claims` struct in `crates/gc-service/src/auth/claims.rs` duplicates `common::jwt::ServiceClaims` (pre-existing, added to TODO.md)

### Operations Reviewer
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

No CI/CD, Docker, K8s, migration, or runtime config impact.

---

## Tech Debt

### Deferred Findings

No deferred findings — all reviewers CLEAR with 0 findings.

### Cross-Service Duplication (from DRY Reviewer)

| Pattern | New Location | Existing Location | Follow-up Task |
|---------|--------------|-------------------|----------------|
| GC Claims duplication | `crates/gc-service/src/auth/claims.rs` | `crates/common/src/jwt.rs:ServiceClaims` | Migrate GC Claims to common::jwt::ServiceClaims |

### Temporary Code (from Code Reviewer)

No temporary code detected.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `db2c3a3245a49b3c9f544880adcc53d41b405f97`
2. Review all changes: `git diff db2c3a3245a49b3c9f544880adcc53d41b405f97..HEAD`
3. Soft reset (preserves changes): `git reset --soft db2c3a3245a49b3c9f544880adcc53d41b405f97`
4. Hard reset (clean revert): `git reset --hard db2c3a3245a49b3c9f544880adcc53d41b405f97`

---

## Reflection

All teammates reported no significant INDEX.md updates needed (type extraction follows established pattern). Minor updates:
- **code-reviewer**: Added shared JWT claims location to INDEX
- **security**: Added common::jwt claims + GC default scopes seam to INDEX
- **dry-reviewer**: Added extraction reference + GC Claims tech debt to TODO.md
- **semantic-guard**: Added cross-service boundary entry for common/jwt.rs

---

## Issues Encountered & Resolutions

None — clean single-iteration implementation.

---

## Lessons Learned

1. The ServiceClaims extraction pattern (move to common, re-export in original location) is well-established and works cleanly for UserClaims.
2. Pre-existing `cargo audit` vulnerabilities (ring, rsa) are transitive dependencies — not actionable in individual tasks.
3. GC Claims duplication in `auth/claims.rs` is a known tech debt item that should be addressed as a follow-up.

---

## Appendix: Verification Commands

```bash
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
cargo test --workspace
cargo clippy --workspace --lib --bins -- -D warnings
cargo audit
```
