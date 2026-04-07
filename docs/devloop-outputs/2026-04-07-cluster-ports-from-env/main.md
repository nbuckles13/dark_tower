# Devloop Output: ClusterPorts::from_env()

**Date**: 2026-04-07
**Task**: Add ClusterPorts::from_env() to read cluster URLs from environment variables
**Specialist**: test
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/adr0030-cluster-ports-from-env`
**Duration**: ~25m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `29011b3db67e4c3c6296ff8d171fb8891a85ff36` |
| Branch | `feature/adr0030-cluster-ports-from-env` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@cluster-ports-from-env` |
| Implementing Specialist | `test` |
| Iteration | `3` |
| Security | `security@cluster-ports-from-env` |
| Test | `test@cluster-ports-from-env` |
| Observability | `observability@cluster-ports-from-env` |
| Code Quality | `code-reviewer@cluster-ports-from-env` |
| DRY | `dry-reviewer@cluster-ports-from-env` |
| Operations | `operations@cluster-ports-from-env` |

---

## Task Overview

### Objective
Add `ClusterPorts::from_env()` method that reads `ENV_TEST_AC_URL`, `ENV_TEST_GC_URL`, `ENV_TEST_PROMETHEUS_URL`, `ENV_TEST_GRAFANA_URL`, `ENV_TEST_LOKI_URL` as full URLs from environment variables. Fall back to current hardcoded localhost defaults when env vars are unset. MC/MH endpoints come from GC join response, not configuration.

### Scope
- **Service(s)**: env-tests
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED - Implementation spec fully defined in ADR-0030 section "Env-Test URL Configuration"

---

## Planning

Implementer proposed refactoring `ClusterPorts` fields from `u16` ports to `String` URLs, adding `from_env()` with env var reading and validation, `parse_host_port()` for TCP health checks, and updating `ClusterConnection::new()` to use `from_env()`. All 6 reviewers confirmed the plan.

---

## Pre-Work

None

---

## Implementation Summary

### Core Changes
| Item | Before | After |
|------|--------|-------|
| `ClusterPorts` fields | `u16` port numbers | `String` full URLs |
| `ClusterPorts::default()` | Port numbers (8082, 8080, etc.) | Full URLs (`http://localhost:8082`, etc.) |
| `ClusterConnection::new()` | `ClusterPorts::default()` | `ClusterPorts::from_env()?` |
| `check_tcp_port` | `(port: u16)`, hardcoded `127.0.0.1` | `(host: &str, port: u16)`, uses `ToSocketAddrs` |
| URL validation | None | Scheme check (http/https only), credential rejection (@) |

### New Functions
- `ClusterPorts::from_env()` — reads 5 env vars, returns `Result<Self, ClusterError>`
- `parse_host_port(url: &str)` — extracts host:port from URL for TCP health checks
- `read_env_url(var_name, default)` — reads one env var with validation and logging

### Additional Changes
- Removed unused `PortForwardNotFound` error variant
- Added `UrlParseError` error variant
- Debug logging (`eprintln!`) for each URL showing source (env vs default)
- Trailing slash stripping on env var URLs

---

## Files Modified

```
crates/env-tests/src/cluster.rs | 496 ++++++++++++++++++++++++++++++++++------
```

### Key Changes by File
| File | Changes |
|------|---------|
| `crates/env-tests/src/cluster.rs` | Refactored ClusterPorts to URL strings, added from_env(), parse_host_port(), DNS-resolving TCP checks, 20 unit tests |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS

### Layer 3: Simple Guards
**Status**: 15/16 PASS (1 pre-existing failure)

| Guard | Status |
|-------|--------|
| api-version-check | PASS |
| no-hardcoded-secrets | PASS |
| no-pii-in-logs | PASS |
| no-secrets-in-logs | PASS |
| no-test-removal | PASS |
| test-coverage | PASS |
| validate-knowledge-index | FAIL (pre-existing INDEX size violations) |

### Layer 4: Unit Tests
**Status**: PASS
**Tests**: 50 passed, 0 failed

### Layer 5: Clippy
**Status**: PASS

### Layer 6: Audit
**Status**: 3 pre-existing vulnerabilities (quinn-proto, ring via wtransport)

### Layer 7: Semantic Guards
**Status**: PASS (after fixes)

Initial run found blocking bug (SocketAddr::parse doesn't do DNS resolution) and 2 warnings. All fixed in iteration 3.

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

Scheme validation, credential rejection, DNS resolution, and logging all verified correct.

### Test Specialist
**Verdict**: CLEAR
**Findings**: 3 found, 3 fixed, 0 deferred

1. Partial env var test added (test_from_env_partial)
2. IPv6 parse tests added (2 tests)
3. Trailing slash stripping added with test

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

Defaults match Kind config, debug logging present, observability tests unaffected.

### Code Quality Reviewer
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed, 0 deferred

1. `unwrap_or("<unset>")` for Loki logging — fixed
2. `pub(crate)` visibility for `parse_host_port` — fixed

### DRY Reviewer
**Verdict**: CLEAR

**True duplication findings**: None
**Extraction opportunities**: None (env-test URL config is distinct from service config)

### Operations Reviewer
**Verdict**: CLEAR
**Findings**: 1 found, 1 fixed, 0 deferred

1. Trailing slash stripping on env var URLs — fixed with test

---

## Tech Debt

### Deferred Findings

No deferred findings — all 6 findings were fixed.

### Cross-Service Duplication (from DRY Reviewer)

No cross-service duplication detected.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `29011b3db67e4c3c6296ff8d171fb8891a85ff36`
2. Review all changes: `git diff 29011b3..HEAD`
3. Soft reset (preserves changes): `git reset --soft 29011b3`
4. Hard reset (clean revert): `git reset --hard 29011b3`

---

## Reflection

All 7 teammates updated their INDEX.md files:
- Security, observability, operations trimmed to ≤75 lines
- Test and code-reviewer added pointers for `from_env()` and `parse_host_port()`
- DRY reviewer updated TODO.md with env-test portability status

---

## Issues Encountered & Resolutions

### Issue 1: Env var race condition in tests
**Problem**: `test_from_env_custom` failed under parallel execution due to env var leakage between tests
**Resolution**: Added `#[serial]` from `serial_test` to all env-var-mutating tests

### Issue 2: DNS resolution in check_tcp_port
**Problem**: `SocketAddr::parse` doesn't resolve hostnames — would fail for `host.containers.internal`
**Resolution**: Switched to `ToSocketAddrs` for DNS resolution before `connect_timeout`

### Issue 3: Duplicated validation logic
**Problem**: Both `validate_url()` and `parse_host_port()` checked scheme and credentials independently
**Resolution**: Removed `validate_url()`, unified validation in `parse_host_port()`

---

## Lessons Learned

1. Env var tests need `#[serial]` — process-global state causes flaky failures under parallel execution
2. `SocketAddr::parse` doesn't do DNS resolution — use `ToSocketAddrs` for hostname support
3. URL validation should be centralized in one function to prevent divergence
