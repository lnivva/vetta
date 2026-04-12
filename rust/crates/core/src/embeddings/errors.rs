use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum EmbeddingError {
    #[error("Failed to connect to Embedding service: {0}")]
    #[diagnostic(help("Is the embedding service running via the daemon?"))]
    Connection(#[from] tonic::transport::Error),

    #[error("Embedding service error: {0}")]
    Service(Box<tonic::Status>),

    #[error("Socket not found: {0}")]
    #[diagnostic(help("Start the vetta daemon or check the socket path in config.toml"))]
    SocketNotFound(String),
}

impl From<tonic::Status> for EmbeddingError {
    fn from(status: tonic::Status) -> Self {
        EmbeddingError::Service(Box::new(status))
    }
}