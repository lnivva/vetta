use miette::Diagnostic;
use std::fs;
use std::path::Path;
use thiserror::Error;

use crate::db::{Db, DbConfig, DbError, EarningsRepository, SegmentInput, StoreEarningsRequest};
use crate::domain::{Quarter, Transcript, TranscriptSegment};
use crate::stt::{Stt, SttError, TranscribeOptions};

// ── Constants ────────────────────────────────────────────────

const MAX_FILE_SIZE_MB: u64 = 500;
const ALLOWED_MIME_TYPES: [&str; 5] = [
    "audio/mpeg",
    "audio/wav",
    "audio/x-wav",
    "audio/x-m4a",
    "video/mp4",
];

// ── Errors ───────────────────────────────────────────────────

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

#[derive(Error, Debug, Diagnostic)]
pub enum PipelineError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Ingest(#[from] IngestError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Transcription(#[from] SttError),

    #[error("Database error: {0}")]
    #[diagnostic(code(vetta::pipeline::database))]
    Database(String),

    #[error("Duplicate earnings call: {0}")]
    #[diagnostic(
        code(vetta::pipeline::duplicate),
        help("This earnings call has already been processed. Use --replace to overwrite.")
    )]
    Duplicate(String),
}

impl From<DbError> for PipelineError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::Duplicate(msg) => PipelineError::Duplicate(msg),
            other => PipelineError::Database(other.to_string()),
        }
    }
}

// ── Pipeline events ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum PipelineEvent {
    ValidationPassed { format_info: String },
    TranscriptionProgress { segments: u32 },
    TranscriptionComplete { transcript: Transcript },
    StoringChunks { chunk_count: u32 },
    Stored { call_id: String, chunk_count: u32 },
}

// ── Pipeline request ─────────────────────────────────────────

pub struct ProcessRequest {
    pub file_path: String,
    pub ticker: String,
    pub year: u16,
    pub quarter: Quarter,
    pub language: Option<String>,
    pub initial_prompt: Option<String>,
    pub replace: bool,
}

// ── Validation ───────────────────────────────────────────────

pub fn validate_media_file(path_str: &str) -> Result<String, IngestError> {
    let path = Path::new(path_str);

    if !path.exists() {
        return Err(IngestError::FileNotFound(path_str.to_string()));
    }

    let metadata = fs::metadata(path)?;
    if metadata.len() == 0 {
        return Err(IngestError::FileEmpty);
    }

    let size_bytes = metadata.len();
    let size_mb = size_bytes / (1024 * 1024);

    if size_bytes > MAX_FILE_SIZE_MB * 1024 * 1024 {
        return Err(IngestError::FileTooLarge {
            limit: MAX_FILE_SIZE_MB,
            got: size_bytes.div_ceil(1024 * 1024),
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

// ── Orchestrator ─────────────────────────────────────────────

pub struct EarningsProcessor {
    stt: Box<dyn Stt>,
    db: Db,
}

impl EarningsProcessor {
    pub fn new(stt: Box<dyn Stt>, db: Db) -> Self {
        Self { stt, db }
    }

    pub async fn from_env(stt: Box<dyn Stt>) -> Result<Self, PipelineError> {
        let db_config = DbConfig::from_env().map_err(|e| PipelineError::Database(e.to_string()))?;

        let db = Db::connect(&db_config)
            .await
            .map_err(|e| PipelineError::Database(e.to_string()))?;

        // Ensure indexes on startup
        let repo = EarningsRepository::new(&db);
        repo.ensure_indexes()
            .await
            .map_err(|e| PipelineError::Database(e.to_string()))?;

        Ok(Self { stt, db })
    }

    /// Runs the full pipeline, yielding progress events through a callback.
    pub async fn process(
        &self,
        request: ProcessRequest,
        mut on_event: impl FnMut(PipelineEvent),
    ) -> Result<Transcript, PipelineError> {
        let repo = EarningsRepository::new(&self.db);

        // ── Stage 1: Validation ──────────────────────────────
        let format_info = validate_media_file(&request.file_path)?;
        on_event(PipelineEvent::ValidationPassed {
            format_info: format_info.clone(),
        });

        Self::ensure_not_duplicate(&request, &repo).await?;

        // ── Stage 2: Transcription ───────────────────────────
        let options = TranscribeOptions {
            language: request.language.clone(),
            initial_prompt: request.initial_prompt.clone(),
            diarization: true,
            num_speakers: None,
        };

        let mut stream = self.stt.transcribe(&request.file_path, options).await?;

        let mut segments: Vec<TranscriptSegment> = Vec::new();

        use tokio_stream::StreamExt;
        while let Some(result) = stream.next().await {
            let chunk = result?;

            let text = chunk.text.trim().to_string();
            if !text.is_empty() {
                segments.push(TranscriptSegment {
                    start_time: chunk.start_time,
                    end_time: chunk.end_time,
                    text,
                    speaker_id: chunk.speaker_id,
                });
            }

            on_event(PipelineEvent::TranscriptionProgress {
                segments: segments.len() as u32,
            });
        }

        let transcript = Transcript { segments };

        on_event(PipelineEvent::TranscriptionComplete {
            transcript: transcript.clone(),
        });

        // ── Stage 3: Store in MongoDB ────────────────────────
        let file_name = Path::new(&request.file_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| request.file_path.clone());

        // Parse format from the validation info (e.g. "audio/mpeg (12MB)" → "audio/mpeg")
        let file_format = format_info.split(' ').next().map(String::from);

        let chunk_count = transcript.segments.len() as u32;
        on_event(PipelineEvent::StoringChunks { chunk_count });

        let store_request = StoreEarningsRequest {
            ticker: request.ticker.clone(),
            year: request.year,
            quarter: request.quarter.to_string(),
            file_name,
            file_hash: None, // TODO: compute SHA-256 during validation
            format: file_format,
            duration_seconds: transcript.duration(),
            stt_model: "whisper-large-v3".into(), // TODO: get from STT strategy
            segments: transcript
                .segments
                .iter()
                .map(|s| SegmentInput {
                    start_time: s.start_time,
                    end_time: s.end_time,
                    text: s.text.clone(),
                    speaker_id: s.speaker_id.clone(),
                })
                .collect(),
        };

        let call_id = if request.replace {
            repo.replace(store_request).await?
        } else {
            repo.store(store_request).await?
        };

        on_event(PipelineEvent::Stored {
            call_id: call_id.to_hex(),
            chunk_count,
        });

        Ok(transcript)
    }

    async fn ensure_not_duplicate(
        request: &ProcessRequest,
        repo: &EarningsRepository,
    ) -> Result<(), PipelineError> {
        let quarter = request.quarter.to_string();

        let existing = repo
            .find_call(&request.ticker, request.year, &quarter)
            .await?;

        if existing.is_some() && !request.replace {
            return Err(PipelineError::Duplicate(format!(
                "{} {} {}",
                request.ticker, request.year, request.quarter
            )));
        }

        Ok(())
    }
}

// ── Tests ────────────────────────────────────────────────────

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
