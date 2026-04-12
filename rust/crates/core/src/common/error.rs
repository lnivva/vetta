use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum UdsChannelError {
    #[error("socket not found: {0}")]
    #[diagnostic(help(
        "Check that the service is running and the socket path in config.toml is correct"
    ))]
    SocketNotFound(String),

    #[error("failed to connect via Unix socket: {0}")]
    #[diagnostic(help("Is the gRPC service running? Try: make run"))]
    Transport(#[from] tonic::transport::Error),
}
