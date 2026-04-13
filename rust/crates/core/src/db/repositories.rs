use crate::db::models::{
    CallStatus, ChunkSpeaker, ChunkType, EarningsCallDocument, EarningsChunkDocument,
    ModelVersions, MongoDocument, SegmentData, SourceMetadata, SpeakerInfo, TranscriptData,
    TranscriptStats,
};
use crate::db::{Db, DbError};
use serde::Deserialize;
use tracing::{debug, error, info, instrument, warn};

use futures::{StreamExt, TryStreamExt};
use mongodb::bson::{DateTime, doc, oid::ObjectId, serialize_to_bson};
use mongodb::{Client, Collection};

const CALLS_COLLECTION: &str = "earnings_calls";
const CHUNKS_COLLECTION: &str = "earnings_chunks";
const UNKNOWN_SPEAKER: &str = "UNKNOWN";

/// Lightweight projection struct used when we only need document IDs.
#[derive(Debug, Deserialize)]
struct IdOnly {
    #[serde(rename = "_id")]
    id: ObjectId,
}

/// Repository for storing, retrieving, and maintaining earnings call documents
/// and their derived dialogue chunks.
#[derive(Clone)]
pub struct EarningsRepository {
    client: Client,
    calls: Collection<EarningsCallDocument>,
    chunks: Collection<EarningsChunkDocument>,
}

/// Request to store a new earnings call and its derived chunks.
pub struct StoreEarningsRequest {
    pub ticker: String,
    pub year: u16,
    pub quarter: String,
    pub file_name: String,
    pub file_hash: Option<String>,
    pub format: Option<String>,
    pub duration_seconds: f32,
    pub stt_model: String,
    pub segments: Vec<SegmentInput>,
    pub chunks: Vec<ChunkInput>,
}

pub struct ChunkInput {
    pub chunk_index: u32,
    pub speaker_id: String,
    pub start_time: f32,
    pub end_time: f32,
    pub text: String,
    pub word_count: u32,
    pub previous_text: Option<String>,
    pub previous_speaker: Option<String>,
    pub next_text: Option<String>,
    pub next_speaker: Option<String>,
}

pub struct SegmentInput {
    pub start_time: f32,
    pub end_time: f32,
    pub text: String,
    pub speaker_id: String,
}

impl EarningsRepository {
    pub fn new(db: &Db) -> Self {
        Self {
            client: db.client().clone(),
            calls: db.collection(CALLS_COLLECTION),
            chunks: db.collection(CHUNKS_COLLECTION),
        }
    }

    #[instrument(skip(self, req), fields(ticker = %req.ticker, quarter = %req.quarter, year = %req.year
    ))]
    pub async fn store(&self, req: StoreEarningsRequest) -> Result<ObjectId, DbError> {
        let now = DateTime::now();
        let call_doc = build_call_document(&req, now);

        let mut session = self.client.start_session().await?;
        session.start_transaction().await?;

        let ctx = StoreTransactionContext::from_doc(&call_doc, now);

        match self
            .store_in_transaction(&mut session, &ctx, call_doc, &req.chunks)
            .await
        {
            Ok(call_id) => {
                session.commit_transaction().await?;
                info!(call_id = %call_id, "Successfully stored new earnings call");
                Ok(call_id)
            }
            Err(e) => {
                error!(error = %e, "Failed to store earnings call, aborting transaction");
                let _ = session.abort_transaction().await;
                Err(e)
            }
        }
    }

    #[instrument(skip(self, req), fields(ticker = %req.ticker, quarter = %req.quarter, year = %req.year
    ))]
    pub async fn replace(&self, req: StoreEarningsRequest) -> Result<ObjectId, DbError> {
        let now = DateTime::now();
        let call_doc = build_call_document(&req, now);

        let mut session = self.client.start_session().await?;
        session.start_transaction().await?;

        match self
            .replace_in_transaction(&mut session, call_doc, &req.chunks, now)
            .await
        {
            Ok(call_id) => {
                session.commit_transaction().await?;
                debug!(call_id = %call_id, "Successfully replaced earnings call");
                Ok(call_id)
            }
            Err(e) => {
                error!(error = %e, "Failed to replace earnings call, aborting transaction");
                let _ = session.abort_transaction().await;
                Err(e)
            }
        }
    }

    async fn replace_in_transaction(
        &self,
        session: &mut mongodb::ClientSession,
        call_doc: EarningsCallDocument,
        chunks: &[ChunkInput],
        now: DateTime,
    ) -> Result<ObjectId, DbError> {
        if let Some(existing) = self
            .calls
            .find_one(doc! {
                "ticker": &call_doc.ticker,
                "year": call_doc.year as i32,
                "quarter": &call_doc.quarter,
            })
            .session(&mut *session)
            .await?
        {
            let call_id = existing
                .id()
                .map_err(|e| DbError::Serialization(e.to_string()))?;
            debug!(call_id = %call_id, "Found existing call for business key, replacing...");

            self.chunks
                .delete_many(doc! { "call_id": call_id })
                .session(&mut *session)
                .await?;

            self.calls
                .delete_one(doc! { "_id": call_id })
                .session(&mut *session)
                .await?;
        }

        let ctx = StoreTransactionContext::from_doc(&call_doc, now);

        self.store_in_transaction(session, &ctx, call_doc, chunks)
            .await
    }

    async fn store_in_transaction(
        &self,
        session: &mut mongodb::ClientSession,
        ctx: &StoreTransactionContext,
        call_doc: EarningsCallDocument,
        chunks: &[ChunkInput],
    ) -> Result<ObjectId, DbError> {
        debug_assert!(
            matches!(call_doc.status, CallStatus::Chunked),
            "store() must persist fully chunked calls"
        );

        let call_result = self
            .calls
            .insert_one(call_doc)
            .session(&mut *session)
            .await?;

        let call_id = call_result
            .inserted_id
            .as_object_id()
            .ok_or_else(|| DbError::Serialization("Expected ObjectId".into()))?;

        let chunk_docs: Vec<EarningsChunkDocument> = chunks
            .iter()
            .map(|c| EarningsChunkDocument {
                id: None,
                call_id,
                ticker: ctx.ticker.clone(),
                year: ctx.year,
                quarter: ctx.quarter.clone(),
                call_date: None,
                sector: None,
                chunk_index: c.chunk_index,
                chunk_type: ChunkType::Unknown,
                speaker: ChunkSpeaker {
                    speaker_id: c.speaker_id.clone(),
                    name: None,
                    role: None,
                    title: None,
                },
                start_time: c.start_time,
                end_time: c.end_time,
                text: c.text.clone(),
                embedding: None,
                previous_text: c.previous_text.clone(),
                previous_speaker: c.previous_speaker.clone(),
                next_text: c.next_text.clone(),
                next_speaker: c.next_speaker.clone(),
                word_count: c.word_count,
                token_count: None,
                embedding_model: None,
                created_at: ctx.now,
            })
            .collect();

        if !chunk_docs.is_empty() {
            self.chunks
                .insert_many(chunk_docs)
                .session(&mut *session)
                .await?;
        }

        Ok(call_id)
    }

    pub async fn find_call(
        &self,
        ticker: &str,
        year: u16,
        quarter: &str,
    ) -> Result<Option<EarningsCallDocument>, DbError> {
        let doc = self
            .calls
            .find_one(doc! {
                "ticker": ticker,
                "year": year as i32,
                "quarter": quarter,
            })
            .await?;

        Ok(doc)
    }

    #[instrument(skip(self))]
    pub async fn get_chunks(
        &self,
        call_id: ObjectId,
    ) -> Result<Vec<EarningsChunkDocument>, DbError> {
        let cursor = self
            .chunks
            .find(doc! { "call_id": call_id })
            .sort(doc! { "chunk_index": 1 })
            .await?;

        let chunks: Vec<EarningsChunkDocument> = cursor.try_collect().await?;
        Ok(chunks)
    }

    #[instrument(skip(self, updates))]
    pub async fn update_embeddings(
        &self,
        updates: Vec<(ObjectId, Vec<f32>)>,
        model_version: &str,
    ) -> Result<u64, DbError> {
        if updates.is_empty() {
            return Ok(0);
        }

        let mut modified = 0u64;
        let concurrency_limit = 50;
        let chunks_collection = self.chunks.clone();

        let mut stream = futures::stream::iter(updates.into_iter())
            .map(|(chunk_id, embedding)| {
                let chunks = chunks_collection.clone();
                let model_ver = model_version.to_string();

                async move {
                    let embedding_bson = serialize_to_bson(&embedding)
                        .map_err(|e| DbError::Serialization(e.to_string()))?;

                    let result = chunks
                        .update_one(
                            doc! { "_id": chunk_id },
                            doc! {
                                "$set": {
                                    "embedding": embedding_bson,
                                    "embedding_model": model_ver,
                                }
                            },
                        )
                        .await?;

                    Ok::<u64, DbError>(result.modified_count)
                }
            })
            .buffer_unordered(concurrency_limit);

        while let Some(result) = stream.next().await {
            modified += result?;
        }

        info!(
            modified_chunks = modified,
            model_version, "Successfully applied vector embeddings via concurrent updates"
        );
        Ok(modified)
    }

    #[instrument(skip(self))]
    pub async fn find_chunks_needing_embedding(
        &self,
        current_model: &str,
    ) -> Result<Vec<ObjectId>, DbError> {
        let untyped: Collection<IdOnly> = self.chunks.clone_with_type();

        let cursor = untyped
            .find(doc! {
                "$or": [
                    { "embedding": null },
                    { "embedding_model": { "$ne": current_model } }
                ]
            })
            .projection(doc! { "_id": 1 })
            .await?;

        let docs: Vec<IdOnly> = cursor.try_collect().await?;
        Ok(docs.into_iter().map(|d| d.id).collect())
    }

    #[instrument(skip(self))]
    pub async fn delete_call(&self, call_id: ObjectId) -> Result<(), DbError> {
        info!(call_id = %call_id, "Deleting call and associated chunks");

        let mut session = self.client.start_session().await?;
        session.start_transaction().await?;

        match self.delete_in_transaction(&mut session, call_id).await {
            Ok(()) => {
                session.commit_transaction().await?;
                info!(call_id = %call_id, "Successfully deleted call and all chunks");
                Ok(())
            }
            Err(e) => {
                error!(
                    call_id = %call_id,
                    error = %e,
                    "Failed to delete call, aborting transaction"
                );
                let _ = session.abort_transaction().await;
                Err(e)
            }
        }
    }

    async fn delete_in_transaction(
        &self,
        session: &mut mongodb::ClientSession,
        call_id: ObjectId,
    ) -> Result<(), DbError> {
        let chunk_result = self
            .chunks
            .delete_many(doc! { "call_id": call_id })
            .session(&mut *session)
            .await?;

        info!(
            call_id = %call_id,
            deleted_chunks = chunk_result.deleted_count,
            "Removed associated chunks"
        );

        let call_result = self
            .calls
            .delete_one(doc! { "_id": call_id })
            .session(&mut *session)
            .await?;

        if call_result.deleted_count == 0 {
            warn!(
                call_id = %call_id,
                "No call document found for the given ID — chunks (if any) were still removed"
            );
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn mark_call_processed(
        &self,
        call_id: ObjectId,
        embedding_model: &str,
        embedding_dimensions: u32,
    ) -> Result<(), DbError> {
        let now = DateTime::now();

        let result = self
            .calls
            .update_one(
                doc! { "_id": call_id },
                doc! {
                    "$set": {
                        "status": "processed",
                        "model_versions.embedding": embedding_model,
                        "model_versions.embedding_dimensions": embedding_dimensions,
                        "updated_at": now,
                    }
                },
            )
            .await?;

        if result.matched_count == 0 {
            return Err(DbError::NotFound(format!(
                "Call {} not found when marking as processed",
                call_id
            )));
        }

        Ok(())
    }
}

struct StoreTransactionContext {
    ticker: String,
    year: u16,
    quarter: String,
    now: DateTime,
}

impl StoreTransactionContext {
    fn from_doc(doc: &EarningsCallDocument, now: DateTime) -> Self {
        Self {
            ticker: doc.ticker.clone(),
            year: doc.year,
            quarter: doc.quarter.clone(),
            now,
        }
    }
}

fn build_call_document(req: &StoreEarningsRequest, now: DateTime) -> EarningsCallDocument {
    let speakers = unique_speaker_ids(&req.segments);

    EarningsCallDocument {
        id: None,
        ticker: req.ticker.clone(),
        year: req.year,
        quarter: req.quarter.clone(),
        company: None,
        call_date: None,
        source: SourceMetadata {
            file_name: req.file_name.clone(),
            file_hash: req.file_hash.clone(),
            format: req.format.clone(),
            duration_seconds: req.duration_seconds,
            ingested_at: now,
        },
        stats: TranscriptStats {
            segment_count: req.segments.len() as u32,
            speaker_count: speakers.len() as u32,
            word_count: req.chunks.iter().map(|c| c.word_count).sum(),
            chunk_count: req.chunks.len() as u32,
        },
        speakers: speakers
            .into_iter()
            .map(|s| SpeakerInfo {
                speaker_id: s,
                role: Default::default(),
                name: None,
                title: None,
                firm: None,
            })
            .collect(),

        transcript: TranscriptData {
            segments: req
                .segments
                .iter()
                .map(|s| SegmentData {
                    start_time: s.start_time,
                    end_time: s.end_time,
                    text: s.text.clone(),
                    speaker_id: s.speaker_id.clone(),
                })
                .collect(),
        },

        status: CallStatus::Chunked,
        model_versions: ModelVersions {
            stt: req.stt_model.clone(),
            embedding: None,
            embedding_dimensions: None,
        },
        updated_at: now,
    }
}

fn unique_speaker_ids(segments: &[SegmentInput]) -> Vec<String> {
    let mut speakers: Vec<String> = segments
        .iter()
        .map(|s| {
            if s.speaker_id.is_empty() {
                UNKNOWN_SPEAKER.to_string()
            } else {
                s.speaker_id.clone()
            }
        })
        .collect();
    speakers.sort();
    speakers.dedup();
    speakers
}
