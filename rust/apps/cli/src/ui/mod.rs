#![allow(dead_code)]

pub mod earnings;

use console::{Emoji, Style, style};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use miette::IntoDiagnostic;
use std::fs::File;
use std::io::{Read, Write, stdin, stdout};
use std::path::PathBuf;
use std::time::Duration;

pub static SUCCESS: Emoji<'_, '_> = Emoji("✓", "√");
pub static ERROR: Emoji<'_, '_> = Emoji("✗", "x");
pub static WARN: Emoji<'_, '_> = Emoji("⚠", "!");
pub static INFO: Emoji<'_, '_> = Emoji("ℹ", "i");
pub static STEP: Emoji<'_, '_> = Emoji("›", ">");
pub static DOT: Emoji<'_, '_> = Emoji("·", "-");
pub static PIPE: Emoji<'_, '_> = Emoji("│", "|");
pub static ARROW: Emoji<'_, '_> = Emoji("→", "->");

pub const INDENT: &str = "  ";

const SPINNER_CHARS: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";
const SPINNER_TEMPLATE: &str = "{spinner:.cyan} {msg}";

const SPEAKER_COLORS: &[console::Color] = &[
    console::Color::Yellow,
    console::Color::Green,
    console::Color::Magenta,
    console::Color::Cyan,
    console::Color::Blue,
    console::Color::Red,
];

pub struct Styles;

impl Styles {
    pub fn primary() -> Style {
        Style::new().cyan().bold()
    }
    pub fn heading() -> Style {
        Style::new().bold().bright()
    }
    pub fn stat() -> Style {
        Style::new().cyan()
    }
    pub fn dimmed() -> Style {
        Style::new().dim()
    }
    pub fn success() -> Style {
        Style::new().green()
    }
    pub fn error() -> Style {
        Style::new().red().bold()
    }
    pub fn warning() -> Style {
        Style::new().yellow()
    }
    pub fn speaker(index: usize) -> Style {
        Style::new()
            .fg(SPEAKER_COLORS[index % SPEAKER_COLORS.len()])
            .bold()
    }
}

pub fn term_width() -> usize {
    // Bind to stderr so redirecting stdout to a file doesn't break terminal sizing
    console::Term::stderr().size().1 as usize
}

pub fn content_width() -> usize {
    term_width().saturating_sub(INDENT.len() * 2)
}

pub fn text_width() -> usize {
    content_width().saturating_sub(4).max(60)
}

pub fn separator() -> String {
    style("─".repeat(content_width())).dim().to_string()
}

pub fn text_prefix() -> String {
    style(format!("{INDENT}{PIPE} ")).dim().to_string()
}

pub fn success_msg(msg: &str) -> String {
    format!("{INDENT}{} {}", Styles::success().apply_to(SUCCESS), msg)
}

pub fn error_msg(msg: &str) -> String {
    format!(
        "{INDENT}{} {}",
        Styles::error().apply_to(ERROR),
        Styles::error().apply_to(msg)
    )
}

pub fn warn_msg(msg: &str) -> String {
    format!(
        "{INDENT}{} {}",
        Styles::warning().apply_to(WARN),
        Styles::warning().apply_to(msg)
    )
}

pub fn info_msg(msg: &str) -> String {
    format!("{INDENT}{} {}", Styles::primary().apply_to(INFO), msg)
}

pub fn step_msg(msg: &str) -> String {
    format!("{INDENT}{} {}", Styles::dimmed().apply_to(STEP), msg)
}

pub fn kv_msg(key: &str, value: &str) -> String {
    format!(
        "{INDENT}{} {}",
        Styles::dimmed().apply_to(format!("{key}:")),
        value
    )
}

pub fn timestamp(seconds: f64) -> String {
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

pub fn spinner() -> ProgressBar {
    let pb = ProgressBar::with_draw_target(u64::MAX.into(), ProgressDrawTarget::stderr());
    pb.set_style(
        ProgressStyle::with_template(SPINNER_TEMPLATE)
            .unwrap()
            .tick_chars(SPINNER_CHARS),
    );
    pb.set_prefix(INDENT.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/// Creates a dynamic writer based on an optional path.
/// Defaults to Stdout if None.
pub fn get_writer(path: &Option<PathBuf>) -> miette::Result<Box<dyn Write>> {
    match path {
        Some(p) => {
            let file = File::create(p).into_diagnostic()?;
            Ok(Box::new(file))
        }
        None => Ok(Box::new(stdout())),
    }
}

/// Creates a dynamic reader based on an optional path.
/// Defaults to Stdin if None or if the path is "-".
pub fn get_reader(path: &Option<PathBuf>) -> miette::Result<Box<dyn Read>> {
    match path {
        Some(p) if p.to_str() != Some("-") => {
            let file = File::open(p).into_diagnostic()?;
            Ok(Box::new(file))
        }
        _ => Ok(Box::new(stdin())),
    }
}
