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

    #[error("Embedding dimension mismatch: established dimension {expected}, but got {got}")]
    #[diagnostic(
        code(vetta::embedding::dimension_mismatch),
        help(
            "All embedding vectors for a single call must have the same dimensionality. A vector was returned with a different length than the first vector in the batch set. This may indicate a model configuration issue or a service bug."
        )
    )]
    DimensionMismatch { expected: usize, got: usize },

    #[error(transparent)]
    #[diagnostic(transparent)]
    Channel(#[from] UdsChannelError),
}

impl From<tonic::Status> for EmbeddingError {
    fn from(status: tonic::Status) -> Self {
        EmbeddingError::Service(Box::new(status))
    }
}
