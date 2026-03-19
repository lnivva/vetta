mod cli;
mod commands;
mod context;
mod infra;
mod output;
mod reporter;

use clap::Parser;
use context::{AppContext, OutputMode};
use miette::{Result, set_panic_hook};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv().ok();
    set_panic_hook();

    let cli = cli::Cli::parse();

    let ctx = AppContext {
        socket: cli.socket,
        quiet: cli.quiet,
        output: if cli.json {
            OutputMode::Json
        } else {
            OutputMode::Pretty
        },
    };

    commands::dispatch(cli.command, &ctx).await
}
