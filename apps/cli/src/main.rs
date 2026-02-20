use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use miette::{Context, IntoDiagnostic, Result, set_panic_hook};
use std::path::Path;
use std::{
    io::{self, Write},
    path::PathBuf,
};
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
#[command(
    name = "vetta",
    about = "Institutional-grade Financial Analysis Engine",
    version
)]
struct Cli {
    #[arg(
        long,
        env = "WHISPER_SOCK",
        default_value = "/tmp/whisper.sock",
        global = true
    )]
    socket: PathBuf,

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
        file: PathBuf,
        #[arg(short, long)]
        ticker: String,
        #[arg(short, long)]
        year: u16,
        #[arg(short, long, value_enum)]
        quarter: CliQuarter,

        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,

        #[arg(long)]
        print: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv().ok();
    set_panic_hook();

    let cli = Cli::parse();

    match cli.command {
        Resource::Earnings { action } => match action {
            EarningsAction::Process {
                file,
                ticker,
                year,
                quarter,
                out,
                print,
            } => {
                run_processing_pipeline(file, ticker, year, quarter, &cli.socket, out, print)
                    .await?;
            }
        },
    }

    Ok(())
}

async fn run_processing_pipeline(
    file: PathBuf,
    ticker: String,
    year: u16,
    quarter: CliQuarter,
    socket_path: &Path,
    out: Option<PathBuf>,
    print: bool,
) -> Result<()> {
    let core_quarter: CoreQuarter = quarter.into();

    print_banner();

    println!(
        "   {:<10} {} {} {}",
        "TARGET:".dimmed(),
        ticker.yellow().bold(),
        core_quarter.to_string().yellow(),
        year.to_string().yellow()
    );

    let file_path = std::fs::canonicalize(&file)
        .into_diagnostic()
        .wrap_err("Failed to resolve input path")?;

    println!("   {:<10} {}", "INPUT:".dimmed(), file_path.display());
    println!("   {:<10} {}", "SOCKET:".dimmed(), socket_path.display());
    println!();

    let file_info =
        validate_media_file(&file_path.to_string_lossy()).wrap_err("Validation phase failed")?;

    println!("   {}", "✔ VALIDATION PASSED".green().bold());
    println!("   {:<10} {}", "Format:".dimmed(), file_info);
    println!();

    println!("   {}", "Processing Pipeline:".bold().blue());
    println!("   1. [✔] Validation");
    println!("   2. [{}] Transcription (Whisper)", "RUNNING".yellow());

    let stt = LocalSttStrategy::connect(socket_path.to_string_lossy())
        .await
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "Failed to connect to STT service at '{}'",
                socket_path.display()
            )
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

    let mut stream = stt
        .transcribe(&file_path.to_string_lossy(), options)
        .await
        .into_diagnostic()
        .wrap_err("Transcription failed")?;

    let mut segment_count = 0u32;
    let mut full = String::new();

    while let Some(result) = stream.next().await {
        let chunk = result
            .into_diagnostic()
            .wrap_err("Error reading transcript chunk")?;
        segment_count += 1;

        let line = chunk.text.trim_end();
        if !line.is_empty() {
            full.push_str(line);
            full.push('\n');
        }

        print!("\r\x1B[K   Transcribing… {} segments", segment_count);
        io::stdout().flush().into_diagnostic()?;
    }

    println!(
        "\r\x1B[K   2. [✔] Transcription ({} segments)",
        segment_count
    );

    if let Some(out_path) = out {
        std::fs::write(&out_path, full.as_bytes())
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to write transcript to {}", out_path.display()))?;
        println!("   {:<10} {}", "OUTPUT:".dimmed(), out_path.display());
    }

    if print {
        println!();
        print!("{full}");
    }

    Ok(())
}

fn print_banner() {
    println!();
    println!("   {}", "VETTA FINANCIAL ENGINE".bold());
    println!("   {}", "======================".dimmed());
}
