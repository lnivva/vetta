use clap::{Parser, Subcommand, ValueEnum};
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

    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[arg(long, global = true, env = "VETTA_DEBUG")]
    pub debug: bool,

    #[arg(short, global = true, long, value_enum, default_value_t = CliOutputOptions::Json)]
    pub output: CliOutputOptions,

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

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum CliOutputOptions {
    Json,
    Plain,
}
