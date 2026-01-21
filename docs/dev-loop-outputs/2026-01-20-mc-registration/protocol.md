# Protocol Specialist - MC Registration Support

**Date**: 2026-01-20
**Specialist**: Protocol
**Status**: SUCCESS

## Summary

Updated protocol definitions and build configuration to support MC registration via gRPC. This enables Meeting Controllers to register with the Global Controller and send heartbeats using the correct direction (MC->GC) per ADR-0010.

## Files Modified

1. **Cargo.toml** (workspace)
   - Added `tonic = "0.12"` to workspace dependencies
   - Added `tonic-build = "0.12"` to workspace dependencies

2. **crates/proto-gen/Cargo.toml**
   - Added `tonic` to dependencies (workspace)
   - Replaced `prost-build` with `tonic-build` in build-dependencies

3. **crates/global-controller/Cargo.toml**
   - Added `tonic` dependency (workspace) for gRPC server implementation

4. **proto/internal.proto**
   - Extended `HealthStatus` enum with `PENDING` (value 0) and `DRAINING` (value 4)
   - Added `RegisterMCRequest` message with dual endpoints (gRPC + WebTransport)
   - Added `RegisterMCResponse` message with heartbeat intervals
   - Added `FastHeartbeatRequest` message (10s interval, capacity only)
   - Added `ComprehensiveHeartbeatRequest` message (30s interval, full metrics)
   - Added `GlobalControllerService` service definition (MC->GC direction)

5. **crates/proto-gen/build.rs**
   - Switched from `prost_build` to `tonic_build` for gRPC service trait generation

6. **crates/proto-gen/src/lib.rs**
   - Added `pub use tonic;` re-export

7. **crates/proto-gen/src/generated/dark_tower.internal.rs** (auto-generated)
   - Now includes `GlobalControllerServiceServer` trait
   - Now includes `GlobalControllerServiceClient` for MC implementations

## Protocol Changes Detail

### New Messages

```protobuf
message RegisterMCRequest {
  string id = 1;                      // Unique controller ID
  string region = 2;                  // Deployment region (e.g., "us-east-1")
  string grpc_endpoint = 3;           // gRPC endpoint for GC->MC calls
  string webtransport_endpoint = 4;   // WebTransport endpoint for clients
  uint32 max_meetings = 5;            // Maximum concurrent meetings
  uint32 max_participants = 6;        // Maximum total participants
}

message RegisterMCResponse {
  bool accepted = 1;
  string message = 2;
  uint64 fast_heartbeat_interval_ms = 3;          // 10000ms
  uint64 comprehensive_heartbeat_interval_ms = 4; // 30000ms
}

message FastHeartbeatRequest {
  string controller_id = 1;
  ControllerCapacity capacity = 2;
  HealthStatus health = 3;
}

message ComprehensiveHeartbeatRequest {
  string controller_id = 1;
  ControllerCapacity capacity = 2;
  HealthStatus health = 3;
  float cpu_usage_percent = 4;
  float memory_usage_percent = 5;
}
```

### New Service

```protobuf
service GlobalControllerService {
  rpc RegisterMC(RegisterMCRequest) returns (RegisterMCResponse);
  rpc FastHeartbeat(FastHeartbeatRequest) returns (HeartbeatResponse);
  rpc ComprehensiveHeartbeat(ComprehensiveHeartbeatRequest) returns (HeartbeatResponse);
}
```

### Updated Enum

```protobuf
enum HealthStatus {
  PENDING = 0;    // Just registered, not yet verified
  HEALTHY = 1;
  DEGRADED = 2;
  UNHEALTHY = 3;
  DRAINING = 4;   // Graceful shutdown in progress
}
```

## Verification

- `cargo check -p proto-gen` - PASSED
- `cargo check -p global-controller` - PASSED
- Generated code includes `GlobalControllerServiceServer` trait
- Generated code includes `GlobalControllerServiceClient` for MC side

## Backward Compatibility

- Existing `MeetingControllerService` preserved (marked as legacy)
- Existing `Heartbeat` message preserved (marked as legacy)
- `HealthStatus` enum extended with new values at safe positions

## Next Steps

The GC specialist can now implement `GlobalControllerService` trait to handle:
1. MC registration with endpoint validation
2. Fast heartbeat processing (10s interval)
3. Comprehensive heartbeat processing (30s interval)

---RESULT---
STATUS: SUCCESS
SUMMARY: Added tonic dependencies, created GlobalControllerService with RegisterMC, FastHeartbeat, and ComprehensiveHeartbeat RPCs, updated HealthStatus enum
FILES_MODIFIED: Cargo.toml, crates/proto-gen/Cargo.toml, crates/proto-gen/build.rs, crates/proto-gen/src/lib.rs, crates/global-controller/Cargo.toml, proto/internal.proto, crates/proto-gen/src/generated/dark_tower.internal.rs
ERROR: none
---END---
