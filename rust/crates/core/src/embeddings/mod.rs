pub mod domain;
pub(crate) mod errors;
mod local;

pub use domain::Embedder;
pub use errors::EmbeddingError;
pub use local::LocalEmbeddingsStrategy;
