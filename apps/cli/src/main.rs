mod cli;
mod commands;
mod context;
mod infra;
mod output;
mod reporter;

use clap::Parser;
use context::AppContext;
use miette::{set_panic_hook, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv().ok();
    set_panic_hook();

    let cli = cli::Cli::parse();

    let ctx = AppContext {
        socket: cli.socket,
        quiet: cli.quiet,
    };

    commands::dispatch(cli.command, &ctx).await
}