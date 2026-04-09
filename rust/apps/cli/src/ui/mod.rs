pub mod earnings;

use console::{Emoji, Style, style};
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

// ── Icons with ASCII fallback ────────────────────────────────────────

pub static OK: Emoji<'_, '_> = Emoji("✔", "ok");
pub static FAIL: Emoji<'_, '_> = Emoji("✖", "!!");
pub static WARN: Emoji<'_, '_> = Emoji("⚠", "!!");
pub static DOT: Emoji<'_, '_> = Emoji("●", "*");
pub static PIPE: Emoji<'_, '_> = Emoji("│", "|");
pub static ARROW: Emoji<'_, '_> = Emoji("→", "->");
pub static TRANSCRIPT: Emoji<'_, '_> = Emoji("📄", "##");

pub const INDENT: &str = "   ";

const SPINNER_CHARS: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";
const SPINNER_TEMPLATE: &str = "{spinner:.cyan} {msg}";

// ── Semantic styles ──────────────────────────────────────────────────

const SPEAKER_COLORS: &[console::Color] = &[
    console::Color::Yellow,
    console::Color::Green,
    console::Color::Magenta,
    console::Color::Cyan,
    console::Color::Blue,
    console::Color::Red,
    console::Color::Color256(208),
    console::Color::Color256(177),
];

pub struct Styles;

impl Styles {
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

    pub fn speaker(index: usize) -> Style {
        Style::new()
            .fg(SPEAKER_COLORS[index % SPEAKER_COLORS.len()])
            .bold()
    }
}

// ── Layout ───────────────────────────────────────────────────────────

pub fn term_width() -> usize {
    console::Term::stdout().size().1 as usize
}

pub fn content_width() -> usize {
    term_width().saturating_sub(INDENT.len() * 2)
}

pub fn text_width() -> usize {
    content_width().saturating_sub(6).max(40)
}

pub fn separator() -> String {
    style("━".repeat(content_width())).dim().to_string()
}

pub fn text_prefix() -> String {
    style(format!("{INDENT}{PIPE}  ")).dim().to_string()
}

// ── Formatters ───────────────────────────────────────────────────────

pub fn timestamp(seconds: f32) -> String {
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

pub fn success_msg(msg: &str) -> String {
    format!("{} {msg}", Styles::success().apply_to(OK))
}

pub fn error_msg(msg: &str) -> String {
    format!("{} {msg}", Styles::error().apply_to(FAIL))
}

// ── Spinner factory ──────────────────────────────────────────────────

pub fn spinner() -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template(SPINNER_TEMPLATE)
            .unwrap()
            .tick_chars(SPINNER_CHARS),
    );
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}
