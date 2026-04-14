use crate::commands::{ConfigAction, DebugAction, EarningsAction};
use crate::context::AppContext;
use clap::{Parser, Subcommand, ValueEnum};
use miette::{IntoDiagnostic, miette};
use serde::de::DeserializeOwned;
use std::io::{IsTerminal, Read, stdin};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "vetta",
    about = "Institutional-grade Financial Analysis Engine",
    version,
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[arg(long, global = true, env = "VETTA_DEBUG")]
    pub debug: bool,

    /// Format of the final results (affects stdout or the --output file)
    #[arg(short = 'f', long, value_enum, default_value_t = CliOutputFormat::Json)]
    pub format: CliOutputFormat,

    /// Path to a JSON payload file containing job arguments (omit or use '-' for stdin)
    #[arg(short, long, global = true)]
    pub input: Option<PathBuf>,

    /// Path to save the processed results (omit to print to stdout)
    #[arg(short, long, global = true)]
    pub output: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Earnings {
        #[command(subcommand)]
        action: EarningsAction,
    },

    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    #[command(hide = true)]
    Debug {
        #[command(subcommand)]
        action: DebugAction,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum CliOutputFormat {
    Json,
    Plain,
}

pub trait PayloadDriven: DeserializeOwned {
    type CliArgs;

    fn from_cli(args: &Self::CliArgs) -> Option<Self>;

    fn merge_cli(&mut self, args: &Self::CliArgs);

    fn resolve(ctx: &AppContext, args: &Self::CliArgs) -> miette::Result<Self> {
        if let Some(payload) = Self::from_cli(args) {
            return Ok(payload);
        }

        if ctx.input.is_none() && stdin().is_terminal() {
            return Err(miette::miette!("Missing required arguments..."));
        }

        let mut reader = crate::ui::get_reader(&ctx.input)?;
        let mut input_data = String::new();
        reader.read_to_string(&mut input_data).into_diagnostic()?;

        let mut payload: Self = serde_json::from_str(&input_data)
            .into_diagnostic()
            .map_err(|e| miette!("Failed to parse input JSON: {}", e))?;

        payload.merge_cli(args);

        Ok(payload)
    }
}
