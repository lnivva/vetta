mod commands;
mod output;

use clap::{Parser, Subcommand};
use miette::{Result, set_panic_hook};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "vetta",
    about = "Institutional-grade Financial Analysis Engine",
    version,
    propagate_version = true,
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[arg(
        long,
        env = "WHISPER_SOCK",
        default_value = "/tmp/whisper.sock",
        global = true
    )]
    socket: PathBuf,

    /// Suppress all progress output
    #[arg(long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Resource,
}

#[derive(Subcommand)]
enum Resource {
    /// Ingest and process earnings calls
    Earnings {
        #[command(subcommand)]
        action: commands::earnings::EarningsAction,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv().ok();
    set_panic_hook();

    let cli = Cli::parse();

    match cli.command {
        Resource::Earnings { action } => {
            commands::earnings::handle(action, &cli.socket, cli.quiet).await
        }
    }
}
