# Meeting Controller Specialist Checkpoint

**Date**: 2026-01-30
**Task**: ADR-0023 Phase 6c - Wire MC-GC Integration
**Status**: Implementation Complete

---

## Patterns Discovered

### Pattern: Config Module Separation (Bin vs Lib)
When a binary crate (`main.rs`) imports from its own library crate (`meeting_controller::*`), avoid re-declaring modules that exist in the library. The binary should import from the library, not declare its own `mod config`. This prevents "crate compiled multiple times with different configurations" errors where the same struct has different types.

**Wrong approach**:
```rust
// main.rs
mod config;  // Declares its own config module
use meeting_controller::grpc::GcClient;  // Uses lib's GcClient which expects lib's Config
```

**Correct approach**:
```rust
// main.rs
use meeting_controller::config::Config;  // Use lib's config
use meeting_controller::grpc::GcClient;
```

### Pattern: Heartbeat Task Shutdown via CancellationToken
For graceful shutdown of background tasks, use `tokio_util::sync::CancellationToken` with parent-child relationships. This provides:
- Consistent pattern with actor system shutdown
- Automatic propagation from parent to child tokens
- Clean loop exit with `cancelled().await` in `select!`

```rust
let shutdown_token = controller_handle.child_token();
let fast_heartbeat_token = shutdown_token.child_token();

tokio::spawn(async move {
    loop {
        tokio::select! {
            () = fast_heartbeat_token.cancelled() => {
                info!("Heartbeat task shutting down");
                break;
            }
            _ = ticker.tick() => { /* heartbeat work */ }
        }
    }
});

// On shutdown - just cancel the parent token:
shutdown_token.cancel();
```

**Note**: Initially used `watch::channel` but this was inconsistent with the actor system's use of `CancellationToken`. The fix applied in Iteration 2 unified the shutdown mechanism.

### Pattern: System Info Gathering at Heartbeat Time
For comprehensive heartbeats that include CPU/memory metrics, gather system info at the moment of the heartbeat rather than maintaining a continuously-updated global. This avoids:
- Extra background task complexity
- Stale data between measurements
- Memory overhead of sysinfo::System instance

The 30s heartbeat interval makes the overhead of `System::new_all()` per heartbeat acceptable.

### Pattern: Shared Metrics for Heartbeat Reporting
Created a dedicated `ControllerMetrics` struct separate from `ActorMetrics`:
- `ControllerMetrics`: For heartbeat reporting (meetings, participants) - uses `AtomicU32` with `SeqCst` ordering
- `ActorMetrics`: For internal actor system monitoring (mailbox depths, panics)

This separation ensures heartbeat tasks have a clean interface to read metrics without coupling to actor implementation details.

---

## Gotchas Encountered

### Gotcha: sysinfo API Differences Between Versions
The `sysinfo` crate API changed between versions. Version 0.30 uses:
- `sys.global_cpu_info().cpu_usage()` (not `sys.global_cpu_usage()`)
- Method returns `f32`, not a struct with a field

Always check the specific version's documentation when using sysinfo.

### Gotcha: Arc Wrapping for Service Constructors
`McAssignmentService::new()` expects `Arc<MeetingControllerActorHandle>`, but `MeetingControllerActorHandle::new()` returns `Self` directly. Must wrap in Arc:

```rust
let controller_handle = Arc::new(MeetingControllerActorHandle::new(...));
```

Not:
```rust
let controller_handle = MeetingControllerActorHandle::new(...);
// ERROR: type mismatch when passing to McAssignmentService::new()
```

### Gotcha: Clippy Precision Loss for Small Integers to f32
Clippy warns about `u32 as f32` precision loss, but for percentages (0-100), this is safe. Use explicit allow:

```rust
#[allow(clippy::cast_precision_loss)]
let cpu = sys_info.cpu_percent as f32;  // 0-100, no precision loss
```

### Gotcha: Signal Handler expect() is Acceptable
For shutdown signal handlers, `expect()` is appropriate because:
1. If signal handlers can't be installed, graceful shutdown is impossible
2. The service should fail fast in this case
3. Document with `# Panics` section and use `#[expect(..., reason = "...")]`

---

## Key Decisions

### Decision: Eager Connection Initialization
Both GcClient and FencedRedisClient initialize connections eagerly at startup (fail fast) rather than lazily on first use. This:
- Reveals configuration issues immediately
- Simplifies error handling (no Option<T> wrappers)
- Aligns with existing GcClient pattern from Phase 6b

### Decision: Service Token in Config
Added `MC_SERVICE_TOKEN` as a required environment variable in Config. This token authenticates MC to GC for registration and heartbeats. Stored as `SecretString` to prevent accidental logging.

### Decision: ControllerMetrics vs ActorMetrics
Created separate `ControllerMetrics` struct for heartbeat data rather than extending `ActorMetrics`:
- `ActorMetrics` uses `AtomicUsize` (internal counting)
- `ControllerMetrics` uses `AtomicU32` (matches proto message types)
- Clean separation of concerns between internal monitoring and external reporting

### Decision: Comprehensive Heartbeat System Info
CPU and memory percentages are gathered at heartbeat time using `sysinfo` crate. Alternative considered: background task updating shared atomics. Rejected because:
- 30s interval makes per-heartbeat gathering acceptable
- Simpler code with fewer moving parts
- No stale data between measurements

---

## Current Status

**Implementation Complete (Iteration 2)**

All components wired together in main.rs:
1. Config loading with MC_SERVICE_TOKEN
2. Redis connection (FencedRedisClient)
3. Actor system initialization (MeetingControllerActorHandle)
4. GC client creation and registration (GcClient with 5-minute retry resilience)
5. Fast heartbeat task (10s interval, CancellationToken shutdown)
6. Comprehensive heartbeat task (30s interval with CPU/memory, CancellationToken shutdown)
7. gRPC server for GC->MC communication (MeetingControllerServiceServer)
8. Graceful shutdown handling via CancellationToken hierarchy

**Fixes Applied in Iteration 2**:
- ISSUE-1: Registration retry duration increased from ~31s to ~5 minutes
- ISSUE-2: Replaced watch channel with CancellationToken for consistent shutdown
- BLOCKER-001: Added 9 integration tests in `tests/gc_integration.rs`
- CRITICAL-001: Added 4 heartbeat task tests in `tests/heartbeat_tasks.rs`
- Additional unit tests for retry logic and system info

**Verification Results**:
- Layer 1 (check): PASS
- Layer 2 (fmt): PASS
- Layer 3 (guards): PASS (9/9)
- Layer 4 (unit tests): PASS (125 tests)
- Layer 5 (all tests): PASS (138 tests total)
- Layer 6 (clippy): PASS
- Layer 7 (semantic): PASS (9/9)

**Files Modified**:
- `crates/meeting-controller/Cargo.toml` - Added sysinfo, tokio-stream dependencies
- `crates/meeting-controller/src/lib.rs` - Export system_info module
- `crates/meeting-controller/src/config.rs` - Added service_token field
- `crates/meeting-controller/src/main.rs` - Full wiring with CancellationToken shutdown
- `crates/meeting-controller/src/actors/mod.rs` - Export ControllerMetrics
- `crates/meeting-controller/src/actors/metrics.rs` - Added ControllerMetrics
- `crates/meeting-controller/src/grpc/gc_client.rs` - Increased retry duration, added tests
- `crates/meeting-controller/src/system_info.rs` - Added more tests

**Files Created**:
- `crates/meeting-controller/src/system_info.rs` - System info gathering
- `crates/meeting-controller/tests/gc_integration.rs` - MC-GC integration tests
- `crates/meeting-controller/tests/heartbeat_tasks.rs` - Heartbeat task behavior tests

---

## Next Steps

1. ~~**Integration Testing**: Create tests that verify MC↔GC communication with mock GC~~ ✅ Done
2. **End-to-End Testing**: Full flow test with real services
3. **Phase 6d**: Session binding token validation
4. **Phase 6g**: WebTransport server integration
5. **Phase 6h**: Health endpoints and monitoring

---

## Reflection Summary (Iteration 2)

**Knowledge Updated**: 2026-01-31

**Patterns Added**:
- Mock gRPC Server for Integration Tests - Using TcpListenerStream with tonic for testable client code

**Gotchas Added**:
- sysinfo API Differences Between Versions - v0.30 uses `global_cpu_info().cpu_usage()`
- MissedTickBehavior::Burst for Deterministic Test Tick Counts - Use Burst in tests, Skip in production

**Integration Updated**:
- MC Registration with GC - Updated retry duration from 5 retries to 20 retries with 5-minute deadline

**Key Takeaway**: The watch channel for shutdown was a deviation from established CancellationToken pattern. Always check patterns.md before introducing alternative approaches - the existing pattern was designed for this exact use case and provides automatic parent-child propagation.
