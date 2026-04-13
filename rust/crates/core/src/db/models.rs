use mongodb::bson::DateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::Display;

/// Returned when a document's `_id` is accessed before it has been persisted.
#[derive(Debug, Clone)]
pub struct MissingIdError {
    pub document_type: &'static str,
}

impl Display for MissingIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} document has no _id (not yet persisted?)",
            self.document_type
        )
    }
}

impl Error for MissingIdError {}

/// Shared behavior for any Mongo document that carries an optional `_id`.
pub trait MongoDocument {
    /// The human-readable type name used in error messages.
    const DOC_TYPE: &'static str;

    /// Returns the raw optional `_id`.
    fn id_opt(&self) -> Option<ObjectId>;

    /// Returns the `_id` or an error if the document hasn't been persisted.
    fn id(&self) -> Result<ObjectId, MissingIdError> {
        self.id_opt().ok_or(MissingIdError {
            document_type: Self::DOC_TYPE,
        })
    }

    /// Returns the `_id` as a hex string, or an error.
    fn id_hex(&self) -> Result<String, MissingIdError> {
        self.id().map(|oid| oid.to_hex())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarningsCallDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub ticker: String,
    pub year: u16,
    pub quarter: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company: Option<CompanyInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_date: Option<DateTime>,
    pub source: SourceMetadata,
    pub stats: TranscriptStats,
    #[serde(default)]
    pub speakers: Vec<SpeakerInfo>,
    #[serde(default)]
    pub transcript: TranscriptData,
    pub status: CallStatus,
    pub model_versions: ModelVersions,
    pub updated_at: DateTime,
}

impl MongoDocument for EarningsCallDocument {
    const DOC_TYPE: &'static str = "EarningsCallDocument";

    fn id_opt(&self) -> Option<ObjectId> {
        self.id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarningsChunkDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub call_id: ObjectId,
    pub ticker: String,
    pub year: u16,
    pub quarter: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_date: Option<DateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sector: Option<String>,
    pub chunk_index: u32,
    pub chunk_type: ChunkType,
    pub speaker: ChunkSpeaker,
    pub start_time: f32,
    pub end_time: f32,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    pub word_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    pub created_at: DateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_speaker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_speaker: Option<String>,
}

impl MongoDocument for EarningsChunkDocument {
    const DOC_TYPE: &'static str = "EarningsChunkDocument";

    fn id_opt(&self) -> Option<ObjectId> {
        self.id
    }
}

#[derive(Debug, Clone)]
pub struct OptimizedChunk {
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    Diarized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVersions {
    pub stt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_dimensions: Option<u32>,
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
