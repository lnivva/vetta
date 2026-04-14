mod cli;
mod commands;
mod config;
mod context;
mod infra;
mod ui;

use crate::config::VettaConfig;
use clap::Parser;
use context::AppContext;
use miette::{IntoDiagnostic, Result};
use std::io;
use std::path::PathBuf;
use tracing::debug;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[tokio::main]
async fn main() -> Result<()> {
    let _env_path = load_env_vars()?;

    let persistent_config = VettaConfig::load();
    let cli = cli::Cli::parse();

    let log_level = if cli.debug { "debug" } else { "error" };

    let subscriber = FmtSubscriber::builder()
        .with_writer(io::stderr)
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(log_level.parse().expect("Invalid log level"))
                .from_env_lossy(),
        )
        .finish();
    tracing::subscriber::set_global_default(subscriber).into_diagnostic()?;

    miette::set_panic_hook();

    let ctx = AppContext {
        config: persistent_config,
        debug: cli.debug,
        format: cli.format,
        input: cli.input,
        output: cli.output,
    };

    commands::dispatch(cli.command, &ctx).await
}

fn load_env_vars() -> Result<Option<PathBuf>> {
    let env_path = match dotenvy::dotenv() {
        Ok(path) => {
            debug!("Loaded environment variables from {}", path.display());
            Some(path)
        }
        Err(e) if e.not_found() => {
            debug!("Environment variables not found: {}", e);
            None
        }
        Err(e) => {
            return Err(miette::miette!("Failed to load .env file: {}", e));
        }
    };
    Ok(env_path)
}
