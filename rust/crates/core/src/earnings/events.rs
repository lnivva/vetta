use crate::domain::Transcript;

/// Every discrete progress signal the earnings pipeline can emit.
///
/// `Clone + Send` so it can cross thread / channel boundaries when used with `tokio::sync::mpsc` or similar.
#[derive(Debug, Clone)]
pub enum EarningsEvent {
    /// File passed all validation checks.
    ValidationPassed { format_info: String },

    /// A new batch of segments has been decoded.
    TranscriptionProgress { segments: u32 },

    /// Full transcript is available.
    TranscriptionComplete { transcript: Transcript },

    /// About to write chunks to MongoDB.
    StoringChunks { chunk_count: u32 },

    /// Persisted successfully.
    Stored { call_id: String, chunk_count: u32 },
}

/// Trait that any consumer (CLI, desktop, server) implements to react to earnings pipeline progress.
///
/// All methods have default no-op implementations so consumers only override the events they care about.
///
/// # Example (CLI)
///
/// ```rust,ignore
/// struct CliObserver;
///
/// impl EarningsObserver for CliObserver {
///     fn on_event(&self, event: &EarningsEvent) {
///         match event {
///             EarningsEvent::TranscriptionProgress { segments } => {
///                 eprintln!("  …{segments} segments so far");
///             }
///             EarningsEvent::Stored { call_id, .. } => {
///                 eprintln!("  ✔ stored as {call_id}");
///             }
///             _ => {}
///         }
///     }
/// }
/// ```
pub trait EarningsObserver: Send + Sync {
    /// Called for every pipeline event. Override this single method for a catch-all handler, or leave default to dispatch to fine-grained hooks.
    fn on_event(&self, event: &EarningsEvent) {
        match event {
            EarningsEvent::ValidationPassed { format_info } => {
                self.on_validation_passed(format_info);
            }
            EarningsEvent::TranscriptionProgress { segments } => {
                self.on_transcription_progress(*segments);
            }
            EarningsEvent::TranscriptionComplete { transcript } => {
                self.on_transcription_complete(transcript);
            }
            EarningsEvent::StoringChunks { chunk_count } => {
                self.on_storing_chunks(*chunk_count);
            }
            EarningsEvent::Stored {
                call_id,
                chunk_count,
            } => {
                self.on_stored(call_id, *chunk_count);
            }
        }
    }

    fn on_validation_passed(&self, _format_info: &str) {}
    fn on_transcription_progress(&self, _segments: u32) {}
    fn on_transcription_complete(&self, _transcript: &Transcript) {}
    fn on_storing_chunks(&self, _chunk_count: u32) {}
    fn on_stored(&self, _call_id: &str, _chunk_count: u32) {}
}

/// A no-op observer useful in tests or headless batch runs.
pub struct NullObserver;
impl EarningsObserver for NullObserver {}
