use clap::ValueEnum;
use directories::ProjectDirs;
use miette::{IntoDiagnostic, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum EmbeddingModel {
    #[serde(rename = "voyage-4-large")]
    #[value(name = "voyage-4-large")]
    Voyage4Large,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VettaConfig {
    pub ai_grpc_service_socket_path: PathBuf,
    pub embedding_model: EmbeddingModel,
    pub mongodb_uri: String,
    pub mongodb_database: String,
}

impl Default for VettaConfig {
    fn default() -> Self {
        Self {
            ai_grpc_service_socket_path: PathBuf::from("/tmp/whisper.sock"),
            embedding_model: EmbeddingModel::Voyage4Large,
            mongodb_uri: "mongodb://localhost:27017".to_string(),
            mongodb_database: "vetta".to_string(),
        }
    }
}

impl VettaConfig {
    /// Gets the path to the config file (e.g., ~/.config/Vetta CLI/config.toml on Linux/Mac)
    pub fn file_path() -> Option<PathBuf> {
        ProjectDirs::from("com", "Vetta", "Vetta CLI")
            .map(|proj_dirs| proj_dirs.config_dir().join("config.toml"))
    }

    /// Loads the configuration from disk.
    /// If it doesn't exist, it creates a default config file and returns it.
    pub fn load() -> Self {
        let Some(path) = Self::file_path() else {
            return Self::default();
        };

        if path.exists() {
            if let Ok(contents) = fs::read_to_string(&path) {
                return toml::from_str(&contents).unwrap_or_default();
            }
        } else {
            let default_config = Self::default();

            let _ = default_config.save();

            return default_config;
        }

        Self::default()
    }

    /// Saves the current configuration to disk
    pub fn save(&self) -> Result<()> {
        let Some(path) = Self::file_path() else {
            return Ok(());
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }

        let toml_string = toml::to_string_pretty(self).into_diagnostic()?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut options = fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            options.mode(0o600);

            let mut file = options.open(&path).into_diagnostic()?;
            file.write_all(toml_string.as_bytes()).into_diagnostic()?;
        }

        #[cfg(not(unix))]
        {
            // On Windows, use standard write (Windows handles user-dir permissions differently)
            fs::write(&path, toml_string).into_diagnostic()?;
        }

        Ok(())
    }
}
