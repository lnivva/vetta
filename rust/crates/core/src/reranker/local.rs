use crate::common::UdsChannel;
use async_trait::async_trait;
use std::path::Path;
use std::time::Duration;

pub mod pb {
    tonic::include_proto!("reranker");
}

use crate::reranker::domain::{DomainRerankResponse, DomainRerankResult, Reranker};
use crate::reranker::errors::RerankerError;
use pb::RerankRequest;
use pb::reranker_service_client::RerankerServiceClient;

pub struct LocalRerankerStrategy {
    channel: UdsChannel,
}

impl LocalRerankerStrategy {
    pub async fn connect(socket: impl AsRef<Path>) -> Result<Self, RerankerError> {
        let channel =
            UdsChannel::new(socket).map_err(|e| RerankerError::Connection(e.to_string()))?;
        Ok(Self { channel })
    }

    async fn client(
        &self,
    ) -> Result<RerankerServiceClient<tonic::transport::Channel>, RerankerError> {
        let ch = self
            .channel
            .connect()
            .await
            .map_err(|e| RerankerError::Connection(e.to_string()))?;
        Ok(RerankerServiceClient::new(ch))
    }
}

#[async_trait]
impl Reranker for LocalRerankerStrategy {
    async fn rerank(
        &self,
        model: &str,
        query: &str,
        documents: Vec<String>,
        top_k: Option<i32>,
    ) -> Result<DomainRerankResponse, RerankerError> {
        let mut client = self.client().await?;

        let doc_count = documents.len();

        let mut request = tonic::Request::new(RerankRequest {
            model: model.to_string(),
            query: query.to_string(),
            documents,
            top_k,
            truncate: Some(true),
            extra_params: None,
        });
        request.set_timeout(Duration::from_secs(10));

        let response = client.rerank(request).await?.into_inner();

        let results: Vec<DomainRerankResult> = response
            .results
            .into_iter()
            .map(|res| {
                let index = res.index as usize;
                if index >= doc_count {
                    return Err(RerankerError::Connection(format!(
                        "reranker returned out-of-bounds index {index} for {doc_count} document(s)"
                    )));
                }
                Ok(DomainRerankResult {
                    relevance_score: res.relevance_score,
                    index,
                    document: res.document,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let total_tokens = response.usage.map(|u| u.total_tokens).unwrap_or(0);

        Ok(DomainRerankResponse {
            model: response.model,
            results,
            total_tokens,
        })
    }
}
