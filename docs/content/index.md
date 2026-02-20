---
pageType: home

hero:
  name: Vetta
  text: Financial Analysis Engine
  tagline: Institutional-grade earnings call processing — ASR, diarization, vector search
  actions:
    - theme: brand
      text: Quick Start
      link: /guide/quick-start
    - theme: alt
      text: Architecture
      link: /architecture/overview

features:
  - title: Local-first ASR
    details: faster-whisper (large-v3) running locally via CTranslate2. No data leaves your machine. GPU or CPU, auto-detected at startup.
  - title: Strategy Pattern
    details: The SpeechToText trait in the Rust core crate decouples the interface from the implementation. Switch from local to cloud without changing a line of downstream code.
  - title: Streaming gRPC
    details: Server-side streaming over a Unix domain socket. Transcript chunks flow back to Rust as they are produced — no waiting for a 2-hour file to finish.
  - title: MongoDB Atlas
    details: Voyage AI embeddings stored in Atlas Vector Search. Semantic search across earnings calls, quarters, speakers, and topics.
---
