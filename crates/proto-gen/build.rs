// Build script to compile Protocol Buffer definitions

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protobuf files
    prost_build::Config::new()
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
