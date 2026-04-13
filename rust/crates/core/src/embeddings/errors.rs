use crate::common::UdsChannelError;
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

    #[error("Embedding response length mismatch: expected {expected}, got {got}")]
    #[diagnostic(
        code(vetta::embedding::length_mismatch),
        help(
            "The embedding service returned a different number of embeddings than the number of texts submitted. This may indicate a service bug or silent filtering."
        )
    )]
    LengthMismatch { expected: usize, got: usize },

    #[error(transparent)]
    #[diagnostic(transparent)]
    Channel(#[from] UdsChannelError),
}

impl From<tonic::Status> for EmbeddingError {
    fn from(status: tonic::Status) -> Self {
        EmbeddingError::Service(Box::new(status))
    }
}
