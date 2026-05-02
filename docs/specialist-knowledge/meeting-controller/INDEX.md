# Meeting Controller Navigation

## Architecture & Design
- MC architecture, actor model, session binding, capacity → ADR-0023
- User auth, meeting access, join flow → ADR-0020
- Observability pattern (metrics crate facade) → ADR-0011
- Metric testability (component tests, `MetricAssertion`, presence guard, rollout SLO) → ADR-0032
- Service-owned dashboards and alerts → ADR-0031

## Code Locations
- Service entry point → `crates/mc-service/src/main.rs`
- Config (SecretString, env loading, ac_jwks_url, TLS paths, advertise addresses) → `crates/mc-service/src/config.rs`
- Error types (McError hierarchy, From<JwtError>, MhAssignmentMissing) → `crates/mc-service/src/errors.rs`
- Auth: McJwtValidator, validate_meeting_token, validate_guest_token → `crates/mc-service/src/auth/mod.rs`
- Actors: controller, meeting, participant, messages, session (HMAC/HKDF), metrics → `crates/mc-service/src/actors/`
- WebTransport: server (accept loop, TLS, capacity) → `crates/mc-service/src/webtransport/server.rs`
- WebTransport: connection (join flow, bridge loop, MediaConnectionFailed handler R-20) → `crates/mc-service/src/webtransport/connection.rs`
- WebTransport: async RegisterMeeting trigger (R-12, first participant) → `crates/mc-service/src/webtransport/connection.rs:register_meeting_with_handlers()`
- WebTransport: handler (encode_participant_update) → `crates/mc-service/src/webtransport/handler.rs`
- gRPC: GC client (registration, heartbeats, advertise) → `crates/mc-service/src/grpc/gc_client.rs`
- gRPC: MC service (AssignMeetingWithMh) → `crates/mc-service/src/grpc/mc_service.rs`
- gRPC: MH client + MhRegistrationClient trait (RegisterMeeting, per-call Channel) → `crates/mc-service/src/grpc/mh_client.rs`
- gRPC: auth interceptor + McAuthLayer (async JWKS + scope check, R-22) → `crates/mc-service/src/grpc/auth_interceptor.rs`
- gRPC: media coordination service (MH→MC notifications, R-15) → `crates/mc-service/src/grpc/media_coordination.rs`
- MH connection registry (participant→MH state, R-18, lifecycle via controller actor) → `crates/mc-service/src/mh_connection_registry.rs`
- Redis: fenced client + MhAssignmentStore trait + MhAssignmentData (handlers Vec) → `crates/mc-service/src/redis/client.rs`
- Redis: Lua scripts (atomic fencing) → `crates/mc-service/src/redis/lua_scripts.rs`
- Health/readiness, system info → `crates/mc-service/src/observability/health.rs`, `crates/mc-service/src/system_info.rs`
- Prometheus metric wrappers (record_register_meeting, record_mh_notification, record_media_connection_failed, record_webtransport_connection, record_jwt_validation, record_session_join, record_token_refresh_metrics) → `crates/mc-service/src/observability/metrics.rs`
- MC metrics catalog → `docs/observability/metrics/mc-service.md`

## Protocols
- Client signaling (join, mute, session recovery, MediaConnectionFailed) → `proto/signaling.proto`
- Internal service RPCs (RegisterMc, AssignMeeting, MediaCoordinationService, RegisterMeeting) → `proto/internal.proto`

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
- Shared bring-up (TestStackHandles, build_test_stack, seed_meeting_with_mh) + mock MH stores → `crates/mc-service/tests/common/mod.rs`
- Accept-loop component rig → `crates/mc-service/tests/common/accept_loop_rig.rs`
- Join flow tests (TestServer, MockMhRegistrationClient.wait_for_calls, multi-MH and skip-grpc-endpoint cases) → `crates/mc-service/tests/join_tests.rs`
- Accept-loop status + per-failure-class drilldown → `crates/mc-service/tests/webtransport_accept_loop_integration.rs`
- gRPC auth-layer per-failure-reason → `crates/mc-service/tests/auth_layer_integration.rs`
- Media coordination notifications + connect/disconnect round-trip → `crates/mc-service/tests/media_coordination_integration.rs`
- RegisterMeeting metrics (stub MH gRPC) → `crates/mc-service/tests/register_meeting_integration.rs`
- ActorMetrics / MailboxMonitor metrics → `crates/mc-service/tests/actor_metrics_integration.rs`
- Redis-class wrapper coverage → `crates/mc-service/tests/redis_metrics_integration.rs`
- Token-refresh integration → `crates/mc-service/tests/token_refresh_integration.rs`
- GC integration + heartbeat metrics → `crates/mc-service/tests/gc_integration.rs`
- Heartbeat task tests → `crates/mc-service/tests/heartbeat_tasks.rs`
- Per-cluster MetricAssertion tests + Cat B matrix → `crates/mc-service/src/observability/metrics.rs`
- Test utilities (mock GC/Redis/MH, jwt_test) → `crates/mc-test-utils/src/`
- Env-tests MC-GC integration → `crates/env-tests/tests/22_mc_gc_integration.rs`
- Env-tests MH QUIC + MC↔MH coordination metrics → `crates/env-tests/tests/26_mh_quic.rs`

## Advertise Address Config
- Config fields `grpc_advertise_address` / `webtransport_advertise_address`; consumed by GC registration + MH RegisterMeeting → `crates/mc-service/src/config.rs`, `crates/mc-service/src/grpc/gc_client.rs`, `crates/mc-service/src/webtransport/connection.rs`

## Infrastructure
- K8s deployment (POD_IP downward API, advertise addresses) → `infra/services/mc-service/deployment.yaml`
- K8s network policy (MH ingress on 50052) → `infra/services/mc-service/network-policy.yaml`
- Grafana dashboard → `infra/grafana/dashboards/mc-overview.json`
- Prometheus alert rules (incl. MCMediaConnectionAllFailed) → `infra/docker/prometheus/rules/mc-alerts.yaml`
- Incident runbook (Sc 11 MediaConnectionFailed, Sc 12 RegisterMeeting, Sc 13 unexpected MH notifications) → `docs/runbooks/mc-incident-response.md`
- Deployment runbook (post-deploy MC↔MH coordination addendum) → `docs/runbooks/mc-deployment.md`
