use super::domain::{DomainEmbedding, DomainEmbeddingResponse, Embedder};
use super::errors::EmbeddingError;
use async_trait::async_trait;
use hyper_util::rt::TokioIo;
use std::path::{Path, PathBuf};
use tokio::net::UnixStream;
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;

pub mod pb {
    tonic::include_proto!("embeddings");
}

use pb::embedding_service_client::EmbeddingServiceClient;
use pb::EmbeddingRequest;

pub struct LocalEmbeddingsStrategy {
    socket_path: PathBuf,
}

impl LocalEmbeddingsStrategy {
    pub async fn connect(socket: impl AsRef<Path>) -> Result<Self, EmbeddingError> {
        let path = socket.as_ref();

        if !path.exists() {
            return Err(EmbeddingError::SocketNotFound(path.to_string_lossy().to_string()));
        }

        Ok(Self {
            socket_path: path.to_path_buf(),
        })
    }

    async fn client(&self) -> Result<EmbeddingServiceClient<tonic::transport::Channel>, EmbeddingError> {
        let path = self.socket_path.clone();

        let channel = Endpoint::try_from("http://localhost")?
            .connect_with_connector(service_fn(move |_: Uri| {
                let path = path.clone();
                async move { UnixStream::connect(&path).await.map(TokioIo::new) }
            }))
            .await?;

        Ok(EmbeddingServiceClient::new(channel))
    }
}

#[async_trait]
impl Embedder for LocalEmbeddingsStrategy {
    async fn embed(
        &self,
        model: &str,
        inputs: Vec<String>,
        input_type: Option<&str>,
        truncate: bool,
    ) -> Result<DomainEmbeddingResponse, EmbeddingError> {
        let mut client = self.client().await?;

        let request = tonic::Request::new(EmbeddingRequest {
            model: model.to_string(),
            inputs,
            input_type: input_type.map(|s| s.to_string()),
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
            Some(usage) => (usage.prompt_tokens as u32, usage.total_tokens as u32),
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