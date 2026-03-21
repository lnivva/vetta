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

## 2. MongoDB Setup

Vetta requires a running MongoDB instance **and** two environment variables:

| Variable           | Description                          | Example                                            |  
|--------------------|--------------------------------------|----------------------------------------------------|  
| `MONGODB_URI`      | Connection string to your deployment | `mongodb://localhost:27017/?directConnection=true` |  
| `MONGODB_DATABASE` | Database name for Vetta data         | `vetta`                                            |  

Choose whichever option fits your setup, then export both variables.

### Option A: Use an existing MongoDB instance

If you already have MongoDB running (Atlas cluster, self-hosted, etc.), skip to exporting  
your variables and continue to [Step 2](#2-start-the-stt-service):

```bash  
export MONGODB_URI="your-connection-string"  
export MONGODB_DATABASE="vetta"  
```

### Option B: Run MongoDB locally with Atlas CLI

The [Atlas CLI](https://www.mongodb.com/docs/atlas/cli/current/install-atlas-cli/) can spin up a  
full-featured local Atlas deployment inside Docker — no cloud account required.

```bash  
# Install the Atlas CLI (macOS)  
brew install mongodb-atlas-cli  
  
# A Docker-compatible runtime is required.  
# If you don't have Docker Desktop, Colima works well on macOS:  
brew install colima docker  
colima start  
```  

Start the local deployment:

```bash  
atlas local setup vetta-local --port 27017 --bindIpAll
```  

On first run the CLI pulls the required container images. Once ready you'll see:

```text
Deployment vetta-local created.
```  

Then **export the required environment variables**:

```bash  
export MONGODB_URI="mongodb://localhost:27017/?directConnection=true"
export MONGODB_DATABASE="vetta"
```  

::: warning Required  
Both variables must be set in every shell session. Consider adding them to your  
`~/.bashrc`, `~/.zshrc`, or a local `.env` file.  
:::

::: tip Managing the local deployment

```bash  
# Check status  
atlas local list
  
# Stop (data is preserved)  
atlas local pause vetta-local
  
# Start again later  
atlas local start vetta-local
  
# Remove completely (data is lost)  
atlas local delete vetta-local
```  

:::

## 3. Hugging Face Token Setup (Required)

Vetta downloads models from the Hugging Face Hub. Some models, notably the PyAnnote speaker diarization pipeline, *
*require authenticated access**.

Without authentication, you may encounter:

- Lower rate limits
- Slower downloads
- Failures when loading diarization models

### Step 1: Create a Hugging Face access token

1. Visit https://huggingface.co/settings/tokens
2. Click **New token**
3. Select **Read** access
4. Copy the generated token

### Step 2: Configure the token in `config.toml`

Edit the diarization section of:

```
services/stt/local/config.toml
```

```toml
[diarization]
enabled = true
hf_token = "hf_xxxxxxxxxxxxxxxxxxxxxxxxx"
model = "pyannote/speaker-diarization-3.1"
device = "cpu"
min_speakers = 0
max_speakers = 0
```

> **Important**
>
> - Do **not** commit your Hugging Face token to version control
> - For production deployments, prefer injecting the token via environment variables or a secrets manager

Once configured, authenticated downloads will no longer emit this warning:

```text
Warning: You are sending unauthenticated requests to the HF Hub
```

## 4. Start the STT service

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

## 5. Generating a test audio file (macOS Only)

::: tip Platform Support
The `say` command below is macOS-specific. Linux/Windows users should instead pass their MP3 using the
`--file /path/to/file.mp3` flag.  
:::

```bash
# Speaker 1 (Samantha)  
say -v Samantha "Good morning everyone and welcome to the Q3 2024 earnings call. We are pleased to report \
record revenue of 4.2 billion dollars. This represents a 15 percent increase year over year. I will now hand it \
over to our CFO for the financial details." -o /tmp/speaker1.aiff
  
# Speaker 2 (Daniel)  
say -v Daniel "Thank you. As mentioned, total revenue came in at 4.2 billion. Operating expenses were 2.1 billion,\
resulting in a healthy margin. We also saw strong growth in our cloud division, which contributed 1.8 billion in \
recurring revenue." -o /tmp/speaker2.aiff
  
# Speaker 1 again (Samantha)  
say -v Samantha "Thank you for that overview. Let me now open the floor for questions. We have several analysts \
on the line today." -o /tmp/speaker3.aiff  
  
# Combine with ffmpeg - convert to 16kHz mono WAV  
ffmpeg -y -i /tmp/speaker1.aiff -i /tmp/speaker2.aiff -i /tmp/speaker3.aiff \
-filter_complex "[0:a][1:a][2:a]concat=n=3:v=0:a=1[out]" -map "[out]" -ar 16000 -ac 1 /tmp/test.wav  
```

## 6. Process test audio file

```bash
cargo run -- earnings process --file /tmp/test.wav --ticker XXXX --year 2024 --quarter q3
```

The pipeline prints live progress as transcript chunks stream back:

```text
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
