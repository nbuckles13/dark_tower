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
    tonic_build::configure().compile_protos(
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
