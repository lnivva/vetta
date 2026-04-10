use crate::stt::domain::Transcript;

/// Every discrete progress signal the earnings pipeline can emit.
///
/// Ordered roughly by pipeline stage. Each variant carries just enough context for a CLI or
/// GUI observer to render a meaningful status line.
///
/// `Clone + Send` so it can cross thread / channel boundaries.
#[derive(Debug, Clone)]
pub enum EarningsEvent {
    /// The pipeline has accepted a new ingest request.
    PipelineStarted { file_path: String, replace: bool },

    /// Beginning file-level validation (existence, size, format).
    ValidatingFile { file_path: String },

    /// File passed all validation checks.
    ValidationPassed { format_info: String },

    /// Checking whether this call already exists in the store.
    CheckingDuplicate { file_path: String },

    /// The file is a duplicate but `--replace` was set, so we proceed.
    DuplicateOverridden { existing_call_id: String },

    /// No duplicate found — first ingest for this file.
    DuplicateCheckPassed,

    /// Transcription has started.
    TranscriptionStarted,

    /// A new batch of segments has been decoded (incremental progress).
    TranscriptionProgress { segments_completed: u32 },

    /// Diarization (speaker identification) has started.
    DiarizationStarted,

    /// Diarization progress update.
    DiarizationProgress { speakers_identified: u32 },

    /// Diarization complete.
    DiarizationComplete { speaker_count: u32 },

    /// Full transcript (with speaker labels) is available.
    TranscriptionComplete { transcript: Transcript },

    /// Starting semantic chunk optimization.
    ChunkOptimizationStarted { raw_chunk_count: u32 },

    /// Chunk optimization progress.
    ChunkOptimizationProgress {
        chunks_processed: u32,
        total_chunks: u32,
    },

    /// Chunk optimization finished; may have merged or split chunks.
    ChunkOptimizationComplete { final_chunk_count: u32 },

    /// About to write the earnings call document and its chunks to MongoDB.
    StoringCall { chunk_count: u32 },

    /// Earnings call and chunks persisted successfully.
    CallStored { call_id: String, chunk_count: u32 },

    /// Starting embedding generation for stored chunks.
    EmbeddingStarted { chunk_count: u32 },

    /// Embedding generation progress.
    EmbeddingProgress {
        chunks_embedded: u32,
        total_chunks: u32,
    },

    /// All embeddings generated.
    EmbeddingComplete { chunk_count: u32 },

    /// Writing embedding vectors to MongoDB.
    StoringEmbeddings { chunk_count: u32 },

    /// Embedding vectors persisted.
    EmbeddingsStored { chunk_count: u32 },

    /// The full pipeline finished successfully.
    PipelineComplete {
        call_id: String,
        chunk_count: u32,
        speaker_count: u32,
        duration_secs: f64,
    },

    /// The pipeline failed at some stage.
    PipelineFailed {
        stage: PipelineStage,
        error_message: String,
    },
}

/// Identifies which pipeline stage an error originated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    Validation,
    DuplicateCheck,
    Transcription,
    Diarization,
    ChunkOptimization,
    StoringCall,
    Embedding,
    StoringEmbeddings,
}

impl std::fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation => write!(f, "validation"),
            Self::DuplicateCheck => write!(f, "duplicate check"),
            Self::Transcription => write!(f, "transcription"),
            Self::Diarization => write!(f, "diarization"),
            Self::ChunkOptimization => write!(f, "chunk optimization"),
            Self::StoringCall => write!(f, "storing call"),
            Self::Embedding => write!(f, "embedding generation"),
            Self::StoringEmbeddings => write!(f, "storing embeddings"),
        }
    }
}

/// Trait that any consumer (CLI, desktop, server) implements to react to earnings pipeline
/// progress.
///
/// All methods have default no-op implementations so consumers only override the events they
/// care about.
///
/// # Example (CLI)
///
/// ```rust,ignore
/// struct CliObserver;
///
/// impl EarningsObserver for CliObserver {
///     fn on_event(&self, event: &EarningsEvent) {
///         match event {
///             EarningsEvent::PipelineStarted { file_path, replace } => {
///                 eprintln!("⏳ Processing {file_path} (replace={replace})");
///             }
///             EarningsEvent::TranscriptionProgress { segments_completed } => {
///                 eprintln!("  …{segments_completed} segments so far");
///             }
///             EarningsEvent::PipelineComplete { call_id, duration_secs, .. } => {
///                 eprintln!("  ✔ {call_id} done in {duration_secs:.1}s");
///             }
///             EarningsEvent::PipelineFailed { stage, error_message } => {
///                 eprintln!("  ✘ failed at {stage}: {error_message}");
///             }
///             _ => {}
///         }
///     }
/// }
/// ```
pub trait EarningsObserver: Send + Sync {
    /// Called for every pipeline event. Override this single method for a
    /// catch-all handler, or leave the default to dispatch to fine-grained hooks.
    fn on_event(&self, event: &EarningsEvent) {
        match event {
            EarningsEvent::PipelineStarted { file_path, replace } => {
                self.on_pipeline_started(file_path, *replace);
            }

            EarningsEvent::ValidatingFile { file_path } => {
                self.on_validating_file(file_path);
            }
            EarningsEvent::ValidationPassed { format_info } => {
                self.on_validation_passed(format_info);
            }

            EarningsEvent::CheckingDuplicate { file_path } => {
                self.on_checking_duplicate(file_path);
            }
            EarningsEvent::DuplicateOverridden { existing_call_id } => {
                self.on_duplicate_overridden(existing_call_id);
            }
            EarningsEvent::DuplicateCheckPassed => {
                self.on_duplicate_check_passed();
            }

            EarningsEvent::TranscriptionStarted => {
                self.on_transcription_started();
            }
            EarningsEvent::TranscriptionProgress { segments_completed } => {
                self.on_transcription_progress(*segments_completed);
            }
            EarningsEvent::DiarizationStarted => {
                self.on_diarization_started();
            }
            EarningsEvent::DiarizationProgress {
                speakers_identified,
            } => {
                self.on_diarization_progress(*speakers_identified);
            }
            EarningsEvent::DiarizationComplete { speaker_count } => {
                self.on_diarization_complete(*speaker_count);
            }
            EarningsEvent::TranscriptionComplete { transcript } => {
                self.on_transcription_complete(transcript);
            }

            EarningsEvent::ChunkOptimizationStarted { raw_chunk_count } => {
                self.on_chunk_optimization_started(*raw_chunk_count);
            }
            EarningsEvent::ChunkOptimizationProgress {
                chunks_processed,
                total_chunks,
            } => {
                self.on_chunk_optimization_progress(*chunks_processed, *total_chunks);
            }
            EarningsEvent::ChunkOptimizationComplete { final_chunk_count } => {
                self.on_chunk_optimization_complete(*final_chunk_count);
            }

            EarningsEvent::StoringCall { chunk_count } => {
                self.on_storing_call(*chunk_count);
            }
            EarningsEvent::CallStored {
                call_id,
                chunk_count,
            } => {
                self.on_call_stored(call_id, *chunk_count);
            }

            EarningsEvent::EmbeddingStarted { chunk_count } => {
                self.on_embedding_started(*chunk_count);
            }
            EarningsEvent::EmbeddingProgress {
                chunks_embedded,
                total_chunks,
            } => {
                self.on_embedding_progress(*chunks_embedded, *total_chunks);
            }
            EarningsEvent::EmbeddingComplete { chunk_count } => {
                self.on_embedding_complete(*chunk_count);
            }

            EarningsEvent::StoringEmbeddings { chunk_count } => {
                self.on_storing_embeddings(*chunk_count);
            }
            EarningsEvent::EmbeddingsStored { chunk_count } => {
                self.on_embeddings_stored(*chunk_count);
            }

            EarningsEvent::PipelineComplete {
                call_id,
                chunk_count,
                speaker_count,
                duration_secs,
            } => {
                self.on_pipeline_complete(call_id, *chunk_count, *speaker_count, *duration_secs);
            }
            EarningsEvent::PipelineFailed {
                stage,
                error_message,
            } => {
                self.on_pipeline_failed(*stage, error_message);
            }
        }
    }

    fn on_pipeline_started(&self, _file_path: &str, _replace: bool) {}

    fn on_validating_file(&self, _file_path: &str) {}
    fn on_validation_passed(&self, _format_info: &str) {}

    fn on_checking_duplicate(&self, _file_path: &str) {}
    fn on_duplicate_overridden(&self, _existing_call_id: &str) {}
    fn on_duplicate_check_passed(&self) {}

    fn on_transcription_started(&self) {}
    fn on_transcription_progress(&self, _segments_completed: u32) {}
    fn on_diarization_started(&self) {}
    fn on_diarization_progress(&self, _speakers_identified: u32) {}
    fn on_diarization_complete(&self, _speaker_count: u32) {}
    fn on_transcription_complete(&self, _transcript: &Transcript) {}

    fn on_chunk_optimization_started(&self, _raw_chunk_count: u32) {}
    fn on_chunk_optimization_progress(&self, _chunks_processed: u32, _total_chunks: u32) {}
    fn on_chunk_optimization_complete(&self, _final_chunk_count: u32) {}

    fn on_storing_call(&self, _chunk_count: u32) {}
    fn on_call_stored(&self, _call_id: &str, _chunk_count: u32) {}

    fn on_embedding_started(&self, _chunk_count: u32) {}
    fn on_embedding_progress(&self, _chunks_embedded: u32, _total_chunks: u32) {}
    fn on_embedding_complete(&self, _chunk_count: u32) {}

    fn on_storing_embeddings(&self, _chunk_count: u32) {}
    fn on_embeddings_stored(&self, _chunk_count: u32) {}

    fn on_pipeline_complete(
        &self,
        _call_id: &str,
        _chunk_count: u32,
        _speaker_count: u32,
        _duration_secs: f64,
    ) {
    }
    fn on_pipeline_failed(&self, _stage: PipelineStage, _error_message: &str) {}
}

/// A no-op observer useful in tests or headless batch runs.
pub struct NullObserver;
impl EarningsObserver for NullObserver {}
