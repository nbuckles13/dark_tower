# Media Handler Navigation

## Architecture & Design
- SFU architecture, MH registration/load reports → ADR-0010 (Section 4a)
- Actor pattern for concurrency (handle/task, message passing, no locks) → ADR-0001
- MH assignment, selection algorithm, cross-region coordination → ADR-0023 (Section 5)
- Service authentication (MH→GC OAuth, MC→MH Bearer) → ADR-0003
- gRPC auth scopes (two-layer: JWKS+scope server-wide, service_type per-path) → ADR-0003
- Observability pattern (metrics crate facade) → ADR-0011
- Dashboard metric presentation (counters vs rates) → ADR-0029
- Service-owned dashboards and alerts → ADR-0031
- Metric testability (component tests drive accept loop; `MetricAssertion` snapshots) → ADR-0032
- Fuzz testing for media frames → ADR-0006
- Client-to-MH QUIC connection user story (closed) → `docs/user-stories/2026-04-12-mh-quic-connection.md`

## Code Locations
- Service entry point → `crates/mh-service/src/main.rs`
- Library root (module declarations) → `crates/mh-service/src/lib.rs`
- Config (TLS, advertise addrs, AC_JWKS_URL, register_meeting_timeout, max_connections) → `crates/mh-service/src/config.rs`
- Error types (MhError hierarchy) → `crates/mh-service/src/errors.rs`
- gRPC: GC client (registration, heartbeats, re-registration) → `crates/mh-service/src/grpc/gc_client.rs`
- gRPC: MC client (Notify connect/disconnect, retry with backoff, auth short-circuit) → `crates/mh-service/src/grpc/mc_client.rs`
- gRPC: MH service (RegisterMeeting via SessionManagerHandle) → `crates/mh-service/src/grpc/mh_service.rs`
- gRPC: auth layer (MhAuthLayer: JWKS + scope + Layer 2 service_type routing, ADR-0003) → `crates/mh-service/src/grpc/auth_interceptor.rs`
- gRPC: classify_jwt_error (JwtError → bounded failure_reason label) → `crates/mh-service/src/grpc/auth_interceptor.rs:classify_jwt_error`
- JWT validation (MhJwtValidator wrapping common JwtValidator, token_type=meeting) → `crates/mh-service/src/auth/mod.rs`
- Session management (SessionManagerActor/Handle, pending promotion via Notify) → `crates/mh-service/src/session/mod.rs`
- WebTransport server (TLS 1.3, capacity-bounded accept loop) → `crates/mh-service/src/webtransport/server.rs`
- WebTransport connection handler (framed JWT read, provisional accept, MC notifications) → `crates/mh-service/src/webtransport/connection.rs`
- Provisional-accept select helper (Registered/Timeout/Cancelled outcomes) → `crates/mh-service/src/webtransport/connection.rs:await_meeting_registration`
- Health + readiness endpoints → `crates/mh-service/src/observability/health.rs`
- Prometheus metric recorders → `crates/mh-service/src/observability/metrics.rs`
- MH metrics catalog → `docs/observability/metrics/mh-service.md`

## Media Protocol (v2 frame codec — ADR-0036 §2)
- Frame v2 layout, exported size constants, decoded `MediaFrameView` → `crates/media-protocol/src/frame.rs`
- Forward-path buffer-sizing constants (`MAX_HEADER_BYTES`, `MAX_FRAME_BYTES`, `MAX_PAYLOAD_BYTES`) → `frame.rs` (derived; do not recompute in MH)
- Sequence-reset asymmetry: `hop_sequence` resettable / `stream_sequence` MUST NEVER reset (AEAD nonce input — reset = GCM auth-key recovery) → `frame.rs` accessors (task 16 `HopSequence` lands here, no reset API)
- Single layout parser + four entry points (`decode_datagram`, `decode_stream_frame`, `peek_frame_len`, `rewrite_relay_region`); reject reasons via enum-derived `ALL_REJECT_REASONS` → `crates/media-protocol/src/codec.rs`
- Relay-region rewrite (offset derived per frame via `parse_layout`, NEVER a constant) → `codec.rs:rewrite_relay_region()`
- Reader-side pre-allocation bound (`MAX_PAYLOAD_BYTES` enforced before buffering) → `codec.rs:peek_frame_len()`
- Extension TLV registry + salience `0x01` selector input (value `0..=100`) → `crates/media-protocol/src/extensions.rs` (story-5 selector)
- Fuzz: decode / roundtrip → `crates/media-protocol/fuzz/fuzz_targets/`

## Proto Definitions
- MC↔MH / MH↔GC / MH→MC RPCs + assignment + DisconnectReason → `proto/dark_tower/internal/v1/internal.proto`
- Client signaling (MediaStream, StreamAssignment, MediaConnectionUpdate, layout) → `proto/dark_tower/signaling/v1/signaling.proto`
- Generated Rust code → `crates/proto-gen/build.rs`

## Integration Seams
- MH → GC registration/heartbeat → `crates/mh-service/src/grpc/gc_client.rs`
- MH → MC notifications (connect/disconnect) → `crates/mh-service/src/grpc/mc_client.rs`
- MC → MH gRPC service (RegisterMeeting) → `crates/mh-service/src/grpc/mh_service.rs`
- MH → AC token management → `crates/common/src/token_manager.rs`
- MH → AC JWKS (meeting + service token validation) → `crates/common/src/jwt.rs:JwksClient`
- Client → MH WebTransport (QUIC/TLS 1.3, framed JWT first) → `crates/mh-service/src/webtransport/server.rs`
- MH depends on common, proto-gen, media-protocol crates

## Testing
- Integration: GC mock → `crates/mh-service/tests/gc_integration.rs`
- Integration: MC client retry + auth short-circuit → `crates/mh-service/tests/mc_client_integration.rs`
- Integration: MhAuthLayer over real tonic (alg:none + HS256, Layer 2 routing) → `crates/mh-service/tests/auth_layer_integration.rs`
- Integration: RegisterMeeting over real gRPC → `crates/mh-service/tests/register_meeting_integration.rs`
- Integration: WebTransport accept path, provisional timeout, MC notify lifecycle → `crates/mh-service/tests/webtransport_integration.rs`
- Integration: WebTransport accept_loop component coverage → `crates/mh-service/tests/webtransport_accept_loop_integration.rs`
- Integration rigs (JWKS mock, mock MC, gRPC rig, WT rig, token minters) → `crates/mh-service/tests/common/`
- Env-tests: full Kind cluster MH QUIC flow (R-33 scenarios) → `crates/env-tests/tests/26_mh_quic.rs`

## Infrastructure & Operations
- K8s deployment (ports, probes, env, downward API, advertise addresses) → `infra/services/mh-service/mh-0-deployment.yaml`, `infra/services/mh-service/mh-1-deployment.yaml`
- K8s configmap (bind addresses, region, GC URL, AC_JWKS_URL) → `infra/services/mh-service/configmap.yaml`
- MH↔MC network policy → `infra/services/mh-service/network-policy.yaml`, `infra/services/mc-service/network-policy.yaml`
- Grafana dashboard + kustomization → `infra/grafana/dashboards/mh-overview.json`, `infra/grafana/kustomization.yaml`
- Deployment runbook (post-deploy checklist) → `docs/runbooks/mh-deployment.md`
- Incident response runbook → `docs/runbooks/mh-incident-response.md`
