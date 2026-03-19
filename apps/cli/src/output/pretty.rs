use miette::{IntoDiagnostic, Result};
use std::path::Path;
use vetta_core::domain::Transcript;

pub fn emit(
    transcript: &Transcript,
    out: Option<&Path>,
    print: bool,
) -> Result<()> {
    let text = transcript.to_string();

    if let Some(path) = out {
        std::fs::write(path, &text).into_diagnostic()?;
    }

    if print {
        println!("{}", text);
    }

    Ok(())
}