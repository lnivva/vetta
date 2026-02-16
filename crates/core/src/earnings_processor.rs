use infer;
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

#[derive(Error, Debug)]
pub enum IngestError {
    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("File is empty")]
    FileEmpty,

    #[error("File too large. Limit is {limit}MB. Got: {got}MB")]
    FileTooLarge { limit: u64, got: u64 },

    #[error("Invalid format: {0}. Allowed: {1:?}")]
    InvalidFormat(String, &'static [&'static str]),

    #[error("Could not determine file type")]
    UnknownType,

    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
}

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
        return Err(IngestError::InvalidFormat(
            kind.mime_type().to_string(),
            &ALLOWED_MIME_TYPES,
        ));
    }

    Ok(format!("{} ({}MB)", kind.mime_type(), size_mb))
}
