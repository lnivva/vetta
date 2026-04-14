use async_trait::async_trait;
use std::fmt::{self, Display};

use super::errors::EmbeddingError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    Document,
    Query,
}

impl InputType {
    pub fn as_str(&self) -> &'static str {
        match self {
            InputType::Document => "document",
            InputType::Query => "query",
        }
    }
}

impl Display for InputType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

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
    async fn embed(
        &self,
        model: &str,
        inputs: Vec<String>,
        input_type: InputType,
        truncate: bool,
    ) -> Result<DomainEmbeddingResponse, EmbeddingError>;
}
