# STT Service Configuration Reference

The Whisper STT service is configured via a `config.toml` file. Every value can be overridden at runtime with an
environment variable following the pattern:

```text
WHISPER_<SECTION>_<KEY>
```

For example, `WHISPER_MODEL_SIZE=medium` overrides `[model] size`.

## `[service]`

General service-level settings that control the gRPC server behavior and operational limits.

| Property            | Type      | Default                    | Description                                                                                                                                                                                                                        |
|---------------------|-----------|----------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `address`           | `string`  | `unix:///tmp/whisper.sock` | The address the gRPC server binds to. Supports two formats: **Unix domain socket** (`unix:///path/to/socket`) for same-host communication, or **TCP** (`host:port`) for remote/cross-machine access. See details below.            |
| `log_level`         | `string`  | `info`                     | Minimum severity level for log output. Logs are emitted as structured JSON to stdout.                                                                                                                                              |
| `max_audio_size_mb` | `integer` | `100`                      | Maximum allowed audio payload size in megabytes. Applies to both inline `data` (raw bytes) and remote `uri` sources (checked via the `Content-Length` header). Requests exceeding this limit are rejected with `INVALID_ARGUMENT`. |

### `address` formats

| Format          | Example                    | Description                                                                                                                                                                                                                          |
|-----------------|----------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `unix://<path>` | `unix:///tmp/whisper.sock` | Binds to a Unix domain socket. The socket file is created on startup (any existing file at this path is removed first) and its permissions are set to `0600` (owner read/write only). **Best for same-host or sidecar deployments.** |
| `<host>:<port>` | `0.0.0.0:50051`            | Binds to a TCP address. Use `0.0.0.0` to listen on all interfaces, or a specific IP to restrict. **Required for cross-machine access.**                                                                                              |

:::tip Choosing between UDS and TCP

| Consideration        | Unix Domain Socket                    | TCP                              |
|----------------------|---------------------------------------|----------------------------------|
| Cross-machine access | Same host only                        | Any network topology             |
| Performance          | ~2–3× lower latency (no TCP overhead) | Slight overhead from TCP stack   |
| Security             | Filesystem permissions (simple)       | Requires TLS/mTLS or firewall    |
| Containers           | Requires shared volume mount          | Works natively across containers |
| Load balancing       | Not applicable                        | Standard L4/L7 balancing         |

**Use UDS** when the client runs on the same machine or in a sidecar container.
**Use TCP** when the client runs on a different machine or behind a load balancer.
:::

:::warning TCP security
When using a TCP address, the server binds **without TLS** by default. In production, place the service behind a
TLS-terminating proxy (e.g., Envoy, nginx) or implement mTLS at the gRPC level. Never expose an insecure TCP port to
untrusted networks.
:::

### `log_level` values

| Value   | Description                                                                                                                |
|---------|----------------------------------------------------------------------------------------------------------------------------|
| `debug` | Verbose output including internal pipeline details. Useful during development.                                             |
| `info`  | Standard operational logging — request starts, transcription metadata, diarization status. **Recommended for production.** |
| `warn`  | Only warnings and errors.                                                                                                  |
| `error` | Only errors.                                                                                                               |

:::tip
When using a Unix domain socket, the socket path must be accessible to both the gRPC server process and any client
connecting to it. When running in a container, mount the socket directory as a shared volume.
:::

## `[model]`

Controls which Whisper model is loaded, how it runs on the available hardware, and authentication for downloading gated
models.

| Property       | Type     | Default                   | Description                                                                                                                                                                                                                                     |
|----------------|----------|---------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `size`         | `string` | `large-v3`                | The Whisper model variant to load. Larger models are more accurate but require more memory and are slower to run.                                                                                                                               |
| `download_dir` | `string` | `/var/lib/whisper/models` | Local directory where model weights are cached. On first run the model is downloaded from Hugging Face to this path. Subsequent starts load from cache.                                                                                         |
| `download_dir` | `string` | `/tmp/whisper_models`     | Local directory where model weights are cached. On first run the model is downloaded from Hugging Face to this path. Subsequent starts load from cache.                                                                                         |
| `device`       | `string` | `cpu`                     | Compute device for the Whisper (CTranslate2) model.                                                                                                                                                                                             |
| `compute_type` | `string` | `int8`                    | Numerical precision used for model inference. Affects speed, memory usage, and — to a minor degree — accuracy.                                                                                                                                  |
| `hf_token`     | `string` | `""`                      | Hugging Face API token used for downloading gated models. When set, the service logs into the Hugging Face Hub at startup and exports `HF_TOKEN` to the environment. This token is also used by the diarization pipeline (pyannote) if enabled. |

### `size` values

| Value      | Parameters | Relative Speed | English WER | VRAM (float16) | Notes                                                |
|------------|------------|----------------|-------------|----------------|------------------------------------------------------|
| `tiny`     | 39M        | ~32x           | ~7.7%       | ~1 GB          | Fastest. Suitable for real-time on CPU.              |
| `base`     | 74M        | ~16x           | ~5.8%       | ~1 GB          | Good balance for low-resource environments.          |
| `small`    | 244M       | ~6x            | ~4.2%       | ~2 GB          | Solid mid-range choice.                              |
| `medium`   | 769M       | ~2x            | ~3.5%       | ~5 GB          | High accuracy. Works well on 8 GB GPUs.              |
| `large-v3` | 1550M      | 1x             | ~2.7%       | ~10 GB         | Best accuracy. **Recommended when hardware allows.** |

:::note
Word Error Rate (WER) figures are approximate and vary by language, audio quality, and domain. These are based on
OpenAI's published benchmarks on English test sets.
:::

### `device` values

| Value  | Description                                                                                                                                                                                                                    |
|--------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `auto` | **Recommended.** Automatically selects `cuda` if a compatible NVIDIA GPU with CTranslate2 CUDA support is detected; otherwise falls back to `cpu`. On Apple Silicon, logs a note that MPS is not yet supported by CTranslate2. |
| `cuda` | Force GPU inference via CUDA. Requires an NVIDIA GPU with compatible drivers and CTranslate2 CUDA support.                                                                                                                     |
| `cpu`  | Force CPU inference. Works on all platforms. Pair with `compute_type = "int8"` for best CPU performance.                                                                                                                       |

:::warning
CTranslate2 (the engine behind faster-whisper) does **not** support Apple MPS. On macOS with Apple Silicon, the model
runs on CPU using optimized ARM NEON instructions. Setting `device = "cuda"` on a machine without a compatible GPU will
cause a startup error.
:::

### `compute_type` values

| Value          | Compatible Devices | Description                                                                                                                                            |
|----------------|--------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------|
| `auto`         | All                | **Recommended.** Selects the best type for your hardware: `float16` for CUDA with ≥8 GB VRAM, `int8_float16` for CUDA with <8 GB VRAM, `int8` for CPU. |
| `float16`      | `cuda`             | Half-precision floating point. Best GPU throughput with minimal accuracy loss. Requires NVIDIA GPU with FP16 support (Pascal or newer).                |
| `int8_float16` | `cuda`             | Mixed precision — weights in INT8, activations in FP16. Reduces VRAM usage by ~40% vs. `float16` with a small speed trade-off.                         |
| `int8`         | `cpu`, `cuda`      | 8-bit integer quantization. **Best choice for CPU inference** — leverages AVX2/AVX-512 on x86 and NEON on ARM. ~2x faster than `float32` on CPU.       |
| `float32`      | `cpu`, `cuda`      | Full 32-bit precision. Slowest but highest numerical fidelity. Rarely needed in practice.                                                              |

### `hf_token` setup

The `hf_token` in the `[model]` section serves as the **global** Hugging Face authentication token for the entire
service. It is used both for downloading Whisper models and for accessing gated diarization models (pyannote).

Before using diarization, you must:

1. **Create a Hugging Face account** at [huggingface.co](https://huggingface.co)
2. **Accept the model licenses:**
    - [pyannote/speaker-diarization-3.1](https://huggingface.co/pyannote/speaker-diarization-3.1)
    - [pyannote/segmentation-3.0](https://huggingface.co/pyannote/segmentation-3.0)
3. **Generate an access token** at [huggingface.co/settings/tokens](https://huggingface.co/settings/tokens) with `read`
   scope
4. Set the token in `config.toml` or via the environment variable:

```bash
export WHISPER_MODEL_HF_TOKEN="hf_abc123..."
```

:::danger
Never commit your `hf_token` to version control. Use environment variables or a secrets manager in production.
:::

## `[inference]`

Parameters that control the transcription behavior of the Whisper model at request time.

| Property                      | Type      | Default | Range        | Description                                                                                                                                                                                                                 |
|-------------------------------|-----------|---------|--------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `beam_size`                   | `integer` | `5`     | `1`–`10`     | Number of beams for beam search decoding. Higher values improve accuracy at the cost of speed. `1` disables beam search (greedy decoding).                                                                                  |
| `vad_filter`                  | `boolean` | `true`  | —            | Enable Voice Activity Detection preprocessing using Silero VAD. Filters out silent regions before transcription, which reduces hallucinations on audio with long pauses and improves throughput.                            |
| `vad_min_silence_ms`          | `integer` | `1000`  | `100`–`2000` | Minimum silence duration in milliseconds for the VAD to split a segment. Lower values produce more segments (more aggressive splitting); higher values keep longer phrases together.                                        |
| `no_speech_threshold`         | `float`   | `0.4`   | `0.0`–`1.0`  | If the model's no-speech probability for a segment exceeds this threshold, the segment is skipped. Lower values are stricter (skip more); higher values are more permissive.                                                |
| `log_prob_threshold`          | `float`   | `-0.3`  | `−inf`–`0.0` | Average log probability threshold for a segment. Segments with an average log probability below this value are treated as low-confidence and may be discarded. More negative values are more permissive.                    |
| `compression_ratio_threshold` | `float`   | `2.0`   | `1.0`–`5.0`  | Segments with a text compression ratio (using gzip) above this threshold are considered likely hallucinations and are discarded. Repetitive hallucinated text compresses very well, yielding high ratios.                   |
| `word_timestamps`             | `boolean` | `true`  | —            | Enable per-word timestamp extraction. When `true`, each `TranscriptChunk` includes a `words` array with start time, end time, text, and confidence for every word. Automatically enabled when diarization is requested..    |
| `initial_prompt`              | `string`  | `""`    | —            | Default prompt prepended to the transcription context. Useful for guiding the model toward specific terminology, spelling, or formatting conventions. Can be overridden per-request via `TranscribeOptions.initial_prompt`. |

:::tip Tuning for your use case

**Meetings / conversations:** Lower `vad_min_silence_ms` to `200`–`300` to capture quick speaker turns. Keep `beam_size`
at `5`.

**Podcasts / monologues:** `vad_min_silence_ms` of `500`–`800` works well. Consider `beam_size = 3` for faster
processing.

**Noisy audio:** Tighten `no_speech_threshold` to `0.4` and `log_prob_threshold` to `-0.3` to aggressively filter
low-confidence output.

**Domain-specific vocabulary:** Use `initial_prompt` to prime the model, e.g.:

```toml
initial_prompt = "Earnings call transcription."
```

:::

## `[concurrency]`

Controls parallelism and threading for the gRPC server and the CTranslate2 inference engine.

| Property      | Type      | Default    | Description                                                                                       |
|---------------|-----------|------------|---------------------------------------------------------------------------------------------------|
| `max_workers` | `integer` | `0` (auto) | Maximum number of concurrent gRPC request handler threads.                                        |
| `cpu_threads` | `integer` | `0` (auto) | Number of intra-op threads used by CTranslate2 for CPU inference. Ignored when `device = "cuda"`. |
| `num_workers` | `integer` | `1`        | Number of internal faster-whisper DataLoader workers for loading and preprocessing audio.         |

### `max_workers` behavior

| Value | Behavior                                                                                                                                                                 |
|-------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `0`   | **Auto-detect.** `1` for `cuda` (GPU is the bottleneck, parallel requests don't help); `2` for `cpu`.                                                                    |
| `1`   | Serial request processing. Simplest and safest.                                                                                                                          |
| `2`+  | Allows concurrent transcriptions. On CPU, each request shares the `cpu_threads` pool. On GPU, requests are serialized by the GPU anyway, so values >1 only add overhead. |

### `cpu_threads` behavior

| Value | Behavior                                                                                                                       |
|-------|--------------------------------------------------------------------------------------------------------------------------------|
| `0`   | **Auto-detect.** Uses half the physical CPU cores (minimum 1). This leaves headroom for the diarization pipeline and OS tasks. |
| `1`+  | Explicit thread count. Set this if you want precise control, e.g., when co-locating with other services.                       |

:::warning
Setting `cpu_threads` higher than your physical core count causes thread contention
and **reduces** performance. When diarization is enabled, ensure that the combined
thread usage of CTranslate2 + PyTorch (pyannote) does not exceed your core count.

**Rule of thumb:** `cpu_threads` ≤ `physical_cores / 2` when diarization is enabled.
:::

## `[diarization]`

Configuration for the optional speaker diarization pipeline powered
by [pyannote.audio](https://github.com/pyannote/pyannote-audio). When enabled, the service can identify **who spoke when
** and populate the `speaker_id` field in `TranscriptChunk` responses.

| Property       | Type      | Default                            | Description                                                                                                                                                  |
|----------------|-----------|------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `enabled`      | `boolean` | `true`                             | Whether to load the diarization pipeline at startup. When `false`, the pipeline is not loaded and diarization requests are rejected with `INVALID_ARGUMENT`. |
| `model`        | `string`  | `pyannote/speaker-diarization-3.1` | The pyannote pipeline model identifier on Hugging Face.                                                                                                      |
| `device`       | `string`  | `auto`                             | Compute device for the diarization (PyTorch) model.                                                                                                          |
| `min_speakers` | `integer` | `0`                                | Default minimum number of expected speakers. Can be overridden per-request via `TranscribeOptions.num_speakers`.                                             |
| `max_speakers` | `integer` | `0`                                | Default maximum number of expected speakers. Can be overridden per-request via `TranscribeOptions.num_speakers`.                                             |

:::note Authentication
The diarization pipeline uses the `hf_token` from the `[model]` section for authenticating with Hugging Face. There is
no separate token for diarization. See the [`[model]` hf_token setup](#hf_token-setup) section for details.
:::

### `device` values

| Value  | Description                                                                                                                                                  |
|--------|--------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `auto` | Uses the same device as the Whisper model (`[model] device`). If the Whisper model resolved to `cuda`, diarization will also use `cuda`; likewise for `cpu`. |
| `cuda` | Force GPU inference via CUDA. The pyannote pipeline uses PyTorch, so standard CUDA + PyTorch requirements apply.                                             |
| `cpu`  | Force CPU inference. Useful for offloading diarization to CPU when the GPU is reserved for Whisper.                                                          |

:::note Apple Silicon
On Apple Silicon (M1/M2/M3/M4), both CTranslate2 (Whisper) and the diarization device resolution are limited to `cpu`
and `cuda`. The current device resolution logic does not support `mps`. Whisper pipeline will run on CPU using optimized
ARM NEON/INT8 instructions:

```toml
[model]
device = "cpu"          # CTranslate2 doesn't support MPS
compute_type = "int8"   # Fast on ARM NEON

[diarization]
device = "cpu"          # Resolved via same logic as [model] device
```

:::

### `min_speakers` / `max_speakers` behavior

| Value | Behavior                                                                                                                           |
|-------|------------------------------------------------------------------------------------------------------------------------------------|
| `0`   | **Auto-detect.** Let pyannote determine the number of speakers automatically. Works well for most cases.                           |
| `1`+  | Constrain the speaker count. If you know the audio contains exactly 2 speakers, set both to `2` for significantly better accuracy. |

These serve as **defaults** — they can be overridden per-request via the `num_speakers` field in `TranscribeOptions`.
When `num_speakers` is set in a request, it is used for both `min_speakers` and `max_speakers`.

:::note Diarization and streaming
When diarization is enabled for a request, transcript segments are **collected before being returned** (rather than
streamed incrementally). This is because speaker labels are assigned by computing temporal overlap between Whisper
segments and pyannote speaker turns — which requires all segments to be available.

This adds latency proportional to the audio duration. For real-time streaming without speaker labels, send requests with
`diarization = false`.
:::

## Environment Variable Reference

All settings support environment variable overrides. The variable name follows the pattern `WHISPER_<SECTION>_<KEY>` in
uppercase.

| Environment Variable                            | Config Equivalent                         | Type      |
|-------------------------------------------------|-------------------------------------------|-----------|
| `WHISPER_SERVICE_ADDRESS`                       | `[service] address`                       | `string`  |
| `WHISPER_SERVICE_LOG_LEVEL`                     | `[service] log_level`                     | `string`  |
| `WHISPER_SERVICE_MAX_AUDIO_SIZE_MB`             | `[service] max_audio_size_mb`             | `integer` |
| `WHISPER_MODEL_SIZE`                            | `[model] size`                            | `string`  |
| `WHISPER_MODEL_DOWNLOAD_DIR`                    | `[model] download_dir`                    | `string`  |
| `WHISPER_MODEL_DEVICE`                          | `[model] device`                          | `string`  |
| `WHISPER_MODEL_COMPUTE_TYPE`                    | `[model] compute_type`                    | `string`  |
| `WHISPER_MODEL_HF_TOKEN`                        | `[model] hf_token`                        | `string`  |
| `WHISPER_INFERENCE_BEAM_SIZE`                   | `[inference] beam_size`                   | `integer` |
| `WHISPER_INFERENCE_VAD_FILTER`                  | `[inference] vad_filter`                  | `boolean` |
| `WHISPER_INFERENCE_VAD_MIN_SILENCE_MS`          | `[inference] vad_min_silence_ms`          | `integer` |
| `WHISPER_INFERENCE_NO_SPEECH_THRESHOLD`         | `[inference] no_speech_threshold`         | `float`   |
| `WHISPER_INFERENCE_LOG_PROB_THRESHOLD`          | `[inference] log_prob_threshold`          | `float`   |
| `WHISPER_INFERENCE_COMPRESSION_RATIO_THRESHOLD` | `[inference] compression_ratio_threshold` | `float`   |
| `WHISPER_INFERENCE_WORD_TIMESTAMPS`             | `[inference] word_timestamps`             | `boolean` |
| `WHISPER_INFERENCE_INITIAL_PROMPT`              | `[inference] initial_prompt`              | `string`  |
| `WHISPER_CONCURRENCY_MAX_WORKERS`               | `[concurrency] max_workers`               | `integer` |
| `WHISPER_CONCURRENCY_CPU_THREADS`               | `[concurrency] cpu_threads`               | `integer` |
| `WHISPER_CONCURRENCY_NUM_WORKERS`               | `[concurrency] num_workers`               | `integer` |
| `WHISPER_DIARIZATION_ENABLED`                   | `[diarization] enabled`                   | `boolean` |
| `WHISPER_DIARIZATION_MODEL`                     | `[diarization] model`                     | `string`  |
| `WHISPER_DIARIZATION_DEVICE`                    | `[diarization] device`                    | `string`  |
| `WHISPER_DIARIZATION_MIN_SPEAKERS`              | `[diarization] min_speakers`              | `integer` |
| `WHISPER_DIARIZATION_MAX_SPEAKERS`              | `[diarization] max_speakers`              | `integer` |

**Type casting rules:**

- **boolean**: `1`, `true`, `yes` (case-insensitive) → `true`; anything else → `false`
- **integer**: Parsed with `int()`
- **float**: Parsed with `float()`
- **string**: Used as-is

## Example Configurations

### Minimal CPU setup

```toml
[service]
address = "unix:///tmp/whisper.sock"

[model]
device = "cpu"
compute_type = "int8"
size = "base"

[diarization]
enabled = false
```

### Production GPU server (TCP)

```toml
[service]
address = "127.0.0.1:50051"

[model]
device = "cuda"
compute_type = "float16"
size = "large-v3"
hf_token = ""  # use WHISPER_MODEL_HF_TOKEN env var

[concurrency]
max_workers = 1
cpu_threads = 4

[diarization]
enabled = true
device = "cuda"
```

### Apple Silicon (M-series Mac)

```toml
[service]
address = "unix:///tmp/whisper.sock"

[model]
device = "cpu"
compute_type = "int8"
size = "large-v3"
hf_token = ""  # use WHISPER_MODEL_HF_TOKEN env var

[concurrency]
cpu_threads = 4

[diarization]
enabled = true
device = "cpu"
```