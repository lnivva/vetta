mod cli;
mod commands;
mod context;
mod infra;
mod output;
mod reporter;

use clap::Parser;
use context::AppContext;
use miette::{Result, set_panic_hook};
use std::process::Command;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    match dotenvy::dotenv() {
        Ok(path) => info!("Loaded environment variables from {}", path.display()),
        Err(e) if e.not_found() => {}
        Err(e) => {
            error!("Failed to load .env file: {}", e);
            std::process::exit(1);
        }
    }

    set_panic_hook();

    let cli = cli::Cli::parse();

    let ctx = AppContext {
        socket: cli.socket,
        quiet: cli.quiet,
    };

    ensure_migrated(&ctx)?;

    commands::dispatch(cli.command, &ctx).await
}

/// Run the migration tool to ensure all required indexes exist.
fn ensure_migrated(ctx: &AppContext) -> Result<()> {
    if !ctx.quiet {
        info!("Ensuring database indexes are up to date...");
    }

    let status = Command::new("vetta_migrate")
        .stdout(if ctx.quiet {
            std::process::Stdio::null()
        } else {
            std::process::Stdio::inherit()
        })
        .stderr(std::process::Stdio::inherit())
        .status();

    match status {
        Ok(s) if s.success() => {
            if !ctx.quiet {
                info!("Database migration check passed.");
            }
            Ok(())
        }
        Ok(s) => {
            error!(
                exit_code = s.code().unwrap_or(-1),
                "Database migration failed"
            );
            std::process::exit(1);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            error!(
                "vetta_migrate binary not found. \
                 Build it with: cargo build --bin vetta_migrate"
            );
            std::process::exit(1);
        }
        Err(e) => {
            error!(error = %e, "Failed to run vetta_migrate");
            std::process::exit(1);
        }
    }
}
