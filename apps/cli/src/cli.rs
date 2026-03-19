use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::commands::earnings::EarningsAction;

#[derive(Parser)]
#[command(
    name = "vetta",
    about = "Institutional-grade Financial Analysis Engine",
    version,
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
    pub socket: PathBuf,

    #[arg(long, global = true)]
    pub quiet: bool,

    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Earnings {
        #[command(subcommand)]
        action: EarningsAction,
    },
}
