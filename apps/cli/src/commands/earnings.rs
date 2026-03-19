use crate::output;
use clap::{Subcommand, ValueEnum};
use colored::*;
use miette::{IntoDiagnostic, Result};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use vetta_core::domain::Quarter as CoreQuarter;
use vetta_core::earnings_processor::{EarningsProcessor, PipelineEvent, ProcessRequest};
use vetta_core::stt::local::LocalSttStrategy;

#[derive(Debug, Clone, ValueEnum)]
pub enum CliQuarter {
    Q1,
    Q2,
    Q3,
    Q4,
}

impl From<CliQuarter> for CoreQuarter {
    fn from(cli: CliQuarter) -> Self {
        match cli {
            CliQuarter::Q1 => CoreQuarter::Q1,
            CliQuarter::Q2 => CoreQuarter::Q2,
            CliQuarter::Q3 => CoreQuarter::Q3,
            CliQuarter::Q4 => CoreQuarter::Q4,
        }
    }
}

#[derive(Subcommand)]
pub enum EarningsAction {
    /// Process an audio/video file through the analysis pipeline
    Process {
        #[arg(short, long, value_name = "FILE")]
        file: PathBuf,

        #[arg(short, long, value_name = "TICKER")]
        ticker: String,

        #[arg(short, long)]
        year: u16,

        #[arg(short, long, value_enum)]
        quarter: CliQuarter,

        /// Dump raw transcript to a file
        #[arg(long, value_name = "PATH", conflicts_with = "print")]
        out: Option<PathBuf>,

        /// Stream transcript to stdout
        #[arg(long)]
        print: bool,
    },
}

pub async fn handle(action: EarningsAction, socket: &Path, quiet: bool) -> Result<()> {
    let EarningsAction::Process {
        file,
        ticker,
        year,
        quarter,
        out,
        print,
    } = action;

    let file_path = std::fs::canonicalize(&file).into_diagnostic()?;

    let stt = LocalSttStrategy::connect(socket.to_string_lossy())
        .await
        .into_diagnostic()?;

    if !quiet {
        print_banner(&ticker, &quarter.clone().into(), year, &file_path, socket);
    }

    let processor = EarningsProcessor::from_env(Box::new(stt))
        .await
        .into_diagnostic()?;

    let transcript = processor
        .process(
            ProcessRequest {
                file_path: file_path.to_string_lossy().into(),
                ticker,
                year,
                quarter: quarter.into(),
                language: Some("en".into()),
                initial_prompt: Some(
                    "Earnings call transcript. Financial terminology, company names, analyst \
                    questions and management responses.".into(),
                ),
            },
            |event| match event {
                PipelineEvent::ValidationPassed { format_info } => {
                    if !quiet {
                        println!("   {}", "✔ VALIDATION PASSED".green().bold());
                        println!("   {:<10} {}", "Format:".dimmed(), format_info);
                        println!();
                        println!("   {}", "Processing Pipeline:".bold().blue());
                        println!("   1. [✔] Validation");
                        println!("   2. [{}] Transcription (Whisper)", "RUNNING".yellow());
                    }
                }
                PipelineEvent::TranscriptionProgress { segments } => {
                    if !quiet {
                        print!("\r\x1B[K   Transcribing… {segments} segments");
                        let _ = io::stdout().flush();
                    }
                }
                PipelineEvent::TranscriptionComplete { ref transcript } => {
                    if !quiet {
                        let seg_count = transcript.segments.len();
                        let speaker_count = transcript.unique_speakers().len();
                        println!(
                            "\r\x1B[K   2. [✔] Transcription ({seg_count} segments, {speaker_count} speakers)"
                        );
                    }
                }
                PipelineEvent::StoringChunks { chunk_count } => {
                    if !quiet {
                        print!("   3. [{}] Storing {chunk_count} chunks…", "RUNNING".yellow());
                        let _ = io::stdout().flush();
                    }
                }
                PipelineEvent::Stored { ref call_id, chunk_count } => {
                    if !quiet {
                        println!(
                            "\r\x1B[K   3. [✔] Stored ({chunk_count} chunks → {call_id})"
                        );
                    }
                }
            },
        )
        .await
        .into_diagnostic()?;

    // Format output as dialogue with speaker labels
    let output_text = transcript.to_string();

    if let Some(ref path) = out {
        output::write_file(path, &output_text)?;
    }

    if print {
        output::print_transcript(&transcript)?;
    }

    if !quiet {
        println!();
        println!("   {}", "✔ PIPELINE COMPLETE".green().bold());
        println!();
    }

    Ok(())
}

fn print_banner(
    ticker: &str,
    quarter: &CoreQuarter,
    year: u16,
    file_path: &Path,
    socket_path: &Path,
) {
    println!();
    println!("   {}", "VETTA FINANCIAL ENGINE".bold());
    println!("   {}", "======================".dimmed());
    println!(
        "   {:<10} {} {} {}",
        "TARGET:".dimmed(),
        ticker.yellow().bold(),
        quarter.to_string().yellow(),
        year.to_string().yellow()
    );
    println!("   {:<10} {}", "INPUT:".dimmed(), file_path.display());
    println!("   {:<10} {}", "SOCKET:".dimmed(), socket_path.display());
    println!();
}
