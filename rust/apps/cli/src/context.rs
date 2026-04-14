use crate::cli::CliOutputFormat;
use crate::config::VettaConfig;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct AppContext {
    pub config: VettaConfig,
    pub debug: bool,
    pub format: CliOutputFormat,
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
}
