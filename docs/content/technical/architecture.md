# Architecture

Vetta is designed around two core principles: **decoupled inference** and **strict separation between logic and
presentation**.

Speech-to-text is exposed as a **streaming gRPC service** with a well-defined protobuf interface. The orchestration
pipeline coordinates audio resolution, optional diarization, transcription, and post‑processing, but remains **agnostic
to downstream consumers**.

The core library never produces user-facing output. It emits **structured transcript chunks and events** that
consumers (CLI, APIs, indexers) render or store however they choose.

## System Overview

```mermaid
graph LR
    CLI --> CORE["Vetta Core"]
    CORE --> STT["Speech-to-Text gRPC Service"]
    CORE --> DB["Data Store"]
```

## Speech-to-Text Pipeline

The STT service implements a **dual-path transcription pipeline**:

1. Audio resolution and validation (URL, upload, or inline audio)
2. Audio preprocessing for inference
3. Optional speaker diarization (lazy-loaded, configuration-driven)
4. Whisper transcription (word-level timestamps supported)
5. Execution Routing (Streaming vs. Batching)
6. Speaker label assignment and post-processing (stitching, entity correction, truecasing)
7. Yielding transcript chunks to the caller

To balance latency with accuracy, the service dynamically routes requests based on the required features:

- **Streaming Fast-Path**: If diarization and post-processing are disabled, the service yields `TranscriptChunk`
  messages incrementally as soon as the Whisper model recognizes them. This ensures real-time progress reporting,
  minimal latency, and bounded memory usage.
- **Full-Context Batch Path**: If diarization or advanced post-processing (such as sentence stitching or neural
  punctuation) is requested, the service must evaluate the transcript context as a whole. The pipeline exhausts the
  transcription generator, applies labels/corrections across the full text, and then yields the batched chunks to the
  caller.

## Storage

Two collections with distinct responsibilities:

- **`earnings_calls`** — One document per call. Immutable source of truth.

  - Full transcript (speaker-labeled, post-processed)
  - Speaker registry
  - Ingestion and processing metadata
  - No embeddings

- **`earnings_chunks`** — One document per dialogue turn.

  - Search-optimized text
  - Embeddings
  - Denormalized metadata for filtering (speaker, call ID, timestamps)

This separation allows chunking strategies, embedding models, and reprocessing logic to evolve independently.
Reprocessing chunks never mutates source transcripts.

See [Data Model](/technical/data-model) for schemas, field references, and indexes.

## Search

`earnings_chunks` supports three retrieval modes:

- **Semantic** — Atlas Vector Search over embeddings with metadata pre-filtering
- **Full-text** — Atlas Search using a language analyzer
- **Hybrid** — Candidates from both paths merged and reranked application-side

## Key Decisions

| Decision                   | Rationale                                                                                                  |
|----------------------------|------------------------------------------------------------------------------------------------------------|
| gRPC-based STT service     | Clear contract, streaming support, language-agnostic clients                                               |
| Dual-path Execution        | Provides true low-latency streaming when possible, while supporting full-context operations when requested |
| Optional diarization       | Speaker labeling when available, graceful degradation when not                                             |
| Post-processing pipeline   | Improves readability and normalization without affecting raw timing data                                   |
| Event / chunk-based output | Core emits structured data; presentation is handled by consumers                                           |
| Two-collection model       | Source transcripts and search chunks are separated so embeddings and chunking can evolve independently     |
| Denormalized filters       | Metadata lives on chunks so search can filter without cross-collection joins                               |
| Context windows on chunks  | Each chunk stores neighboring turns, giving rerankers and LLMs surrounding context without extra queries   |
