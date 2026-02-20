# Monorepo Layout

Vetta is a single repository containing Rust, Python, and documentation.
The structure is designed so that each layer can be understood and developed independently.

## Directory tree

```text
vetta/
├── Cargo.toml                  # Rust workspace root
├── Cargo.lock
│
├── proto/
│   └── speech.proto            # Source of truth for Rust ↔ Python contract
│
├── crates/
│   └── core/                   # Shared Rust library
│       ├── Cargo.toml
│       ├── build.rs            # Compiles proto → Rust stubs at build time
│       └── src/
│           ├── lib.rs
│           ├── domain/         # Quarter, Ticker, etc.
│           ├── earnings_processor.rs
│           └── stt/
│               ├── mod.rs      # SpeechToText trait + domain types
│               ├── local.rs    # LocalSttStrategy (gRPC client)
│               └── error.rs    # SttError (miette::Diagnostic)
│
├── apps/
│   └── cli/                    # The vetta CLI binary
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
│
├── services/
│   └── stt/
│       └── local/              # Python faster-whisper gRPC service
│           ├── .python-version # Pins Python 3.12.x for uv
│           ├── pyproject.toml  # Dependencies + build metadata
│           ├── uv.lock         # Committed lockfile
│           ├── config.toml     # Runtime config (device, model, socket, etc.)
│           ├── main.py         # Entrypoint — starts the gRPC server
│           ├── servicer.py     # WhisperServicer — gRPC handler
│           ├── settings.py     # Config loading + hardware detection
│           ├── speech_pb2.py           # Generated — do not edit
│           ├── speech_pb2_grpc.py      # Generated — do not edit
│           ├── Makefile
│           └── tests/
│               ├── conftest.py
│               ├── test_settings.py
│               ├── test_servicer.py
│               └── test_integration.py
│
├── docs/                       # This documentation (RSPress)
│   ├── rspress.config.ts
│   ├── package.json
│   └── docs/
│
├── .github/
│   ├── dependabot.yml
│   └── workflows/
│       └── stt-service.yml
│
└── .gitignore
```

## Key boundaries

**`proto/`** is owned by neither Rust nor Python. Both sides are consumers.
If the proto changes, both sides must be updated before the next release.

**`crates/core`** has no knowledge of the CLI, the Python service, or any specific
strategy implementation beyond what it owns. It exports a trait; the CLI decides
which concrete type to construct.

**`services/stt/local`** is a standalone Python project. It has its own venv,
its own tests, its own config. It could be extracted to its own repo without
touching a single Rust file.

**`apps/cli`** is the assembly layer. It wires config → strategy → pipeline
and handles user-facing concerns: argument parsing, progress rendering, error presentation.
