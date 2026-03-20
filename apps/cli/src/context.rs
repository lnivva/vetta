use std::path::PathBuf;

#[derive(Clone)]
pub struct AppContext {
    pub socket: PathBuf,
    pub quiet: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum OutputMode {
    Pretty,
    Json,
    Both,
}