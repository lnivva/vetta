use clap::{Args, Subcommand};
use miette::{IntoDiagnostic, Result, miette};
use std::io::Write;
use std::path::PathBuf;

use crate::cli::{CliOutputFormat, PayloadDriven};
use crate::config::{EmbeddingModel, VettaConfig};
use crate::context::AppContext;
use crate::ui::{get_writer, info_msg, separator, success_msg, warn_msg};

#[derive(Args, Debug, Clone, serde::Deserialize)]
pub struct ConfigSetArgs {
    #[arg(long)]
    pub socket: Option<PathBuf>,
    #[arg(long)]
    pub mongo_uri: Option<String>,
    #[arg(long)]
    pub mongo_db: Option<String>,
    #[arg(long)]
    pub embedding_model: Option<EmbeddingModel>,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Shows the current configuration values
    Show,

    /// Initializes a default configuration file
    Init {
        #[arg(short, long)]
        force: bool,
    },

    /// Updates configuration values (via flags or JSON payload)
    Set(ConfigSetArgs),
}

/// Payload for bulk configuration updates
#[derive(Debug, serde::Deserialize)]
pub struct ConfigPayload {
    pub socket: Option<PathBuf>,
    pub mongo_uri: Option<String>,
    pub mongo_db: Option<String>,
    pub embedding_model: Option<EmbeddingModel>,
}

impl PayloadDriven for ConfigPayload {
    type CliArgs = ConfigSetArgs;

    fn from_cli(args: &Self::CliArgs) -> Option<Self> {
        if args.socket.is_some()
            || args.mongo_uri.is_some()
            || args.mongo_db.is_some()
            || args.embedding_model.is_some()
        {
            Some(Self {
                socket: args.socket.clone(),
                mongo_uri: args.mongo_uri.clone(),
                mongo_db: args.mongo_db.clone(),
                embedding_model: args.embedding_model.clone(),
            })
        } else {
            None
        }
    }

    fn merge_cli(&mut self, args: &Self::CliArgs) {
        if let Some(s) = &args.socket {
            self.socket = Some(s.clone());
        }
        if let Some(u) = &args.mongo_uri {
            self.mongo_uri = Some(u.clone());
        }
        if let Some(d) = &args.mongo_db {
            self.mongo_db = Some(d.clone());
        }
        if let Some(m) = &args.embedding_model {
            self.embedding_model = Some(m.clone());
        }
    }
}

pub async fn handle(action: ConfigAction, ctx: &AppContext) -> Result<()> {
    let config_path = VettaConfig::file_path()
        .ok_or_else(|| miette!("Could not determine the system configuration directory."))?;

    let mut writer = get_writer(&ctx.output)?;

    match action {
        ConfigAction::Show => {
            if ctx.format == CliOutputFormat::Json {
                let json = serde_json::to_string_pretty(&ctx.config).into_diagnostic()?;
                writeln!(writer, "{}", json).into_diagnostic()?;
            } else {
                eprintln!(
                    "{}",
                    info_msg(&format!("Config path: {}", config_path.display()))
                );
                eprintln!("{}", separator());

                // Actual data to stdout
                let toml_string = toml::to_string_pretty(&ctx.config).into_diagnostic()?;
                writeln!(writer, "{}", toml_string).into_diagnostic()?;
            }
        }

        ConfigAction::Init { force } => {
            if config_path.exists() && !force {
                eprintln!(
                    "{}",
                    warn_msg(&format!(
                        "Config already exists at {}",
                        config_path.display()
                    ))
                );
                eprintln!("  Use --force to overwrite.");
                return Ok(());
            }

            let default_config = VettaConfig::default();
            default_config.save()?;
            eprintln!(
                "{}",
                success_msg(&format!("Initialized config at {}", config_path.display()))
            );
        }

        ConfigAction::Set(args) => {
            let payload = ConfigPayload::resolve(ctx, &args)?;
            let mut current_config = ctx.config.clone();
            let mut updated = false;

            if let Some(s) = payload.socket {
                current_config.ai_grpc_service_socket_path = s;
                updated = true;
            }
            if let Some(uri) = payload.mongo_uri {
                current_config.mongodb_uri = uri;
                updated = true;
            }
            if let Some(db) = payload.mongo_db {
                current_config.mongodb_database = db;
                updated = true;
            }
            if let Some(m) = payload.embedding_model {
                current_config.embedding_model = m;
                updated = true;
            }

            if updated {
                current_config.save()?;
                eprintln!("{}", success_msg("Configuration updated."));
            } else {
                eprintln!("{}", warn_msg("No updates provided."));
            }
        }
    }

    Ok(())
}
