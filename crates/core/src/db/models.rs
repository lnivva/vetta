use mongodb::bson::DateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// Top-level earnings call document — source of truth.
/// Collection: `earnings_calls`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarningsCallDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,

    // --- Identity ---
    pub ticker: String,
    pub year: u16,
    pub quarter: String,

    // --- Company ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company: Option<CompanyInfo>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_date: Option<DateTime>,

    // --- Source ---
    pub source: SourceMetadata,

    // --- Aggregates ---
    pub stats: TranscriptStats,

    // --- Speakers ---
    #[serde(default)]
    pub speakers: Vec<SpeakerInfo>,

    // --- Raw transcript ---
    pub transcript: TranscriptData,

    // --- Lifecycle ---
    pub status: CallStatus,
    pub model_versions: ModelVersions,
    pub updated_at: DateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exchange: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMetadata {
    pub file_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    pub duration_seconds: f32,
    pub ingested_at: DateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptStats {
    pub segment_count: u32,
    pub turn_count: u32,
    pub speaker_count: u32,
    pub word_count: u32,
    pub chunk_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerInfo {
    pub speaker_id: String,
    #[serde(default)]
    pub role: SpeakerRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firm: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerRole {
    Operator,
    Executive,
    Analyst,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptData {
    pub segments: Vec<SegmentData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentData {
    pub start_time: f32,
    pub end_time: f32,
    pub text: String,
    pub speaker_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallStatus {
    Ingested,
    Transcribed,
    Chunked,
    Processed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVersions {
    pub stt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_dimensions: Option<u32>,
}

/// Search-optimized chunk document — one per dialogue turn.
/// Collection: `earnings_chunks`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarningsChunkDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,

    // --- Parent reference ---
    pub call_id: ObjectId,

    // --- Denormalized filters ---
    pub ticker: String,
    pub year: u16,
    pub quarter: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_date: Option<DateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sector: Option<String>,

    // --- Chunk identity ---
    pub chunk_index: u32,
    pub chunk_type: ChunkType,

    // --- Speaker ---
    pub speaker: ChunkSpeaker,

    // --- Temporal position ---
    pub start_time: f32,
    pub end_time: f32,

    // --- Content ---
    pub text: String,

    // --- Context window for reranker ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ChunkContext>,

    // --- Embedding ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,

    // --- Search metadata ---
    pub word_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<u32>,

    // --- Lineage ---
    pub model_version: String,
    pub created_at: DateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkType {
    PreparedRemarks,
    QaQuestion,
    QaAnswer,
    Operator,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSpeaker {
    pub speaker_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_speaker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_speaker: Option<String>,
}
