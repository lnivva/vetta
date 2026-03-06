use miette::{Context, IntoDiagnostic, Result};
use std::path::Path;
use std::io::{self, Write}
    
pub fn write_file(path: &Path, content: &str) -> Result<()> {
    std::fs::write(path, content.as_bytes())
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to write to {}", path.display()))
}

pub fn write_stdout(content: &str) {
    print!("{content}");
    let _ = io::stdout().flush();
}
