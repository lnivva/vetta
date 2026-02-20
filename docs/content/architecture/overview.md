# High-level overview

Vetta is structured as a small **app layer** (a CLI today) sitting on top of a reusable **core library**. The core
library defines **stable stage interfaces** (traits) for pipeline steps like speech-to-text, and uses **provider
adapters** to connect those interfaces to concrete implementations (local services or cloud APIs).

This separation keeps user-facing code focused on orchestration and UX, while allowing the underlying providers (
transport, runtime, deployment, vendors) to change without rewriting the pipeline.

## System diagram

```text
┌───────────────────────────────────────────────┐
│  App Layer (CLI today, other apps later)      │
│  - Parses user input                          │
│  - Orchestrates pipeline stages               │
│  - Renders progress + diagnostics             │
└───────────────────────┬───────────────────────┘
                        │ calls stage interfaces
┌───────────────────────▼───────────────────────┐
│  Core Library (crates/core)                   │
│                                               │
│  Stage interfaces (traits)                    │
│    - SpeechToText                             │
│    - (future) Diarization / Embeddings        │
│    - (future) Storage                         │
│                                               │
│  Provider adapters (implementations)          │
│    - Local adapter                            │
│    - Remote/cloud adapter                     │
│                                               │
│  Shared domain types                          │
│    - Quarter, TranscriptChunk, Word, …        │
└───────────────────────┬───────────────────────┘
                        │ streaming RPC / SDK
┌───────────────────────▼───────────────────────┐
│  External Services / Providers                │
│  - Speech-to-text service (local or remote)   │
│  - (future) Diarization service               │
│  - (future) Embedding provider                │
│  - (future) Vector store / database           │
└───────────────────────────────────────────────┘
```

## Layers

### App layer (`apps/cli`)

The user-facing binary (built with `clap`). Responsible for:

- Parsing arguments (ticker, year, quarter, file, output options)
- Driving pipeline stages in sequence
- Rendering terminal progress
- Surfacing errors with `miette` diagnostics

The CLI does not contain provider-specific transcription logic. It depends on the `SpeechToText` interface and calls
`transcribe()`.

### Core library (`crates/core`)

A reusable library crate shared by all entrypoints (CLI today, potentially a server later). Contains:

- **Domain types** — `Quarter`, `TranscriptChunk`, `Word`
- **Stage interfaces** — e.g. `SpeechToText`
- **Provider adapters** — implementations of interfaces (e.g., local vs remote)
- **Pipeline utilities** — validation and stage wiring (future stages may be added here)

### Provider boundary (service/API)

Speech-to-text is provided by an external component (local process or remote service). The core library talks to it via
a streaming interface so transcript segments can be consumed incrementally.

Implementation details (transport, auth, model/runtime) are intentionally encapsulated behind provider adapters and are
not part of the core pipeline contract.

### Proto / API contract (`proto/speech.proto`) *(if using gRPC)*

When gRPC is used, the `.proto` definition is the contract between the core library and the provider implementation.

- Rust generates client stubs at build time (e.g., `tonic-build`)
- Provider implementations generate server stubs as part of their build/dev workflow

## Data flow (single earnings call)

```text
1. User runs:  vetta earnings process --file call.mp3 --ticker AAPL ...
2. CLI validates the input file (exists, size, supported type)
3. CLI selects an STT provider adapter (local or remote) based on config
4. CLI calls SpeechToText::transcribe(file, options)
5. Core/provider sends a request to the STT service (streaming)
6. STT service produces transcript segments incrementally
7. Provider streams TranscriptChunk messages back to the caller
8. CLI consumes chunks:
   - optionally writes to --out
   - optionally prints with --print
9. (future) transcript forwarded to downstream stages (diarization → embeddings → storage)
```
