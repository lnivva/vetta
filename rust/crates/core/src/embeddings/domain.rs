use async_trait::async_trait;

use super::errors::EmbeddingError;

#[derive(Debug, Clone)]
pub struct DomainEmbedding {
    pub vector: Vec<f32>,
    pub index: usize,
}

#[derive(Debug, Clone)]
pub struct DomainEmbeddingResponse {
    pub model: String,
    pub embeddings: Vec<DomainEmbedding>,
    pub prompt_tokens: u32,
    pub total_tokens: u32,
}

#[async_trait]
pub trait Embedder: Send + Sync {
    /// Takes raw text inputs and returns domain embedding objects.
    async fn embed(
        &self,
        model: &str,
        inputs: Vec<String>,
        input_type: Option<&str>,
        truncate: bool,
    ) -> Result<DomainEmbeddingResponse, EmbeddingError>;
}
