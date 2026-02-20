# Architecture Overview

## System diagram

```
┌─────────────────────────────────────────────────┐
│  CLI Crate  (apps/cli)                          │
│    └─ clap commands → pipeline stages           │
└────────────────┬────────────────────────────────┘
                 │  calls trait methods
┌────────────────▼────────────────────────────────┐
│  Core Crate  (crates/core)                      │
│                                                 │
│  trait SpeechToText {                           │
│    async fn transcribe(audio, options)          │
│      -> TranscriptStream                        │
│  }                                              │
│                                                 │
│  ┌──────────────────┐  ┌──────────────────────┐ │
│  │ LocalSttStrategy │  │ CloudSttStrategy     │ │
│  │ (gRPC client,    │  │ (gRPC client,        │ │
│  │  Unix socket)    │  │  TLS + auth header)  │ │
│  └────────┬─────────┘  └──────────────────────┘ │
└───────────┼─────────────────────────────────────┘
            │  gRPC server-side streaming
            │  unix:///tmp/whisper.sock
            │
┌───────────▼─────────────────────────────────────┐
│  Python gRPC Service  (services/stt/local)      │
│  - faster-whisper (large-v3 via CTranslate2)    │
│  - model loaded once at startup                 │
│  - VAD filter strips silence                    │
│  - streams TranscriptChunk messages back        │
└─────────────────────────────────────────────────┘
                 │  (future stages)
┌────────────────▼────────────────────────────────┐
│  Diarization  →  Embedding  →  MongoDB Atlas    │
│  pyannote         Voyage AI     Vector Search   │
└─────────────────────────────────────────────────┘
```

## Layers

### CLI (`apps/cli`)

The user-facing binary. Built with `clap`. Responsible for:

- Parsing arguments (ticker, year, quarter, file)
- Driving the pipeline stages in sequence
- Rendering live progress to the terminal
- Propagating errors with `miette` diagnostics

The CLI knows nothing about how transcription works internally.
It holds a `Box<dyn SpeechToText>` and calls `.transcribe()`.

### Core (`crates/core`)

The library crate shared by all apps (CLI today, potentially a server or WASM target later).
Contains:

- **Domain types** — `Quarter`, `TranscriptChunk`, `Word`
- **The `SpeechToText` trait** — the strategy interface
- **Strategy implementations** — `LocalSttStrategy` (ships now), `CloudSttStrategy` (future)
- **Pipeline orchestration** — validation, diarization, embedding (future stages)

### Python STT Service (`services/stt/local`)

A gRPC service wrapping `faster-whisper`. Runs as a long-lived process. The model is loaded once at startup — the
expensive part. Each `Transcribe` RPC call streams segments back as they are produced by the model's internal chunker.

### Proto contract (`proto/speech.proto`)

The single source of truth for the Rust ↔ Python interface.
Rust generates client stubs via `tonic-build` at compile time.
Python generates stubs via `grpc_tools.protoc` at dev setup time (and in CI).

## Data flow for a single earnings call

```
1. User runs:  vetta earnings process --file call.mp3 --ticker AAPL ...
2. CLI validates the file (mime type, extension)
3. CLI calls LocalSttStrategy::connect("/tmp/whisper.sock")
4. CLI calls .transcribe("call.mp3", options)
5. Rust sends TranscribeRequest over the Unix socket
6. Python service receives request, calls model.transcribe() → lazy generator
7. Each segment yields a TranscriptChunk back over the gRPC stream
8. Rust receives chunks one by one, prints live progress
9. (future) chunks forwarded to diarization stage
10. (future) diarized chunks embedded and written to MongoDB Atlas
```
