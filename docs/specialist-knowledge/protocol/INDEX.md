# Protocol Navigation

## Architecture & Design
- API versioning strategy → `docs/decisions/adr-0004-api-versioning.md`
- User auth meeting access (protocol implications) → `docs/decisions/adr-0020-user-auth-meeting-access.md`
- API contracts and component interactions → `docs/API_CONTRACTS.md`
- WebTransport connection flow and message framing → `docs/WEBTRANSPORT_FLOW.md`

## Code Locations
- Signaling proto (client-server) → `proto/signaling.proto`
- Internal proto (service-to-service) → `proto/internal.proto`
- Proto codegen build script → `crates/proto-gen/build.rs`
- Proto codegen crate config → `crates/proto-gen/Cargo.toml`
- Proto re-exports and module wiring → `crates/proto-gen/src/lib.rs`
- Generated signaling code → `crates/proto-gen/src/generated/dark_tower.signaling.rs`
- Generated internal code → `crates/proto-gen/src/generated/dark_tower.internal.rs`
- Media protocol crate root → `crates/media-protocol/src/lib.rs`
- Binary frame definitions → `crates/media-protocol/src/frame.rs`
- Codec encode/decode → `crates/media-protocol/src/codec.rs`
- Stream handling → `crates/media-protocol/src/stream.rs`

## Fuzz Targets
- Codec decode fuzzer → `crates/media-protocol/fuzz/fuzz_targets/codec_decode.rs`
- Codec roundtrip fuzzer → `crates/media-protocol/fuzz/fuzz_targets/codec_roundtrip.rs`

## Integration Seams
- Proto-gen consumed by services → `crates/proto-gen/src/lib.rs` (re-exports prost::Message, tonic)
- Media protocol consumed by MH service → `crates/media-protocol/Cargo.toml`
