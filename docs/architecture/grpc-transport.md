# gRPC Transport

## Unix Domain Socket

The local STT service communicates over a Unix domain socket (UDS) rather than a TCP port.

```
unix:///tmp/whisper.sock
```

### Why UDS over localhost TCP?

| Property         | TCP (localhost)                          | Unix Domain Socket                |
|------------------|------------------------------------------|-----------------------------------|
| Network exposure | Binds a port, visible to other processes | No port, filesystem only          |
| Access control   | Firewall rules or port binding           | File permissions (`chmod 0600`)   |
| Performance      | Loopback overhead                        | Kernel-mediated, no network stack |
| Discovery        | Port conflicts possible                  | Path is explicit                  |

Security is the primary reason. The socket is `chmod 0600` — only the owning user
can connect. No firewall rule, no port scanner exposure, no accidental remote access.

### The 104-character limit

Unix domain socket paths are limited to 104 characters on macOS (108 on Linux) —
this is a kernel-level restriction, not configurable. The socket path must be kept short.
`/tmp/whisper.sock` is 17 characters. This limit is why tests use
`/tmp/whisper_test_{pid}.sock` rather than pytest's default long temp paths.

## Proto contract

```protobuf
syntax = "proto3";
package speech;

service SpeechToText {
  rpc Transcribe(TranscribeRequest) returns (stream TranscriptChunk);
}

message TranscribeRequest {
  string            audio_path = 1;
  string            language = 2;
  TranscribeOptions options = 3;
}

message TranscribeOptions {
  bool   diarization = 1;
  int32  num_speakers = 2;
  string initial_prompt = 3;
}

message TranscriptChunk {
  float          start_time = 1;
  float          end_time = 2;
  string         text = 3;
  string         speaker_id = 4;
  float          confidence = 5;
  repeated Word  words = 6;
}

message Word {
  float  start_time = 1;
  float  end_time = 2;
  string text = 3;
  float  confidence = 4;
}
```

`proto/speech.proto` is the single source of truth. Both stubs are generated from it:

```bash
# Rust — happens automatically at cargo build via build.rs
cargo build

# Python — run once at setup, then on proto changes
cd services/stt/local && make proto
```

## Rust client: `TokioIo` wrapper

tonic 0.12+ uses hyper 1.x, which defines its own `Read`/`Write` traits
that do not match tokio's `AsyncRead`/`AsyncWrite`. The bridge is `TokioIo`:

```rust
use hyper_util::rt::TokioIo;

Endpoint::try_from("http://localhost") ?
.connect_with_connector(service_fn( move | _: Uri| {
let path = path.clone();
async move {
UnixStream::connect( & path).await.map(TokioIo::new)
}
}))
.await?
```

`TokioIo::new` wraps the `UnixStream` and implements hyper's traits by delegating
to tokio's. Without this wrapper, the compiler rejects the connection with trait
bound errors on `hyper::rt::io::Read` and `hyper::rt::io::Write`.

## Future: Cloud transport

When a `CloudSttStrategy` is added, the only change is the connector:

```rust
// Local: Unix socket
.connect_with_connector(service_fn( | _ | UnixStream::connect( & path).await.map(TokioIo::new)))

// Cloud: TLS
Endpoint::from_static("https://stt.example.com")
.tls_config(tls) ?
.connect()
.await?
```

The generated client code (`SpeechToTextClient`) and all downstream Rust code
are unchanged. The proto contract is identical.
