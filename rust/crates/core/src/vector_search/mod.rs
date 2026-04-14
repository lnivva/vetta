use async_trait::async_trait;
use futures::stream::TryStreamExt;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};

use crate::db::{Db, DbError};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VectorSearchResult {
    #[serde(rename = "_id")]
    pub id: mongodb::bson::oid::ObjectId,
    pub ticker: String,
    pub year: u16,
    pub quarter: String,
    pub text: String,
    pub score: f64,
}

/// A struct to hold optional filters for the vector search.
#[derive(Debug, Default, Clone)]
pub struct SearchFilters {
    pub ticker: Option<String>,
    pub year: Option<u16>,
    pub quarter: Option<String>,
}

#[async_trait]
pub trait VectorSearcher: Send + Sync {
    async fn search_earnings(
        &self,
        query_vector: &[f32],
        limit: usize,
        filters: SearchFilters,
    ) -> Result<Vec<VectorSearchResult>, DbError>;
}

pub fn build_searcher(db: &Db) -> Box<dyn VectorSearcher> {
    Box::new(MongoVectorSearcher {
        chunks: db.collection("earnings_chunks"),
    })
}

struct MongoVectorSearcher {
    chunks: mongodb::Collection<mongodb::bson::Document>,
}

#[async_trait]
impl VectorSearcher for MongoVectorSearcher {
    async fn search_earnings(
        &self,
        query_vector: &[f32],
        limit: usize,
        filters: SearchFilters,
    ) -> Result<Vec<VectorSearchResult>, DbError> {
        let num_candidates = (limit * 10) as i32;

        // 1. Dynamically build the filter document
        let mut filter_doc = mongodb::bson::Document::new();

        if let Some(ticker) = filters.ticker {
            filter_doc.insert("ticker", ticker);
        }
        if let Some(year) = filters.year {
            filter_doc.insert("year", year as i32);
        }
        if let Some(quarter) = filters.quarter {
            filter_doc.insert("quarter", quarter);
        }

        // 2. Build the base vector search stage
        let mut vector_search_stage = doc! {
            "index": "chunk_vector_index",
            "path": "embedding",
            "queryVector": query_vector,
            "numCandidates": num_candidates,
            "limit": limit as i32,
        };

        // 3. Inject the filter into the stage ONLY if we have filters
        if !filter_doc.is_empty() {
            vector_search_stage.insert("filter", filter_doc);
        }

        let pipeline = vec![
            doc! {
                "$vectorSearch": vector_search_stage
            },
            doc! {
                "$project": {
                    "_id": 1,
                    "ticker": 1,
                    "year": 1,
                    "quarter": 1,
                    "text": 1,
                    "score": { "$meta": "vectorSearchScore" }
                }
            },
        ];

        let mut cursor = self
            .chunks
            .aggregate(pipeline)
            .with_type::<VectorSearchResult>()
            .await?;

        let mut results = Vec::with_capacity(limit);

        while let Some(result) = cursor.try_next().await? {
            results.push(result);
        }

        Ok(results)
    }
}
