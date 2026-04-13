use miette::Diagnostic;
use thiserror::Error;

use crate::db::DbError;
use crate::embeddings::errors::EmbeddingError;
use crate::stt::SttError;

#[derive(Error, Debug, Diagnostic)]
pub enum IngestError {
    #[error("File not found: {0}")]
    #[diagnostic(
        code(vetta::ingest::file_not_found),
        help("Please check if the path is correct and you have read permissions.")
    )]
    FileNotFound(String),

    #[error("Path is not a regular file: {0}")]
    #[diagnostic(
        code(vetta::ingest::not_a_file),
        help(
            "The path exists but points to a directory or other non-file resource. Please provide a path to an actual media file."
        )
    )]
    NotAFile(String),

    #[error("File is empty (0 bytes)")]
    #[diagnostic(
        code(vetta::ingest::empty_file),
        help("The file exists but has no content. Check if the download completed successfully.")
    )]
    FileEmpty,

    #[error("File too large")]
    #[diagnostic(
        code(vetta::ingest::file_too_large),
        help(
            "The file is {got}MB, but the limit is {limit}MB. Try compressing the audio or splitting it."
        )
    )]
    FileTooLarge { limit: u64, got: u64 },

    #[error("Unsupported format detected: {0}")]
    #[diagnostic(
        code(vetta::ingest::invalid_format),
        help(
            "Vetta only supports: mp3, wav, m4a, mp4. Please convert the file using ffmpeg first."
        )
    )]
    InvalidFormat(String),

    #[error("Could not determine file type")]
    #[diagnostic(
        code(vetta::ingest::unknown_type),
        help("The file header is corrupt or missing magic bytes.")
    )]
    UnknownType,

    #[error(transparent)]
    #[diagnostic(code(vetta::io::error))]
    Io(#[from] std::io::Error),
}

#[derive(Error, Debug, Diagnostic)]
pub enum EarningsError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Ingest(#[from] IngestError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Transcription(#[from] SttError),

    #[error("Database error: {0}")]
    #[diagnostic(code(vetta::earnings::database))]
    Database(String),

    #[error("Duplicate earnings call: {0}")]
    #[diagnostic(
        code(vetta::earnings::duplicate),
        help("This earnings call has already been processed. Use --replace to overwrite.")
    )]
    Duplicate(String),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Embedding(#[from] EmbeddingError),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<DbError> for EarningsError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::Duplicate(msg) => Self::Duplicate(msg),
            other => Self::Database(other.to_string()),
        }
    }
}
