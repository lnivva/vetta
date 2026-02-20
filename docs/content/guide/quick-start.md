# Quick Start

## Prerequisites

| Tool          | Purpose               | Install                                                           |
|---------------|-----------------------|-------------------------------------------------------------------|
| Rust (stable) | Core crate + CLI      | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| uv            | Python env management | `curl -LsSf https://astral.sh/uv/install.sh \| sh`                |
| protoc        | Proto compilation     | `brew install protobuf`                                           |
| ffmpeg        | Audio conversion      | `brew install ffmpeg`                                             |

## 1. Clone and build

```bash
git clone https://github.com/lnivva/vetta
cd vetta
cargo build
```

## 2. Start the STT service

```bash
cd services/stt/local
uv sync
uv run python main.py --config config.toml
```

On first run the service downloads `whisper-large-v3` (~3GB). Subsequent starts load from cache.
You will see the ready line when it is accepting connections:

```
[whisper] ready on /tmp/whisper.sock
```

## 3. Generating a test audio file (macOS Only)

```bash
say -v Samantha \
  "Good morning everyone and welcome to the Q3 2024 earnings call. \
   We are pleased to report record revenue of 4.2 billion dollars." \
  -o /tmp/test.aiff

ffmpeg -i /tmp/test.aiff /tmp/test.mp3
```

## 4. Process test audio file

::: tip Platform Support
The `say` command above is macOS-specific. Linux/Windows users should instead pass their MP3 using the
`--file /path/to/file.mp3` flag.  
:::

```bash
cargo run -- earnings process \
  --file /tmp/test.mp3 \
  --ticker XXXX \
  --year 2024 \
  --quarter q3
```

The pipeline prints live progress as transcript chunks stream back:

```
   VETTA FINANCIAL ENGINE
   ======================

   TARGET:    XXXX Q3 2024
   INPUT:     /tmp/test.mp3

   ✔ VALIDATION PASSED
   Format:    audio/mpeg

   Processing Pipeline:
   1. [✔] Validation
   2. [RUNNING] Transcription (Whisper)
   [0.0s → 3.5s] Good morning and welcome to the Q3 2024 earnings call...
   2. [✔] Transcription (142 segments)
   3. [WAITING] Vector Embedding
```
