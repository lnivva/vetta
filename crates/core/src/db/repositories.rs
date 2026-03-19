use crate::db::models::{
    CallStatus, ChunkContext, ChunkSpeaker, ChunkType, EarningsCallDocument, EarningsChunkDocument,
    ModelVersions, SegmentData, SourceMetadata, SpeakerInfo, TranscriptData, TranscriptStats,
};
use crate::db::{Db, DbError};
use mongodb::bson::{DateTime, doc, oid::ObjectId, serialize_to_bson};
use mongodb::options::IndexOptions;
use mongodb::{Client, Collection, IndexModel};
use serde::Deserialize;

const CALLS_COLLECTION: &str = "earnings_calls";
const CHUNKS_COLLECTION: &str = "earnings_chunks";
const UNKNOWN_SPEAKER: &str = "UNKNOWN";

/// Lightweight projection struct used when we only need document IDs.
#[derive(Debug, Deserialize)]
struct IdOnly {
    #[serde(rename = "_id")]
    id: ObjectId,
}

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
}

pub struct SegmentInput {
    pub start_time: f32,
    pub end_time: f32,
    pub text: String,
    pub speaker_id: String,
}

struct StoreTransactionContext<'a> {
    ticker: &'a str,
    year: u16,
    quarter: &'a str,
    stt_model: &'a str,
    now: DateTime,
}

impl EarningsRepository {
    pub fn new(db: &Db) -> Self {
        Self {
            client: db.client().clone(),
            calls: db.collection::<EarningsCallDocument>(CALLS_COLLECTION),
            chunks: db.collection::<EarningsChunkDocument>(CHUNKS_COLLECTION),
        }
    }

    /// Create required indexes. Idempotent.
    pub async fn ensure_indexes(&self) -> Result<(), DbError> {
        // --- earnings_calls indexes ---
        self.calls
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "ticker": 1, "year": 1, "quarter": 1 })
                    .options(IndexOptions::builder().unique(true).build())
                    .build(),
            )
            .await?;

        self.calls
            .create_index(IndexModel::builder().keys(doc! { "call_date": -1 }).build())
            .await?;

        self.calls
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "company.sector": 1, "call_date": -1 })
                    .build(),
            )
            .await?;

        self.calls
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "status": 1, "updated_at": -1 })
                    .build(),
            )
            .await?;

        // --- earnings_chunks indexes ---
        self.chunks
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "call_id": 1, "chunk_index": 1 })
                    .build(),
            )
            .await?;

        self.chunks
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "ticker": 1, "call_date": -1 })
                    .build(),
            )
            .await?;

        self.chunks
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "model_version": 1 })
                    .build(),
            )
            .await?;

        Ok(())
    }

    /// Store a fully transcribed earnings call and generate its search chunks.
    pub async fn store(&self, req: StoreEarningsRequest) -> Result<ObjectId, DbError> {
        let now = DateTime::now();

        // Build dialogue turns from raw segments
        let turns = build_dialogue_turns(&req.segments);
        let unique_speakers = unique_speaker_ids(&req.segments);
        let word_count: u32 = req
            .segments
            .iter()
            .map(|s| s.text.split_whitespace().count() as u32)
            .sum();

        // Capture fields needed after call_doc consumes parts of `req`.
        let ticker = req.ticker.clone();
        let year = req.year;
        let quarter = req.quarter.clone();
        let stt_model = req.stt_model.clone();
        let segment_count = req.segments.len() as u32;

        // --- Build the call document ---
        let call_doc = EarningsCallDocument {
            id: None,
            ticker: ticker.clone(),
            year,
            quarter: quarter.clone(),
            company: None,
            call_date: None,
            source: SourceMetadata {
                file_name: req.file_name,
                file_hash: req.file_hash,
                format: req.format,
                duration_seconds: req.duration_seconds,
                ingested_at: now,
            },
            stats: TranscriptStats {
                segment_count,
                turn_count: turns.len() as u32,
                speaker_count: unique_speakers.len() as u32,
                word_count,
                chunk_count: turns.len() as u32,
            },
            speakers: unique_speakers
                .iter()
                .map(|sid| SpeakerInfo {
                    speaker_id: sid.clone(),
                    role: Default::default(),
                    name: None,
                    title: None,
                    firm: None,
                })
                .collect(),
            transcript: TranscriptData {
                segments: req
                    .segments
                    .into_iter()
                    .map(|s| SegmentData {
                        start_time: s.start_time,
                        end_time: s.end_time,
                        text: s.text,
                        speaker_id: s.speaker_id,
                    })
                    .collect(),
            },
            status: CallStatus::Transcribed,
            model_versions: ModelVersions {
                stt: stt_model.clone(),
                embedding: None,
                embedding_dimensions: None,
            },
            updated_at: now,
        };

        // --- Start a session and transaction ---
        let mut session = self
            .client
            .start_session()
            .await
            .map_err(|e| DbError::Connection(format!("Failed to start session: {e}")))?;

        session
            .start_transaction()
            .await
            .map_err(|e| DbError::QueryFailure(format!("Failed to start transaction: {e}")))?;

        let ctx = StoreTransactionContext {
            ticker: &ticker,
            year,
            quarter: &quarter,
            stt_model: &stt_model,
            now,
        };

        let result = self
            .store_in_transaction(&mut session, &ctx, call_doc, &turns)
            .await;

        match result {
            Ok(call_id) => {
                session.commit_transaction().await.map_err(|e| {
                    DbError::QueryFailure(format!("Failed to commit transaction: {e}"))
                })?;
                Ok(call_id)
            }
            Err(e) => {
                let _ = session.abort_transaction().await;
                Err(e)
            }
        }
    }

    async fn store_in_transaction(
        &self,
        session: &mut mongodb::ClientSession,
        ctx: &StoreTransactionContext<'_>,
        call_doc: EarningsCallDocument,
        turns: &[DialogueTurn],
    ) -> Result<ObjectId, DbError> {
        // Insert call document
        let call_result = self
            .calls
            .insert_one(call_doc)
            .session(&mut *session)
            .await
            .map_err(|e| {
                if is_duplicate_key(&e) {
                    DbError::Duplicate(format!("{} {} {}", ctx.ticker, ctx.quarter, ctx.year))
                } else {
                    DbError::from(e)
                }
            })?;

        let call_id = call_result
            .inserted_id
            .as_object_id()
            .ok_or_else(|| DbError::Serialization("Expected ObjectId from insert".into()))?;

        // --- Build chunk documents ---
        let chunk_docs: Vec<EarningsChunkDocument> = turns
            .iter()
            .enumerate()
            .map(|(i, turn)| {
                let context = build_context(turns, i);
                let wc = turn.text.split_whitespace().count() as u32;

                EarningsChunkDocument {
                    id: None,
                    call_id,
                    ticker: ctx.ticker.to_string(),
                    year: ctx.year,
                    quarter: ctx.quarter.to_string(),
                    call_date: None,
                    sector: None,
                    chunk_index: i as u32,
                    chunk_type: ChunkType::Unknown,
                    speaker: ChunkSpeaker {
                        speaker_id: turn.speaker_id.clone(),
                        name: None,
                        role: None,
                        title: None,
                    },
                    start_time: turn.start_time,
                    end_time: turn.end_time,
                    text: turn.text.clone(),
                    context: Some(context),
                    embedding: None,
                    word_count: wc,
                    token_count: None,
                    model_version: ctx.stt_model.to_string(),
                    created_at: ctx.now,
                }
            })
            .collect();

        // Bulk insert chunks
        if !chunk_docs.is_empty() {
            let chunk_count = chunk_docs.len();
            let result = self
                .chunks
                .insert_many(chunk_docs)
                .ordered(false)
                .session(&mut *session)
                .await
                .map_err(DbError::from)?;

            if result.inserted_ids.len() != chunk_count {
                return Err(DbError::BulkWrite {
                    success: result.inserted_ids.len() as u64,
                    failure: (chunk_count - result.inserted_ids.len()) as u64,
                });
            }
        }

        // Update status to chunked
        self.calls
            .update_one(
                doc! { "_id": call_id },
                doc! {
                    "$set": {
                        "status": "chunked",
                        "updated_at": DateTime::now(),
                    }
                },
            )
            .session(&mut *session)
            .await?;

        Ok(call_id)
    }

    /// Find a call by its business key.
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

    /// Retrieve all chunks for a call, ordered by position.
    pub async fn get_chunks(
        &self,
        call_id: ObjectId,
    ) -> Result<Vec<EarningsChunkDocument>, DbError> {
        use futures::TryStreamExt;

        let cursor = self
            .chunks
            .find(doc! { "call_id": call_id })
            .sort(doc! { "chunk_index": 1 })
            .await?;

        let chunks: Vec<EarningsChunkDocument> = cursor.try_collect().await?;
        Ok(chunks)
    }

    pub async fn update_embeddings(
        &self,
        updates: Vec<(ObjectId, Vec<f32>)>,
        model_version: &str,
    ) -> Result<u64, DbError> {
        let mut modified = 0u64;

        for batch in updates.chunks(100) {
            for (chunk_id, embedding) in batch {
                let embedding_bson = serialize_to_bson(embedding)
                    .map_err(|e| DbError::Serialization(e.to_string()))?;

                let result = self
                    .chunks
                    .update_one(
                        doc! { "_id": chunk_id },
                        doc! {
                            "$set": {
                                "embedding": embedding_bson,
                                "model_version": model_version,
                            }
                        },
                    )
                    .await?;

                modified += result.modified_count;
            }
        }

        Ok(modified)
    }

    /// Find all chunk IDs that need (re-)embedding for a given model version.
    pub async fn find_chunks_needing_embedding(
        &self,
        current_model: &str,
    ) -> Result<Vec<ObjectId>, DbError> {
        use futures::TryStreamExt;

        let untyped: Collection<IdOnly> = self.chunks.clone_with_type();

        let cursor = untyped
            .find(doc! {
                "$or": [
                    { "embedding": null },
                    { "model_version": { "$ne": current_model } }
                ]
            })
            .projection(doc! { "_id": 1 })
            .await?;

        let docs: Vec<IdOnly> = cursor.try_collect().await?;
        Ok(docs.into_iter().map(|d| d.id).collect())
    }

    /// Delete a call and all its associated chunks.
    pub async fn delete_call(&self, call_id: ObjectId) -> Result<(), DbError> {
        self.chunks.delete_many(doc! { "call_id": call_id }).await?;
        self.calls.delete_one(doc! { "_id": call_id }).await?;
        Ok(())
    }
}

// ─── Internal helpers ────────────────────────────────────────────────────────

struct DialogueTurn {
    speaker_id: String,
    start_time: f32,
    end_time: f32,
    text: String,
}

/// Merge consecutive segments from the same speaker into dialogue turns.
fn build_dialogue_turns(segments: &[SegmentInput]) -> Vec<DialogueTurn> {
    let mut turns: Vec<DialogueTurn> = Vec::new();

    for seg in segments {
        let text = seg.text.trim();
        if text.is_empty() {
            continue;
        }

        let can_merge = match turns.last() {
            Some(last) => !last.speaker_id.is_empty() && last.speaker_id == seg.speaker_id,
            None => false,
        };

        if can_merge {
            let last = turns.last_mut().expect("checked above");
            last.text.push(' ');
            last.text.push_str(text);
            last.end_time = seg.end_time;
        } else {
            turns.push(DialogueTurn {
                speaker_id: seg.speaker_id.clone(),
                start_time: seg.start_time,
                end_time: seg.end_time,
                text: text.to_string(),
            });
        }
    }

    turns
}

/// Build context window (previous/next turn) for a chunk.
fn build_context(turns: &[DialogueTurn], index: usize) -> ChunkContext {
    let prev = if index > 0 {
        let t = &turns[index - 1];
        (Some(truncate(&t.text, 300)), Some(t.speaker_id.clone()))
    } else {
        (None, None)
    };

    let next = if index + 1 < turns.len() {
        let t = &turns[index + 1];
        (Some(truncate(&t.text, 300)), Some(t.speaker_id.clone()))
    } else {
        (None, None)
    };

    ChunkContext {
        previous_text: prev.0,
        previous_speaker: prev.1,
        next_text: next.0,
        next_speaker: next.1,
    }
}

/// Extract sorted, deduplicated speaker IDs.
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

/// Truncate text to a max character length at a word boundary.
fn truncate(text: &str, max_chars: usize) -> String {
    let end = match text.char_indices().nth(max_chars) {
        Some((idx, _)) => idx,
        None => return text.to_string(),
    };

    match text[..end].rfind(' ') {
        Some(pos) => format!("{}…", &text[..pos]),
        None => format!("{}…", &text[..end]),
    }
}

/// Check if a MongoDB error is a duplicate key error (code 11000).
fn is_duplicate_key(err: &mongodb::error::Error) -> bool {
    if let mongodb::error::ErrorKind::Write(mongodb::error::WriteFailure::WriteError(ref we)) =
        *err.kind
    {
        return we.code == 11000;
    }
    false
}
