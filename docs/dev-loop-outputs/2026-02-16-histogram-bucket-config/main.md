# Dev-Loop Output: Fix Unconfigured Histogram Buckets

**Date**: 2026-02-16
**Task**: Add SLO-aligned bucket configuration for all histograms + co-locate recorder setup with metric definitions
**Specialist**: observability
**Mode**: Agent Teams (v2) — Light
**Branch**: `feature/gc-token-metrics`
**Duration**: ~15m (2 iterations)

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
| Implementer | `observability` |
| Implementing Specialist | `observability` |
| Iteration | `2` |
| Security | `CLEAR` |
| Code Quality | `CLEAR` (iteration 1) |
| Operations | `CLEAR` |

---

## Task Overview

### Objective
1. Fix all 5 unconfigured histogram buckets flagged by validate-histogram-buckets.sh
2. Co-locate recorder setup (`init_metrics_recorder()`) with metric definitions in `observability/metrics.rs`

### Scope
- **Service(s)**: ac-service, global-controller, meeting-controller
- **Schema**: No
- **Cross-cutting**: No (each service gets its own recorder in its own metrics module)

### Debate Decision
NOT NEEDED - Straightforward bucket configuration + code relocation following established patterns.

---

## Planning

Light mode — planning gate skipped.

---

## Pre-Work

None

---

## Iteration 1: Add Missing Bucket Configurations

### AC Service (1 histogram)
| Metric | Bucket Type | Range |
|--------|-------------|-------|
| `ac_http_request_duration_seconds` | HTTP request durations | 0.005s - 10.0s |

### MC Service (4 histograms)
| Metric | Bucket Type | Range |
|--------|-------------|-------|
| `mc_gc_heartbeat_latency_seconds` | Internal latency | 0.001s - 1.0s |
| `mc_message_latency_seconds` | Internal latency | 0.001s - 1.0s |
| `mc_recovery_duration_seconds` | Recovery (HTTP-style) | 0.005s - 10.0s |
| `mc_redis_latency_seconds` | Internal latency | 0.001s - 1.0s |

### Review Results (Iteration 1)
| Reviewer | Verdict | Findings |
|----------|---------|----------|
| Security | CLEAR | 0 |
| Code Quality | CLEAR | 0 |
| Operations | CLEAR | 0 |

---

## Human Review (Iteration 2)

**Feedback**: "Metrics recorder settings are done in a different file than the metrics themselves. That seems non-ideal."

Move `init_metrics_recorder()` from routes/mod.rs (AC, GC) and main.rs (MC) into each service's `observability/metrics.rs` to co-locate bucket config with metric definitions.

---

## Iteration 2: Co-locate Recorder Setup

### Changes per service

**GC (global-controller)**:
- Moved `init_metrics_recorder()` from `routes/mod.rs` to `observability/metrics.rs`
- Updated caller in `main.rs` and test imports in gc-test-utils, auth_tests, meeting_tests
- Removed unused `PrometheusBuilder` import from routes/mod.rs

**AC (ac-service)**:
- Moved `init_metrics_recorder()` from `routes/mod.rs` to `observability/metrics.rs`
- Updated caller in `main.rs` and test imports in ac-test-utils and routes/mod.rs tests
- Removed unused `PrometheusBuilder` import from routes/mod.rs

**MC (meeting-controller)**:
- Extracted inline PrometheusBuilder block from `main.rs` into `init_metrics_recorder()` in `observability/metrics.rs`
- Added re-export in `observability/mod.rs`
- Removed unused `PrometheusBuilder` import from main.rs

**Guard script**:
- Updated `scripts/guards/common.sh` `get_recorder_file()` to check `observability/metrics.rs` first

### Review Results (Iteration 2)
| Reviewer | Verdict | Findings |
|----------|---------|----------|
| Security | CLEAR | 0 |
| Operations | CLEAR | 0 |

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/global-controller/src/observability/metrics.rs` | Received `init_metrics_recorder()` from routes/mod.rs |
| `crates/global-controller/src/routes/mod.rs` | Function removed, imports cleaned |
| `crates/global-controller/src/main.rs` | Import path updated |
| `crates/ac-service/src/observability/metrics.rs` | Received `init_metrics_recorder()` from routes/mod.rs |
| `crates/ac-service/src/routes/mod.rs` | Function removed, imports cleaned |
| `crates/ac-service/src/main.rs` | Import path updated |
| `crates/meeting-controller/src/observability/metrics.rs` | New `init_metrics_recorder()` extracted from main.rs |
| `crates/meeting-controller/src/observability/mod.rs` | Re-export added |
| `crates/meeting-controller/src/main.rs` | Inline block replaced with function call |
| `scripts/guards/common.sh` | `get_recorder_file()` checks metrics.rs first |

---

## Dev-Loop Verification Steps

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS

### Layer 3: Simple Guards
**Status**: PASS (histogram bucket guard: 15/15 configured)

### Layer 5: Clippy
**Status**: PASS

---

## Tech Debt

No deferred findings. No cross-service duplication. No temporary code.

---

## Rollback Procedure

1. Start commit: `ac5e3a22fc73d6393b4ea401b5d72ad5d2df648e`
2. Review: `git diff ac5e3a2..HEAD`
3. Soft reset: `git reset --soft ac5e3a2`
4. Hard reset: `git reset --hard ac5e3a2`
