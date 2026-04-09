use std::path::PathBuf;

#[derive(Clone)]
pub struct AppContext {
    pub socket: PathBuf,
    pub quiet: bool,
}
