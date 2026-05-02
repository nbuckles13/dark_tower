# Protocol Navigation

## Architecture & Design
- API versioning strategy → ADR-0004
- User auth meeting access (protocol implications) → ADR-0020
- API contracts and component interactions → `docs/API_CONTRACTS.md`
- WebTransport connection flow and message framing → `docs/WEBTRANSPORT_FLOW.md`
- Client architecture (frame codec, BigInt/u64, SFrame layering, trace context proto fields) → ADR-0028

## Proto Definitions
- Signaling proto (client-server) → `proto/signaling.proto`
- Internal proto (service-to-service) → `proto/internal.proto`
- Proto codegen build script → `crates/proto-gen/build.rs`
- Proto re-exports and module wiring → `crates/proto-gen/src/lib.rs`

## Media Protocol Crate
- Media protocol crate root → `crates/media-protocol/src/lib.rs`
- Binary frame definitions → `crates/media-protocol/src/frame.rs`
- Codec encode/decode → `crates/media-protocol/src/codec.rs`
- Stream handling → `crates/media-protocol/src/stream.rs`
- Codec decode fuzzer → `crates/media-protocol/fuzz/fuzz_targets/codec_decode.rs`
- Codec roundtrip fuzzer → `crates/media-protocol/fuzz/fuzz_targets/codec_roundtrip.rs`

## gRPC Services (internal.proto)
- MediaHandlerService (MC→MH): Register, RegisterMeeting, RouteMedia, StreamTelemetry → `proto/internal.proto`
- MediaCoordinationService (MH→MC): NotifyParticipantConnected, NotifyParticipantDisconnected → `proto/internal.proto`
- MeetingControllerService (GC→MC): AssignMeetingWithMh → `proto/internal.proto`
- GlobalControllerService (MC→GC): RegisterMC, FastHeartbeat, ComprehensiveHeartbeat → `proto/internal.proto`
- MediaHandlerRegistryService (MH→GC): RegisterMH, SendLoadReport → `proto/internal.proto`
- DisconnectReason enum (bounded for metrics) → `proto/internal.proto`

## gRPC Service Implementations
- MH MediaHandlerService impl → `crates/mh-service/src/grpc/mh_service.rs`
- MC MeetingControllerService impl → `crates/mc-service/src/grpc/mc_service.rs`
- MC MediaCoordinationService impl → `crates/mc-service/src/grpc/media_coordination.rs`

## gRPC Clients (cross-service)
- GC→MC client (AssignMeetingWithMh) → `crates/gc-service/src/services/mc_client.rs`
- MC→MH client (RegisterMeeting) → `crates/mc-service/src/grpc/mh_client.rs`
- MhRegistrationClient trait (testability seam) → `crates/mc-service/src/grpc/mh_client.rs`
- MH→MC client (NotifyParticipant*) → `crates/mh-service/src/grpc/mc_client.rs`
- GC MH selection (handlers Vec, MhAssignmentInfo) → `crates/gc-service/src/services/mh_selection.rs`

## Auth Layer Pattern (JWKS-based async tower Layer)
- MH service-token auth layer → `crates/mh-service/src/grpc/auth_interceptor.rs`
- MC service-token auth layer → `crates/mc-service/src/grpc/auth_interceptor.rs`

## Signaling Messages (signaling.proto)
- MediaServerInfo (in JoinResponse.media_servers) → `proto/signaling.proto`
- MediaConnectionFailed (client→MC) → `proto/signaling.proto`
- MediaConnectionFailed handler → `crates/mc-service/src/webtransport/connection.rs`

## Integration Seams
- Proto-gen consumed by services (re-exports prost::Message, tonic) → `crates/proto-gen/src/lib.rs`
- Media protocol consumed by MH service → `crates/media-protocol/Cargo.toml`
- MH gRPC metrics (method cardinality) → `crates/mh-service/src/observability/metrics.rs:record_grpc_request()`
