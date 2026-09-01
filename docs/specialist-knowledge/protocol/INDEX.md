# Protocol Navigation

## Architecture & Design
- API versioning → ADR-0004; user auth meeting access → ADR-0020
- Media flow: frame format, sender auth, KEK model, send/receive contract → ADR-0036
- Proto conventions (buf STANDARD, `vN`, bare RPC names, distinct response types, reserve-the-vacated-tag-and-name §5, intentional-wire-break protocol) → `docs/protocol/CONVENTIONS.md`
- API contracts → `docs/API_CONTRACTS.md`; WebTransport flow/framing → `docs/WEBTRANSPORT_FLOW.md`
- Client architecture (frame codec, SFrame layering, trace context fields) → ADR-0028

## Proto Definitions
- Signaling (client↔MC) → `proto/dark_tower/signaling/v1/signaling.proto`; internal (service↔service) → `proto/dark_tower/internal/v1/internal.proto`
- Codegen build script + skip_debug → `crates/proto-gen/build.rs`; re-exports, module wiring, redacting Debug → `crates/proto-gen/src/lib.rs`

## Media Protocol Crate (frame header v2, ADR-0036 §2 + Appendix)
- Crate root; no-crypto / no-telemetry scope, lint denies → `crates/media-protocol/src/lib.rs`
- Wire layout, size constants, flags, zero-copy view, publisher/relay split, key-id layout → `crates/media-protocol/src/frame.rs`
- TLV extension grammar + registry → `crates/media-protocol/src/extensions.rs`
- Single parser, four entry points, encode, reject reasons; relay rewrite (`rewrite_relay_region()`), reader bound (`peek_frame_len()`) → `crates/media-protocol/src/codec.rs`
- Tests: no-skipped-byte proof, reject-reason precedence, zero-copy/roundtrip/redaction, fuzz seed corpus + shared fixtures → `crates/media-protocol/tests/`
- Codec fuzzers (decode, roundtrip) → `crates/media-protocol/fuzz/fuzz_targets/`

## gRPC Services (all in `proto/dark_tower/internal/v1/internal.proto`)
- MediaHandlerService (MC→MH): Register, RegisterMeeting, RouteMedia, StreamTelemetry
- MediaCoordinationService (MH→MC): NotifyParticipantConnected/Disconnected
- MeetingControllerService (GC→MC): AssignMeetingWithMh
- GlobalControllerService (MC→GC): RegisterMC, FastHeartbeat, ComprehensiveHeartbeat
- MediaHandlerRegistryService (MH→GC): RegisterMH, SendLoadReport
- DisconnectReason enum (bounded for metrics)

## gRPC Service Implementations
- MH MediaHandlerService → `crates/mh-service/src/grpc/mh_service.rs`
- MC MeetingControllerService, MediaCoordinationService → `crates/mc-service/src/grpc/mc_service.rs`, `media_coordination.rs`

## gRPC Clients (cross-service)
- GC→MC (AssignMeetingWithMh) → `crates/gc-service/src/services/mc_client.rs`
- MC→MH (RegisterMeeting) + MhRegistrationClient testability seam → `crates/mc-service/src/grpc/mh_client.rs`
- MH→MC (NotifyParticipant*) → `crates/mh-service/src/grpc/mc_client.rs`
- GC MH selection (handlers Vec, MhAssignmentInfo) → `crates/gc-service/src/services/mh_selection.rs`

## Auth Layer Pattern (JWKS-based async tower Layer)
- MH / MC service-token auth layers → `crates/mh-service/src/grpc/auth_interceptor.rs`, `crates/mc-service/src/grpc/auth_interceptor.rs`

## ADR-0036 Media Contract (signaling.proto)
All message/enum shapes → `proto/dark_tower/signaling/v1/signaling.proto`
- Vocabulary: `MediaKind`, `Codec`, `TransportMode`, `SlotState` (7 §6 states + UNSPECIFIED)
- Receive capability, per-slot optional pin, slot-id scoping → `ReceiveCapability`/`ReceiveSlot`
- Send directive (transport mode, NOT §7 priority group) → `SendDirective`/`SendTarget`
- Slot assignment, attribution chain, switch-pending command id → `StreamAssignment`
- `sender_id` canonical contract (1..=65535, zero invalid) → `JoinResponse.sender_id`
- Meeting KEK, generation, additive push → `JoinResponse.meeting_kek`, `MeetingKekUpdate`
- Roster identity key, no attestation this story → `Participant.identity_public_key`
- Client-mute vs server-mute (§5) → `MuteRequest`/`ServerMuteRequest`
- Redacting `Debug` (skip_debug + impls) → `crates/proto-gen/build.rs`, `src/lib.rs`
- Wire-shape tests → `crates/proto-gen/tests/signaling_roundtrip.rs`
- Codegen oracle, presence + absence → `packages/proto-gen/scripts/verify-codegen.sh`
- SDK codec vocabulary, wire-keyed oracle → `packages/sdk-core/src/signaling/codecMap.ts`

## Signaling Messages (signaling.proto)
- MediaConnectionUpdate (client→MC per-MH ConnectionState, R-60) + handler → `crates/mc-service/src/webtransport/connection.rs`

## HTTP API Error Contracts
- GC error taxonomy (status + `error.code` + `error_type` label) → `crates/gc-service/src/errors.rs`
- Meeting-creation refusal causes → `crates/gc-service/src/repositories/meetings.rs`

## Story Manifest Schema (dt-story)
- Manifest contract, versioning, per-task state → ADR-0035
- Schema, `deny_unknown_fields`, `v1` marker, `SLUG_PATTERN`, fence-safe emission (`to_block_body()`) → `crates/dt-story/src/manifest.rs`
- Block discovery, orphan detection → `crates/dt-story/src/markdown.rs:find_manifest_block()`
- CLI (`validate`, `next`, `complete --slug`, `add-task --deps`) → `crates/dt-story/src/main.rs`
- Guards: manifest validity, slug-class drift → `scripts/guards/simple/validate-story-manifest.sh`, `validate-slug-class-sync.sh`

## Integration Seams
- Proto-gen consumed by services (re-exports prost::Message, tonic) → `crates/proto-gen/src/lib.rs`
- MH gRPC metrics (method cardinality) → `crates/mh-service/src/observability/metrics.rs:record_grpc_request()`
