use std::path::PathBuf;
use crate::cli::CliOutputOptions;

#[derive(Clone, Debug)]
pub struct AppContext {
    pub socket: PathBuf,
    pub verbose: bool,
    pub debug: bool,
    pub output: CliOutputOptions,
}