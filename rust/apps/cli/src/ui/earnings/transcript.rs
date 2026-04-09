use miette::{IntoDiagnostic, Result};
use std::io::{self, Write};
use vetta_core::domain::Transcript;

use crate::ui::{DOT, INDENT, Styles, TRANSCRIPT, separator, text_prefix, text_width, timestamp};

pub fn print_transcript(transcript: &Transcript) -> Result<()> {
    let mut out = io::stdout().lock();
    let speakers = transcript.unique_speakers();

    // ── Header ───────────────────────────────────────────────────
    writeln!(out).into_diagnostic()?;
    writeln!(out, "{INDENT}{}", separator()).into_diagnostic()?;
    writeln!(
        out,
        "{INDENT}{TRANSCRIPT} {}",
        Styles::heading().apply_to("TRANSCRIPT")
    )
    .into_diagnostic()?;
    writeln!(
        out,
        "{INDENT}{} segments  •  {} speakers  •  {} duration",
        Styles::stat().apply_to(transcript.segments.len()),
        Styles::stat().apply_to(speakers.len()),
        Styles::stat().apply_to(timestamp(transcript.duration())),
    )
    .into_diagnostic()?;
    writeln!(out, "{INDENT}{}", separator()).into_diagnostic()?;
    writeln!(out).into_diagnostic()?;

    // ── Turns ────────────────────────────────────────────────────
    let turns = transcript.as_dialogue();
    let prefix = text_prefix();

    for (i, turn) in turns.iter().enumerate() {
        if i > 0 {
            writeln!(out).into_diagnostic()?;
        }

        let speaker = if turn.speaker.is_empty() {
            "Unknown"
        } else {
            &turn.speaker
        };

        let idx = speakers.iter().position(|s| s == speaker).unwrap_or(0);

        writeln!(
            out,
            "{INDENT}{}  {}",
            Styles::speaker(idx).apply_to(speaker),
            Styles::dimmed().apply_to(format!("{DOT} [{}]", timestamp(turn.start_time))),
        )
        .into_diagnostic()?;

        for line in textwrap::fill(turn.text.trim(), text_width()).lines() {
            writeln!(out, "{prefix}{line}").into_diagnostic()?;
        }
    }

    // ── Footer ───────────────────────────────────────────────────
    writeln!(out).into_diagnostic()?;
    writeln!(out, "{INDENT}{}", separator()).into_diagnostic()?;
    writeln!(
        out,
        "{INDENT}{} {}",
        Styles::dimmed().apply_to("Total Duration:"),
        Styles::heading().apply_to(timestamp(transcript.duration())),
    )
    .into_diagnostic()?;
    writeln!(out).into_diagnostic()?;

    out.flush().into_diagnostic()?;
    Ok(())
}
