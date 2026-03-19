use miette::{IntoDiagnostic, Result};
use vetta_core::domain::Transcript;

pub fn emit(transcript: &Transcript) -> Result<()> {
    let json = serde_json::to_string_pretty(transcript).into_diagnostic()?;

    println!("{json}");

    Ok(())
}
