// Build script to compile Protocol Buffer definitions with gRPC service traits.
//
// Generated code lands in `OUT_DIR` (target/.../build/proto-gen-*/out/) and is
// included from `lib.rs` via `include!(concat!(env!("OUT_DIR"), ...))`. No
// in-tree generated files, no `extern_path` remapping — every proto package
// gets a Rust module at its proto-package path (e.g. `dark_tower::signaling::v1`).
//
// `tonic-build` maps `.google.protobuf.*` to `::prost_types` by default, so
// `signaling.MhConnectionStatus.observed_at` (`google.protobuf.Timestamp`)
// generates as `::prost_types::Timestamp` without an explicit `extern_path`.
// WKT `.proto` files are resolved from the system protoc include path
// (`/usr/include/google/protobuf/`) — provided by the `libprotobuf-dev`
// package in `infra/devloop/Dockerfile`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Suppress prost's derived `Debug` on every message that carries live
    // credential or key material, and hand-write a redacting `Debug` for each
    // in `src/lib.rs` (ADR-0036 §4; the shape mirrors
    // `crates/media-protocol/src/frame.rs`'s `impl Debug for
    // WrappedTransmitKey`).
    //
    // This is a control, not a convention. Without it, `JoinResponse` derives
    // `Debug`, `ServerMessage` derives `Debug` transitively, and any `{:?}` on
    // a server message prints the meeting KEK in full. A dt-guard vocabulary
    // entry cannot catch that: `\bkek\b` does not match `meeting_kek` because
    // `_` is a word character, so the guard would report clean while the leak
    // shipped.
    //
    // `JoinRequest` is here for the same reason on the same mechanism: it
    // carries `join_token` (a bearer meeting JWT) and `binding_token` (the
    // HMAC session-binding credential).
    //
    // `MhConnectRequest` is here on the identical mechanism: its `join_token`
    // is documented as "same semantics as JoinRequest.join_token" (a bearer
    // meeting JWT), it rides inside `MhClientMessage` which derives `Debug`
    // transitively, and `crates/mh-service/src/webtransport/connection.rs`'s
    // malformed-envelope `warn!` arms are one `?envelope` away from logging it.
    // The `\btoken\b` guard gap above applies verbatim.
    tonic_build::configure()
        .skip_debug("dark_tower.signaling.v1.JoinRequest")
        .skip_debug("dark_tower.signaling.v1.JoinResponse")
        .skip_debug("dark_tower.signaling.v1.MeetingKekUpdate")
        .skip_debug("dark_tower.signaling.v1.MhConnectRequest")
        // `Participant` carries `name` (PII, ADR-0011 membership disclosure) and
        // `identity_public_key` (a stable per-participant identifier). It is
        // broadcast on every join via `ParticipantJoined`, which rides inside
        // `ServerMessage`'s transitive `Debug` — so redacting only `JoinResponse`
        // would leave the same PII printable one participant at a time. Redact
        // the type itself; both consumers (`JoinResponse` roster, which prints a
        // count, and `ParticipantJoined`) are covered by one impl.
        .skip_debug("dark_tower.signaling.v1.Participant")
        .compile_protos(
            &[
                "../../proto/dark_tower/signaling/v1/signaling.proto",
                "../../proto/dark_tower/internal/v1/internal.proto",
            ],
            &["../../proto/"],
        )?;

    println!("cargo:rerun-if-changed=../../proto/dark_tower/signaling/v1/signaling.proto");
    println!("cargo:rerun-if-changed=../../proto/dark_tower/internal/v1/internal.proto");

    Ok(())
}
