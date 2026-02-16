use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use miette::{set_panic_hook, Context, Result};
use vetta_core::domain::Quarter as CoreQuarter;
use vetta_core::earnings_processor::validate_media_file;

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
    Earnings {
        #[command(subcommand)]
        action: EarningsAction,
    },
}

#[derive(Subcommand)]
enum EarningsAction {
    /// Ingest and process a raw earnings call file
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

fn main() -> Result<()> {
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

                let file_info = validate_media_file(&file).wrap_err("Validation phase failed")?;

                println!("   {}", "✔ VALIDATION PASSED".green().bold());
                println!("   {:<10} {}", "Format:".dimmed(), file_info);
                println!();

                println!("   {}", "Processing Pipeline:".bold().blue());
                println!("   1. [✔] Validation");
                println!("   2. [{}] Transcription (Whisper)", "WAITING".yellow());
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
