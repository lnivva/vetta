mod error;
pub mod local;

pub use error::SttError;
pub use local::LocalSttStrategy;

use async_trait::async_trait;
use std::pin::Pin;
use tokio_stream::Stream;

#[derive(Debug, Clone)]
pub struct TranscriptChunk {
    pub start_time: f32,
    pub end_time: f32,
    pub text: String,
    pub speaker_id: String,
    pub confidence: f32,
    pub words: Vec<Word>,
}

#[derive(Debug, Clone)]
pub struct Word {
    pub start_time: f32,
    pub end_time: f32,
    pub text: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Default)]
pub struct TranscribeOptions {
    pub language: Option<String>,
    pub initial_prompt: Option<String>,
    pub diarization: bool,
    pub num_speakers: u32,
}

pub type TranscriptStream = Pin<Box<dyn Stream<Item = Result<TranscriptChunk, SttError>> + Send>>;

#[async_trait]
pub trait SpeechToText: Send + Sync {
    async fn transcribe(
        &self,
        audio_path: &str,
        options: TranscribeOptions,
    ) -> Result<TranscriptStream, SttError>;
}
