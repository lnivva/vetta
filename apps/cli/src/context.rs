use std::path::PathBuf;

#[derive(Clone)]
pub struct AppContext {
    pub socket: PathBuf,
    pub quiet: bool,
    pub output: OutputMode,
}

#[derive(Clone, Copy)]
pub enum OutputMode {
    Pretty,
    Json,
}
