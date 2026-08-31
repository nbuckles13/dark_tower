# Protocol Navigation

## Architecture & Design
- API versioning strategy → ADR-0004
- User auth meeting access (protocol implications) → ADR-0020
- Proto conventions (buf STANDARD, file layout, `vN` suffix, bare RPC names, distinct response types) → `docs/protocol/CONVENTIONS.md`
- API contracts and component interactions → `docs/API_CONTRACTS.md`
- WebTransport connection flow and message framing → `docs/WEBTRANSPORT_FLOW.md`
- Client architecture (frame codec, BigInt/u64, SFrame layering, trace context proto fields) → ADR-0028

## Proto Definitions
- Signaling proto (client-server) → `proto/dark_tower/signaling/v1/signaling.proto`
- Internal proto (service-to-service) → `proto/dark_tower/internal/v1/internal.proto`
- Proto codegen build script → `crates/proto-gen/build.rs`
- Proto re-exports and module wiring → `crates/proto-gen/src/lib.rs`

## Media Protocol Crate (frame header v2, ADR-0036 §2 + Appendix)
- Crate root; no-crypto / no-telemetry scope, lint denies → `crates/media-protocol/src/lib.rs`
- Wire layout, size constants, flags, zero-copy view, publisher/relay split → `crates/media-protocol/src/frame.rs`
- TLV extension grammar + registry (type → value length → accepted set) → `crates/media-protocol/src/extensions.rs`
- Single parser, four entry points, encode, reject-reason vocabulary → `crates/media-protocol/src/codec.rs`
- Relay-region rewrite (offset derived per frame, never a constant) → `crates/media-protocol/src/codec.rs:rewrite_relay_region()`
- Reader-side pre-allocation bound → `crates/media-protocol/src/codec.rs:peek_frame_len()`
- No-skipped-byte proof (every byte and bit mutated) → `crates/media-protocol/tests/byte_coverage.rs`
- Reject reasons, precedence, producibility, prefix invariant → `crates/media-protocol/tests/reject_reasons.rs`
- Zero-copy aliasing, canonical roundtrip, relay rewrite, redaction → `crates/media-protocol/tests/frame_properties.rs`
- Fuzz seed corpus (asserted under `cargo test`) + shared 2x2 fixture matrix → `crates/media-protocol/tests/corpus_seeds.rs`, `crates/media-protocol/tests/common/mod.rs`
- Codec fuzzers (decode, roundtrip) → `crates/media-protocol/fuzz/fuzz_targets/`

## gRPC Services (internal.proto)
- MediaHandlerService (MC→MH): Register, RegisterMeeting, RouteMedia, StreamTelemetry → `proto/dark_tower/internal/v1/internal.proto`
- MediaCoordinationService (MH→MC): NotifyParticipantConnected, NotifyParticipantDisconnected → `proto/dark_tower/internal/v1/internal.proto`
- MeetingControllerService (GC→MC): AssignMeetingWithMh → `proto/dark_tower/internal/v1/internal.proto`
- GlobalControllerService (MC→GC): RegisterMC, FastHeartbeat, ComprehensiveHeartbeat → `proto/dark_tower/internal/v1/internal.proto`
- MediaHandlerRegistryService (MH→GC): RegisterMH, SendLoadReport → `proto/dark_tower/internal/v1/internal.proto`
- DisconnectReason enum (bounded for metrics) → `proto/dark_tower/internal/v1/internal.proto`

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
- MediaServerInfo (in JoinResponse.media_servers) → `proto/dark_tower/signaling/v1/signaling.proto`
- MediaConnectionUpdate (client→MC; per-MH ConnectionState statuses, browser-client-join R-60) → `proto/dark_tower/signaling/v1/signaling.proto`
- MediaConnectionUpdate handler → `crates/mc-service/src/webtransport/connection.rs`

## HTTP API Error Contracts
- GC error taxonomy (HTTP status + `error.code` + `error_type` label per variant) → `crates/gc-service/src/errors.rs`
- Meeting-creation refusal causes (`MeetingRefusal`, `CreateMeetingOutcome`) → `crates/gc-service/src/repositories/meetings.rs`

## Story Manifest Schema (dt-story)
- Manifest contract, versioning and per-task state → ADR-0035
- Manifest/Task schema, `deny_unknown_fields`, `v1` marker, canonical `SLUG_PATTERN` → `crates/dt-story/src/manifest.rs`
- Fence-safe block emission → `crates/dt-story/src/manifest.rs:to_block_body()`
- Block discovery and orphan-entry detection → `crates/dt-story/src/markdown.rs:find_manifest_block()`
- CLI surface (`validate`, `next`, `complete --slug`, `add-task --deps`) → `crates/dt-story/src/main.rs`
- Manifest validity guard → `scripts/guards/simple/validate-story-manifest.sh`
- Slug-class drift guard → `scripts/guards/simple/validate-slug-class-sync.sh`

## Integration Seams
- Proto-gen consumed by services (re-exports prost::Message, tonic) → `crates/proto-gen/src/lib.rs`
- Media protocol consumed by MH service → `crates/media-protocol/Cargo.toml`
- MH gRPC metrics (method cardinality) → `crates/mh-service/src/observability/metrics.rs:record_grpc_request()`
