use super::domain::{DomainEmbedding, DomainEmbeddingResponse, Embedder, InputType};
use super::errors::EmbeddingError;
use crate::common::UdsChannel;
use async_trait::async_trait;
use std::path::Path;

pub mod pb {
    tonic::include_proto!("embeddings");
}

use pb::EmbeddingRequest;
use pb::embedding_service_client::EmbeddingServiceClient;

pub struct LocalEmbeddingsStrategy {
    channel: UdsChannel,
}

impl LocalEmbeddingsStrategy {
    pub async fn connect(socket: impl AsRef<Path>) -> Result<Self, EmbeddingError> {
        let channel = UdsChannel::new(socket)?;
        Ok(Self { channel })
    }

    async fn client(
        &self,
    ) -> Result<EmbeddingServiceClient<tonic::transport::Channel>, EmbeddingError> {
        let ch = self.channel.connect().await?;
        Ok(EmbeddingServiceClient::new(ch))
    }
}

#[async_trait]
impl Embedder for LocalEmbeddingsStrategy {
    async fn embed(
        &self,
        model: &str,
        inputs: Vec<String>,
        input_type: InputType,
        truncate: bool,
    ) -> Result<DomainEmbeddingResponse, EmbeddingError> {
        let mut client = self.client().await?;

        let proto_input_type = match input_type {
            InputType::Document => pb::InputType::Document as i32,
            InputType::Query => pb::InputType::Query as i32,
        };

        let request = tonic::Request::new(EmbeddingRequest {
            model: model.to_string(),
            inputs,
            input_type: proto_input_type,
            truncate,
            output_dimension: None,
            extra_params: None,
        });

        let response = client.create_embeddings(request).await?.into_inner();

        let domain_embeddings = response
            .data
            .into_iter()
            .map(|emb| DomainEmbedding {
                vector: emb.vector,
                index: emb.index as usize,
            })
            .collect();

        let (prompt_tokens, total_tokens) = match response.usage {
            Some(usage) if usage.prompt_tokens >= 0 && usage.total_tokens >= 0 => {
                (usage.prompt_tokens as u32, usage.total_tokens as u32)
            }
            Some(_) => {
                return Err(EmbeddingError::from(tonic::Status::internal(
                    "embedding service returned negative token counts",
                )));
            }
            None => (0, 0),
        };

        Ok(DomainEmbeddingResponse {
            model: response.model,
            embeddings: domain_embeddings,
            prompt_tokens,
            total_tokens,
        })
    }
}
