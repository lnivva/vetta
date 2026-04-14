use std::path::Path;
use std::time::Instant;

use tokio_stream::StreamExt;

use crate::db::models::{MongoDocument, OptimizedChunk};
use crate::db::{ChunkInput, Db, EarningsRepository, SegmentInput, StoreEarningsRequest};
use crate::embeddings::domain::{Embedder, InputType};
use crate::embeddings::errors::EmbeddingError;
use crate::stt::domain::{Quarter, Transcript, TranscriptSegment};
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
    embedder: Box<dyn Embedder>,
    db: Db,
}

impl EarningsProcessor {
    /// Create a new processor with injected dependencies.
    pub fn new(stt: Box<dyn Stt>, embedder: Box<dyn Embedder>, db: Db) -> Self {
        Self { stt, embedder, db }
    }

    /// Runs the full pipeline, notifying the observer at each stage boundary.
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

        // ── 5. Chunk Optimization ────────────────────────────
        let optimized_chunks = self.optimize_chunks(&transcript, observer);
        let chunk_count = optimized_chunks.len() as u32;

        // ── 6. Persist Earnings Call & Chunks ────────────────
        observer.on_event(&EarningsEvent::StoringCall { chunk_count });

        let call_id = self
            .store(
                &request,
                &transcript,
                &optimized_chunks,
                &format_info,
                &repo,
            )
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

        // ── 7. Generate & Store Embeddings ────────────────────
        let model_version = "voyage-4-large";

        self.process_embeddings(call_id, chunk_count, model_version, &repo, observer)
            .await
            .inspect_err(|e| {
                observer.on_event(&EarningsEvent::PipelineFailed {
                    stage: PipelineStage::Embedding,
                    error_message: e.to_string(),
                });
            })?;

        // ── 8. Pipeline Summary ──────────────────────────────
        let speaker_count = self.count_speakers(&transcript);
        let duration_secs = started_at.elapsed().as_secs_f64();

        observer.on_event(&EarningsEvent::PipelineComplete {
            call_id: call_id_hex,
            chunk_count,
            speaker_count,
            duration_secs,
        });

        let grouped_segments = transcript
            .as_dialogue()
            .into_iter()
            .map(|turn| TranscriptSegment {
                start_time: turn.start_time,
                end_time: turn.end_time,
                text: turn.text,
                speaker_id: turn.speaker,
                words: Vec::new(),
            })
            .collect();

        Ok(Transcript {
            segments: grouped_segments,
        })
    }

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

        while let Some(result) = stream.next().await {
            let chunk = result?;
            let text = chunk.text.trim().to_string();

            if !text.is_empty() {
                let domain_words = chunk
                    .words
                    .into_iter()
                    .map(|w| crate::stt::domain::WordTiming {
                        start_time: w.start_time,
                        end_time: w.end_time,
                        text: w.text,
                        confidence: w.confidence,
                    })
                    .collect();

                segments.push(TranscriptSegment {
                    start_time: chunk.start_time,
                    end_time: chunk.end_time,
                    text,
                    speaker_id: chunk.speaker_id,
                    words: domain_words,
                });
            }

            observer.on_event(&EarningsEvent::TranscriptionProgress {
                segments_completed: segments.len() as u32,
            });
        }

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

    fn optimize_chunks(
        &self,
        transcript: &Transcript,
        observer: &dyn EarningsObserver,
    ) -> Vec<OptimizedChunk> {
        let raw_chunk_count = transcript.segments.len() as u32;
        observer.on_event(&EarningsEvent::ChunkOptimizationStarted { raw_chunk_count });

        const TARGET_WORD_COUNT: usize = 300;

        let mut preliminary_chunks: Vec<OptimizedChunk> = Vec::new();
        let mut current_chunk: Option<OptimizedChunk> = None;

        for seg in &transcript.segments {
            let trimmed_text = seg.text.trim();
            if trimmed_text.is_empty() {
                continue;
            }

            let has_word_timings = !seg.words.is_empty();
            let seg_word_count = if has_word_timings {
                seg.words.len()
            } else {
                trimmed_text.split_whitespace().count()
            };

            if seg_word_count == 0 {
                continue;
            }

            if let Some(mut c) = current_chunk.take() {
                if c.speaker_id == seg.speaker_id
                    && (c.word_count as usize + seg_word_count) <= TARGET_WORD_COUNT
                {
                    c.text.push(' ');
                    c.text.push_str(trimmed_text);
                    c.end_time = seg.end_time;
                    c.word_count += seg_word_count as u32;
                    current_chunk = Some(c);
                    continue;
                } else {
                    preliminary_chunks.push(c);
                }
            }

            if has_word_timings && seg_word_count > TARGET_WORD_COUNT {
                for chunk_words in seg.words.chunks(TARGET_WORD_COUNT) {
                    let text = chunk_words
                        .iter()
                        .map(|w| w.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                        .trim()
                        .to_string();

                    let wc = chunk_words.len() as u32;

                    let chunk_start = chunk_words.first().unwrap().start_time;
                    let chunk_end = chunk_words.last().unwrap().end_time;

                    preliminary_chunks.push(OptimizedChunk {
                        speaker_id: seg.speaker_id.clone(),
                        start_time: chunk_start,
                        end_time: chunk_end,
                        text,
                        word_count: wc,
                        previous_text: None,
                        previous_speaker: None,
                        next_text: None,
                        next_speaker: None,
                    });
                }
            } else {
                current_chunk = Some(OptimizedChunk {
                    speaker_id: seg.speaker_id.clone(),
                    start_time: seg.start_time,
                    end_time: seg.end_time,
                    text: trimmed_text.to_string(),
                    word_count: seg_word_count as u32,
                    previous_text: None,
                    previous_speaker: None,
                    next_text: None,
                    next_speaker: None,
                });
            }
        }

        if let Some(c) = current_chunk {
            preliminary_chunks.push(c);
        }

        let final_count = preliminary_chunks.len();
        for i in 0..final_count {
            let prev_text = if i > 0 {
                Some(preliminary_chunks[i - 1].text.clone())
            } else {
                None
            };
            let prev_speaker = if i > 0 {
                Some(preliminary_chunks[i - 1].speaker_id.clone())
            } else {
                None
            };

            let next_text = if i < final_count.saturating_sub(1) {
                Some(preliminary_chunks[i + 1].text.clone())
            } else {
                None
            };
            let next_speaker = if i < final_count.saturating_sub(1) {
                Some(preliminary_chunks[i + 1].speaker_id.clone())
            } else {
                None
            };

            preliminary_chunks[i].previous_text = prev_text;
            preliminary_chunks[i].previous_speaker = prev_speaker;
            preliminary_chunks[i].next_text = next_text;
            preliminary_chunks[i].next_speaker = next_speaker;
        }

        let final_chunk_count = final_count as u32;

        observer.on_event(&EarningsEvent::ChunkOptimizationProgress {
            chunks_processed: final_chunk_count,
            total_chunks: final_chunk_count,
        });

        observer.on_event(&EarningsEvent::ChunkOptimizationComplete { final_chunk_count });

        preliminary_chunks
    }

    async fn store(
        &self,
        request: &ProcessEarningsCallRequest,
        transcript: &Transcript,
        optimized_chunks: &[OptimizedChunk],
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
                .as_dialogue()
                .into_iter()
                .map(|turn| SegmentInput {
                    start_time: turn.start_time,
                    end_time: turn.end_time,
                    text: turn.text,
                    speaker_id: turn.speaker,
                })
                .collect(),

            chunks: optimized_chunks
                .iter()
                .enumerate()
                .map(|(index, c)| ChunkInput {
                    chunk_index: index as u32,
                    speaker_id: c.speaker_id.clone(),
                    start_time: c.start_time,
                    end_time: c.end_time,
                    text: c.text.clone(),
                    word_count: c.word_count,
                    previous_text: c.previous_text.clone(),
                    previous_speaker: c.previous_speaker.clone(),
                    next_text: c.next_text.clone(),
                    next_speaker: c.next_speaker.clone(),
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

    /// Fetches chunks for a call, generates vector embeddings in batches via gRPC,
    /// stores them via concurrent updates, and marks the parent call as processed.
    async fn process_embeddings(
        &self,
        call_id: mongodb::bson::oid::ObjectId,
        chunk_count: u32,
        model_version: &str,
        repo: &EarningsRepository,
        observer: &dyn EarningsObserver,
    ) -> Result<(), EarningsError> {
        observer.on_event(&EarningsEvent::EmbeddingStarted { chunk_count });

        let chunks = repo.get_chunks(call_id).await?;

        let batch_size = 120;
        let mut updates: Vec<(mongodb::bson::oid::ObjectId, Vec<f32>)> =
            Vec::with_capacity(chunks.len());
        let mut chunks_embedded = 0;
        let mut embedding_dimension: Option<usize> = None;

        for batch in chunks.chunks(batch_size) {
            let texts: Vec<String> = batch.iter().map(|c| c.text.clone()).collect();

            let response = self
                .embedder
                .embed(model_version, texts, InputType::Document, true)
                .await?;

            if response.embeddings.len() != batch.len() {
                return Err(EmbeddingError::LengthMismatch {
                    expected: batch.len(),
                    got: response.embeddings.len(),
                }
                .into());
            }

            for (i, embedding) in response.embeddings.into_iter().enumerate() {
                // Validate vector dimensionality is consistent across all embeddings.
                let dim = embedding.vector.len();
                match embedding_dimension {
                    None => {
                        embedding_dimension = Some(dim);
                    }
                    Some(expected_dim) if dim != expected_dim => {
                        return Err(EmbeddingError::DimensionMismatch {
                            expected: expected_dim,
                            got: dim,
                        }
                        .into());
                    }
                    _ => {}
                }

                let chunk_id = batch[i].id.ok_or_else(|| {
                    EarningsError::Internal(format!(
                        "chunk at index {} for call {} has no _id",
                        chunks_embedded as usize + i,
                        call_id,
                    ))
                })?;

                updates.push((chunk_id, embedding.vector));
            }

            chunks_embedded += batch.len() as u32;
            observer.on_event(&EarningsEvent::EmbeddingProgress {
                chunks_embedded,
                total_chunks: chunk_count,
            });
        }

        let final_dimension = embedding_dimension.unwrap_or(0) as u32;

        observer.on_event(&EarningsEvent::EmbeddingComplete { chunk_count });
        observer.on_event(&EarningsEvent::StoringEmbeddings { chunk_count });

        repo.update_embeddings(updates, model_version).await?;

        repo.mark_call_processed(call_id, model_version, final_dimension)
            .await?;

        observer.on_event(&EarningsEvent::EmbeddingsStored { chunk_count });

        Ok(())
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
