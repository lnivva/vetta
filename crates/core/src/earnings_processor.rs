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

    fn write_temp(bytes: &[u8]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(bytes).unwrap();
        file.flush().unwrap();
        file
    }

    fn validate_path(path: &Path) -> Result<String, IngestError> {
        validate_media_file(path.to_str().expect("utf-8 temp path"))
    }

    #[test]
    fn file_not_found_includes_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("non_existent_file.mp3");
        let path_str = path.to_str().unwrap();
        let err = validate_media_file(path_str).unwrap_err();
        assert!(matches!(err, IngestError::FileNotFound(p) if p == path_str));
    }

    #[test]
    fn empty_file_is_rejected() {
        let file = NamedTempFile::new().unwrap();
        let err = validate_path(file.path()).unwrap_err();
        assert!(matches!(err, IngestError::FileEmpty));
    }

    #[test]
    fn file_too_large_reports_limit_and_got() {
        let mut file = NamedTempFile::new().unwrap();

        file.as_file_mut()
            .set_len((MAX_FILE_SIZE_MB + 1) * 1024 * 1024)
            .unwrap();

        let err = validate_path(file.path()).unwrap_err();

        assert!(matches!(
            err,
            IngestError::FileTooLarge { limit, got }
                if limit == MAX_FILE_SIZE_MB && got == MAX_FILE_SIZE_MB + 1
        ));
    }

    #[test]
    fn rejects_disallowed_format_pdf() {
        let file = write_temp(b"%PDF-1.4\n...payload...");
        let err = validate_path(file.path()).unwrap_err();
        assert!(matches!(err, IngestError::InvalidFormat(m) if m == "application/pdf"));
    }

    #[test]
    fn rejects_unknown_type() {
        let file = write_temp(&[0x00, 0x01, 0x02, 0x03, 0x04, 0xFF, 0xEE, 0xDD]);
        let err = validate_path(file.path()).unwrap_err();
        assert!(matches!(err, IngestError::UnknownType));
    }

    #[test]
    fn accepts_allowed_formats_smoke() {
        let cases: &[(&str, &[u8])] = &[
            ("mp3 (ID3)", b"ID3\x03\x00\x00\x00\x00\x00\x21some_payload"),
            ("wav (RIFF/WAVE)", b"RIFF\x24\x00\x00\x00WAVEfmt "),
            (
                "mp4 (ftyp)",
                b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00mp42isom",
            ),
        ];

        for (name, bytes) in cases {
            let file = write_temp(bytes);
            let res = validate_path(file.path());
            assert!(res.is_ok(), "expected Ok for {name}, got {res:?}");
        }
    }

    #[test]
    fn ok_message_includes_mime_and_size_suffix() {
        let file = write_temp(b"ID3\x03\x00\x00\x00\x00\x00\x21some_payload");
        let msg = validate_path(file.path()).unwrap();

        assert!(
            msg.contains("audio/mpeg"),
            "expected audio/mpeg in message, got: {msg}"
        );
        assert!(
            msg.ends_with("MB)"),
            "expected message to end with 'MB)', got: {msg}"
        );
    }
}
