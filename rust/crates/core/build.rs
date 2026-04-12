use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../../proto/speech/speech.proto");
    println!("cargo:rerun-if-changed=../../../proto/embeddings/embeddings.proto");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);

    let speech_proto = manifest_dir.join("../../../proto/speech/speech.proto");
    let embeddings_proto = manifest_dir.join("../../../proto/embeddings/embeddings.proto");

    let proto_dir = manifest_dir.join("../../../proto");

    tonic_prost_build::configure()
        .build_server(false)
        .compile_protos(&[speech_proto, embeddings_proto], &[proto_dir])?;

    Ok(())
}
