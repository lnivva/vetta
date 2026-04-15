mod common;
pub mod db;
mod embeddings;
mod reranker;
mod stt;
mod vector_search;

pub use embeddings::{Embedder, EmbeddingError, InputType, LocalEmbeddingsStrategy};

pub mod earnings {
    //! Public API for the earnings pipeline.

    // Private implementation modules
    mod errors;
    mod events;
    mod processor;
    mod utils;

    // Public surface
    pub use errors::{EarningsError, IngestError};
    pub use events::PipelineStage;
    pub use events::{EarningsEvent, EarningsObserver, NullObserver};
    pub use processor::{EarningsProcessor, ProcessEarningsCallRequest};
}

pub use vector_search::{SearchFilters, VectorSearchResult, build_searcher};

pub use reranker::{LocalRerankerStrategy, Reranker};

pub use stt::{
    LocalSttStrategy, Stt, SttError, TranscribeOptions, TranscriptChunk, TranscriptStream, Word,
    domain::Quarter, domain::Transcript,
};
