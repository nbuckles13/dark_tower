# Media Handler Navigation

## Architecture & Design
- SFU architecture, MH registration/load reports → ADR-0010 (Section 4a)
- Actor pattern for concurrency (handle/task, message passing, no locks) → ADR-0001
- MH assignment, selection algorithm, cross-region coordination → ADR-0023 (Section 5)
- Service authentication (MH→GC OAuth, MC→MH Bearer) → ADR-0003
- gRPC auth scopes (two-layer: JWKS+scope server-wide, service_type per-path) → ADR-0003
- Observability pattern (metrics crate facade) → ADR-0011
- Dashboard metric presentation (counters vs rates) → ADR-0029
- Metric testability (component tests drive real `WebTransportServer::bind() → accept_loop()`; `MetricAssertion` snapshots via shared `DebuggingRecorder`; `wt_rig` `handle_connection`-direct bypass deleted once accept-loop component test lands) → ADR-0032 §Implementation Notes phasing step 2
- Service-owned dashboards and alerts (collapsed Phase 4) → ADR-0031
- Fuzz testing for media frames → ADR-0006
- Client-to-MH QUIC connection user story → `docs/user-stories/2026-04-12-mh-quic-connection.md`

## Code Locations
- Service entry point → `crates/mh-service/src/main.rs`
- Library root (module declarations) → `crates/mh-service/src/lib.rs`
- Config (SecretString, env loading, TLS paths, advertise addresses) → `crates/mh-service/src/config.rs`
- Error types (MhError hierarchy) → `crates/mh-service/src/errors.rs`
- gRPC: GC client (registration, heartbeats, re-registration) → `crates/mh-service/src/grpc/gc_client.rs`
- gRPC: MC client (NotifyParticipantConnected/Disconnected, retry with backoff) → `crates/mh-service/src/grpc/mc_client.rs`
- gRPC: MH service (RegisterMeeting via SessionManagerHandle, other RPCs stub) → `crates/mh-service/src/grpc/mh_service.rs`
- gRPC: auth layer (MhAuthLayer: JWKS + scope + Layer 2 service_type routing + claims injection, ADR-0003) → `crates/mh-service/src/grpc/auth_interceptor.rs`
- gRPC: classify_jwt_error helper (JwtError → bounded failure_reason label) → `crates/mh-service/src/grpc/auth_interceptor.rs:classify_jwt_error`
- JWT validation (MhJwtValidator, meeting token validation) → `crates/mh-service/src/auth/mod.rs`
- Session management (SessionManagerActor/SessionManagerHandle, actor pattern ADR-0001) → `crates/mh-service/src/session/mod.rs`
- WebTransport server (TLS, capacity, accept loop) → `crates/mh-service/src/webtransport/server.rs`
- WebTransport connection handler (JWT read, provisional accept, MC notifications) → `crates/mh-service/src/webtransport/connection.rs`
- Health + readiness endpoints → `crates/mh-service/src/observability/health.rs`
- Prometheus metric recorders (mh_jwt_validations_total, mh_caller_type_rejected_total, mh_register_meeting_timeouts_total, etc.) → `crates/mh-service/src/observability/metrics.rs`
- RegisterMeeting timeout fire site (provisional-accept select helper; behavioral tests enforce metric-only-on-timeout invariant) → `crates/mh-service/src/webtransport/connection.rs:await_meeting_registration`
- MH metrics catalog → `docs/observability/metrics/mh-service.md`

## Media Protocol
- Frame types (MediaFrame, FrameType, FrameFlags) → `crates/media-protocol/src/frame.rs`
- Binary codec (encode_frame, decode_frame) → `crates/media-protocol/src/codec.rs`
- Stream state (MediaStream, StreamConfig) → `crates/media-protocol/src/stream.rs`
- Fuzz: decode → `crates/media-protocol/fuzz/fuzz_targets/codec_decode.rs`
- Fuzz: roundtrip → `crates/media-protocol/fuzz/fuzz_targets/codec_roundtrip.rs`

## Proto Definitions
- MC↔MH / MH↔GC / MH→MC RPCs + assignment + DisconnectReason → `proto/internal.proto`
- Client signaling (MediaStream, StreamAssignment, layout) → `proto/signaling.proto`
- Generated Rust code → `crates/proto-gen/build.rs`

## Integration Seams
- MH → GC registration/heartbeat → `crates/mh-service/src/grpc/gc_client.rs`
- MH → MC notifications (connect/disconnect) → `crates/mh-service/src/grpc/mc_client.rs`
- MC → MH gRPC service → `crates/mh-service/src/grpc/mh_service.rs`
- MH → AC token management → `crates/common/src/token_manager.rs`
- MH → AC JWKS (meeting + service token validation) → `crates/common/src/jwt.rs:JwksClient`
- Client → MH WebTransport (QUIC/TLS 1.3) → `crates/mh-service/src/webtransport/server.rs`
- MH depends on common crate → `crates/common/src/lib.rs`
- MH depends on proto-gen → `crates/proto-gen/src/lib.rs`
- MH depends on media-protocol → `crates/media-protocol/src/lib.rs`

## Testing
- Integration: GC mock → `crates/mh-service/tests/gc_integration.rs`
- Integration: MC client retry + auth short-circuit → `crates/mh-service/tests/mc_client_integration.rs`
- Integration: MhAuthLayer over real tonic transport (alg:none + HS256 confusion, Layer 2 routing) → `crates/mh-service/tests/auth_layer_integration.rs`
- Integration: RegisterMeeting over real gRPC (happy path + InvalidArgument) → `crates/mh-service/tests/register_meeting_integration.rs`
- Integration: WebTransport accept path, provisional timeout kick/survive, MC notify lifecycle → `crates/mh-service/tests/webtransport_integration.rs`
- Integration rigs (JWKS mock, mock MC, gRPC rig, WT rig, token minters) → `crates/mh-service/tests/common/`
- Unit tests live alongside source modules listed in Code Locations (config, auth layer, JWT validator, session manager, RegisterMeeting handler, health, metrics)

## Infrastructure
- K8s deployment (ports, probes, env, downward API, advertise addresses) → `infra/services/mh-service/deployment.yaml`
- Advertise address config + GC registration → `crates/mh-service/src/config.rs`, `gc_client.rs:register()`
- K8s configmap (bind addresses, region, GC URL) → `infra/services/mh-service/configmap.yaml`
- Grafana dashboard + kustomization → `infra/grafana/dashboards/mh-overview.json`, `infra/grafana/kustomization.yaml`
