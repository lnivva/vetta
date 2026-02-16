use miette::Diagnostic;
use std::fs;
use std::path::Path;
use thiserror::Error;

const MAX_FILE_SIZE_MB: u64 = 500;
const ALLOWED_MIME_TYPES: [&str; 4] = [
    "audio/mpeg",  // .mp3
    "audio/wav",   // .wav
    "audio/x-m4a", // .m4a
    "video/mp4",   // .mp4
];

#[derive(Error, Debug, Diagnostic)]
pub enum IngestError {
    #[error("File not found: {0}")]
    #[diagnostic(
        code(vetta::ingest::file_not_found),
        help("Please check if the path is correct and you have read permissions.")
    )]
    FileNotFound(String),

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

/// Validates that an Earnings Call audio/video file is suitable for processing.
/// Checks existence, size limits, and magic bytes (content type).
pub fn validate_media_file(path_str: &str) -> Result<String, IngestError> {
    let path = Path::new(path_str);

    if !path.exists() {
        return Err(IngestError::FileNotFound(path_str.to_string()));
    }

    let metadata = fs::metadata(path)?;
    if metadata.len() == 0 {
        return Err(IngestError::FileEmpty);
    }

    let size_mb = metadata.len() / (1024 * 1024);
    if size_mb > MAX_FILE_SIZE_MB {
        return Err(IngestError::FileTooLarge {
            limit: MAX_FILE_SIZE_MB,
            got: size_mb,
        });
    }

    let kind = infer::get_from_path(path)
        .map_err(|_| IngestError::UnknownType)?
        .ok_or(IngestError::UnknownType)?;

    if !ALLOWED_MIME_TYPES.contains(&kind.mime_type()) {
        return Err(IngestError::InvalidFormat(kind.mime_type().to_string()));
    }

    Ok(format!("{} ({}MB)", kind.mime_type(), size_mb))
}
