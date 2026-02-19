# STT Strategy Pattern

## The trait

```rust
#[async_trait]
pub trait SpeechToText: Send + Sync {
    async fn transcribe(
        &self,
        audio_path: &str,
        options: TranscribeOptions,
    ) -> Result<TranscriptStream, SttError>;
}

pub type TranscriptStream =
Pin<Box<dyn Stream<Item=Result<TranscriptChunk, SttError>> + Send>>;
```

This is the only interface the CLI depends on. Everything downstream —
which model, which hardware, which provider, which transport — is hidden behind it.

## Why a trait, not an enum?

An enum like `SttProvider::Local | SttProvider::OpenAi` would require touching
a central file every time a new strategy is added. A trait lets each strategy
live in its own module with zero changes to existing code — open/closed principle.

It also enables the CLI to hold `Box<dyn SpeechToText>` and be entirely ignorant
of what is behind it:

```rust
let strategy: Box<dyn SpeechToText> = match config.stt.strategy.as_str() {
"local" => Box::new(LocalSttStrategy::connect(& config.stt.socket_path).await ?),
"openai" => Box::new(CloudSttStrategy::new( & config.stt.api_key)),
s => bail ! ("Unknown STT strategy: {s}"),
};

// The rest of the pipeline never changes regardless of which strategy is chosen
let mut stream = strategy.transcribe( & file, options).await?;
```

## Current strategies

### `LocalSttStrategy`

Connects to the Python service over a Unix domain socket.
The socket path defaults to `/tmp/whisper.sock` and is configurable.

```rust
let stt = LocalSttStrategy::connect("/tmp/whisper.sock").await?;
```

Connection is validated eagerly — if the socket file does not exist,
you get a clear diagnostic immediately rather than a confusing gRPC transport error.

### `CloudSttStrategy` (planned)

Same trait, same `TranscriptStream` return type.
Uses TLS + bearer token instead of a Unix socket.
The Rust client code is structurally identical; only the transport differs.

## Domain types

```rust
pub struct TranscriptChunk {
    pub start_time: f32,
    pub end_time: f32,
    pub text: String,
    pub speaker_id: String,   // empty until diarization stage runs
    pub confidence: f32,
    pub words: Vec<Word>,
}

pub struct Word {
    pub start_time: f32,
    pub end_time: f32,
    pub text: String,
    pub confidence: f32,
}
```

These are Rust-native types — not proto types. The strategy implementations
are responsible for mapping from proto to domain. Nothing outside `stt/` ever
imports from `speech_pb2`.

## Error handling

`SttError` implements `miette::Diagnostic`, so errors render with the same
styled output as the rest of the CLI — help text, source spans, and suggestions:

```
Error: Socket not found: /tmp/whisper.sock

  ✖ Start the whisper service or check the socket path in config.toml
```

The CLI bridges `SttError` into `miette::Result` with `.into_diagnostic()`.
