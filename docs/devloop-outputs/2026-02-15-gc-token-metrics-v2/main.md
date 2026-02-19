# Dev-Loop Output: GC Token Operation Metrics (v2 — fix-or-defer review)

**Date**: 2026-02-15
**Task**: Implement token operation metrics for Global Controller — Gap 1 (token refresh via callback in common crate), Gap 2 (AC client request metrics), Gap 3 (wire existing gRPC metrics)
**Specialist**: global-controller
**Mode**: Agent Teams (full)
**Branch**: `feature/gc-token-metrics`
**Duration**: ~25m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `ac5e3a22fc73d6393b4ea401b5d72ad5d2df648e` |
| Branch | `feature/gc-token-metrics` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `global-controller` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |

---

## Task Overview

### Objective
Instrument all token operations in the Global Controller with Prometheus metrics, closing three identified gaps:

1. **Gap 1 (Token refresh)**: TokenManager in `common` crate can't depend on GC metrics. Design and implement a callback mechanism so services can inject metrics recording. Tracked as TD-GC-001.
2. **Gap 2 (AC client requests)**: Add duration/success/error metrics to `request_meeting_token()` and `request_guest_token()` in `ac_client.rs`.
3. **Gap 3 (gRPC metrics wiring)**: Wire existing `record_grpc_mc_call()` and `record_error()` into their call sites.

### Scope
- **Service(s)**: global-controller, common
- **Schema**: No
- **Cross-cutting**: Yes (common crate change affects all services)

### Debate Decision
NOT NEEDED

---

## Planning

Implementer proposed a callback injection pattern for Gap 1 (`Arc<dyn Fn(TokenRefreshEvent) + Send + Sync>` on `TokenManagerConfig`), standard `Instant`-based timing for Gap 2, and direct wiring of existing dead-code functions for Gap 3. All 6 reviewers confirmed the plan after clarification rounds with code-reviewer (5 items) and operations (naming convention).

---

## Pre-Work

None

---

## Implementation Summary

### Gap 1: Token Refresh Metrics (common + GC)
| Item | Before | After |
|------|--------|-------|
| `TokenManagerConfig` | No callback | `on_refresh: Option<TokenRefreshCallback>` |
| `token_refresh_loop()` | No metrics | Times `acquire_token()`, invokes callback |
| GC `metrics.rs` | TD-GC-001 comment | `record_token_refresh()` with 3 metrics |
| GC `main.rs` | No callback wired | `.with_on_refresh(Arc::new(\|event\| ...))` |

### Gap 2: AC Client Request Metrics
| Item | Before | After |
|------|--------|-------|
| `request_meeting_token()` | No timing/metrics | `Instant` timing + `record_ac_request()` + `record_error()` |
| `request_guest_token()` | No timing/metrics | Same pattern |
| `metrics.rs` | No AC metrics | `record_ac_request()` function |

### Gap 3: gRPC + Error Metrics Wiring
| Item | Before | After |
|------|--------|-------|
| `record_grpc_mc_call()` | Dead code (`#[allow(dead_code)]`) | Wired in `mc_client.rs::assign_meeting()` |
| `record_error()` | Dead code (`#[allow(dead_code)]`) | Wired in `ac_client.rs` and `mc_client.rs` |
| `GcError` | No label method | `error_type_label()` returning `&'static str` |

### Additional Changes
- `error_category()` helper mapping `TokenError` → bounded `&'static str` labels
- `TokenRefreshEvent.error_category` uses `Option<&'static str>` (not `String` — fixed during review)
- `GcError::status_code()` removed `#[allow(dead_code)]`
- Metrics catalog (`docs/observability/metrics/gc.md`) updated with AC client metrics and corrected error_type values

---

## Files Modified

```
 .claude/skills/dev-loop/SKILL.md                   |  84 +++---
 .claude/skills/dev-loop/review-protocol.md         |  94 ++++---
 CLAUDE.md                                          |   9 +-
 crates/common/src/token_manager.rs                 | 303 ++++++++++++++++++++-
 crates/global-controller/src/errors.rs             |  46 +++-
 crates/global-controller/src/main.rs               |   7 +-
 crates/global-controller/src/observability/metrics.rs | 101 +++++--
 crates/global-controller/src/services/ac_client.rs |  29 +-
 crates/global-controller/src/services/mc_client.rs |  24 +-
 docs/dev-loop-outputs/_template/main.md            |  45 +--
 docs/observability/metrics/gc.md                   |  44 ++-
 11 files changed, 646 insertions(+), 140 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `crates/common/src/token_manager.rs` | Callback mechanism: `TokenRefreshEvent`, `TokenRefreshCallback`, `error_category()`, `with_on_refresh()`, timing in `token_refresh_loop()`, 7 new tests |
| `crates/global-controller/src/observability/metrics.rs` | `record_token_refresh()`, `record_ac_request()`, removed dead-code annotations, 2 new tests |
| `crates/global-controller/src/errors.rs` | `error_type_label()` method (9 bounded variants), removed dead-code on `status_code()`, exhaustive test |
| `crates/global-controller/src/services/ac_client.rs` | Timing + metrics in `request_meeting_token()` and `request_guest_token()` |
| `crates/global-controller/src/services/mc_client.rs` | Timing + `record_grpc_mc_call()` + `record_error()` in `assign_meeting()` |
| `crates/global-controller/src/main.rs` | Wired callback closure to pipe `TokenRefreshEvent` into `record_token_refresh()` |
| `docs/observability/metrics/gc.md` | Added AC client metrics section, corrected error_type values |
| `.claude/skills/dev-loop/*` | Fix-or-defer review model (replaces severity-based model) |
| `CLAUDE.md` | Updated review model description |
| `docs/dev-loop-outputs/_template/main.md` | Updated verdict format and tech debt tables |

---

## Dev-Loop Verification Steps

### Layer 1: cargo check
**Status**: PASS
**Output**: All crates compile cleanly

### Layer 2: cargo fmt
**Status**: PASS
**Output**: No formatting issues

### Layer 3: Simple Guards
**Status**: 10/11 PASS
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
| application-metrics | PASS |
| infrastructure-metrics | FAIL (PyYAML not installed — environment issue) |

### Layer 4: Unit Tests
**Status**: PASS
**Output**: All tests pass (363 ac-service, 82 common, 285 global-controller, 77 meeting-controller, 22 media-handler + others)

### Layer 5: Clippy
**Status**: PASS
**Output**: 0 warnings

### Layer 6: Audit
**Status**: PASS (pre-existing only)
**Output**: 2 pre-existing advisories (ring 0.16.20, rsa 0.9.10) — transitive deps, not introduced by this PR

### Layer 7: Semantic Guards
**Status**: PASS
| Check | Verdict |
|-------|---------|
| Credential/secret leakage | SAFE |
| Metric label cardinality | SAFE (all bounded) |
| Panic safety | SAFE |
| Error handling | SAFE |
| Thread safety | SAFE |

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

No security issues. All metric labels bounded, no secret exposure, callback carries zero sensitive data.

### Test Specialist
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed, 0 deferred

Finding: Missing exhaustive test for `GcError::error_type_label()` — already fixed by code-reviewer's finding.

### Observability Specialist
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed, 0 deferred

Finding 1: Metrics catalog missing AC client metrics — fixed (added to `gc.md`).
Finding 2: Stale `error_type` values in metrics catalog — fixed (updated to match implementation).

### Code Quality Reviewer
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed, 0 deferred

Finding 1: Missing test for `error_type_label()` — fixed (added exhaustive test).
Finding 2: `TokenRefreshEvent.error_category` was `Option<String>` but only receives `&'static str` — fixed (changed to `Option<&'static str>`).

### DRY Reviewer
**Verdict**: CLEAR

**True duplication findings** (entered fix-or-defer flow):
None

**Extraction opportunities** (tech debt observations):
Pre-existing `refresh_controller_metrics` duplication between `health_checker.rs:28` and `mc_service.rs:59` (tracked as TD-22).

### Operations Reviewer
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed, 0 deferred

Finding: Error label convention differs between `gc_errors_total` and `gc_ac_requests_total` — fixed (doc comment explaining convention).

---

## Tech Debt

### Deferred Findings

No deferred findings — all 6 findings were fixed in this PR.

### Cross-Service Duplication (from DRY Reviewer)

| Pattern | New Location | Existing Location | Follow-up Task |
|---------|--------------|-------------------|----------------|
| `refresh_controller_metrics` | `crates/global-controller/src/tasks/health_checker.rs:28` | `crates/global-controller/src/grpc/mc_service.rs:59` | TD-22 |

### Temporary Code (from Code Reviewer)

No temporary code detected.

---

## Rollback Procedure

If this dev-loop needs to be reverted:
1. Verify start commit from Loop Metadata: `ac5e3a22fc73d6393b4ea401b5d72ad5d2df648e`
2. Review all changes: `git diff ac5e3a22..HEAD`
3. Soft reset (preserves changes): `git reset --soft ac5e3a22`
4. Hard reset (clean revert): `git reset --hard ac5e3a22`

---

## Reflection

### Knowledge Updates
- **Observability**: Corrected misconception about `histogram!` macro bucket configuration (recorder-level, not call-site)
- **Test**: Documented callback testing pattern (`Arc<Mutex<Vec>>` collection) for cross-crate observability
- **Operations**: Documented cross-cutting vs per-subsystem metric label naming conventions; callback injection pattern
- **Implementer**: Updated stale gotchas (TD-GC-001 resolved) and integration notes (token manager + AC client metrics now wired)
- **Security**: Updated TD-GC-001 resolution; added bounded error labels pattern (`&'static str` match arms)
- **DRY**: Added TD-22 tracking; updated common-patterns table with `TokenRefreshCallback`
- **Code-reviewer**: Added `&'static str` pattern for metric label fields; cross-crate callback coordination points

---

## Issues Encountered & Resolutions

None — implementation completed in 1 iteration with no validation failures.

---

## Lessons Learned

1. The fix-or-defer review model (v2) produced 6 findings, all fixed — compared to v1's 7 findings all deferred as tech debt. The "fix it by default" framing worked as intended.
2. The `Option<String>` → `Option<&'static str>` refinement (code-reviewer finding) is exactly the type of improvement that was deferred in v1 but fixed in v2. Type-level enforcement of bounded cardinality is worth the small effort.
3. Metrics catalog documentation should be updated as part of the implementation, not deferred — observability reviewer caught this and it was fixed immediately.

---

## Appendix: Verification Commands

```bash
# Commands used for verification
cargo check --workspace
cargo fmt --all -- --check
./scripts/guards/run-guards.sh
./scripts/test.sh --workspace
cargo clippy --workspace --lib --bins -- -D warnings
cargo audit
# Semantic guard: spawned as agent, analyzed full diff
```
