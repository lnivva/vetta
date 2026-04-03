use miette::{Context, IntoDiagnostic, Result};
use std::path::Path;
use vetta_core::domain::Transcript;

pub fn write_json(path: &Path, transcript: &Transcript) -> Result<()> {
    let json = serde_json::to_string_pretty(transcript)
        .into_diagnostic()
        .wrap_err("Failed to serialize transcript to JSON")?;

    std::fs::write(path, json)
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to write JSON to {}", path.display()))?;

    Ok(())
}
