// Build script to compile Protocol Buffer definitions with gRPC service traits

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protobuf files with tonic for gRPC service generation.
    //
    // tonic-build maps `.google.protobuf.*` to `::prost_types` by default,
    // so `signaling.MhConnectionStatus.observed_at`
    // (`google.protobuf.Timestamp`) generates as `::prost_types::Timestamp`
    // without an explicit `extern_path`. WKT `.proto` files are resolved
    // from the system protoc include path (`/usr/include/google/protobuf/`)
    // — provided by the `libprotobuf-dev` package in
    // `infra/devloop/Dockerfile`.
    // Remap cross-package references so the generator emits
    // `crate::signaling::MediaStream` (matching Approach A's flat
    // `proto_gen::signaling` re-export) instead of the nested
    // `super::super::signaling::v1::MediaStream` that the proto-package-path
    // mirror would produce. Only the signaling package needs `extern_path`:
    // the sole cross-package reference is `internal.proto`'s import of
    // `MediaStream`. Listing the `internal` package here would suppress
    // type-definition emission for in-package types — keep it out.
    tonic_build::configure()
        .out_dir("src/generated")
        .extern_path(".dark_tower.signaling.v1", "crate::signaling")
        .compile_protos(
            &[
                "../../proto/dark_tower/signaling/v1/signaling.proto",
                "../../proto/dark_tower/internal/v1/internal.proto",
            ],
            &["../../proto/"],
        )?;

    // Tell Cargo to rerun if proto files change
    println!("cargo:rerun-if-changed=../../proto/dark_tower/signaling/v1/signaling.proto");
    println!("cargo:rerun-if-changed=../../proto/dark_tower/internal/v1/internal.proto");

    Ok(())
}
