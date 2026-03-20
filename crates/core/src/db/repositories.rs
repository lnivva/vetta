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

/// Repository for storing, retrieving, and maintaining earnings call documents
/// and their derived dialogue chunks.
pub struct EarningsRepository {
    client: Client,
    calls: Collection<EarningsCallDocument>,
    chunks: Collection<EarningsChunkDocument>,
}

/// Request to store a new earnings call and its derived chunks.
pub struct StoreEarningsRequest {
    /// Stock ticker symbol.
    pub ticker: String,
    /// Fiscal year of the call.
    pub year: u16,
    /// Fiscal quarter, such as `Q1` or `Q4`.
    pub quarter: String,
    /// Original uploaded file name.
    pub file_name: String,
    /// Optional content hash for deduplication.
    pub file_hash: Option<String>,
    /// Optional media format or container type.
    pub format: Option<String>,
    /// Audio duration in seconds.
    pub duration_seconds: f32,
    /// Speech-to-text model identifier used for transcription.
    pub stt_model: String,
    /// Raw transcript segments to persist and chunk.
    pub segments: Vec<SegmentInput>,
}

/// Input segment used to build transcript turns and chunk documents.
pub struct SegmentInput {
    /// Segment start time in seconds.
    pub start_time: f32,
    /// Segment end time in seconds.
    pub end_time: f32,
    /// Recognized transcript text.
    pub text: String,
    /// ASR-assigned speaker identifier.
    pub speaker_id: String,
}

impl EarningsRepository {
    /// Create a new repository instance backed by the given database handle.
    pub fn new(db: &Db) -> Self {
        Self {
            client: db.client().clone(),
            calls: db.collection(CALLS_COLLECTION),
            chunks: db.collection(CHUNKS_COLLECTION),
        }
    }

    /// Ensure all collection indexes required by the repository are present.
    pub async fn ensure_indexes(&self) -> Result<(), DbError> {
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

    /// Store a new call and all derived dialogue chunks in a single transaction.
    pub async fn store(&self, req: StoreEarningsRequest) -> Result<ObjectId, DbError> {
        let now = DateTime::now();
        let (call_doc, turns) = build_call_and_turns(req, now);

        let mut session = self.client.start_session().await?;
        session.start_transaction().await?;

        let ctx = StoreTransactionContext::from_doc(&call_doc, now);

        let result = self
            .store_in_transaction(&mut session, &ctx, call_doc, &turns)
            .await;

        match result {
            Ok(call_id) => {
                session.commit_transaction().await?;
                Ok(call_id)
            }
            Err(e) => {
                let _ = session.abort_transaction().await;
                Err(e)
            }
        }
    }

    /// Replace an existing call identified by its business key, deleting any
    /// old chunks before inserting the new transcript and chunk set.
    pub async fn replace(&self, req: StoreEarningsRequest) -> Result<ObjectId, DbError> {
        let now = DateTime::now();
        let (call_doc, turns) = build_call_and_turns(req, now);

        let mut session = self.client.start_session().await?;
        session.start_transaction().await?;

        if let Some(existing) = self
            .calls
            .find_one(doc! {
                "ticker": &call_doc.ticker,
                "year": call_doc.year as i32,
                "quarter": &call_doc.quarter,
            })
            .session(&mut session)
            .await?
        {
            let call_id = existing.id.expect("existing must have id");

            self.chunks
                .delete_many(doc! { "call_id": call_id })
                .session(&mut session)
                .await?;

            self.calls
                .delete_one(doc! { "_id": call_id })
                .session(&mut session)
                .await?;
        }

        let ctx = StoreTransactionContext::from_doc(&call_doc, now);

        let call_id = self
            .store_in_transaction(&mut session, &ctx, call_doc, &turns)
            .await?;

        session.commit_transaction().await?;
        Ok(call_id)
    }

    async fn store_in_transaction(
        &self,
        session: &mut mongodb::ClientSession,
        ctx: &StoreTransactionContext,
        call_doc: EarningsCallDocument,
        turns: &[DialogueTurn],
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

        let chunk_docs: Vec<EarningsChunkDocument> = turns
            .iter()
            .enumerate()
            .map(|(i, t)| EarningsChunkDocument {
                id: None,
                call_id,
                ticker: ctx.ticker.clone(),
                year: ctx.year,
                quarter: ctx.quarter.clone(),
                call_date: None,
                sector: None,
                chunk_index: i as u32,
                chunk_type: ChunkType::Unknown,
                speaker: ChunkSpeaker {
                    speaker_id: t.speaker_id.clone(),
                    name: None,
                    role: None,
                    title: None,
                },
                start_time: t.start_time,
                end_time: t.end_time,
                text: t.text.clone(),
                context: Some(build_context(turns, i)),
                embedding: None,
                word_count: t.text.split_whitespace().count() as u32,
                token_count: None,
                model_version: ctx.stt_model.clone(),
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

    /// Retrieve all chunks for a call, ordered by chunk position.
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

    /// Update chunk embeddings and record the embedding model version used.
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

    /// Find all chunk IDs that need to be embedded or re-embedded for the
    /// given model version.
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

    /// Delete a call and all of its associated chunks.
    pub async fn delete_call(&self, call_id: ObjectId) -> Result<(), DbError> {
        self.chunks.delete_many(doc! { "call_id": call_id }).await?;
        self.calls.delete_one(doc! { "_id": call_id }).await?;
        Ok(())
    }
}

struct StoreTransactionContext {
    ticker: String,
    year: u16,
    quarter: String,
    stt_model: String,
    now: DateTime,
}

impl StoreTransactionContext {
    fn from_doc(doc: &EarningsCallDocument, now: DateTime) -> Self {
        Self {
            ticker: doc.ticker.clone(),
            year: doc.year,
            quarter: doc.quarter.clone(),
            stt_model: doc.model_versions.stt.clone(),
            now,
        }
    }
}

struct DialogueTurn {
    speaker_id: String,
    start_time: f32,
    end_time: f32,
    text: String,
}

fn build_call_and_turns(
    req: StoreEarningsRequest,
    now: DateTime,
) -> (EarningsCallDocument, Vec<DialogueTurn>) {
    let turns = build_dialogue_turns(&req.segments);
    let speakers = unique_speaker_ids(&req.segments);

    let call_doc = EarningsCallDocument {
        id: None,
        ticker: req.ticker,
        year: req.year,
        quarter: req.quarter,
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
            segment_count: req.segments.len() as u32,
            turn_count: turns.len() as u32,
            speaker_count: speakers.len() as u32,
            word_count: req
                .segments
                .iter()
                .map(|s| s.text.split_whitespace().count() as u32)
                .sum(),
            chunk_count: turns.len() as u32,
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
                .into_iter()
                .map(|s| SegmentData {
                    start_time: s.start_time,
                    end_time: s.end_time,
                    text: s.text,
                    speaker_id: s.speaker_id,
                })
                .collect(),
        },
        status: CallStatus::Chunked,
        model_versions: ModelVersions {
            stt: req.stt_model,
            embedding: None,
            embedding_dimensions: None,
        },
        updated_at: now,
    };

    (call_doc, turns)
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
