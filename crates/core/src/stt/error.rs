use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum SttError {
    #[error("Failed to connect to STT service: {0}")]
    #[diagnostic(help("Is the whisper service running? Try: make run"))]
    Connection(#[from] tonic::transport::Error),

    #[error("STT service error: {0}")]
    Service(#[from] tonic::Status),

    #[error("Socket not found: {0}")]
    #[diagnostic(help("Start the whisper service or check the socket path in config.toml"))]
    SocketNotFound(String),

    #[error("Audio file not found: {0}")]
    #[diagnostic(help("Check that the file path is correct and the file exists"))]
    AudioFileNotFound(String),
}
