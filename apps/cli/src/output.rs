use colored::*;
use miette::{Context, IntoDiagnostic, Result};
use std::io::{self, Write};
use std::path::Path;
use vetta_core::domain::Transcript;

pub fn write_file(path: &Path, content: &str) -> Result<()> {
    std::fs::write(path, content.as_bytes())
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to write to {}", path.display()))
}

/// Pretty-print a transcript to stdout with colors, timestamps, and speaker labels.
pub fn print_transcript(transcript: &Transcript) -> Result<()> {
    let mut stdout = io::stdout();
    let term_width = terminal_width().saturating_sub(6);
    let text_indent = "   │ ";
    let text_width = term_width.saturating_sub(text_indent.len()).max(40);

    let separator = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";

    // Header
    writeln!(stdout).into_diagnostic()?;
    writeln!(stdout, "   {}", separator.dimmed()).into_diagnostic()?;
    writeln!(stdout, "   {}", "TRANSCRIPT".bold()).into_diagnostic()?;
    writeln!(
        stdout,
        "   {} segments · {} speakers",
        transcript.segments.len().to_string().cyan(),
        transcript.unique_speakers().len().to_string().cyan(),
    )
    .into_diagnostic()?;
    writeln!(stdout, "   {}", separator.dimmed()).into_diagnostic()?;
    writeln!(stdout).into_diagnostic()?;

    let speaker_colors = [
        Color::Yellow,
        Color::Green,
        Color::Magenta,
        Color::Cyan,
        Color::Red,
        Color::Blue,
        Color::BrightYellow,
        Color::BrightGreen,
    ];

    let speakers = transcript.unique_speakers();
    let color_for = |speaker: &str| -> Color {
        speakers
            .iter()
            .position(|s| s == speaker)
            .map(|i| speaker_colors[i % speaker_colors.len()])
            .unwrap_or(Color::White)
    };

    // Use as_dialogue() to group consecutive segments by speaker
    let turns = transcript.as_dialogue();

    for (i, turn) in turns.iter().enumerate() {
        if i > 0 {
            writeln!(stdout).into_diagnostic()?;
        }

        let speaker = if turn.speaker.is_empty() {
            "Unknown"
        } else {
            &turn.speaker
        };
        let color = color_for(speaker);
        let timestamp = format_timestamp(turn.start_time);

        writeln!(
            stdout,
            "   {} {}",
            speaker.color(color).bold(),
            format!("[{timestamp}]").dimmed(),
        )
        .into_diagnostic()?;

        // Word-wrap the text and print with gutter
        let wrapped = textwrap::fill(turn.text.trim(), text_width);
        for line in wrapped.lines() {
            writeln!(stdout, "{}{}", text_indent.dimmed(), line).into_diagnostic()?;
        }
    }

    // Footer
    writeln!(stdout).into_diagnostic()?;
    writeln!(stdout, "   {}", separator.dimmed()).into_diagnostic()?;
    writeln!(
        stdout,
        "   {} {}",
        "Duration:".dimmed(),
        format_timestamp(transcript.duration()),
    )
    .into_diagnostic()?;
    writeln!(stdout).into_diagnostic()?;
    stdout.flush().into_diagnostic()?;

    Ok(())
}

/// Format seconds into HH:MM:SS or MM:SS.
fn format_timestamp(seconds: f32) -> String {
    let total = seconds.round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;

    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// Best-effort terminal width detection, fallback to 100.
fn terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(100)
}
