use crate::cli::CliOutputOptions;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct AppContext {
    pub socket: PathBuf,
    pub verbose: bool,
    pub debug: bool,
    pub output: CliOutputOptions,
}
