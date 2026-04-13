mod cli;
mod commands;
mod context;
mod infra;
mod ui;

use clap::Parser;
use context::AppContext;
use miette::{IntoDiagnostic, Result, WrapErr, bail};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::debug;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[tokio::main]
async fn main() -> Result<()> {
    let env_path = load_env_vars()?;

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
        socket: cli.socket,
        verbose: cli.verbose,
        debug: cli.debug,
        output: cli.output,
    };

    ensure_migrated(&ctx, env_path.as_deref())?;

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

#[cfg(debug_assertions)]
fn ensure_migrated(ctx: &AppContext, env_path: Option<&Path>) -> Result<()> {
    if ctx.debug {
        debug!("Ensuring database indexes are up to date...");
    }

    let mut migrate_bin = std::env::current_exe()
        .into_diagnostic()
        .wrap_err("Failed to resolve current executable path")?;
    migrate_bin.pop();
    migrate_bin.push("vetta_migrate");

    let mut cmd = Command::new(&migrate_bin);

    if let Some(path) = env_path {
        cmd.env("VETTA_ENV_PATH", path);
    }

    if !ctx.debug {
        cmd.env("RUST_LOG", "error");
    }

    let status = cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .status()
        .into_diagnostic()
        .wrap_err("Failed to execute vetta_migrate binary. Did you build it with: `cargo build --bin vetta_migrate`?")?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        bail!("Database migration failed with exit code: {}", code);
    }

    if ctx.debug {
        debug!("Database migration check passed.");
    }

    Ok(())
}

#[cfg(not(debug_assertions))]
fn ensure_migrated(_ctx: &AppContext, _env_path: Option<&Path>) -> Result<()> {
    Ok(())
}
