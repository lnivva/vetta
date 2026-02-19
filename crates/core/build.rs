use std::env;
use std::path::PathBuf;

/// Build a script entry point that compiles the speech protobuf into Rust types using tonic
/// while disabling server code generation.
///
/// Emits cargo rerun-if-changed directives for the proto files, configures tonic_build to not
/// generate server code, and compiles the proto located at ../../proto/speech/speech.proto
/// relative to the crate manifest directory.
///
/// Returns `Ok(())` on success or an error if environment
/// access, path handling, or protobuf compilation fails.
///
/// # Examples
///
/// ```
/// // In tests you can call the build script function directly and assert it succeeds.
/// # fn try_main() -> Result<(), Box<dyn std::error::Error>> {
/// let res = crate::main();
/// assert!(res.is_ok());
/// # Ok(())
/// # }
/// ```
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
