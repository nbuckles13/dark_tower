# Media Handler Navigation

## Architecture & Design
- SFU architecture, MH registration/load reports → ADR-0010 (Section 4a)
- MH assignment, selection algorithm, cross-region coordination → ADR-0023 (Section 5)
- Service authentication (MH→GC OAuth, MC→MH Bearer) → ADR-0003
- Observability pattern (metrics crate facade) → ADR-0011
- Dashboard metric presentation (counters vs rates) → ADR-0029
- Fuzz testing for media frames → ADR-0006

## Code Locations
- Service entry point → `crates/mh-service/src/main.rs`
- Library root (module declarations) → `crates/mh-service/src/lib.rs`
- Config (SecretString, env loading, TLS paths) → `crates/mh-service/src/config.rs`
- Error types (MhError hierarchy) → `crates/mh-service/src/errors.rs`
- gRPC: GC client (registration, heartbeats, re-registration) → `crates/mh-service/src/grpc/gc_client.rs`
- gRPC: MH service stub (Register, RouteMedia, StreamTelemetry) → `crates/mh-service/src/grpc/mh_service.rs`
- gRPC: auth interceptor (Bearer validation for MC→MH) → `crates/mh-service/src/grpc/auth_interceptor.rs`
- Health + readiness endpoints → `crates/mh-service/src/observability/health.rs`
- Prometheus metric wrappers → `crates/mh-service/src/observability/metrics.rs`
- MH metrics catalog → `docs/observability/metrics/mh-service.md`

## Media Protocol
- Frame types (MediaFrame, FrameType, FrameFlags) → `crates/media-protocol/src/frame.rs`
- Binary codec (encode_frame, decode_frame) → `crates/media-protocol/src/codec.rs`
- Stream state (MediaStream, StreamConfig) → `crates/media-protocol/src/stream.rs`
- Fuzz: decode → `crates/media-protocol/fuzz/fuzz_targets/codec_decode.rs`
- Fuzz: roundtrip → `crates/media-protocol/fuzz/fuzz_targets/codec_roundtrip.rs`

## Proto Definitions
- MC-to-MH RPC (Register, RouteMedia, StreamTelemetry) → `proto/internal.proto`
- MH-to-GC RPC (RegisterMH, SendLoadReport) → `proto/internal.proto`
- MH assignment messages (MhAssignment, MhRole) → `proto/internal.proto`
- Client signaling (MediaStream, StreamAssignment, layout) → `proto/signaling.proto`
- Generated Rust code → `crates/proto-gen/build.rs`

## Integration Seams
- MH → GC registration/heartbeat → `crates/mh-service/src/grpc/gc_client.rs`
- MC → MH gRPC service → `crates/mh-service/src/grpc/mh_service.rs`
- MH → AC token management → `crates/common/src/token_manager.rs`
- MH depends on common crate → `crates/common/src/lib.rs`
- MH depends on proto-gen → `crates/proto-gen/src/lib.rs`
- MH depends on media-protocol → `crates/media-protocol/src/lib.rs`

## Testing
- GC integration tests (mock server) → `crates/mh-service/tests/gc_integration.rs`
- Config unit tests → `crates/mh-service/src/config.rs`
- Auth interceptor unit tests → `crates/mh-service/src/grpc/auth_interceptor.rs`
- Health endpoint tests → `crates/mh-service/src/observability/health.rs`
- Metrics unit tests → `crates/mh-service/src/observability/metrics.rs`

## Infrastructure
- Grafana dashboard → `infra/grafana/dashboards/mh-overview.json`
- Grafana kustomization → `infra/grafana/kustomization.yaml`
