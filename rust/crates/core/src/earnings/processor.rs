use std::path::Path;
use std::time::Instant;

use tokio_stream::StreamExt;

use crate::db::models::MongoDocument;
use crate::db::{Db, EarningsRepository, SegmentInput, StoreEarningsRequest};
use crate::domain::{Quarter, Transcript, TranscriptSegment};
use crate::stt::{Stt, TranscribeOptions};

use super::errors::EarningsError;
use super::events::{EarningsEvent, EarningsObserver, PipelineStage};
use super::utils::validate_media_file;

pub struct ProcessEarningsCallRequest {
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

    /// Runs the full pipeline, notifying the observer at each stage boundary.
    ///
    /// Pipeline stages:
    /// 1. Command received
    /// 2. Validate media file
    /// 3. Duplicate check
    /// 4. Transcription (STT + diarization)
    /// 5. Chunk optimization (TODO)
    /// 6. Persist earnings call & chunks
    /// 7. Generate embeddings (TODO)
    /// 8. Store embeddings (TODO)
    /// 9. Pipeline summary
    pub async fn process(
        &self,
        request: ProcessEarningsCallRequest,
        observer: &dyn EarningsObserver,
    ) -> Result<Transcript, EarningsError> {
        let started_at = Instant::now();
        let repo = EarningsRepository::new(&self.db);

        // ── 1. Command Received ──────────────────────────────
        observer.on_event(&EarningsEvent::PipelineStarted {
            file_path: request.file_path.clone(),
            replace: request.replace,
        });

        // ── 2. Validate Media File ──────────────────────────
        observer.on_event(&EarningsEvent::ValidatingFile {
            file_path: request.file_path.clone(),
        });

        let format_info = validate_media_file(&request.file_path).inspect_err(|e| {
            observer.on_event(&EarningsEvent::PipelineFailed {
                stage: PipelineStage::Validation,
                error_message: e.to_string(),
            });
        })?;

        observer.on_event(&EarningsEvent::ValidationPassed {
            format_info: format_info.clone(),
        });

        // ── 3. Duplicate Check ──────────────────────────────
        observer.on_event(&EarningsEvent::CheckingDuplicate {
            file_path: request.file_path.clone(),
        });

        let existing_call_id = self
            .check_duplicate(&request, &repo)
            .await
            .inspect_err(|e| {
                observer.on_event(&EarningsEvent::PipelineFailed {
                    stage: PipelineStage::DuplicateCheck,
                    error_message: e.to_string(),
                });
            })?;

        match existing_call_id {
            Some(id) => {
                observer.on_event(&EarningsEvent::DuplicateOverridden {
                    existing_call_id: id,
                });
            }
            None => {
                observer.on_event(&EarningsEvent::DuplicateCheckPassed);
            }
        }

        // ── 4. Transcription (STT + Diarization) ────────────
        let transcript = self.transcribe(&request, observer).await.inspect_err(|e| {
            observer.on_event(&EarningsEvent::PipelineFailed {
                stage: PipelineStage::Transcription,
                error_message: e.to_string(),
            });
        })?;

        // ── 5. Chunk Optimization (TODO) ─────────────────────
        let chunk_count = transcript.segments.len() as u32;
        self.optimize_chunks(chunk_count, observer);

        // ── 6. Persist Earnings Call & Chunks ────────────────
        observer.on_event(&EarningsEvent::StoringCall { chunk_count });

        let call_id = self
            .store(&request, &transcript, &format_info, &repo)
            .await
            .inspect_err(|e| {
                observer.on_event(&EarningsEvent::PipelineFailed {
                    stage: PipelineStage::StoringCall,
                    error_message: e.to_string(),
                });
            })?;

        let call_id_hex = call_id.to_hex();

        observer.on_event(&EarningsEvent::CallStored {
            call_id: call_id_hex.clone(),
            chunk_count,
        });

        // ── 7. Generate Embeddings (TODO) ────────────────────
        self.generate_embeddings(chunk_count, observer);

        // ── 8. Store Embeddings (TODO) ───────────────────────
        self.store_embeddings(chunk_count, observer);

        // ── 9. Pipeline Summary ──────────────────────────────
        let speaker_count = self.count_speakers(&transcript);
        let duration_secs = started_at.elapsed().as_secs_f64();

        observer.on_event(&EarningsEvent::PipelineComplete {
            call_id: call_id_hex,
            chunk_count,
            speaker_count,
            duration_secs,
        });

        Ok(transcript)
    }

    // ── Private helpers ──────────────────────────────────────

    /// Returns `Some(call_id_hex)` when a duplicate exists and `--replace` is set.
    /// Returns `None` when no duplicate exists.
    /// Returns `Err` when a duplicate exists and `--replace` is *not* set.
    async fn check_duplicate(
        &self,
        request: &ProcessEarningsCallRequest,
        repo: &EarningsRepository,
    ) -> Result<Option<String>, EarningsError> {
        let quarter = request.quarter.to_string();

        let existing = repo
            .find_call(&request.ticker, request.year, &quarter)
            .await?;

        match existing {
            Some(doc) => {
                if request.replace {
                    let id = doc.id_hex().unwrap_or_else(|_| "unknown".into());
                    Ok(Some(id))
                } else {
                    Err(EarningsError::Duplicate(format!(
                        "{} {} {}",
                        request.ticker, request.year, request.quarter
                    )))
                }
            }
            None => Ok(None),
        }
    }

    async fn transcribe(
        &self,
        request: &ProcessEarningsCallRequest,
        observer: &dyn EarningsObserver,
    ) -> Result<Transcript, EarningsError> {
        observer.on_event(&EarningsEvent::TranscriptionStarted);

        let options = TranscribeOptions {
            language: request.language.clone(),
            initial_prompt: request.initial_prompt.clone(),
            diarization: true,
            num_speakers: None,
        };

        let mut stream = self.stt.transcribe(&request.file_path, options).await?;
        let mut segments: Vec<TranscriptSegment> = Vec::new();

        // STT streaming — report progress for each decoded chunk
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
                segments_completed: segments.len() as u32,
            });
        }

        // Diarization is currently inline with transcription (whisper‑diarize),
        // but we emit separate events so the CLI can report it distinctly.
        observer.on_event(&EarningsEvent::DiarizationStarted);

        let speaker_count = self.count_speakers_from_segments(&segments);

        observer.on_event(&EarningsEvent::DiarizationProgress {
            speakers_identified: speaker_count,
        });

        observer.on_event(&EarningsEvent::DiarizationComplete { speaker_count });

        let transcript = Transcript { segments };

        observer.on_event(&EarningsEvent::TranscriptionComplete {
            transcript: transcript.clone(),
        });

        Ok(transcript)
    }

    /// TODO: Semantic chunk optimization (merge short segments, split overly long ones, etc.). For now we emit the events with a pass-through.
    fn optimize_chunks(&self, raw_chunk_count: u32, observer: &dyn EarningsObserver) {
        observer.on_event(&EarningsEvent::ChunkOptimizationStarted { raw_chunk_count });

        // Placeholder: no actual optimization yet — output == input
        observer.on_event(&EarningsEvent::ChunkOptimizationProgress {
            chunks_processed: raw_chunk_count,
            total_chunks: raw_chunk_count,
        });

        observer.on_event(&EarningsEvent::ChunkOptimizationComplete {
            final_chunk_count: raw_chunk_count,
        });
    }

    /// TODO: Generate vector embeddings for each chunk.
    fn generate_embeddings(&self, chunk_count: u32, observer: &dyn EarningsObserver) {
        observer.on_event(&EarningsEvent::EmbeddingStarted { chunk_count });

        // Placeholder: no actual embedding generation yet
        observer.on_event(&EarningsEvent::EmbeddingProgress {
            chunks_embedded: 0,
            total_chunks: chunk_count,
        });

        observer.on_event(&EarningsEvent::EmbeddingComplete { chunk_count: 0 });
    }

    /// TODO: Persist embedding vectors to MongoDB.
    fn store_embeddings(&self, chunk_count: u32, observer: &dyn EarningsObserver) {
        observer.on_event(&EarningsEvent::StoringEmbeddings { chunk_count });

        // Placeholder: no actual storage yet
        observer.on_event(&EarningsEvent::EmbeddingsStored { chunk_count: 0 });
    }

    async fn store(
        &self,
        request: &ProcessEarningsCallRequest,
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

    fn count_speakers(&self, transcript: &Transcript) -> u32 {
        self.count_speakers_from_segments(&transcript.segments)
    }

    fn count_speakers_from_segments(&self, segments: &[TranscriptSegment]) -> u32 {
        let mut speakers = std::collections::HashSet::new();
        for seg in segments {
            if !seg.speaker_id.is_empty() {
                speakers.insert(seg.speaker_id.clone());
            }
        }
        speakers.len() as u32
    }
}
