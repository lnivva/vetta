use miette::Diagnostic;
use std::fs;
use std::path::Path;
use thiserror::Error;

const MAX_FILE_SIZE_MB: u64 = 500;
const ALLOWED_MIME_TYPES: [&str; 5] = [
    "audio/mpeg",  // .mp3
    "audio/wav",   // .wav
    "audio/x-wav", // .wav
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
        .map_err(IngestError::Io)?
        .ok_or(IngestError::UnknownType)?;

    if !ALLOWED_MIME_TYPES.contains(&kind.mime_type()) {
        return Err(IngestError::InvalidFormat(kind.mime_type().to_string()));
    }

    Ok(format!("{} ({}MB)", kind.mime_type(), size_mb))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_file_not_found() {
        let result = validate_media_file("non_existent_file.mp3");
        assert!(matches!(result, Err(IngestError::FileNotFound(_))));
    }

    #[test]
    fn test_file_empty() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_str().unwrap();
        let result = validate_media_file(path);
        assert!(matches!(result, Err(IngestError::FileEmpty)));
    }

    #[test]
    fn test_file_too_large() {
        let mut file = NamedTempFile::new().unwrap();
        // 501 MB
        let size = (MAX_FILE_SIZE_MB + 1) * 1024 * 1024;
        file.as_file_mut().set_len(size).unwrap();

        let path = file.path().to_str().unwrap();
        let result = validate_media_file(path);

        match result {
            Err(IngestError::FileTooLarge { limit, got }) => {
                assert_eq!(limit, MAX_FILE_SIZE_MB);
                assert_eq!(got, MAX_FILE_SIZE_MB + 1);
            }
            _ => panic!("Expected FileTooLarge error, got {:?}", result),
        }
    }

    #[test]
    fn test_invalid_format() {
        let mut file = NamedTempFile::new().unwrap();
        // Writing some text content (plain text is not in ALLOWED_MIME_TYPES)
        // However, 'infer' might return None if it doesn't recognize it as any known type.
        // Let's write something that is a known type but not allowed, e.g., a PDF.
        // PDF magic bytes: %PDF- (25 50 44 46 2D)
        file.write_all(b"%PDF-1.4\n").unwrap();

        let path = file.path().to_str().unwrap();
        let result = validate_media_file(path);

        match result {
            Err(IngestError::InvalidFormat(mime)) => {
                assert_eq!(mime, "application/pdf");
            }
            _ => panic!("Expected InvalidFormat error, got {:?}", result),
        }
    }

    #[test]
    fn test_unknown_type() {
        let mut file = NamedTempFile::new().unwrap();
        // Write some random bytes that don't match any known magic bytes
        file.write_all(&[0x00, 0x01, 0x02, 0x03, 0x04]).unwrap();

        let path = file.path().to_str().unwrap();
        let result = validate_media_file(path);

        assert!(matches!(result, Err(IngestError::UnknownType)));
    }

    #[test]
    fn test_valid_mp3() {
        let mut file = NamedTempFile::new().unwrap();
        // MP3 magic bytes can be complex, but 'infer' recognizes ID3 tag (49 44 33)
        file.write_all(&[0x49, 0x44, 0x33, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
            .unwrap();

        let path = file.path().to_str().unwrap();
        let result = validate_media_file(path);

        assert!(result.is_ok());
        assert!(result.unwrap().contains("audio/mpeg"));
    }

    #[test]
    fn test_valid_wav() {
        let mut file = NamedTempFile::new().unwrap();
        // WAV magic bytes: RIFF (52 49 46 46) ... WAVE (57 41 56 45)
        // RIFF + 4 bytes size + WAVE
        let mut wav_data = vec![0u8; 12];
        wav_data[0..4].copy_from_slice(b"RIFF");
        wav_data[8..12].copy_from_slice(b"WAVE");
        file.write_all(&wav_data).unwrap();

        let path = file.path().to_str().unwrap();
        let result = validate_media_file(path);

        match &result {
            Ok(msg) => assert!(msg.contains("audio/wav") || msg.contains("audio/x-wav")),
            Err(e) => panic!("Expected Ok, got Err: {:?}", e),
        }
    }
}
