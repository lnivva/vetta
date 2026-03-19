pub mod pretty;
pub mod json;

use crate::context::{AppContext, OutputMode};
use miette::Result;
use std::path::Path;

use vetta_core::domain::Transcript;

pub fn emit(
    ctx: &AppContext,
    transcript: &Transcript,
    out: Option<&Path>,
    print: bool,
) -> Result<()> {
    match ctx.output {
        OutputMode::Pretty => pretty::emit(transcript, out, print),
        OutputMode::Json => json::emit(transcript),
    }
}