/// Generates Rust gRPC code from the speech.proto file with server-side code generation disabled.
///
/// Configures `tonic_build` to disable server generation and compiles `../../proto/speech.proto` using `../../proto` as the include path. Returns `Ok(())` when compilation succeeds.
///
/// # Errors
///
/// Returns an error if the proto compilation or code generation fails.
///
/// # Examples
///
/// ```
/// # fn try_main() -> Result<(), Box<dyn std::error::Error>> {
/// main()?;
/// # Ok(())
/// # }
/// ```
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(false)
        .compile_protos(&["../../proto/speech.proto"], &["../../proto"])?;
    Ok(())
}