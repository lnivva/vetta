use miette::Result;
use std::path::Path;
use vetta_core::domain::Transcript;

use crate::context::{AppContext, OutputMode};

pub mod json;
pub mod pretty;

pub fn emit(
    _ctx: &AppContext,
    transcript: &Transcript,
    out: Option<&Path>,
    mode: OutputMode,
) -> Result<()> {
    match mode {
        OutputMode::Pretty => {
            pretty::print_transcript(transcript)?;
        }

        OutputMode::Json => {
            if let Some(path) = out {
                json::write_json(path, transcript)?;
            }
        }

        OutputMode::Both => {
            if let Some(path) = out {
                json::write_json(path, transcript)?;
            }
            pretty::print_transcript(transcript)?;
        }
    }

    Ok(())
}
