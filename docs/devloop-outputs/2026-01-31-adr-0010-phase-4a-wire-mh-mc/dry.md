# DRY Review Checkpoint

**Reviewer**: dry-reviewer
**Date**: 2026-01-31
**Status**: APPROVED

---

## Findings

### BLOCKER

None identified. All shared code properly uses existing abstractions from `common` crate.

### NON_BLOCKER

#### 1. Health Checker Task Pattern Duplication

**Pattern**: MC health checker (`health_checker.rs`) and MH health checker (`mh_health_checker.rs`)

**Locations**:
- `crates/global-controller/src/tasks/health_checker.rs:1-382` (MC version)
- `crates/global-controller/src/tasks/mh_health_checker.rs:1-321` (MH version)

**Similarity**: ~95% identical structure

**Issue**: Both health checkers follow the same pattern:
- Same `DEFAULT_CHECK_INTERVAL_SECONDS` constant (5 seconds)
- Identical `tokio::select!` loop structure
- Same graceful shutdown via `CancellationToken`
- Same error handling (log but continue on DB errors)
- Nearly identical integration test structure (4 tests each with same patterns)

**Only differences**:
- Repository method called (`mark_stale_controllers_unhealthy` vs `mark_stale_handlers_unhealthy`)
- Log targets (`gc.task.health_checker` vs `gc.task.mh_health_checker`)
- Entity names in messages ("controllers" vs "handlers")

**Recommendation**: TECH_DEBT - Create a generic health checker task in a future refactor.

Potential extraction:
```rust
// crates/global-controller/src/tasks/generic_health_checker.rs
pub async fn start_health_checker<F, Fut>(
    name: &str,
    check_interval: Duration,
    staleness_threshold: u64,
    mark_stale_fn: F,
    cancel_token: CancellationToken,
) where
    F: Fn(u64) -> Fut + Send,
    Fut: Future<Output = Result<u64, GcError>> + Send,
```

This would eliminate 300+ lines of duplicated code and centralize the health checking pattern.

---

#### 2. gRPC Service Validation Pattern Duplication

**Pattern**: Input validation methods in MC and MH gRPC services

**Locations**:
- `crates/global-controller/src/grpc/mc_service.rs:54-163` (MC validation)
- `crates/global-controller/src/grpc/mh_service.rs:46-121` (MH validation)

**Similarity**: ~85% identical validation logic

**Issue**: Both services duplicate validation patterns:

**Identical constants**:
```rust
// Both services
const MAX_REGION_LENGTH: usize = 50;
const MAX_ENDPOINT_LENGTH: usize = 255;
```

**Identical validation functions**:
- `validate_region()` - 100% identical (lines 102-115 MC, 73-86 MH)
- `validate_endpoint()` - ~95% identical (only differs in grpc:// scheme support)
- `validate_*_id()` - Same pattern, different field names

**Recommendation**: TECH_DEBT - Extract to `crates/global-controller/src/validation.rs` or `crates/common/src/grpc_validation.rs`

Potential extraction:
```rust
// crates/common/src/grpc_validation.rs
pub mod grpc_validation {
    pub const MAX_REGION_LENGTH: usize = 50;
    pub const MAX_ENDPOINT_LENGTH: usize = 255;
    pub const MAX_ID_LENGTH: usize = 255;

    pub fn validate_region(region: &str) -> Result<(), Status> { ... }
    pub fn validate_endpoint(endpoint: &str, field_name: &str, schemes: &[&str]) -> Result<(), Status> { ... }
    pub fn validate_id(id: &str, field_name: &str) -> Result<(), Status> { ... }
}
```

This would eliminate ~100 lines of duplicated validation code and centralize gRPC input validation standards.

---

#### 3. Health Status Proto Conversion Duplication

**Pattern**: Converting proto enum to `HealthStatus` enum

**Locations**:
- `crates/global-controller/src/repositories/meeting_controllers.rs:28-40` (with `from_proto()` method)
- `crates/global-controller/src/grpc/mh_service.rs:202-209` (inline match)

**Similarity**: Same logic, different implementation styles

**Issue**: MH service uses inline match for health status conversion:
```rust
// MH service (inline)
let health_status = match req.health {
    0 => HealthStatus::Pending,
    1 => HealthStatus::Healthy,
    2 => HealthStatus::Degraded,
    3 => HealthStatus::Unhealthy,
    4 => HealthStatus::Draining,
    _ => HealthStatus::Pending,  // Different default than MC!
};
```

MC service uses centralized `HealthStatus::from_proto()` method with `Unhealthy` default.

**Inconsistency**: MH defaults unknown values to `Pending`, MC defaults to `Unhealthy`.

**Recommendation**: TECH_DEBT - Standardize on `HealthStatus::from_proto()` throughout codebase. Update MH service to use the existing method. Document the security rationale for defaulting to `Unhealthy` (fail-closed).

---

#### 4. Heartbeat Interval Constants Duplication

**Pattern**: Heartbeat/report interval constants

**Locations**:
- `crates/global-controller/src/grpc/mc_service.rs:24-27` (MC intervals)
- `crates/global-controller/src/grpc/mh_service.rs:24` (MH interval)
- `crates/meeting-controller/src/grpc/gc_client.rs:42-45` (MC client intervals)

**Similarity**: Same values, different constant names

**Issue**: Heartbeat intervals are defined independently across services:
```rust
// GC → MC
const DEFAULT_FAST_HEARTBEAT_INTERVAL_MS: u64 = 10_000;
const DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL_MS: u64 = 30_000;

// GC → MH
const DEFAULT_LOAD_REPORT_INTERVAL_MS: u64 = 10_000;

// MC → GC (client side)
const DEFAULT_FAST_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
```

**Recommendation**: TECH_DEBT - Extract to protocol-level constants in `proto_gen` or `common::config`:

```rust
// crates/common/src/config.rs
pub mod heartbeat {
    pub const FAST_INTERVAL_MS: u64 = 10_000;
    pub const COMPREHENSIVE_INTERVAL_MS: u64 = 30_000;
}
```

This ensures client and server agree on heartbeat intervals and prevents drift.

---

#### 5. gRPC Client Channel Pooling Pattern

**Pattern**: gRPC channel caching with `Arc<RwLock<HashMap<String, Channel>>>`

**Location**:
- `crates/global-controller/src/services/mc_client.rs:70-122` (MC client)

**Potential duplication**: If MH client or GC-to-GC client are implemented, this pattern will be repeated.

**Recommendation**: TECH_DEBT - Monitor for duplication. If a second gRPC client is implemented with the same pattern, extract to:

```rust
// crates/common/src/grpc_client.rs
pub struct ChannelPool {
    channels: Arc<RwLock<HashMap<String, Channel>>>,
}
```

Not a blocker yet since only one client exists, but worth tracking.

---

## Tech Debt Documented

All NON_BLOCKER findings have been documented in `.claude/TODO.md` under a new "Cross-Service Duplication (DRY)" section.

---

## Verdict

**APPROVED** - No blocking issues found. All shared code properly uses existing abstractions.

**Summary**:
- ✅ No code from `common` crate was reimplemented
- ✅ No BLOCKER-level duplication (existing shared code was used correctly)
- 📋 5 NON_BLOCKER patterns identified for future extraction
- 📋 Total estimated tech debt: ~500 lines of duplicated code

**Key observations**:
1. **Health checkers**: 95% duplicate, good candidate for generic task abstraction
2. **gRPC validation**: 85% duplicate, should be extracted to common validation module
3. **Health status conversion**: Inconsistent (Pending vs Unhealthy default) - security concern
4. **Heartbeat intervals**: Defined in 3 places, should be protocol constants
5. **Channel pooling**: Only one instance exists, monitor for second occurrence

**Priority for extraction**: #3 (health status inconsistency) > #1 (health checkers) > #2 (validation) > #4 (intervals) > #5 (channel pooling)

The implementation correctly used existing `common` crate utilities (SecretString, JWT validation, error types), demonstrating good adherence to DRY principles for already-extracted code. The duplication identified is service-specific code that hasn't yet been extracted, which is acceptable tech debt for follow-up work.
