# Earnings Call Analytics — Data Model

## Overview

This document describes the MongoDB data model for an earnings call analytics platform. The system ingests audio
recordings of public company earnings calls, transcribes them, optionally identifies speakers through diarization,
generates vector embeddings, and exposes the content through text search, semantic search, and reranked hybrid
retrieval.

The model uses **two collections**:

* **`earnings_calls`** — one document per call; the immutable source of truth.
* **`earnings_chunks`** — one document per dialogue turn; the search-optimized unit.

This separation allows chunking strategies and embedding models to evolve independently of the source transcript.

## Architecture

```text
Audio File
  │
  ▼
Resolve + Preprocess
  │
  ├───────────────┐
  ▼               ▼
Transcription     Diarization (optional, parallel)
(Whisper)         (Speaker Attribution)
  │               │
  └───────┬───────┘
          ▼
Post-processing (speaker assignment, stitching, normalization)
          │
          ▼
┌──────────────────────┐
│   earnings_calls     │  ← Source of truth. No embeddings.
│   (one doc per call) │
└──────────┬───────────┘
           │  chunking + embedding
           ▼
┌──────────────────────┐
│   earnings_chunks    │  ← Search collection. One doc per dialogue turn.
│   (one doc per turn) │     Contains text, embedding, and denormalized metadata.
└──────────────────────┘
           │
           ▼
  Atlas Vector Search / Atlas Search / Reranker
```

## Design Principles

| Principle                                 | Rationale                                                                |
|:------------------------------------------|:-------------------------------------------------------------------------|
| **Separate source from derived chunks**   | Re-chunk and re-embed without touching the original transcript.          |
| **Chunk at the dialogue turn level**      | A speaker's contiguous utterance is the natural semantic boundary.       |
| **Collocate embeddings with text**        | Eliminates cross-collection joins during vector search.                  |
| **Denormalize filter fields onto chunks** | Vector and text search stages can filter without a costly `$lookup`.     |
| **Track model lineage**                   | Enables incremental re-embedding when models are upgraded.               |
| **Store speaker attribution in source**   | Speaker labels are part of the canonical transcript, not a derived view. |

## Collection: `earnings_calls`

### Purpose

Stores one document per earnings call. Acts as the authoritative record of the transcript and speaker attribution.
Contains **no embeddings**.

### Schema

```json
{
  "_id": "ObjectId('...')",
  "ticker": "MDB",
  "year": 2024,
  "quarter": "Q3",
  "company": {
    "name": "MongoDB, Inc.",
    "sector": "Technology",
    "industry": "Application Software",
    "exchange": "NASDAQ"
  },
  "call_date": "ISODate('2024-12-09T21:00:00Z')",
  "source": {
    "file_name": "mdb_q3_2024_earnings.mp4",
    "file_hash": "sha256:ab12cd...",
    "format": "mp4/aac",
    "duration_seconds": 2852.4,
    "ingested_at": "ISODate('2024-12-10T03:22:10Z')"
  },
  "stats": {
    "segment_count": 142,
    "turn_count": 47,
    "speaker_count": 6,
    "word_count": 12840,
    "chunk_count": 47
  },
  "speakers": [
    {
      "speaker_id": "Speaker 0",
      "role": "operator",
      "name": "Conference Operator",
      "title": null,
      "firm": null
    }
  ],
  "transcript": {
    "segments": [
      {
        "start_time": 0.0,
        "end_time": 4.2,
        "text": "Good morning and welcome...",
        "speaker_id": "Speaker 0"
      }
    ]
  },
  "status": "processed",
  "model_versions": {
    "stt": "whisper-large-v3",
    "diarization": "pyannote/speaker-diarization-3.1",
    "embedding": "voyage-finance-2",
    "embedding_dimensions": 1024
  },
  "updated_at": "ISODate('2024-12-10T03:25:44Z')"
}
```

### Field Reference

| Field                                 | Type           | Description                                                                                   |
|:--------------------------------------|:---------------|:----------------------------------------------------------------------------------------------|
| `ticker`                              | String         | Stock ticker symbol                                                                           |
| `year`                                | Number         | Fiscal year                                                                                   |
| `quarter`                             | String         | Fiscal quarter (`Q1`–`Q4`)                                                                    |
| `company.name`                        | String         | Legal entity name                                                                             |
| `company.sector`                      | String         | GICS sector classification                                                                    |
| `company.industry`                    | String         | GICS industry classification                                                                  |
| `company.exchange`                    | String         | Stock exchange                                                                                |
| `call_date`                           | Date           | Scheduled call start time (UTC)                                                               |
| `source.file_name`                    | String         | Original uploaded file name                                                                   |
| `source.file_hash`                    | String         | SHA-256 content hash for deduplication                                                        |
| `source.format`                       | String         | Media container/codec format                                                                  |
| `source.duration_seconds`             | Number         | Audio duration in seconds                                                                     |
| `source.ingested_at`                  | Date           | Timestamp of file ingestion                                                                   |
| `stats.segment_count`                 | Number         | Raw ASR segment count                                                                         |
| `stats.turn_count`                    | Number         | Merged dialogue turn count                                                                    |
| `stats.speaker_count`                 | Number         | Distinct speakers detected                                                                    |
| `stats.word_count`                    | Number         | Total transcript word count                                                                   |
| `stats.chunk_count`                   | Number         | Corresponding document count in `earnings_chunks`                                             |
| `speakers[].speaker_id`               | String         | ASR-assigned speaker identifier                                                               |
| `speakers[].role`                     | String         | `operator` \| `executive` \| `analyst` \| `unknown`                                           |
| `speakers[].name`                     | String \| null | Resolved speaker name                                                                         |
| `speakers[].title`                    | String \| null | Job title                                                                                     |
| `speakers[].firm`                     | String \| null | Company or research firm                                                                      |
| `transcript.segments[]`               | Array          | Raw ASR output segments                                                                       |
| `status`                              | String         | Pipeline state: `ingested` → `transcribed` → `diarized` → `chunked` → `processed` \| `failed` |
| `model_versions.stt`                  | String         | Speech-to-text model identifier                                                               |
| `model_versions.diarization`          | String         | Diarization model identifier                                                                  |
| `model_versions.embedding`            | String         | Embedding model identifier                                                                    |
| `model_versions.embedding_dimensions` | Number         | Vector dimensionality                                                                         |
| `updated_at`                          | Date           | Last modification timestamp                                                                   |

### Indexes

```javascript
// Unique business key
db.earnings_calls.createIndex(
    {ticker: 1, year: 1, quarter: 1},
    {unique: true}
)

// Temporal queries — most recent calls first
db.earnings_calls.createIndex({call_date: -1})

// Sector-scoped temporal queries
db.earnings_calls.createIndex({"company.sector": 1, call_date: -1})

// Pipeline status monitoring
db.earnings_calls.createIndex({status: 1, updated_at: -1})
```

## Diarization

Speaker diarization assigns a speaker label to each time span of the audio. The diarization model produces unlabeled
clusters (`Speaker 0`, `Speaker 1`, …). It knows *when* speakers change but not *who* they are.

* **How It Fits:** Diarization is an **optional, independent inference pass** that runs in parallel with transcription
  when enabled.
* **Materialization:** Its output is not stored as a standalone structure. Instead, it is folded directly into
  `transcript.segments[].speaker_id` and the `speakers` registry via temporal overlap alignment.
* **Resolution:** Speakers begin as `Speaker 0` (with `role: "unknown"`). They can be resolved manually, via heuristics,
  or by secondary AI models later. The pipeline does not block on this resolution.

## Collection: `earnings_chunks`

### Purpose

Stores one document per dialogue turn. This is the primary collection for Atlas Vector Search, Atlas Search, and hybrid
retrieval. Chunks are derived from post-processed transcript turns, with speaker attribution when available.

### Schema

```json
{
  "_id": "ObjectId('...')",
  "call_id": "ObjectId('...')",
  "ticker": "MDB",
  "year": 2024,
  "quarter": "Q3",
  "call_date": "ISODate('2024-12-09T21:00:00Z')",
  "sector": "Technology",
  "chunk_index": 12,
  "chunk_type": "qa_answer",
  "speaker": {
    "speaker_id": "Speaker 1",
    "name": "Dev Ittycheria",
    "role": "executive",
    "title": "CEO"
  },
  "start_time": 1023.4,
  "end_time": 1089.7,
  "text": "We're seeing very strong adoption of Atlas, particularly among enterprise customers migrating from legacy relational databases. Our consumption-based revenue grew 38% year over year, and we're now seeing seven-figure Atlas deployments become routine rather than exceptional.",
  "context": {
    "previous_text": "Can you talk about the Atlas adoption trends you're seeing in enterprise?",
    "previous_speaker": "Raimo Lenschow",
    "next_text": "And just to add some color on the financial side...",
    "next_speaker": "Michael Gordon"
  },
  "embedding": [
    0.0123,
    -0.0456,
    "..."
  ],
  "word_count": 44,
  "token_count": 58,
  "model_version": "voyage-finance-2",
  "created_at": "ISODate('2024-12-10T03:25:44Z')"
}
```

### Field Reference

| Field                      | Type            | Description                                                      |
|:---------------------------|:----------------|:-----------------------------------------------------------------|
| `call_id`                  | ObjectId        | Foreign key to `earnings_calls._id`                              |
| `ticker`                   | String          | Denormalized. Stock ticker symbol.                               |
| `year`                     | Number          | Denormalized. Fiscal year.                                       |
| `quarter`                  | String          | Denormalized. Fiscal quarter.                                    |
| `call_date`                | Date            | Denormalized. Call date.                                         |
| `sector`                   | String          | Denormalized. GICS sector.                                       |
| `chunk_index`              | Number          | Zero-based ordinal position within the call                      |
| `chunk_type`               | String          | `prepared_remarks` \| `qa_question` \| `qa_answer` \| `operator` |
| `speaker.speaker_id`       | String          | ASR-assigned speaker identifier                                  |
| `speaker.name`             | String          | Resolved speaker name                                            |
| `speaker.role`             | String          | `operator` \| `executive` \| `analyst` \| `unknown`              |
| `speaker.title`            | String \| null  | Job title                                                        |
| `start_time`               | Number          | Chunk start time in seconds                                      |
| `end_time`                 | Number          | Chunk end time in seconds                                        |
| `text`                     | String          | Dialogue turn content                                            |
| `context.previous_text`    | String \| null  | Preceding turn's text                                            |
| `context.previous_speaker` | String \| null  | Preceding turn's speaker name                                    |
| `context.next_text`        | String \| null  | Following turn's text                                            |
| `context.next_speaker`     | String \| null  | Following turn's speaker name                                    |
| `embedding`                | Array\<Number\> | Vector embedding                                                 |
| `word_count`               | Number          | Word count of `text`                                             |
| `token_count`              | Number          | Token count of `text` (model-specific)                           |
| `model_version`            | String          | Embedding model that produced `embedding`                        |
| `created_at`               | Date            | Chunk creation timestamp                                         |

## Search & Vector Indexes

### 1. Atlas Vector Search Index

Filter fields are explicitly declared in the vector index definition so that pre-filtered approximate nearest neighbor (
ANN) search can accurately narrow candidates **before** distance computation.

```json
{
  "name": "chunk_vector_index",
  "type": "vectorSearch",
  "definition": {
    "fields": [
      {
        "path": "embedding",
        "type": "vector",
        "numDimensions": 1024,
        "similarity": "cosine"
      },
      {
        "path": "ticker",
        "type": "filter"
      },
      {
        "path": "year",
        "type": "filter"
      },
      {
        "path": "quarter",
        "type": "filter"
      },
      {
        "path": "sector",
        "type": "filter"
      },
      {
        "path": "chunk_type",
        "type": "filter"
      },
      {
        "path": "speaker.role",
        "type": "filter"
      },
      {
        "path": "call_date",
        "type": "filter"
      }
    ]
  }
}
```

### 2. Atlas Search Index (Full-Text)

This index handles full-text keyword matching, fuzzy matching, and exact phrase matching.

```json
{
  "name": "chunk_text_index",
  "analyzer": "lucene.english",
  "mappings": {
    "dynamic": false,
    "fields": {
      "text": {
        "type": "string",
        "analyzer": "lucene.english",
        "multi": {
          "keyword": {
            "type": "string",
            "analyzer": "lucene.keyword"
          }
        }
      },
      "speaker.name": {
        "type": "string",
        "analyzer": "lucene.standard"
      },
      "ticker": {
        "type": "token"
      },
      "year": {
        "type": "number"
      },
      "quarter": {
        "type": "token"
      },
      "sector": {
        "type": "token"
      },
      "chunk_type": {
        "type": "token"
      },
      "speaker.role": {
        "type": "token"
      },
      "call_date": {
        "type": "date"
      }
    }
  }
}
```

**Index Design Notes:**

* **Analyzers:** `lucene.english` is used on `text` to enable word stemming (e.g., "running" matches "run").
  `lucene.standard` is used on `speaker.name` to avoid altering human names.
* **Fuzzy Matching:** Inherently supported at query time by the text index. No special configuration is required here.
* **Keyword Multi-field:** The `text.keyword` sub-field bypasses the English stemmer, indexing exact phrasing. This is
  critical when searching for non-standard financial acronyms or exact quotes.
* **Token Types:** Enumerations and tags (`ticker`, `chunk_type`, `quarter`) use the `token` type for highly efficient
  exact-match filtering.

### 3. Standard Indexes

```javascript
// Reconstruct a full call in order
db.earnings_chunks.createIndex({call_id: 1, chunk_index: 1})

// Ticker-scoped temporal queries
db.earnings_chunks.createIndex({ticker: 1, call_date: -1})

// Re-embedding pipeline: find chunks needing model upgrade
db.earnings_chunks.createIndex({model_version: 1})
```

## Query Patterns

### Semantic Search with Pre-Filtering

Retrieve the top 50 executive statements for a given ticker using vector similarity, filtering candidates prior to
calculating cosine distance.

```javascript
db.earnings_chunks.aggregate([
    {
        $vectorSearch: {
            index: "chunk_vector_index",
            path: "embedding",
            queryVector: queryEmbedding,
            numCandidates: 200,
            limit: 50,
            filter: {
                $and: [
                    {ticker: "MDB"},
                    {"speaker.role": "executive"}
                ]
            }
        }
    },
    {
        $project: {
            _id: 1, text: 1, context: 1, speaker: 1, ticker: 1, chunk_type: 1,
            vs_score: {$meta: "vectorSearchScore"}
        }
    }
])
```

### Full-Text Search with Fuzzy Matching

Find Q3 executive statements mentioning terms similar to "acquisition" or "margins". Highly effective for ASR
transcripts where homophones or minor typos exist.

```javascript
db.earnings_chunks.aggregate([
    {
        $search: {
            index: "chunk_text_index",
            compound: {
                must: [
                    {
                        text: {
                            query: "aquisition margin", // Intentional typo 
                            path: "text",
                            fuzzy: {
                                maxEdits: 2,
                                prefixLength: 3
                            }
                        }
                    }
                ],
                filter: [
                    {equals: {path: "quarter", value: "Q3"}},
                    {equals: {path: "speaker.role", value: "executive"}}
                ]
            }
        }
    },
    {$limit: 25},
    {
        $project: {
            ticker: 1, year: 1, text: 1, speaker: 1,
            score: {$meta: "searchScore"}
        }
    }
])
```

### Exact Phrase Match (Keyword)

Find exact, literal mentions of a phrase, bypassing the English stemmer using the multi-field setup.

```javascript
db.earnings_chunks.aggregate([
    {
        $search: {
            index: "chunk_text_index",
            phrase: {
                query: "consumption-based revenue",
                path: "text.keyword"
            }
        }
    }
])
```

### Hybrid Search (Vector + Keyword) via App-Side RRF

For optimal retrieval, query both the vector index and the text index, then fuse the results.

1. **Parallel Execution:** Run `$vectorSearch` and `$search` concurrently from your application tier.
2. **Reciprocal Rank Fusion (RRF):** Combine the lists by ranking unique chunks:
   `1 / (k + Rank_vector) + 1 / (k + Rank_text)` *(k is usually 60)*.
3. **Cross-Encoder Reranking:** Send the top *N* fused results, along with their `context`, to an LLM/Reranker (e.g.,
   Cohere) for the final sort.

### Reconstruct a Full Call

Retrieve all chunks for a call in chronological order.

```javascript
db.earnings_chunks.find({call_id: ObjectId("...")}).sort({chunk_index: 1})
```

### Find Chunks Needing Re-Embedding

Identify chunks still on an older embedding model.

```javascript
db.earnings_chunks.find({model_version: "voyage-finance-1"})
```

## Denormalization Strategy

The following fields are copied from `earnings_calls` onto each `earnings_chunks` document:

| Field       | Source                          |
|:------------|:--------------------------------|
| `ticker`    | `earnings_calls.ticker`         |
| `year`      | `earnings_calls.year`           |
| `quarter`   | `earnings_calls.quarter`        |
| `call_date` | `earnings_calls.call_date`      |
| `sector`    | `earnings_calls.company.sector` |

**Rationale:** `$vectorSearch` and `$search` operate on a single collection. Denormalization eliminates the need for an
expensive `$lookup` pipeline stage to filter by call-level metadata.

## Context Window Design

Each chunk stores its neighboring turns in the `context` subdocument:

```json
{
  "context": {
    "previous_text": "...",
    "previous_speaker": "...",
    "next_text": "...",
    "next_speaker": "..."
  }
}
```

**Purposes:**

1. **Reranking:** The reranker receives the surrounding dialogue, improving relevance scoring for chunks that lack
   explicit subject matter but act as answers to preceding questions.
2. **LLM Grounding:** Provides conversational continuity when injecting context into prompts, without needing extra
   database round-trips.

## Lifecycle States

The `earnings_calls.status` field tracks high-level pipeline progress:

```text
ingested → transcribed → processed
                      ↘ failed
```

| Status        | Meaning                                                |
|:--------------|:-------------------------------------------------------|
| `ingested`    | Audio file received and stored                         |
| `transcribed` | Transcription complete; raw segments available         |
| `processed`   | Speaker attribution, chunking, and embeddings complete |
| `failed`      | An error occurred; check application logs              |

## Extensibility

| Extension                       | Approach                                                                                                                                                                |
|:--------------------------------|:------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Multi-tenancy**               | Add a `tenant_id` field to both collections and include it as a `filter` mapping in both search indexes.                                                                |
| **New embedding model**         | Write new `earnings_chunks` documents with the updated `model_version`; query by `model_version` to track migration progress.                                           |
| **Different chunking strategy** | Drop and recreate `earnings_chunks`; `earnings_calls` remains unchanged.                                                                                                |
| **New diarization model**       | Rerun diarization, update `transcript.segments[].speaker_id`, and re-resolve the `speakers` registry.                                                                   |
| **Custom Financial Synonyms**   | Create a synonym mapping collection in Atlas Search (e.g., "CAPEX" to "capital expenditures") and attach it to the `lucene.english` analyzer in the `chunk_text_index`. |
| **Additional metadata**         | Add fields to `earnings_chunks` and register them as `token` or `filter` mappings in the relevant index definitions.                                                    |