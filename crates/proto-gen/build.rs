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
    tonic_build::configure()
        .out_dir("src/generated")
        .compile_protos(
            &["../../proto/signaling.proto", "../../proto/internal.proto"],
            &["../../proto/"],
        )?;

    // Tell Cargo to rerun if proto files change
    println!("cargo:rerun-if-changed=../../proto/signaling.proto");
    println!("cargo:rerun-if-changed=../../proto/internal.proto");

    Ok(())
}
