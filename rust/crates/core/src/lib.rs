mod common;
pub mod db;
mod embeddings;
pub mod stt;

pub use embeddings::{Embedder, EmbeddingError, LocalEmbeddingsStrategy};

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
