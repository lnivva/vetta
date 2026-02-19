use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use miette::{set_panic_hook, Context, IntoDiagnostic, Result};
use tokio_stream::StreamExt;
use vetta_core::domain::Quarter as CoreQuarter;
use vetta_core::earnings_processor::validate_media_file;
use vetta_core::stt::{LocalSttStrategy, SpeechToText, TranscribeOptions};

#[derive(Debug, Clone, ValueEnum)]
enum CliQuarter {
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

#[derive(Parser)]
#[command(name = "vetta")]
#[command(about = "Institutional-grade Financial Analysis Engine", version)]
struct Cli {
    #[command(subcommand)]
    command: Resource,
}

#[derive(Subcommand)]
enum Resource {
    #[command(about = "Ingest and process earnings calls")]
    Earnings {
        #[command(subcommand)]
        action: EarningsAction,
    },
}

#[derive(Subcommand)]
enum EarningsAction {
    #[command(about = "Process an audio/video file")]
    Process {
        #[arg(short, long)]
        file: String,
        #[arg(short, long)]
        ticker: String,
        #[arg(short, long)]
        year: u16,
        #[arg(short, long, value_enum)]
        quarter: CliQuarter,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    set_panic_hook();

    let cli = Cli::parse();

    match cli.command {
        Resource::Earnings { action } => match action {
            EarningsAction::Process {
                file,
                ticker,
                year,
                quarter,
            } => {
                let core_quarter: CoreQuarter = quarter.into();

                print_banner();

                println!(
                    "   {:<10} {} {} {}",
                    "TARGET:".dimmed(),
                    ticker.yellow().bold(),
                    core_quarter.to_string().yellow(),
                    year.to_string().yellow()
                );
                println!("   {:<10} {}", "INPUT:".dimmed(), file);
                println!();

                // ── 1. Validation ──────────────────────────────────────────
                let file_info = validate_media_file(&file).wrap_err("Validation phase failed")?;

                println!("   {}", "✔ VALIDATION PASSED".green().bold());
                println!("   {:<10} {}", "Format:".dimmed(), file_info);
                println!();

                println!("   {}", "Processing Pipeline:".bold().blue());
                println!("   1. [✔] Validation");
                println!("   2. [{}] Transcription (Whisper)", "RUNNING".yellow());

                // ── 2. Transcription ───────────────────────────────────────
                let stt = LocalSttStrategy::connect("/tmp/whisper.sock")
                    .await
                    .into_diagnostic()
                    .wrap_err(
                        "Failed to connect to STT service — is the whisper service running?",
                    )?;

                let options = TranscribeOptions {
                    language: Some("en".into()),
                    initial_prompt: Some(
                        "Earnings call transcript. Financial terminology, \
                         company names, analyst questions and management responses."
                            .into(),
                    ),
                    diarization: false,
                    num_speakers: 2,
                };

                let mut stream = stt
                    .transcribe(&file, options)
                    .await
                    .into_diagnostic()
                    .wrap_err("Transcription failed")?;

                let mut segment_count = 0u32;

                while let Some(result) = stream.next().await {
                    let chunk = result
                        .into_diagnostic()
                        .wrap_err("Error reading transcript chunk")?;

                    segment_count += 1;

                    // Live progress — overwrite the same line
                    print!(
                        "\r   [{:.1}s → {:.1}s] {}",
                        chunk.start_time,
                        chunk.end_time,
                        chunk.text.trim()
                    );
                }

                // Clear progress line then print final status
                println!();
                println!("   2. [✔] Transcription ({} segments)", segment_count);
                println!("   3. [{}] Vector Embedding", "WAITING".dimmed());
                println!();
            }
        },
    }

    Ok(())
}

fn print_banner() {
    println!();
    println!("   {}", "VETTA FINANCIAL ENGINE".bold());
    println!("   {}", "======================".dimmed());
}
