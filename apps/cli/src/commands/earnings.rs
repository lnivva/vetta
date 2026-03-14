use crate::output;
use clap::{Subcommand, ValueEnum};
use colored::*;
use miette::{Context, IntoDiagnostic, Result};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tokio::time::{Duration, timeout};
use tokio_stream::StreamExt;
use vetta_core::domain::Quarter as CoreQuarter;
use vetta_core::earnings_processor::validate_media_file;
use vetta_core::stt::local::LocalSttStrategy;
use vetta_core::stt::{SpeechToText, TranscribeOptions};

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
        #[arg(
            long,
            value_name = "PATH",
            conflicts_with = "print",
            required_unless_present = "print"
        )]
        out: Option<PathBuf>,

        /// Stream transcript to stdout
        #[arg(long, required_unless_present = "out")]
        print: bool,
    },
}

pub async fn handle(action: EarningsAction, socket: &Path, quiet: bool) -> Result<()> {
    match action {
        EarningsAction::Process {
            file,
            ticker,
            year,
            quarter,
            out,
            print,
        } => {
            let core_quarter: CoreQuarter = quarter.into();

            let file_path = std::fs::canonicalize(&file)
                .into_diagnostic()
                .wrap_err("Failed to resolve input path")?;

            if !quiet {
                print_banner(&ticker, &core_quarter, year, &file_path, socket);
            }

            // ── Stage 1: Validation ──────────────────────────────
            let file_info = validate_media_file(&file_path.to_string_lossy())
                .wrap_err("Validation phase failed")?;

            if !quiet {
                println!("   {}", "✔ VALIDATION PASSED".green().bold());
                println!("   {:<10} {}", "Format:".dimmed(), file_info);
                println!();
                println!("   {}", "Processing Pipeline:".bold().blue());
                println!("   1. [✔] Validation");
                println!("   2. [{}] Transcription (Whisper)", "RUNNING".yellow());
            }

            // ── Stage 2: Transcription ───────────────────────────
            let stt = LocalSttStrategy::connect(socket.to_string_lossy())
                .await
                .into_diagnostic()
                .wrap_err_with(|| {
                    format!("Failed to connect to STT service at '{}'", socket.display())
                })?;

            let options = TranscribeOptions {
                language: Some("en".into()),
                initial_prompt: Some(
                    "Earnings call transcript. Financial terminology, company names, analyst questions and management responses."
                        .into(),
                ),
                diarization: false,
                num_speakers: 2,
            };

            const STREAM_START_TIMEOUT: Duration = Duration::from_secs(120);
            const CHUNK_READ_TIMEOUT: Duration = Duration::from_secs(300);

            let mut stream = timeout(
                STREAM_START_TIMEOUT,
                stt.transcribe(&file_path.to_string_lossy(), options),
            )
            .await
            .into_diagnostic()
            .wrap_err("Transcription stream startup timed out")?
            .into_diagnostic()
            .wrap_err("Transcription failed")?;

            let mut segment_count = 0u32;
            let mut full = String::new();

            loop {
                let maybe_result = timeout(CHUNK_READ_TIMEOUT, stream.next())
                    .await
                    .into_diagnostic()
                    .wrap_err_with(|| {
                        format!(
                            "Timed out waiting for transcript chunk after segment {segment_count} \
                                (no data received within {}s)",
                            CHUNK_READ_TIMEOUT.as_secs()
                        )
                    })?;

                let Some(result) = maybe_result else {
                    break; // stream exhausted  
                };

                let chunk = result
                    .into_diagnostic()
                    .wrap_err("Error reading transcript chunk")?;
                segment_count += 1;

                let line = chunk.text.trim_end();
                if !line.is_empty() {
                    full.push_str(line);
                    full.push('\n');
                }

                if !quiet {
                    print!("\r\x1B[K   Transcribing… {} segments", segment_count);
                    let _ = io::stdout().flush();
                }
            }

            if !quiet {
                println!("\r\x1B[K   2. [✔] Transcription ({segment_count} segments)");
            }

            // ── Stage 3: Output ──────────────────────────────────
            if let Some(ref path) = out {
                output::write_file(path, &full)?;
                if !quiet {
                    println!("   {:<10} {}", "OUTPUT:".dimmed(), path.display());
                }
            }

            if print {
                output::write_stdout(&full)?;
            }

            if !quiet {
                println!();
                println!("   {}", "✔ PIPELINE COMPLETE".green().bold());
                println!();
            }

            Ok(())
        }
    }
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
