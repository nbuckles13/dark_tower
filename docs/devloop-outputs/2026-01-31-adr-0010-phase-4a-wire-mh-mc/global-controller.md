# Global Controller Specialist Checkpoint

**Specialist**: global-controller
**Date**: 2026-01-31
**Status**: COMPLETE

---

## Implementation Summary

Successfully wired all MH components into the Global Controller's main application flow per ADR-0010 Phase 4a requirements.

### Changes Made

#### 1. main.rs - MH Service and Health Checker Integration

**File**: `/home/nathan/code/dark_tower/crates/global-controller/src/main.rs`

Added MhService to gRPC server:
```rust
use grpc::{McService, MhService};
use proto_gen::internal::media_handler_registry_service_server::MediaHandlerRegistryServiceServer;

// Create gRPC services
let mh_service = MhService::new(Arc::new(db_pool.clone()));

// gRPC server with both services
let grpc_server = TonicServer::builder()
    .layer(grpc_auth_layer)
    .add_service(GlobalControllerServiceServer::new(mc_service))
    .add_service(MediaHandlerRegistryServiceServer::new(mh_service))
    .serve(grpc_addr);
```

Added MH health checker background task:
```rust
use tasks::{
    start_assignment_cleanup, start_health_checker, start_mh_health_checker,
    AssignmentCleanupConfig,
};

// Start MH health checker background task
let mh_health_checker_pool = db_pool.clone();
let mh_staleness_threshold = staleness_threshold;
let mh_health_checker_token = cancellation_token.clone();
let mh_health_checker_handle = tokio::spawn(async move {
    start_mh_health_checker(mh_health_checker_pool, mh_staleness_threshold, mh_health_checker_token).await;
});

// Wait for MH health checker on shutdown
if let Err(e) = mh_health_checker_handle.await {
    tracing::error!(error = %e, "MH health checker task panicked");
}
```

Added MC client creation for GC->MC communication:
```rust
// Create MC client for GC->MC communication
let mc_client: Arc<dyn services::McClientTrait> = Arc::new(services::McClient::new(
    common::secret::SecretString::from(std::env::var("GC_SERVICE_TOKEN").unwrap_or_default()),
));

let state = Arc::new(AppState {
    pool: db_pool.clone(),
    config,
    mc_client: Some(mc_client),
});
```

#### 2. handlers/meetings.rs - Switched to assign_meeting_with_mh

**File**: `/home/nathan/code/dark_tower/crates/global-controller/src/handlers/meetings.rs`

Added fallback helper function for testability:
```rust
/// Assign meeting to MC with MH selection, falling back to legacy flow if no mc_client.
///
/// This helper enables tests to run without requiring a real MC RPC server.
/// Production uses mc_client to call MC; tests set mc_client=None to use legacy flow.
async fn assign_with_mh_or_fallback(
    state: &AppState,
    meeting_id: &str,
) -> Result<crate::services::mc_assignment::AssignmentWithMh, GcError> {
    use crate::services::mc_assignment::AssignmentWithMh;
    use crate::services::mh_selection::{MhAssignment, MhSelection};

    match &state.mc_client {
        Some(mc_client) => {
            // Production path: full MH selection + MC RPC
            McAssignmentService::assign_meeting_with_mh(
                &state.pool,
                mc_client.clone(),
                meeting_id,
                &state.config.region,
                &state.config.gc_id,
            )
            .await
        }
        None => {
            // Test fallback: legacy assignment flow (no MH selection, no MC RPC)
            let mc_assignment = McAssignmentService::assign_meeting(
                &state.pool,
                meeting_id,
                &state.config.region,
                &state.config.gc_id,
            )
            .await?;

            // Create empty MH selection for backward compatibility
            let empty_mh = MhAssignment {
                mh_id: String::new(),
                webtransport_endpoint: String::new(),
                role: crate::services::mh_selection::MhRole::Primary,
            };

            Ok(AssignmentWithMh {
                mc_assignment,
                mh_selection: MhSelection {
                    primary: empty_mh,
                    backup: None,
                },
            })
        }
    }
}
```

Updated both join handlers to use `assign_with_mh_or_fallback`:
- `join_meeting` handler (lines ~115)
- `add_participant` handler (lines ~233)

#### 3. routes/mod.rs - Added mc_client to AppState

**File**: `/home/nathan/code/dark_tower/crates/global-controller/src/routes/mod.rs`

```rust
use crate::services::mc_client::McClientTrait;

pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    pub mc_client: Option<Arc<dyn McClientTrait>>,
}
```

#### 4. services/mc_assignment.rs - Changed to dyn trait

**File**: `/home/nathan/code/dark_tower/crates/global-controller/src/services/mc_assignment.rs`

Changed function signature from generic to dyn trait for simpler usage:
```rust
pub async fn assign_meeting_with_mh(
    pool: &PgPool,
    mc_client: Arc<dyn McClientTrait>,  // Changed from Arc<C> where C: McClientTrait
    meeting_id: &str,
    region: &str,
    gc_id: &str,
) -> Result<AssignmentWithMh, GcError>
```

#### 5. Module Export Cleanup

Removed `#[allow(unused_imports)]` from:
- `grpc/mod.rs` - MhService now used
- `tasks/mod.rs` - start_mh_health_checker now used
- `services/mod.rs` - MC client and MH selection types now used

Removed `#![allow(dead_code)]` from:
- `grpc/mh_service.rs`
- `tasks/mh_health_checker.rs`
- `services/mc_assignment.rs`
- `services/mc_client.rs`
- `services/mh_selection.rs`

#### 6. Test Infrastructure Updates

**Files Updated**:
- `tests/meeting_tests.rs` - Added `MockMcClient::accepting()` for production path testing, added `register_healthy_mhs_for_region()` helper
- `tests/auth_tests.rs` - Added `mc_client: None` to AppState (auth tests don't need MC)
- `gc-test-utils/src/server_harness.rs` - Added `mc_client: None` to AppState

Test helper for MH registration:
```rust
async fn register_healthy_mhs_for_region(pool: &PgPool, region: &str) {
    for i in 1..=2 {
        let handler_id = format!("test-mh-{}-{}", region, i);
        sqlx::query(r#"
            INSERT INTO media_handlers (
                handler_id, region, webtransport_endpoint, grpc_endpoint,
                max_streams, current_streams, health_status, last_heartbeat_at, registered_at
            ) VALUES ($1, $2, $3, $4, 1000, 0, 'healthy', NOW(), NOW())
            ON CONFLICT (handler_id) DO UPDATE SET last_heartbeat_at = NOW(), health_status = 'healthy'
        "#)
        .bind(&handler_id)
        .bind(region)
        .bind(format!("https://{}.mh.example.com:4433", handler_id))
        .bind(format!("https://{}.mh.example.com:50052", handler_id))
        .execute(pool)
        .await
        .expect("Failed to register test MH");
    }
}
```

---

## Verification Results

### Layer 5: Tests
All tests pass (workspace-wide):
- 34 meeting tests pass
- All auth tests pass
- All other tests pass

### Layer 6: Clippy
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.71s
```
No warnings.

### Layer 7: Semantic Guards
```
Total guards run: 10
Passed: 10
Failed: 0
All guards passed!
```

---

## Design Decisions

### 1. MockMcClient for Integration Tests (Updated)

**Problem**: `assign_meeting_with_mh` makes RPC calls to MC, but tests don't have a real MC server.

**Initial Solution**: Made `mc_client` optional in `AppState` with fallback to legacy flow.

**Code Review Feedback**: The fallback pattern meant tests weren't exercising the production code path (`assign_meeting_with_mh`).

**Final Solution**: Integration tests now use `MockMcClient::accepting()` which:
- Tests the actual production code path with MH selection + MC notification
- Uses mock responses instead of real gRPC calls
- Ensures test coverage of production code

**Test Setup**:
```rust
let mock_mc_client = Arc::new(MockMcClient::accepting());
let state = Arc::new(AppState {
    pool: pool.clone(),
    config: config.clone(),
    mc_client: Some(mock_mc_client),
});
```

**Benefits**:
- Tests exercise the same code path as production
- Mock infrastructure already existed (just needed to be wired in)
- Fallback path still available if needed for simpler unit tests

### 2. dyn Trait Instead of Generic

**Problem**: `Arc<dyn McClientTrait>` in AppState requires the function to accept `Arc<dyn McClientTrait>` directly, not a generic `Arc<C>`.

**Solution**: Changed `assign_meeting_with_mh` to accept `Arc<dyn McClientTrait>` directly instead of a generic type parameter.

**Benefits**:
- Simpler function signature
- Works naturally with `Arc<dyn McClientTrait>` stored in AppState
- No monomorphization overhead in this case since we always use dynamic dispatch

---

## Files Changed

| File | Change Type | Description |
|------|-------------|-------------|
| `crates/global-controller/src/main.rs` | Modified | Wire MhService + health checker |
| `crates/global-controller/src/routes/mod.rs` | Modified | Add mc_client to AppState |
| `crates/global-controller/src/handlers/meetings.rs` | Modified | Use assign_with_mh_or_fallback |
| `crates/global-controller/src/services/mc_assignment.rs` | Modified | Change to dyn trait |
| `crates/global-controller/src/grpc/mod.rs` | Modified | Remove #[allow(unused_imports)] |
| `crates/global-controller/src/tasks/mod.rs` | Modified | Remove #[allow(unused_imports)] |
| `crates/global-controller/src/services/mod.rs` | Modified | Remove #[allow(unused_imports)] |
| `crates/global-controller/src/grpc/mh_service.rs` | Modified | Remove #![allow(dead_code)] |
| `crates/global-controller/src/tasks/mh_health_checker.rs` | Modified | Remove #![allow(dead_code)] |
| `crates/global-controller/src/services/mc_client.rs` | Modified | Remove #![allow(dead_code)] |
| `crates/global-controller/src/services/mh_selection.rs` | Modified | Remove #![allow(dead_code)] |
| `crates/global-controller/tests/meeting_tests.rs` | Modified | Use MockMcClient, add MH registration |
| `crates/global-controller/tests/auth_tests.rs` | Modified | Use MockMcClient |
| `crates/global-controller/tests/meeting_assignment_tests.rs` | Modified | Convert to assign_meeting_with_mh |
| `crates/gc-test-utils/src/server_harness.rs` | Modified | Use MockMcClient |

---

## Reflection

**Date**: 2026-01-31

### Summary

Completed ADR-0010 Phase 4a by wiring MH components into GC. Key learning: optional dependencies with fallback logic can cause tests to exercise different code than production. The initial implementation used `mc_client: Option<...>` with fallback to legacy `assign_meeting()`, but code review caught that integration tests weren't testing the actual production code path. Fixed by making `mc_client` required and removing all fallback/legacy code.

### Knowledge Updates

| Action | File | Entry |
|--------|------|-------|
| Added | gotchas.md | "Optional Dependencies with Fallback Hide Production Code in Tests" |
| Updated | patterns.md | "Mock Trait for Testing gRPC Clients" - emphasized required vs optional |
| Updated | integration.md | "GC-to-MC Assignment RPC Flow with MH Selection" - added MH selection details |
