use miette::{Context, IntoDiagnostic, Result};
use std::io::{self, Write};
use std::path::Path;

pub fn write_file(path: &Path, content: &str) -> Result<()> {
    std::fs::write(path, content.as_bytes())
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to write to {}", path.display()))
}

pub fn write_stdout(content: &str) -> Result<()> {
    let mut stdout = io::stdout();
    stdout
        .write_all(content.as_bytes())
        .into_diagnostic()
        .wrap_err("Failed to write to stdout")?;
    stdout
        .flush()
        .into_diagnostic()
        .wrap_err("Failed to flush stdout")?;
    Ok(())
}
