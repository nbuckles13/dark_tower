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
- Crate root; no-crypto/no-telemetry scope, lint denies → `crates/media-protocol/src/lib.rs`
- Wire layout, size constants, flags, zero-copy view, publisher/relay split, key-id layout → `frame.rs`; TLV grammar + registry → `extensions.rs`
- Single parser, four entry points, encode, reject reasons; relay rewrite (`rewrite_relay_region()`), reader bound (`peek_frame_len()`) → `crates/media-protocol/src/codec.rs`
- Tests (no-skipped-byte proof, reject-reason precedence, zero-copy/roundtrip/redaction, fuzz corpus) → `crates/media-protocol/tests/`; fuzzers → `fuzz/fuzz_targets/`

## gRPC Services (all in `proto/dark_tower/internal/v1/internal.proto`)
- MediaHandlerService (MC→MH): **RegisterMeeting only** — one RPC by design (ADR-0036 §8: registration IS the control plane and gains fields, not sibling RPCs). `Register`, `RouteMedia`, `StreamTelemetry` retired 2026-09-01; tombstone block in the proto
- MediaCoordinationService (MH→MC): NotifyParticipantConnected/Disconnected
- MeetingControllerService (GC→MC): AssignMeetingWithMh
- GlobalControllerService (MC→GC): RegisterMC, FastHeartbeat, ComprehensiveHeartbeat
- MediaHandlerRegistryService (MH→GC): RegisterMH, SendLoadReport
- DisconnectReason enum (bounded for metrics)

## ADR-0036 MC→MH Control Plane (internal.proto, 2026-09-01 reshape)
One RPC by design; all shapes in `proto/dark_tower/internal/v1/internal.proto`.
- Self-contained forwarding instruction (§6; NOT parallel lists) → `EgressStream`; empty-but-named meeting-level rules → `SelectionRules`
- `sender_id`-is-meeting-scoped MUST (no global index; malformed rejects, not-yet-connected holds) → `SubscriberSlot`; key-id-pair source ref, not `MediaStream` (§7 type-blind) → `CandidateSource`
- Output-derived generation, three-"generation" trap list, task-5/6 ordering constraint → `RegisterMeetingRequest.policy_generation`
- **Applied**-never-received echo, zero=nothing-applied, all-edges-or-none, canonical `outcome` label set → `RegisterMeetingResponse.applied_generation`
- UNSPECIFIED echo is a mismatch NOT a skip; expires at video → `.transport_mode`; restart detection, per-process → `.process_start_epoch_ms`
- Wire-shape tests + "what these do NOT assert" header → `crates/proto-gen/tests/internal_roundtrip.rs`; runtime MUSTs no story-1 test can catch → `docs/TODO.md` §Media Path Obligations

## gRPC Service Implementations
- MH MediaHandlerService → `crates/mh-service/src/grpc/mh_service.rs`
- MC MeetingControllerService, MediaCoordinationService → `crates/mc-service/src/grpc/mc_service.rs`, `media_coordination.rs`

## gRPC Clients (cross-service)
- GC→MC (AssignMeetingWithMh) → `crates/gc-service/src/services/mc_client.rs`; GC MH selection → `services/mh_selection.rs`
- MC→MH (RegisterMeeting) + MhRegistrationClient seam → `crates/mc-service/src/grpc/mh_client.rs`; MH→MC → `crates/mh-service/src/grpc/mc_client.rs`

## Auth Layer Pattern (JWKS-based async tower Layer)
- MH / MC service-token auth layers → `crates/mh-service/src/grpc/auth_interceptor.rs`, `crates/mc-service/src/grpc/auth_interceptor.rs`

## ADR-0036 Media Contract (signaling.proto)
All message/enum shapes → `proto/dark_tower/signaling/v1/signaling.proto`
- Vocabulary: `MediaKind`, `Codec`, `TransportMode`, `SlotState` (7 §6 states + UNSPECIFIED)
- Receive capability, per-slot optional pin, slot-id scoping → `ReceiveCapability`/`ReceiveSlot`
- Send directive (transport mode, NOT §7 priority group) → `SendDirective`/`SendTarget`
- Slot assignment, attribution chain, switch-pending command id → `StreamAssignment`
- `sender_id` contract (1..=65535, zero invalid) → `JoinResponse.sender_id`; meeting KEK + generation → `JoinResponse.meeting_kek`, `MeetingKekUpdate`
- Roster identity key (no attestation yet) → `Participant.identity_public_key`; client- vs server-mute (§5) → `MuteRequest`/`ServerMuteRequest`
- Redacting `Debug` → `crates/proto-gen/build.rs`, `src/lib.rs`; wire-shape tests → `crates/proto-gen/tests/signaling_roundtrip.rs`
- Codegen oracle (presence + absence, both protos) → `packages/proto-gen/scripts/verify-codegen.sh`; SDK codec vocabulary → `packages/sdk-core/src/signaling/codecMap.ts`

## Signaling Messages (signaling.proto)
- MediaConnectionUpdate (client→MC per-MH ConnectionState, R-60) + handler → `crates/mc-service/src/webtransport/connection.rs`

## HTTP API Error Contracts
- GC error taxonomy (status + `error.code` + `error_type`) → `crates/gc-service/src/errors.rs`; meeting-creation refusals → `crates/gc-service/src/repositories/meetings.rs`

## Story Manifest Schema (dt-story)
- Manifest contract, versioning, per-task state → ADR-0035; schema, `deny_unknown_fields`, `v1` marker, `SLUG_PATTERN`, fence-safe emission → `crates/dt-story/src/manifest.rs`
- Block discovery, orphan detection → `crates/dt-story/src/markdown.rs:find_manifest_block()`
- CLI (`validate`, `next`, `complete --slug`, `add-task --deps`) → `crates/dt-story/src/main.rs`
- Guards: manifest validity, slug-class drift → `scripts/guards/simple/validate-story-manifest.sh`, `validate-slug-class-sync.sh`

## Integration Seams
- Proto-gen consumed by services (re-exports prost::Message, tonic) → `crates/proto-gen/src/lib.rs`
- MH gRPC metrics (method cardinality) → `crates/mh-service/src/observability/metrics.rs:record_grpc_request()`
