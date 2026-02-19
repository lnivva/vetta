use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto/speech/speech.proto");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);

    let proto_path = manifest_dir.join("../../proto/speech/speech.proto");
    let proto_dir = manifest_dir.join("../../proto");

    println!("cargo:rerun-if-changed={}", proto_path.display());

    tonic_build::configure()
        .build_server(false)
        .compile_protos(&[proto_path], &[proto_dir])?;

    Ok(())
}
