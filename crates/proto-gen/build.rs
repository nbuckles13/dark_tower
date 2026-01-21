// Build script to compile Protocol Buffer definitions with gRPC service traits

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protobuf files with tonic for gRPC service generation
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
