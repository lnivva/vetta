# Introduction

Vetta is a Rust-native financial analysis engine that processes earnings call audio end-to-end:
from raw MP3/MP4 files through speech-to-text, speaker diarization, and vector embedding,
to semantic search over a MongoDB Atlas index.

## What it does

Given an earnings call recording, Vetta produces:

- A **full transcript** with word-level timestamps
- **Speaker diarization** — who said what and when
- **Vector embeddings** stored in MongoDB Atlas for semantic search
- A queryable index for cross-call analysis (topic, speaker, quarter, ticker)

## What it is not

Vetta is not a SaaS product, a cloud service, or a general-purpose transcription tool.
It is an opinionated, local-first pipeline designed for institutional financial research
where data confidentiality, reproducibility, and cost control matter.

## Core principles

**Local-first.** The default STT strategy runs whisper-large-v3 on your machine.
No audio data is sent to third-party APIs unless you explicitly configure a cloud strategy.

**Streaming.** A 2-hour earnings call does not need to finish transcribing before you
can start reading the transcript. Results stream back chunk by chunk.

**Extensible.** The `SpeechToText` trait in the Rust core crate is the only interface
downstream code depends on. Adding a cloud provider or a new model is a new file,
not a refactor.

**Typed end-to-end.** The proto contract is the source of truth between Rust and Python.
Both sides are generated from the same `.proto` file. If the contract changes, both sides
fail to compile/import until they are updated.
