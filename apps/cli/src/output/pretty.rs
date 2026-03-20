use colored::*;
use miette::{IntoDiagnostic, Result};
use std::io::{self, Write};
use vetta_core::domain::Transcript;

pub fn print_transcript(transcript: &Transcript) -> Result<()> {
    let mut stdout = io::stdout();
    let term_width = terminal_width().saturating_sub(6);
    let text_indent = "   │ ";
    let text_width = term_width.saturating_sub(text_indent.len()).max(40);

    let separator = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";

    writeln!(stdout).into_diagnostic()?;
    writeln!(stdout, "   {}", separator.dimmed()).into_diagnostic()?;
    writeln!(stdout, "   {}", "📄 TRANSCRIPT".bold().bright_white())
        .into_diagnostic()?;

    writeln!(
        stdout,
        "   {} segments  •  {} speakers  •  {} duration",
        transcript.segments.len().to_string().cyan(),
        transcript.unique_speakers().len().to_string().cyan(),
        format_timestamp(transcript.duration()).cyan(),
    )
        .into_diagnostic()?;

    writeln!(stdout, "   {}", separator.dimmed()).into_diagnostic()?;
    writeln!(stdout).into_diagnostic()?;

    let speaker_colors = [
        Color::Yellow,
        Color::Green,
        Color::Magenta,
        Color::Cyan,
        Color::Blue,
        Color::Red,
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
            "   {}  {}",
            speaker.color(color).bold(),
            format!("● [{}]", timestamp).dimmed(),
        )
            .into_diagnostic()?;

        let wrapped = textwrap::fill(turn.text.trim(), text_width);

        for line in wrapped.lines() {
            writeln!(stdout, "{}{}", text_indent.dimmed(), line)
                .into_diagnostic()?;
        }
    }

    writeln!(stdout).into_diagnostic()?;
    writeln!(stdout, "   {}", separator.dimmed()).into_diagnostic()?;
    writeln!(
        stdout,
        "   {} {}",
        "Total Duration:".dimmed(),
        format_timestamp(transcript.duration()).bold()
    )
        .into_diagnostic()?;
    writeln!(stdout).into_diagnostic()?;

    stdout.flush().into_diagnostic()?;
    Ok(())
}

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

fn terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(100)
}