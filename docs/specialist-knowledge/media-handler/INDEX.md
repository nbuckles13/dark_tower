# Media Handler Navigation

## Architecture & Design
- Actor pattern for MH → ADR-0001 (Media Handler section)
- SFU architecture, MH registration/load reports → ADR-0010 (Section 4a)
- MH assignment, selection algorithm, cross-region coordination → ADR-0023 (Section 5)
- Fuzz testing for media frames → ADR-0006
- Service authentication (MH uses meeting-scoped tokens) → ADR-0003

## Code Locations
- Service entry point → `crates/mh-service/src/main.rs`
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
- Generated Rust code → `crates/proto-gen/src/generated/dark_tower.internal.rs`

## Integration Seams
- MH depends on common crate → `crates/common/src/lib.rs`
- MH depends on proto-gen → `crates/proto-gen/src/lib.rs`
- MH depends on media-protocol → `crates/media-protocol/src/lib.rs`

