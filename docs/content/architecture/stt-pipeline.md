# Speech-to-Text Pipeline

## Why a separate process?

The whisper-large-v3 model is ~3GB and takes 10–30 seconds to load into memory.
Running it in the same process as the CLI would mean:

- Reloading the model on every invocation
- Tying the Rust process to Python's runtime and GIL
- Making the model non-reusable across multiple CLI calls

Running it as a separate long-lived process means the model is loaded once and
stays warm. Subsequent calls connect to the already-running service in milliseconds.

## Why gRPC?

The alternatives considered:

| Approach                  | Why rejected                                                  |
|---------------------------|---------------------------------------------------------------|
| `stdin/stdout` subprocess | Brittle for binary data, no streaming, hard to version        |
| REST/FastAPI              | HTTP overhead, no native streaming, not designed for binary   |
| Shared memory             | Complex, unsafe, hard to version across Rust/Python           |
| **gRPC (chosen)**         | Typed contract, streaming, same interface for local and cloud |

The decisive factor: the **strategy pattern maps directly to the proto contract**.
The `.proto` file *is* the `SpeechToText` interface. Local uses a Unix socket;
cloud uses TLS. The Rust client code is identical in both cases.

## The streaming model

`faster-whisper`'s `model.transcribe()` returns a **lazy generator**, not a completed result.
Segments are yielded as the model processes each internal audio chunk (~30 seconds each).

This maps perfectly to gRPC server-side streaming:

```text
Rust client                    Python service
    │                               │
    │── TranscribeRequest ─────────►│
    │                               │ model.transcribe() → generator
    │◄── TranscriptChunk (0–30s) ───│ (first ~30s processed)
    │◄── TranscriptChunk (30–60s) ──│
    │◄── TranscriptChunk (60–90s) ──│
    │         ...                   │ (2 hours later)
    │◄── stream closed ─────────────│
```

A 2-hour earnings call starts returning results after ~30 seconds, not after 2 hours.

## VAD (Voice Activity Detection)

`faster-whisper` includes a Silero VAD filter that strips silence before inference.
This matters significantly for earnings calls which contain:

- Hold music before the call starts
- Operator introductions and dead air
- Pauses between analyst questions and management responses

Without VAD, Whisper hallucinates on silent segments — producing repeated phrases
or completely fabricated text. With VAD enabled, silence is removed before
the audio reaches the model.

## File path, not bytes

The `TranscribeRequest` carries a `path` string in the `audio_source` oneof, not the raw audio bytes.

```protobuf
message TranscribeRequest {
  oneof audio_source {
    string path = 1;  // absolute path on shared filesystem
  }
    ...
}
```

For a 2-hour MP3 (~100MB), serializing the bytes over the socket on every call
would be wasteful and slow. Since both the Rust process and the Python service
run on the same machine, they share the filesystem. The path is all that is needed.

This does mean the service validates the path exists before attempting transcription,
and the Rust client validates it too before sending the request — fail fast,
with a clear error message.
