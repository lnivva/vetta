# Python Service

## Why Python?

`faster-whisper` is a Python library. The CTranslate2 runtime it wraps has Python
bindings as its primary interface. A Rust binding exists but is immature.
Python is the right tool here — the goal is not to avoid Python, it is to
isolate it cleanly so it cannot affect the Rust codebase.

## Why `faster-whisper` over OpenAI Whisper?

| Property         | openai/whisper | faster-whisper   |
|------------------|----------------|------------------|
| Runtime          | PyTorch        | CTranslate2      |
| Speed            | baseline       | ~4x faster       |
| Memory           | baseline       | ~2x lower        |
| Model weights    | identical      | same (converted) |
| Output quality   | identical      | identical        |
| PyTorch required | yes (~2GB)     | no               |

`faster-whisper` converts the model weights to CTranslate2 format at download time.
Inference is faster and cheaper without any quality tradeoff.
The absence of PyTorch also means a significantly smaller Docker image if containerised.

## Hardware detection

At startup, `settings.py` detects the available hardware and selects optimal settings:

```text
startup
  │
  ├─ device=auto?
  │    ├─ CUDA available?  → device=cuda
  │    │    └─ VRAM ≥ 8GB? → float16 : int8_float16
  │    └─ CPU only
  │         ├─ Apple Silicon (arm64) → int8
  │         └─ x86_64               → int8 (AVX2/AVX512 optimized)
  │
  └─ cpu_threads=0? → physical_cores / 2
```

Everything is overridable via environment variables (`WHISPER_MODEL_DEVICE=cuda`)
or directly in `config.toml`.

## Service lifecycle

The service is designed to be a long-lived process, not a per-request subprocess.

```text
startup
  └─ load config
  └─ detect hardware → resolve device + compute_type
  └─ WhisperModel(...) ← expensive, 10–30s, happens once
  └─ bind Unix socket
  └─ chmod 0600 socket
  └─ serve forever (blocking)
```

**The model load is the expensive part.** It should not happen per request.
Run the service as a systemd user unit or a managed background process so it
stays warm between CLI invocations.

## Project structure

```text
services/stt/local/
├── .python-version      # 3.12.x — read by uv automatically
├── pyproject.toml       # dependencies, build metadata
├── uv.lock              # committed lockfile — reproducible everywhere
├── config.toml          # runtime config — not committed in production
├── main.py              # server entrypoint
├── servicer.py          # gRPC handler — pure logic, no config parsing
├── settings.py          # config loading + hardware detection
├── speech_pb2.py        # generated — do not edit
├── speech_pb2_grpc.py   # generated — do not edit
└── tests/
    ├── conftest.py          # shared fixtures (mock model, configs)
    ├── test_settings.py     # unit tests — config loading, hardware detection
    ├── test_servicer.py     # unit tests — gRPC handler logic, mock model
    └── test_integration.py  # integration tests — real gRPC server + client
```

## Testing strategy

Three layers, each with a different purpose:

**Unit (`test_settings.py`)** — config parsing, env overrides, hardware detection logic.
Fast, no model, no network. Runs in milliseconds.

**Servicer (`test_servicer.py`)** — gRPC handler logic with a mocked `WhisperModel`.
Tests that segments are mapped correctly, that prompts are applied in the right priority order,
that empty word lists don't crash. No real inference.

**Integration (`test_integration.py`)** — a real gRPC server starts on a Unix socket,
a real client connects, real proto serialisation happens. The model is still mocked,
but everything else is real — including concurrent request handling.

```bash
# All tests
uv run pytest

# Unit only (fast)
uv run pytest tests/test_settings.py tests/test_servicer.py

# Integration only
uv run pytest tests/test_integration.py
```
