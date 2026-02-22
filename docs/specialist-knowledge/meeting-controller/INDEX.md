# Meeting Controller Navigation

## Architecture & Design
- MC architecture, actor model, session binding, capacity → ADR-0023
- User auth, meeting access, join flow → ADR-0020
- Observability pattern (metrics crate facade) → ADR-0011

## Code Locations
- Service entry point → `crates/mc-service/src/main.rs`
- Config (SecretString, env loading) → `crates/mc-service/src/config.rs`
- Error types (McError hierarchy) → `crates/mc-service/src/errors.rs`
- Actor: controller (root, capacity) → `crates/mc-service/src/actors/controller.rs`
- Actor: meeting (participants, grace period) → `crates/mc-service/src/actors/meeting.rs`
- Actor: connection → `crates/mc-service/src/actors/connection.rs`
- Actor: session binding (HMAC, HKDF) → `crates/mc-service/src/actors/session.rs`
- Actor: metrics (dual system) → `crates/mc-service/src/actors/metrics.rs`
- gRPC: GC client (registration, heartbeats) → `crates/mc-service/src/grpc/gc_client.rs`
- gRPC: MC service (AssignMeetingWithMh) → `crates/mc-service/src/grpc/mc_service.rs`
- gRPC: auth interceptor (Bearer validation) → `crates/mc-service/src/grpc/auth_interceptor.rs`
- Redis: fenced client (Lua scripts) → `crates/mc-service/src/redis/client.rs`
- Redis: Lua scripts (atomic fencing) → `crates/mc-service/src/redis/lua_scripts.rs`
- Health + readiness endpoints → `crates/mc-service/src/observability/health.rs`
- Prometheus metric wrappers → `crates/mc-service/src/observability/metrics.rs`
- System info (sysinfo) → `crates/mc-service/src/system_info.rs`

## Protocols
- Client signaling (join, mute, session recovery) → `proto/signaling.proto`
- Internal service RPCs (RegisterMc, AssignMeeting) → `proto/internal.proto`

## Integration Seams
- MC <-> GC registration/heartbeat → `crates/mc-service/src/grpc/gc_client.rs`
- GC -> MC assignment → `crates/mc-service/src/grpc/mc_service.rs`
- MC -> AC token management → `crates/common/src/token_manager.rs`
- MC -> Redis session/fencing → `crates/mc-service/src/redis/client.rs`

## Testing
- GC integration tests (mock server) → `crates/mc-service/tests/gc_integration.rs`
- Heartbeat task tests → `crates/mc-service/tests/heartbeat_tasks.rs`
- Test utilities (mock GC/Redis/MH) → `crates/mc-test-utils/src/`
- Env-tests MC-GC integration → `crates/env-tests/tests/22_mc_gc_integration.rs`

## Infrastructure
- K8s deployment → `infra/services/mc-service/deployment.yaml`
- Network policy → `infra/services/mc-service/network-policy.yaml`
- Grafana dashboard → `infra/grafana/dashboards/mc-overview.json`
