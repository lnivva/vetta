use std::path::Path;

use tokio_stream::StreamExt;

use crate::db::{Db, EarningsRepository, SegmentInput, StoreEarningsRequest};
use crate::domain::{Quarter, Transcript, TranscriptSegment};
use crate::stt::{Stt, TranscribeOptions};

use super::errors::EarningsError;
use super::events::{EarningsEvent, EarningsObserver};
use super::utils::validate_media_file;

pub struct ProcessRequest {
    pub file_path: String,
    pub ticker: String,
    pub year: u16,
    pub quarter: Quarter,
    pub language: Option<String>,
    pub initial_prompt: Option<String>,
    pub replace: bool,
}

pub struct EarningsProcessor {
    stt: Box<dyn Stt>,
    db: Db,
}

impl EarningsProcessor {
    /// Create a new processor with injected dependencies.
    pub fn new(stt: Box<dyn Stt>, db: Db) -> Self {
        Self { stt, db }
    }

    /// Runs the full validate → transcribe → store pipeline, notifying the
    /// observer at each stage boundary.
    pub async fn process(
        &self,
        request: ProcessRequest,
        observer: &dyn EarningsObserver,
    ) -> Result<Transcript, EarningsError> {
        let repo = EarningsRepository::new(&self.db);

        // ── Stage 1: Validation ──────────────────────────────
        let format_info = validate_media_file(&request.file_path)?;
        observer.on_event(&EarningsEvent::ValidationPassed {
            format_info: format_info.clone(),
        });

        Self::ensure_not_duplicate(&request, &repo).await?;

        // ── Stage 2: Transcription ───────────────────────────
        let transcript = self.transcribe(&request, observer).await?;

        // ── Stage 3: Persist ─────────────────────────────────
        let chunk_count = transcript.segments.len() as u32;
        observer.on_event(&EarningsEvent::StoringChunks { chunk_count });

        let call_id = self
            .store(&request, &transcript, &format_info, &repo)
            .await?;

        observer.on_event(&EarningsEvent::Stored {
            call_id: call_id.to_hex(),
            chunk_count,
        });

        Ok(transcript)
    }

    // ── Private helpers ──────────────────────────────────────

    async fn transcribe(
        &self,
        request: &ProcessRequest,
        observer: &dyn EarningsObserver,
    ) -> Result<Transcript, EarningsError> {
        let options = TranscribeOptions {
            language: request.language.clone(),
            initial_prompt: request.initial_prompt.clone(),
            diarization: true,
            num_speakers: None,
        };

        let mut stream = self.stt.transcribe(&request.file_path, options).await?;
        let mut segments: Vec<TranscriptSegment> = Vec::new();

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

            observer.on_event(&EarningsEvent::TranscriptionProgress {
                segments: segments.len() as u32,
            });
        }

        let transcript = Transcript { segments };

        observer.on_event(&EarningsEvent::TranscriptionComplete {
            transcript: transcript.clone(),
        });

        Ok(transcript)
    }

    async fn store(
        &self,
        request: &ProcessRequest,
        transcript: &Transcript,
        format_info: &str,
        repo: &EarningsRepository,
    ) -> Result<mongodb::bson::oid::ObjectId, EarningsError> {
        let file_name = Path::new(&request.file_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| request.file_path.clone());

        let file_format = format_info.split(' ').next().map(String::from);

        let store_request = StoreEarningsRequest {
            ticker: request.ticker.clone(),
            year: request.year,
            quarter: request.quarter.to_string(),
            file_name,
            file_hash: None,
            format: file_format,
            duration_seconds: transcript.duration(),
            stt_model: "whisper-large-v3".into(),
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

        Ok(call_id)
    }

    async fn ensure_not_duplicate(
        request: &ProcessRequest,
        repo: &EarningsRepository,
    ) -> Result<(), EarningsError> {
        let quarter = request.quarter.to_string();

        let existing = repo
            .find_call(&request.ticker, request.year, &quarter)
            .await?;

        if existing.is_some() && !request.replace {
            return Err(EarningsError::Duplicate(format!(
                "{} {} {}",
                request.ticker, request.year, request.quarter
            )));
        }

        Ok(())
    }
}
