# Configuration

## Two config files, two purposes

| File             | Read by                   | Purpose                                               |
|------------------|---------------------------|-------------------------------------------------------|
| `pyproject.toml` | uv / pip                  | Python package metadata, dependencies, Python version |
| `config.toml`    | Python service at runtime | Model, device, socket, inference tuning               |

These do not overlap. `pyproject.toml` is about the package.
`config.toml` is about how the running service behaves on a specific machine.

## `config.toml` reference

```toml
[service]
socket_path = "/tmp/whisper.sock"
log_level = "info"             # debug | info | warn | error

[model]
size = "large-v3"        # tiny | base | small | medium | large-v3
download_dir = "~/.cache/whisper_models"
device = "auto"            # auto | cuda | cpu
compute_type = "auto"            # auto | float16 | int8_float16 | int8

[inference]
beam_size = 5
vad_filter = true
vad_min_silence_ms = 500
no_speech_threshold = 0.6
log_prob_threshold = -1.0
compression_ratio_threshold = 2.4
word_timestamps = true
initial_prompt = ""   # overridden per-request if provided

[concurrency]
max_workers = 1   # 1 for GPU (serialised), 2 for CPU
cpu_threads = 0   # 0 = auto (half of physical cores)
num_workers = 1
```

## Environment variable overrides

Any value can be overridden at runtime with an environment variable.
The pattern is `WHISPER_<SECTION>_<KEY>`:

```bash
WHISPER_MODEL_SIZE=medium          # override model size
WHISPER_MODEL_DEVICE=cuda          # force CUDA
WHISPER_SERVICE_SOCKET_PATH=/run/whisper.sock
WHISPER_INFERENCE_VAD_FILTER=false
WHISPER_CONCURRENCY_CPU_THREADS=8
```

Environment variables take precedence over `config.toml`.
`config.toml` values take precedence over defaults.

## Hardware matrix

| Hardware               | `device` | `compute_type` | Notes                                           |
|------------------------|----------|----------------|-------------------------------------------------|
| NVIDIA GPU (≥8GB VRAM) | `cuda`   | `float16`      | Best performance                                |
| NVIDIA GPU (<8GB VRAM) | `cuda`   | `int8_float16` | Saves VRAM, minimal quality loss                |
| CPU (x86_64)           | `cpu`    | `int8`         | AVX2/AVX512 optimised                           |
| Apple Silicon (arm64)  | `cpu`    | `int8`         | NEON int8; MPS not yet supported by CTranslate2 |

With `device = "auto"`, the service detects CUDA availability, queries VRAM via
`nvidia-smi`, and selects `compute_type` accordingly. On Apple Silicon it logs
a note that MPS is not yet supported and falls back to CPU.

## Python version

```
# services/stt/local/.python-version
3.12.3
```

`uv` reads this file automatically and installs the exact version if not present.
No `pyenv` configuration is needed. The venv lives at `services/stt/local/.venv`
and is scoped entirely to this service.
