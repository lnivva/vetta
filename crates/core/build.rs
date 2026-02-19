/// Generates Rust gRPC client code from ../../proto/speech.proto with server-side code generation disabled.
///
/// Configures `tonic_build` to disable server generation and compiles the proto file using `../../proto` as the include path.
/// Returns `Ok(())` on success and propagates any error from proto compilation or code generation.
///
/// # Examples
///
/// ```
/// # fn try_main() -> Result<(), Box<dyn std::error::Error>> {
/// main()?;
/// # Ok(()) }
/// ```
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(false)
        .compile_protos(&["../../proto/speech.proto"], &["../../proto"])?;
    Ok(())
}