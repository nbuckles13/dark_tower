# Meeting Controller Navigation

## Architecture & Design
- MC architecture, actor model, session binding, capacity → ADR-0023
- User auth, meeting access, join flow → ADR-0020
- Observability pattern (metrics crate facade) → ADR-0011

## Code Locations
- Service entry point → `crates/mc-service/src/main.rs`
- Config (SecretString, env loading, ac_jwks_url, TLS paths, advertise addresses) → `crates/mc-service/src/config.rs`
- Error types (McError hierarchy, From<JwtError>) → `crates/mc-service/src/errors.rs`
- Auth: McJwtValidator, validate_meeting_token, validate_guest_token → `crates/mc-service/src/auth/mod.rs`
- Actor: controller (root, capacity) → `crates/mc-service/src/actors/controller.rs`
- Actor: meeting (participants, grace period) → `crates/mc-service/src/actors/meeting.rs`
- Actor: messages (inter-actor types, JoinConnection, JoinResult) → `crates/mc-service/src/actors/messages.rs`
- Actor: participant (per-participant, disconnect notify) → `crates/mc-service/src/actors/participant.rs`
- WebTransport: server (accept loop, TLS, capacity) → `crates/mc-service/src/webtransport/server.rs`
- WebTransport: connection (join flow, bridge loop, MediaConnectionFailed R-20) → `crates/mc-service/src/webtransport/connection.rs`
- WebTransport: async RegisterMeeting trigger (R-12, first participant) → `crates/mc-service/src/webtransport/connection.rs:register_meeting_with_handlers()`
- WebTransport: handler (encode_participant_update) → `crates/mc-service/src/webtransport/handler.rs`
- Actor: session binding (HMAC, HKDF) → `crates/mc-service/src/actors/session.rs`
- Actor: metrics (dual system) → `crates/mc-service/src/actors/metrics.rs`
- gRPC: GC client (registration, heartbeats, advertise address usage) → `crates/mc-service/src/grpc/gc_client.rs`
- gRPC: MC service (AssignMeetingWithMh) → `crates/mc-service/src/grpc/mc_service.rs`
- gRPC: MH client (RegisterMeeting, per-call Channel) → `crates/mc-service/src/grpc/mh_client.rs`
- gRPC: MhRegistrationClient trait (testable MH registration) → `crates/mc-service/src/grpc/mh_client.rs:MhRegistrationClient`
- gRPC: auth interceptor (Bearer validation) → `crates/mc-service/src/grpc/auth_interceptor.rs`
- gRPC: auth layer (async JWKS + scope check, R-22) → `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthLayer`
- gRPC: media coordination (MH notifications, R-15) → `crates/mc-service/src/grpc/media_coordination.rs`
- MH connection registry (participant→MH state, R-18) → `crates/mc-service/src/mh_connection_registry.rs`
- Redis: fenced client (Lua scripts) → `crates/mc-service/src/redis/client.rs`
- Redis: MhAssignmentStore trait (testable Redis abstraction) → `crates/mc-service/src/redis/client.rs:MhAssignmentStore`
- Redis: MhAssignmentData (MH endpoints in Redis) → `crates/mc-service/src/redis/client.rs:MhAssignmentData`
- Redis: Lua scripts (atomic fencing) → `crates/mc-service/src/redis/lua_scripts.rs`
- Health + readiness endpoints → `crates/mc-service/src/observability/health.rs`
- Prometheus metric wrappers → `crates/mc-service/src/observability/metrics.rs`
- Join flow metrics (R-13): record_webtransport_connection, record_jwt_validation, record_session_join → `crates/mc-service/src/observability/metrics.rs`
- MH communication metrics: record_register_meeting → `crates/mc-service/src/observability/metrics.rs`
- MH coordination metrics (R-28): record_mh_notification, record_media_connection_failed → `crates/mc-service/src/observability/metrics.rs`
- MC metrics catalog → `docs/observability/metrics/mc-service.md`
- System info (sysinfo) → `crates/mc-service/src/system_info.rs`

## Protocols
- Client signaling (join, mute, session recovery) → `proto/signaling.proto`
- Internal service RPCs (RegisterMc, AssignMeeting) → `proto/internal.proto`

## Integration Seams
- Client -> MC WebTransport (join, signaling) → `crates/mc-service/src/webtransport/server.rs`
- MC <-> GC registration/heartbeat → `crates/mc-service/src/grpc/gc_client.rs`
- GC -> MC assignment → `crates/mc-service/src/grpc/mc_service.rs`
- MH -> MC notifications (connect/disconnect) → `crates/mc-service/src/grpc/media_coordination.rs`
- MC -> AC token management → `crates/common/src/token_manager.rs`
- MC -> AC JWKS (meeting token validation) → `crates/common/src/jwt.rs:JwksClient`
- MC -> MH RegisterMeeting RPC → `crates/mc-service/src/grpc/mh_client.rs:register_meeting()`
- MC -> Redis session/fencing → `crates/mc-service/src/redis/client.rs`
- MC -> Redis MH assignment read (join flow) → `crates/mc-service/src/redis/client.rs:get_mh_assignment()`

## Testing
- GC integration tests (mock server) → `crates/mc-service/tests/gc_integration.rs`
- Heartbeat task tests → `crates/mc-service/tests/heartbeat_tasks.rs`
- Join flow integration tests (WebTransport + mock MH store) → `crates/mc-service/tests/join_tests.rs`
- Test utilities (mock GC/Redis/MH) → `crates/mc-test-utils/src/`
- Env-tests MC-GC integration → `crates/env-tests/tests/22_mc_gc_integration.rs`

## Advertise Address Config
- Config fields: `grpc_advertise_address`, `webtransport_advertise_address` → `crates/mc-service/src/config.rs`
- Used in GC registration and MH RegisterMeeting → `crates/mc-service/src/grpc/gc_client.rs`, `connection.rs`
- K8s: derived from downward API `$(POD_IP)` → `infra/services/mc-service/deployment.yaml`

## Infrastructure
- K8s deployment (incl. POD_IP downward API, advertise addresses) → `infra/services/mc-service/deployment.yaml`
- Network policy → `infra/services/mc-service/network-policy.yaml`
- Grafana dashboard → `infra/grafana/dashboards/mc-overview.json`
- Prometheus alert rules → `infra/docker/prometheus/rules/mc-alerts.yaml`
