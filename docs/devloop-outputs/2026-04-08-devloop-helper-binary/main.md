# Devloop Output: Implement devloop-helper binary (ADR-0030 Step 4)

**Date**: 2026-04-08
**Task**: Implement the devloop-helper compiled Rust binary per ADR-0030
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) - full
**Branch**: `feature/adr0030-helper-binary`
**Duration**: ~60m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `aacf94373cc87f6a216528a3fdecb2cd2fea660f` |
| Branch | `feature/adr0030-helper-binary` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-helper-binary` |
| Implementing Specialist | `infrastructure` |
| Iteration | `2` |
| Security | `security@devloop-helper-binary` |
| Test | `test@devloop-helper-binary` |
| Observability | `observability@devloop-helper-binary` |
| Code Quality | `code-reviewer@devloop-helper-binary` |
| DRY | `dry-reviewer@devloop-helper-binary` |
| Operations | `operations@devloop-helper-binary` |

---

## Task Overview

### Objective
Create the devloop-helper compiled Rust binary at `crates/devloop-helper/` that manages Kind cluster lifecycle and image builds via a Unix socket API, per ADR-0030 step 4.

### Scope
- **Service(s)**: New crate (devloop-helper), workspace Cargo.toml
- **Schema**: No
- **Cross-cutting**: No (dev tooling only)

### Debate Decision
NOT NEEDED - ADR-0030 already accepted via design debate

---

## Planning

Implementer drafted a modular approach with 7 source files. All 6 reviewers confirmed after addressing feedback on: ring vs rand for CSPRNG (DRY/security), JSONL audit log format (observability), workspace lint inheritance (code-reviewer), flock-based PID lifecycle (operations), socket-level injection tests (test), and slug validation (security).

---

## Pre-Work

None - Steps 0-3 already completed on this branch.

---

## Implementation Summary

### New Crate: `crates/devloop-helper/`

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point, PID lifecycle with flock, SIGTERM via AtomicBool, Unix socket listener, injection regression tests |
| `src/error.rs` | HelperError enum (thiserror), ValidSlug type with path traversal prevention |
| `src/protocol.rs` | Request/Response types (serde), Service enum (ac/gc/mc/mh), HelperCommand enum |
| `src/ports.rs` | Hash-preferred 200-stride port allocation, registry with flock, port map generation, template substitution |
| `src/commands.rs` | Command execution (setup/rebuild/rebuild-all/deploy/teardown) via Command::new().arg() |
| `src/auth.rs` | 32-byte CSPRNG token (ring::rand::SystemRandom), constant-time comparison with black_box |
| `src/logging.rs` | JSONL audit log with timestamp, command, args, duration_ms, exit_code, error |

### Workspace Change
- `Cargo.toml`: Added `crates/devloop-helper` to workspace members

---

## Files Modified

### Key Changes by File
| File | Changes |
|------|---------|
| `Cargo.toml` | Added devloop-helper to workspace members |
| `crates/devloop-helper/Cargo.toml` | New crate with serde, serde_json, ring, thiserror, chrono, hex, libc |
| `crates/devloop-helper/src/main.rs` | Socket listener, PID lifecycle, SIGTERM, 13 injection tests |
| `crates/devloop-helper/src/error.rs` | HelperError, ValidSlug with regex validation |
| `crates/devloop-helper/src/protocol.rs` | Request/Response, Service enum, HelperCommand parsing |
| `crates/devloop-helper/src/ports.rs` | Port allocation, registry, port map, template substitution |
| `crates/devloop-helper/src/commands.rs` | 5 commands via Command::new().arg(), container runtime detection |
| `crates/devloop-helper/src/auth.rs` | Token gen/validation with constant-time comparison |
| `crates/devloop-helper/src/logging.rs` | JSONL audit log writer |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS

### Layer 3: Simple Guards
**Status**: ALL PASS (16/16)

### Layer 4: Unit Tests
**Status**: PASS — 72 tests in devloop-helper, all workspace tests passing

### Layer 5: Clippy
**Status**: PASS (clean)

### Layer 6: Audit
**Status**: Pre-existing advisories only (quinn-proto, ring 0.16, rsa — all from existing transitive deps, none from devloop-helper)

### Layer 7: Semantic Guards
**Status**: PASS — no blocking issues, 5 minor non-blocking observations noted

---

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed, 0 deferred

- `constant_time_eq` optimizer escape — replaced `let _ = acc` with `std::hint::black_box(acc)`
- kind-config written without 0600 permissions — replaced `fs::write` with `OpenOptions::new().mode(0o600)`

### Test Specialist
**Verdict**: RESOLVED
**Findings**: 5 found, 5 fixed, 0 deferred

- Simplified handle_connection (removed unused BufReader)
- Oversized payload test now asserts non-empty response
- Added oversized valid JSON test variant
- black_box for constant-time (shared with security)
- Extracted socket_roundtrip_raw test helper

### Observability Specialist
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed, 0 deferred

- Removed misleading base_port from startup audit log
- Added --skip-observability passthrough to setup.sh call

### Code Quality Reviewer
**Verdict**: RESOLVED
**Findings**: 4 found, 3 fixed, 1 deferred

- 3 findings already fixed in prior review rounds
- `which` portability deferred (host-only dev tool targeting Linux/WSL2)

### DRY Reviewer
**Verdict**: RESOLVED

**True duplication findings**: 1 — `rand` crate used instead of `ring::rand::SystemRandom` per ADR-0027. Fixed: `rand` replaced with `ring`.

**Extraction opportunities**: None — devloop-helper is intentionally standalone with no common crate deps.

### Operations Reviewer
**Verdict**: RESOLVED
**Findings**: 5 found, 4 fixed, 1 deferred

- Audit log append mode instead of truncate
- Added MH_HEALTH_PORT and POSTGRES_PORT to shell port map
- Setup idempotency (check existing cluster before creating)
- Cleanup includes auth-token removal
- Child process timeout deferred (requires step 5 coordination with devloop.sh)

---

## Tech Debt

### Deferred Findings

| Finding | Reviewer | Location | Deferral Justification | Follow-up Task |
|---------|----------|----------|------------------------|----------------|
| `which` portability | Code Quality | `commands.rs:68` | Host-only dev tool targeting Linux/WSL2 | Consider PATH iteration if ever containerized |
| Child process timeout | Operations | `commands.rs:490` | Requires cross-component coordination with devloop.sh (step 5) | Add SIGKILL-after-timeout in step 5 |

### Cross-Service Duplication (from DRY Reviewer)

No cross-service duplication detected. Devloop-helper is intentionally standalone.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `aacf94373cc87f6a216528a3fdecb2cd2fea660f`
2. Review all changes: `git diff aacf94373cc87f6a216528a3fdecb2cd2fea660f..HEAD`
3. Soft reset (preserves changes): `git reset --soft aacf94373cc87f6a216528a3fdecb2cd2fea660f`
4. Hard reset (clean revert): `git reset --hard aacf94373cc87f6a216528a3fdecb2cd2fea660f`

---

## Reflection

All 7 teammates updated their INDEX.md files. INDEX guard passed after fixing a glob pattern in dry-reviewer's INDEX.

---

## Human Review (Iteration 2)

**Feedback**: "Stream child stdout/stderr line-by-line back over the socket as they arrive. Claude needs to see build errors, deployment failures, etc. in real time."

### Iteration 2 Implementation

Changed commands.rs to use `.spawn()` with `Stdio::piped()` instead of `.output()`. Two reader threads (stdout/stderr) send lines via mpsc channel. Main thread writes JSONL to socket. Protocol: `CommandStarted` -> `StreamLine`* -> `CommandResult`.

### Iteration 2 Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | CLEAR | 0 | 0 | 0 |
| Test | RESOLVED | 3 | 2 | 0 |
| Observability | RESOLVED | 2 | 1 | 0 |
| Code Quality | RESOLVED | 1 | 1 | 0 |
| DRY | RESOLVED | 2 | 1 | 1 (tech debt) |
| Operations | RESOLVED | 2 | 1 | 0 |

Key fixes: `now_rfc3339()` deduplication, child killed on broken pipe, unused `ok_with_data` removed, `CommandStarted` test added.

---

## Issues Encountered & Resolutions

### Issue 1: DRY-reviewer INDEX used glob pattern (Iteration 1)
**Problem**: dry-reviewer used `{main,auth,...}.rs` glob syntax which isn't a valid file path
**Resolution**: Replaced with individual file pointer

### Issue 2: Validation failed on uncommitted code (Iteration 2)
**Problem**: test.sh runs against committed code but implementer's changes were in working tree
**Resolution**: Implementer committed changes, re-validation passed

---

## Lessons Learned

1. ring::rand::SystemRandom should be used for all secret generation per ADR-0027, even in dev tooling
2. Constant-time comparison needs `std::hint::black_box` to prevent optimizer elimination
3. Audit log should append, not truncate, to preserve cross-restart history
4. Streaming stdout/stderr via mpsc channels is the cleanest pattern for concurrent pipe reading in sync Rust
5. Child processes should be killed on client disconnect to avoid invisible zombie builds
